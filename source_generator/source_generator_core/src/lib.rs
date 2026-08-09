use proc_macro::TokenStream;
use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote, quote_spanned};
use syn::{
    Data, DeriveInput, Expr, Fields, GenericArgument, Ident, ItemFn, MetaNameValue, Pat, Path,
    PathArguments, Token, Type, braced, parenthesized,
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
    visit::{self, Visit},
};

/// 解析调用方实际使用的运行时 crate 路径
fn runtime_crate_path() -> syn::Result<TokenStream2> {
    match crate_name("source_generator_runtime") {
        Ok(FoundCrate::Itself) => Ok(quote! { crate }),
        Ok(FoundCrate::Name(name)) => {
            let ident = Ident::new(&name, proc_macro2::Span::call_site());
            Ok(quote! { ::#ident })
        }
        Err(error) => Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            format!("无法解析 source_generator_runtime 依赖名称: {error}"),
        )),
    }
}

// ─── logging ─────────────────────────────────────────────────────────────────

/// 为 async fn 方法自动注入结构化调用日志
///
/// 在方法执行前记录 executing 日志，执行后记录 executed 日志（含耗时），
/// 发生异常时记录 exception 日志（含耗时与错误描述）
///
/// 仅支持返回 Result<T, E> 的 async fn，E 需实现 Debug
/// 必须通过 key_scope 声明跨构建稳定且全局唯一的方法标识
/// receiver 方法必须显式配置 key_include_self，普通参数类型不参与方法键
/// 方法自身泛型与参数位置的 impl Trait 会加入实际类型维度并需声明 StableKeyType 约束
///
/// # 示例
/// ```ignore
/// use source_generator_runtime::logging;
///
/// impl MyService {
///     #[logging(
///         key_scope = "application.my-service.get-user",
///         key_include_self = false
///     )]
///     pub async fn get_user(&self, id: i64) -> Result<UserDto, AppError> {
///         // 方法体
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn logging(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as LoggingArgs);
    let input = parse_macro_input!(item as ItemFn);

    expand_logging(args, input).unwrap_or_else(|e| e.to_compile_error().into())
}

/// #[logging] 的方法标识参数
struct LoggingArgs {
    /// 显式稳定方法作用域
    key_scope: Option<syn::LitStr>,

    /// 宏调用处显式声明的额外类型维度
    key_types: Vec<Type>,

    /// 宏调用处显式声明的额外常量维度
    key_consts: Vec<Expr>,

    /// 显式作用域是否包含 receiver 的 Self 类型
    key_include_self: Option<bool>,
}

impl Parse for LoggingArgs {
    /// 解析 logging 的稳定方法标识参数
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let metas = Punctuated::<syn::Meta, Token![,]>::parse_terminated(input)?;
        let mut key_scope = None;
        let mut key_types = None;
        let mut key_consts = None;
        let mut key_include_self = None;

        for meta in &metas {
            let name = meta
                .path()
                .get_ident()
                .ok_or_else(|| syn::Error::new_spanned(meta.path(), "期望标识符"))?
                .to_string();
            if !parse_scoped_key_meta(
                meta,
                &name,
                &mut key_scope,
                &mut key_types,
                &mut key_consts,
                &mut key_include_self,
            )? {
                return Err(syn::Error::new_spanned(
                    meta.path(),
                    format!(
                        "未知参数 `{name}`，支持 key_scope / key_types(...) / key_consts(...) / key_include_self"
                    ),
                ));
            }
        }

        validate_scoped_key_options(
            input,
            key_scope.as_ref(),
            key_types.as_deref(),
            key_consts.as_deref(),
            key_include_self,
        )?;

        Ok(Self {
            key_scope,
            key_types: key_types.unwrap_or_default(),
            key_consts: key_consts.unwrap_or_default(),
            key_include_self,
        })
    }
}

/// 展开 #[logging] 宏，生成包含日志注入的完整方法体
fn expand_logging(args: LoggingArgs, mut input: ItemFn) -> syn::Result<TokenStream> {
    bind_wildcard_impl_trait_parameters(&mut input);
    let sig = &input.sig;
    let runtime = runtime_crate_path()?;

    // 仅支持 async fn，同步方法不适用此宏
    if sig.asyncness.is_none() {
        return Err(syn::Error::new_spanned(sig, "#[logging] 只能用于 async fn"));
    }

    validate_receiver_key_strategy(sig, args.key_include_self, "logging")?;

    let attrs = &input.attrs;
    let vis = &input.vis;
    let body = &input.block;
    let key_scope = args
        .key_scope
        .as_ref()
        .ok_or_else(|| syn::Error::new_spanned(sig, "缺少必填参数 key_scope"))?;
    let method_key = build_scoped_method_key(
        sig,
        key_scope,
        &args.key_types,
        &args.key_consts,
        args.key_include_self,
        &runtime,
    )?;

    let expanded: TokenStream2 = quote! {

        #(#attrs)*
        #vis #sig {

            // 生成本次调用的唯一追踪 ID，用于关联 executing/executed/exception 三条日志
            let __log_trace_id = #runtime::new_trace_id();

            // 组合模块、实现类型和完整签名，形成结构化日志中的 method 字段
            let __log_method = #method_key;

            #runtime::log_executing(&__log_method, &__log_trace_id);

            // 记录方法开始时间，用于计算 duration_ms
            let __log_start = ::std::time::Instant::now();

            // 将原始方法体移入 async 块执行，确保参数所有权随 future 传递
            let __log_result = (async move #body).await;

            let __log_duration_ms = __log_start.elapsed().as_millis();

            // 根据执行结果分别记录成功或异常日志
            match &__log_result {
                ::core::result::Result::Ok(_) => {
                    #runtime::log_executed(&__log_method, &__log_trace_id, __log_duration_ms);
                }
                ::core::result::Result::Err(__log_err) => {
                    #runtime::log_exception_debug(
                        &__log_method,
                        &__log_trace_id,
                        __log_duration_ms,
                        __log_err,
                    );
                }
            }

            __log_result

        }

    };

    Ok(expanded.into())
}

// ─── retry ───────────────────────────────────────────────────────────────────

/// 为 async fn 方法开启自动重试，返回 Err 时按配置次数重试
///
/// - `max_retries`：最大重试次数，默认 3
/// - `delay_seconds`：每次重试前等待秒数，默认 0
/// - `delay_ms`：Rust 侧扩展的毫秒级等待配置，不能与 `delay_seconds` 同时使用
///
/// 仅支持返回 Result<T, E> 的 async fn，E 需实现 Debug
///
/// # 使用限制
///
/// receiver 只支持不可变引用，其他参数必须实现 `Copy`，且不支持可变绑定、可变引用或可变裸指针。
/// 必须通过 key_scope 声明跨构建稳定且全局唯一的方法标识。
/// receiver 方法必须显式配置 key_include_self，普通参数类型不参与方法键。
/// 方法自身泛型与参数位置的 impl Trait 会加入实际类型维度并需声明 StableKeyType 约束。
/// 需要在单次尝试中修改参数时，应在方法体内从不可变原参数创建局部副本或克隆，
/// 使该局部值能够在每次重试执行方法体时重新初始化。
///
/// # 示例
/// ```ignore
/// use source_generator_runtime::retry;
///
/// impl MyService {
///     // 参数均为 Copy 类型或引用，适合使用 retry
///     #[retry(
///         max_retries = 3,
///         delay_seconds = 1,
///         key_scope = "application.my-service.call-external",
///         key_include_self = false
///     )]
///     pub async fn call_external(&self, id: i64) -> Result<String, AppError> {
///         // 外部调用可能失败
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn retry(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as RetryArgs);
    let input = parse_macro_input!(item as ItemFn);

    expand_retry(args, input).unwrap_or_else(|e| e.to_compile_error().into())
}

/// 为 async fn 方法开启缓存包装，命中缓存时直接返回序列化结果
///
/// - cache：缓存实例表达式，支持 Arc<HybridCache> 或 &HybridCache
/// - ttl_seconds：L1 与 L2 共同遵守的缓存有效期秒数，默认 60
/// - lock：可选的分布式锁实例表达式，用于缓存回源防击穿
/// - lock_expiry_seconds：防击穿锁有效期秒数，默认 60
/// - key_scope：必填的稳定方法键
/// - key_types(...)：加入实现 StableKeyType 的外围类型键维度，方法自身类型泛型会自动加入
/// - key_consts(...)：显式加入外围 const 泛型等宏不可见的常量键维度，方法自身 const 泛型会自动加入
/// - key_include_self：receiver 方法必须显式声明是否通过 StableKeyType 将 Self 加入键
/// - key：可选的显式参数键表达式，包含所有业务相关参数且不得移动方法参数
/// - cache_value：返回类型别名等无法静态分析时，显式声明是否缓存成功值
///
/// 显式参数键应优先写成 `key = (&id, &tenant_id)`，无序集合应使用
/// `key = source_generator_runtime::unordered_key(&ids)` 明确声明集合语义
/// 未配置 key 时，除 receiver 外的全部参数都必须实现 StableInvocationKey
/// 配置 key 后只要求该表达式实现 StableInvocationKey，调用方负责包含所有影响结果的稳定业务维度
/// StableInvocationKey 不支持可变引用，派生类型中的嵌套可变引用也会被拒绝
/// 稳定参数键生成失败时会记录降级日志并绕过缓存
/// Result<(), E> 或显式 cache_value = false 时不会生成缓存代码，也不会引入缓存相关 Trait 约束
///
/// 仅支持返回 Result<T, E> 的 async fn，T 需实现 Serialize、DeserializeOwned 与 PartialEq
/// receiver 必须是不可变引用，其他参数只支持不带子模式的标识符绑定
/// 启用缓存值时不支持包含可变引用的参数
#[proc_macro_attribute]
pub fn cacheable(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as CacheableArgs);
    let input = parse_macro_input!(item as ItemFn);

    expand_cacheable(args, input).unwrap_or_else(|e| e.to_compile_error().into())
}

/// 为 async fn 方法开启并发限制包装，使用分布式锁控制同 key 并发数
///
/// - lock：必填的分布式锁实例表达式
/// - is_use_parameter：是否将参数加入锁 key，仅接受布尔字面量
/// - is_block：获取不到锁时是否立即返回错误，仅接受布尔字面量
/// - semaphore：允许的最大并发数，默认 1，仅接受整数值字面量
/// - expiry_seconds：锁过期秒数，默认 0 表示使用锁实现默认值，仅接受整数值字面量
/// - key_scope：必填的稳定方法键
/// - key_types(...)：加入实现 StableKeyType 的外围类型键维度，方法自身类型泛型会自动加入
/// - key_consts(...)：显式加入外围 const 泛型等宏不可见的常量键维度，方法自身 const 泛型会自动加入
/// - key_include_self：receiver 方法必须显式声明是否通过 StableKeyType 将 Self 加入键
/// - key：is_use_parameter 为 true 时可使用的显式参数键表达式
///
/// 显式参数键应包含全部会影响并发隔离的业务维度，并通过引用避免移动方法参数
/// 无序集合应使用 source_generator_runtime::unordered_key 显式声明集合语义
/// 未配置 key 时，除 receiver 外的全部参数都必须实现 StableInvocationKey
/// 可变引用参数应在显式 key 中通过 &*value 创建不可变重借用
/// 参数键生成失败时会返回错误，禁止绕过锁继续执行
///
/// 仅支持返回 Result<T, E> 的 async fn，E 需实现 From<String> 与 From<LockError>
#[proc_macro_attribute]
pub fn concurrency_limit(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as ConcurrencyLimitArgs);
    let input = parse_macro_input!(item as ItemFn);

    expand_concurrency_limit(args, input).unwrap_or_else(|e| e.to_compile_error().into())
}

/// 为结构体或枚举生成完整字段稳定调用键实现
///
/// 必须通过 `scope = "..."` 声明稳定类型作用域。默认按照非递归字段的完整声明类型
/// 生成 `StableInvocationKey` 约束；复杂关联类型或自定义递归包装器可以通过
/// `bound = "..."` 替换自动字段约束。所有类型泛型仍需实现 `StableKeyType`，
/// 以便写入派生类型自身的稳定泛型维度。
#[proc_macro_derive(StableInvocationKey, attributes(invocation_key))]
pub fn derive_stable_invocation_key(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);

    expand_stable_invocation_key(input).unwrap_or_else(|error| error.to_compile_error().into())
}

/// 展开 StableInvocationKey 派生实现
fn expand_stable_invocation_key(input: DeriveInput) -> syn::Result<TokenStream> {
    let runtime = runtime_crate_path()?;
    let options = parse_invocation_key_options(&input)?;
    let scope = &options.scope;
    let type_name = &input.ident;
    validate_invocation_key_fields(&input.data)?;

    let mut invocation_generics = input.generics.clone();
    let invocation_bounds = options
        .invocation_bounds
        .unwrap_or_else(|| build_invocation_key_field_bounds(&input.data, type_name, &runtime));
    invocation_generics
        .make_where_clause()
        .predicates
        .extend(invocation_bounds);
    for parameter in input.generics.type_params() {
        let ident = &parameter.ident;
        invocation_generics
            .make_where_clause()
            .predicates
            .push(syn::parse_quote!(#ident: #runtime::StableKeyType));
    }
    let (impl_generics, type_generics, where_clause) = invocation_generics.split_for_impl();
    let generic_writes = build_invocation_key_generic_writes(&input.generics, &runtime);
    let generic_count = generic_writes.len();
    let body = match &input.data {
        Data::Struct(data) => build_struct_invocation_key_body(
            &data.fields,
            scope,
            &runtime,
            &generic_writes,
            generic_count,
        )?,
        Data::Enum(data) => {
            build_enum_invocation_key_body(data, scope, &runtime, &generic_writes, generic_count)?
        }
        Data::Union(data) => {
            return Err(syn::Error::new_spanned(
                data.union_token,
                "StableInvocationKey 不支持 union",
            ));
        }
    };

    let mut type_key_generics = input.generics.clone();
    for parameter in input.generics.type_params() {
        let ident = &parameter.ident;
        type_key_generics
            .make_where_clause()
            .predicates
            .push(syn::parse_quote!(#ident: #runtime::StableKeyType));
    }
    let (type_key_impl_generics, type_key_type_generics, type_key_where_clause) =
        type_key_generics.split_for_impl();
    let generic_type_parts = build_stable_key_type_generic_parts(&input.generics, &runtime);

    Ok(quote! {
        impl #impl_generics #runtime::StableInvocationKey for #type_name #type_generics #where_clause {
            fn write_stable_invocation_key(
                &self,
                writer: &mut #runtime::StableInvocationKeyWriter,
            ) -> ::core::result::Result<(), #runtime::InvocationKeyError> {
                #body
            }
        }

        impl #type_key_impl_generics #runtime::StableKeyType for #type_name #type_key_type_generics #type_key_where_clause {
            fn stable_key_type() -> ::std::string::String {
                #runtime::compose_stable_key_type(
                    "rust:user",
                    &[::std::string::ToString::to_string(#scope), #(#generic_type_parts),*],
                )
            }
        }
    }
    .into())
}

/// StableInvocationKey 派生配置
struct InvocationKeyDeriveOptions {
    /// 用户类型跨构建稳定作用域
    scope: syn::LitStr,

    /// 显式字段编码约束，配置后替换自动字段约束
    invocation_bounds: Option<Vec<syn::WherePredicate>>,
}

/// 解析用户类型显式声明的稳定键作用域和字段约束
fn parse_invocation_key_options(input: &DeriveInput) -> syn::Result<InvocationKeyDeriveOptions> {
    let mut scope = None;
    let mut invocation_bounds = None;

    for attribute in &input.attrs {
        if !attribute.path().is_ident("invocation_key") {
            continue;
        }
        attribute.parse_nested_meta(|meta| {
            if meta.path.is_ident("scope") {
                if scope.is_some() {
                    return Err(meta.error("参数 scope 不能重复配置"));
                }
                let value = meta.value()?.parse::<syn::LitStr>()?;
                if value.value().trim().is_empty() {
                    return Err(syn::Error::new_spanned(
                        &value,
                        "scope 不能为空或只包含空白",
                    ));
                }
                if value.value().chars().any(char::is_control) {
                    return Err(syn::Error::new_spanned(
                        &value,
                        "scope 不能包含换行符或其他控制字符",
                    ));
                }
                scope = Some(value);
                return Ok(());
            }

            if meta.path.is_ident("bound") {
                if invocation_bounds.is_some() {
                    return Err(meta.error("参数 bound 不能重复配置"));
                }
                let value = meta.value()?.parse::<syn::LitStr>()?;
                if value.value().trim().is_empty() {
                    return Err(syn::Error::new_spanned(
                        &value,
                        "bound 不能为空或只包含空白",
                    ));
                }
                let where_clause =
                    syn::parse_str::<syn::WhereClause>(&format!("where {}", value.value()))
                        .map_err(|error| {
                            syn::Error::new_spanned(&value, format!("bound 约束无法解析: {error}"))
                        })?;
                if where_clause.predicates.is_empty() {
                    return Err(syn::Error::new_spanned(
                        &value,
                        "bound 至少需要一个 where 约束",
                    ));
                }
                invocation_bounds = Some(where_clause.predicates.into_iter().collect());
                return Ok(());
            }

            Err(meta.error("未知参数，仅支持 scope / bound"))
        })?;
    }

    let scope = scope.ok_or_else(|| {
        syn::Error::new_spanned(
            &input.ident,
            "派生 StableInvocationKey 时必须配置 #[invocation_key(scope = \"...\")]",
        )
    })?;

    Ok(InvocationKeyDeriveOptions {
        scope,
        invocation_bounds,
    })
}

/// 根据非递归字段的完整声明类型生成稳定键约束
fn build_invocation_key_field_bounds(
    data: &Data,
    type_name: &Ident,
    runtime: &TokenStream2,
) -> Vec<syn::WherePredicate> {
    let fields = match data {
        Data::Struct(data) => data.fields.iter().collect::<Vec<_>>(),
        Data::Enum(data) => data
            .variants
            .iter()
            .flat_map(|variant| variant.fields.iter())
            .collect::<Vec<_>>(),
        Data::Union(_) => Vec::new(),
    };
    let mut seen = std::collections::BTreeSet::new();

    fields
        .into_iter()
        .filter(|field| !type_contains_derived_type(&field.ty, type_name))
        .filter_map(|field| {
            let ty = &field.ty;
            let key = quote! { #ty }.to_string();
            if !seen.insert(key) {
                return None;
            }
            Some(syn::parse_quote!(#ty: #runtime::StableInvocationKey))
        })
        .collect()
}

/// 判断字段是否直接递归包含当前派生类型
fn type_contains_derived_type(ty: &Type, type_name: &Ident) -> bool {
    let mut visitor = DerivedTypeUsageVisitor {
        type_name,
        found: false,
    };
    visitor.visit_type(ty);
    visitor.found
}

/// 当前派生类型递归引用访问器
struct DerivedTypeUsageVisitor<'a> {
    type_name: &'a Ident,
    found: bool,
}

impl<'ast> Visit<'ast> for DerivedTypeUsageVisitor<'_> {
    fn visit_type_path(&mut self, node: &'ast syn::TypePath) {
        if node
            .path
            .segments
            .iter()
            .any(|segment| segment.ident == *self.type_name)
        {
            self.found = true;
            return;
        }

        visit::visit_type_path(self, node);
    }
}

/// 校验派生类型字段并拒绝未支持的字段配置
fn validate_invocation_key_fields(data: &Data) -> syn::Result<()> {
    if let Data::Enum(data) = data {
        for variant in &data.variants {
            if let Some(attribute) = variant
                .attrs
                .iter()
                .find(|attribute| attribute.path().is_ident("invocation_key"))
            {
                return Err(syn::Error::new_spanned(
                    attribute,
                    "StableInvocationKey 当前不支持枚举变体配置",
                ));
            }
        }
    }

    let fields = match data {
        Data::Struct(data) => data.fields.iter().collect::<Vec<_>>(),
        Data::Enum(data) => data
            .variants
            .iter()
            .flat_map(|variant| variant.fields.iter())
            .collect::<Vec<_>>(),
        Data::Union(_) => return Ok(()),
    };

    for field in &fields {
        if let Some(attribute) = field
            .attrs
            .iter()
            .find(|attribute| attribute.path().is_ident("invocation_key"))
        {
            return Err(syn::Error::new_spanned(
                attribute,
                "StableInvocationKey 当前不支持跳过字段，所有字段都必须进入稳定键",
            ));
        }
    }

    Ok(())
}

/// 生成泛型参数的稳定调用键维度
fn build_invocation_key_generic_writes(
    generics: &syn::Generics,
    runtime: &TokenStream2,
) -> Vec<TokenStream2> {
    generics
        .params
        .iter()
        .filter(|parameter| !matches!(parameter, syn::GenericParam::Lifetime(_)))
        .enumerate()
        .filter_map(|(index, parameter)| match parameter {
            syn::GenericParam::Type(parameter) => {
                let ident = &parameter.ident;
                let index = index.to_string();
                Some(quote! {
                    writer.write_node("rust:user-generic-type", 2, |writer| {
                        writer.write_scalar("rust:user-generic-index", #index)?;
                        writer.write_scalar(
                            "rust:user-generic-value",
                            &<#ident as #runtime::StableKeyType>::stable_key_type(),
                        )
                    })?;
                })
            }
            syn::GenericParam::Const(parameter) => {
                let ident = &parameter.ident;
                let index = index.to_string();
                Some(quote! {
                    writer.write_node("rust:user-generic-const", 2, |writer| {
                        writer.write_scalar("rust:user-generic-index", #index)?;
                        writer.write_scalar(
                            "rust:user-generic-value",
                            &::std::string::ToString::to_string(&#ident),
                        )
                    })?;
                })
            }
            syn::GenericParam::Lifetime(_) => None,
        })
        .collect()
}

/// 生成派生类型稳定标签中的泛型维度
fn build_stable_key_type_generic_parts(
    generics: &syn::Generics,
    runtime: &TokenStream2,
) -> Vec<TokenStream2> {
    generics
        .params
        .iter()
        .filter(|parameter| !matches!(parameter, syn::GenericParam::Lifetime(_)))
        .enumerate()
        .filter_map(|(index, parameter)| match parameter {
            syn::GenericParam::Type(parameter) => {
                let ident = &parameter.ident;
                Some(quote! {
                    ::std::format!(
                        "type:{}={}",
                        #index,
                        <#ident as #runtime::StableKeyType>::stable_key_type(),
                    )
                })
            }
            syn::GenericParam::Const(parameter) => {
                let ident = &parameter.ident;
                Some(quote! {
                    ::std::format!("const:{}={}", #index, #ident)
                })
            }
            syn::GenericParam::Lifetime(_) => None,
        })
        .collect()
}

/// 生成派生类型键中的泛型维度节点
fn build_invocation_key_generics_node(
    generic_writes: &[TokenStream2],
    generic_count: usize,
) -> TokenStream2 {
    quote! {
        writer.write_node("rust:user-generics", #generic_count, |writer| {
            #(#generic_writes)*
            ::core::result::Result::Ok(())
        })?;
    }
}

/// 生成结构体稳定键方法体
fn build_struct_invocation_key_body(
    fields: &Fields,
    scope: &syn::LitStr,
    runtime: &TokenStream2,
    generic_writes: &[TokenStream2],
    generic_count: usize,
) -> syn::Result<TokenStream2> {
    let field_writes = build_struct_field_writes(fields, runtime)?;
    let field_count = field_writes.len();
    let generics_node = build_invocation_key_generics_node(generic_writes, generic_count);

    Ok(quote! {
        writer.write_node("rust:user-struct", 3, |writer| {
            writer.write_scalar("rust:user-type", #scope)?;
            #generics_node
            writer.write_node("rust:user-fields", #field_count, |writer| {
                #(#field_writes)*
                ::core::result::Result::Ok(())
            })
        })
    })
}

/// 生成结构体字段稳定键写入语句
fn build_struct_field_writes(
    fields: &Fields,
    runtime: &TokenStream2,
) -> syn::Result<Vec<TokenStream2>> {
    fields
        .iter()
        .enumerate()
        .map(|(index, field)| {
            let (field_name, access) = if let Some(ident) = &field.ident {
                (ident.to_string(), quote! { &self.#ident })
            } else {
                let index = syn::Index::from(index);
                (index.index.to_string(), quote! { &self.#index })
            };
            Ok(quote! {
                writer.write_node("rust:user-field", 2, |writer| {
                    writer.write_scalar("rust:user-field-name", #field_name)?;
                    #runtime::StableInvocationKey::write_stable_invocation_key(#access, writer)
                })?;
            })
        })
        .collect()
}

/// 生成枚举稳定键方法体
fn build_enum_invocation_key_body(
    data: &syn::DataEnum,
    scope: &syn::LitStr,
    runtime: &TokenStream2,
    generic_writes: &[TokenStream2],
    generic_count: usize,
) -> syn::Result<TokenStream2> {
    let generics_node = build_invocation_key_generics_node(generic_writes, generic_count);
    let arms = data
        .variants
        .iter()
        .map(|variant| build_enum_variant_arm(variant, scope, runtime, &generics_node))
        .collect::<syn::Result<Vec<_>>>()?;

    Ok(quote! {
        match self {
            #(#arms),*
        }
    })
}

/// 生成单个枚举变体的稳定键匹配分支
fn build_enum_variant_arm(
    variant: &syn::Variant,
    scope: &syn::LitStr,
    runtime: &TokenStream2,
    generics_node: &TokenStream2,
) -> syn::Result<TokenStream2> {
    let variant_name = variant.ident.to_string();
    let variant_ident = &variant.ident;
    let mut field_writes = Vec::new();

    let pattern = match &variant.fields {
        Fields::Named(fields) => {
            let bindings = fields
                .named
                .iter()
                .map(|field| {
                    field
                        .ident
                        .as_ref()
                        .ok_or_else(|| syn::Error::new_spanned(field, "命名枚举字段缺少标识符"))
                })
                .collect::<syn::Result<Vec<_>>>()?;
            for binding in &bindings {
                let field_name = binding.to_string();
                field_writes.push(quote! {
                    writer.write_node("rust:user-field", 2, |writer| {
                        writer.write_scalar("rust:user-field-name", #field_name)?;
                        #runtime::StableInvocationKey::write_stable_invocation_key(#binding, writer)
                    })?;
                });
            }
            quote! { Self::#variant_ident { #(#bindings),* } }
        }
        Fields::Unnamed(fields) => {
            let bindings = (0..fields.unnamed.len())
                .map(|index| {
                    format_ident!(
                        "__source_generator_invocation_key_field_{index}",
                        span = proc_macro2::Span::mixed_site()
                    )
                })
                .collect::<Vec<_>>();
            for (index, binding) in bindings.iter().enumerate() {
                let field_name = index.to_string();
                field_writes.push(quote! {
                    writer.write_node("rust:user-field", 2, |writer| {
                        writer.write_scalar("rust:user-field-name", #field_name)?;
                        #runtime::StableInvocationKey::write_stable_invocation_key(#binding, writer)
                    })?;
                });
            }
            quote! { Self::#variant_ident(#(#bindings),*) }
        }
        Fields::Unit => quote! { Self::#variant_ident },
    };
    let field_count = field_writes.len();

    Ok(quote! {
        #pattern => writer.write_node("rust:user-enum", 4, |writer| {
            writer.write_scalar("rust:user-type", #scope)?;
            #generics_node
            writer.write_scalar("rust:user-variant", #variant_name)?;
            writer.write_node("rust:user-fields", #field_count, |writer| {
                #(#field_writes)*
                ::core::result::Result::Ok(())
            })
        })
    })
}

/// 生成应用服务工厂方法
///
/// 通过声明式清单生成按上下文分组的工厂方法，减少重复的 `Service::new(context)` 样板代码
#[proc_macro]
pub fn generate_service_factory(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ServiceFactoryInput);

    expand_service_factory(input).unwrap_or_else(|e| e.to_compile_error().into())
}

/// 为具体适配器生成直接服务工厂方法
///
/// 适用于非泛型状态提供器场景，例如 TaskServiceApplicationAdapter
#[proc_macro]
pub fn generate_direct_service_factory(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DirectServiceFactoryInput);

    expand_direct_service_factory(input).unwrap_or_else(|e| e.to_compile_error().into())
}

/// 为任务注册表生成批量注册语句
///
/// 通过声明式清单先注册 queue schedule 再登记 alias，并返回注册结果
#[proc_macro]
pub fn register_task_registry(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as TaskRegistryRegistrationInput);

    expand_task_registry_registration(input).unwrap_or_else(|e| e.to_compile_error().into())
}

/// 服务工厂生成输入
struct ServiceFactoryInput {
    /// impl 泛型参数，省略时保持旧版 `<'a, State>` 兼容行为
    impl_generics: Option<syn::Generics>,

    /// 工厂类型
    factory_type: Type,

    /// 状态类型标识符
    state_type: Ident,

    /// 服务分组
    groups: Vec<ServiceFactoryGroup>,
}

/// 直接服务工厂生成输入
struct DirectServiceFactoryInput {
    /// impl 泛型参数
    impl_generics: syn::Generics,

    /// 工厂类型
    factory_type: Type,

    /// 服务分组
    groups: Vec<DirectServiceFactoryGroup>,
}

/// 直接服务工厂分组
struct DirectServiceFactoryGroup {
    /// 构建服务时使用的上下文表达式
    context_expr: Expr,

    /// 当前分组下的服务清单
    registrations: Vec<ServiceFactoryRegistration>,
}

/// 单个服务分组
struct ServiceFactoryGroup {
    /// 访问该分组所需的上下文提供器约束
    provider_trait: Path,

    /// 构建服务时使用的上下文表达式
    context_expr: Expr,

    /// 当前分组下的服务清单
    registrations: Vec<ServiceFactoryRegistration>,
}

/// 单个服务注册项
struct ServiceFactoryRegistration {
    /// 工厂方法名
    method_name: Ident,

    /// 返回的服务类型
    service_type: Type,
}

/// 任务注册表生成输入
struct TaskRegistryRegistrationInput {
    /// 任务注册表变量表达式
    registry_expr: Expr,

    /// 声明式注册项
    entries: Vec<TaskRegistryRegistrationEntry>,
}

/// 任务注册项
enum TaskRegistryRegistrationEntry {
    /// 队列任务
    Queue { definition: Expr, handler: Expr },

    /// 定时任务
    Schedule { definition: Expr, handler: Expr },

    /// 队列任务别名
    QueueAlias { alias: Expr, target: Expr },

    /// 定时任务别名
    ScheduleAlias { alias: Expr, target: Expr },
}

impl Parse for ServiceFactoryInput {
    /// 解析服务工厂生成配置
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let impl_generics =
            if input.peek(Ident) && input.fork().parse::<Ident>()? == "impl_generics" {
                input.parse::<Ident>()?;
                input.parse::<Token![=]>()?;
                let mut impl_generics = input.parse::<syn::Generics>()?;
                if input.peek(Token![where]) {
                    impl_generics.where_clause = Some(input.parse::<syn::WhereClause>()?);
                }
                input.parse::<Token![;]>()?;
                Some(impl_generics)
            } else {
                None
            };

        let factory_label = input.parse::<Ident>()?;
        if factory_label != "factory" {
            return Err(syn::Error::new(factory_label.span(), "期望 factory"));
        }
        input.parse::<Token![=]>()?;
        let factory_type = input.parse::<Type>()?;
        input.parse::<Token![;]>()?;

        let state_label = input.parse::<Ident>()?;
        if state_label != "state" {
            return Err(syn::Error::new(state_label.span(), "期望 state"));
        }
        input.parse::<Token![=]>()?;
        let state_type = input.parse::<Ident>()?;
        input.parse::<Token![;]>()?;

        let mut groups = Vec::new();
        while !input.is_empty() {
            groups.push(input.parse::<ServiceFactoryGroup>()?);
        }

        Ok(Self {
            impl_generics,
            factory_type,
            state_type,
            groups,
        })
    }
}

impl Parse for DirectServiceFactoryInput {
    /// 解析直接服务工厂生成配置
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let impl_generics_label = input.parse::<Ident>()?;
        if impl_generics_label != "impl_generics" {
            return Err(syn::Error::new(
                impl_generics_label.span(),
                "期望 impl_generics",
            ));
        }
        input.parse::<Token![=]>()?;
        let mut impl_generics = input.parse::<syn::Generics>()?;
        if input.peek(Token![where]) {
            impl_generics.where_clause = Some(input.parse::<syn::WhereClause>()?);
        }
        input.parse::<Token![;]>()?;

        let factory_label = input.parse::<Ident>()?;
        if factory_label != "factory" {
            return Err(syn::Error::new(factory_label.span(), "期望 factory"));
        }
        input.parse::<Token![=]>()?;
        let factory_type = input.parse::<Type>()?;
        input.parse::<Token![;]>()?;

        let mut groups = Vec::new();
        while !input.is_empty() {
            groups.push(input.parse::<DirectServiceFactoryGroup>()?);
        }

        Ok(Self {
            impl_generics,
            factory_type,
            groups,
        })
    }
}

impl Parse for ServiceFactoryGroup {
    /// 解析单个上下文分组
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let provider_trait = input.parse::<Path>()?;
        input.parse::<Token![=>]>()?;

        let context_content;
        parenthesized!(context_content in input);
        let context_expr = context_content.parse::<Expr>()?;

        let registrations_content;
        braced!(registrations_content in input);
        let registrations = Punctuated::<ServiceFactoryRegistration, Token![,]>::parse_terminated(
            &registrations_content,
        )?
        .into_iter()
        .collect::<Vec<_>>();

        Ok(Self {
            provider_trait,
            context_expr,
            registrations,
        })
    }
}

impl Parse for DirectServiceFactoryGroup {
    /// 解析直接服务工厂分组
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let context_content;
        parenthesized!(context_content in input);
        let context_expr = context_content.parse::<Expr>()?;

        let registrations_content;
        braced!(registrations_content in input);
        let registrations = Punctuated::<ServiceFactoryRegistration, Token![,]>::parse_terminated(
            &registrations_content,
        )?
        .into_iter()
        .collect::<Vec<_>>();

        Ok(Self {
            context_expr,
            registrations,
        })
    }
}

impl Parse for ServiceFactoryRegistration {
    /// 解析单个服务注册项
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let method_name = input.parse::<Ident>()?;
        input.parse::<Token![:]>()?;

        Ok(Self {
            method_name,
            service_type: input.parse::<Type>()?,
        })
    }
}

impl Parse for TaskRegistryRegistrationInput {
    /// 解析任务注册表生成配置
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let registry_label = input.parse::<Ident>()?;
        if registry_label != "registry" {
            return Err(syn::Error::new(registry_label.span(), "期望 registry"));
        }
        input.parse::<Token![=]>()?;
        let registry_expr = input.parse::<Expr>()?;
        input.parse::<Token![;]>()?;

        let mut entries = Vec::new();
        while !input.is_empty() {
            entries.push(input.parse::<TaskRegistryRegistrationEntry>()?);
        }

        Ok(Self {
            registry_expr,
            entries,
        })
    }
}

impl Parse for TaskRegistryRegistrationEntry {
    /// 解析单个任务注册项
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let kind = input.parse::<Ident>()?;
        let content;
        braced!(content in input);

        let entry = match kind.to_string().as_str() {
            "queue" => Self::Queue {
                definition: parse_named_expr(&content, "definition")?,
                handler: parse_named_expr(&content, "handler")?,
            },
            "schedule" => Self::Schedule {
                definition: parse_named_expr(&content, "definition")?,
                handler: parse_named_expr(&content, "handler")?,
            },
            "queue_alias" => Self::QueueAlias {
                alias: parse_named_expr(&content, "alias")?,
                target: parse_named_expr(&content, "target")?,
            },
            "schedule_alias" => Self::ScheduleAlias {
                alias: parse_named_expr(&content, "alias")?,
                target: parse_named_expr(&content, "target")?,
            },
            _ => return Err(syn::Error::new(kind.span(), "不支持的任务注册类型")),
        };

        if !content.is_empty() {
            return Err(content.error("任务注册项包含未识别的额外字段"));
        }

        Ok(entry)
    }
}

/// 解析命名表达式字段
fn parse_named_expr(input: ParseStream, expected_name: &str) -> syn::Result<Expr> {
    let field_name = input.parse::<Ident>()?;
    if field_name != expected_name {
        return Err(syn::Error::new(
            field_name.span(),
            format!("期望字段 {expected_name}"),
        ));
    }
    input.parse::<Token![:]>()?;
    let expr = input.parse::<Expr>()?;

    if input.peek(Token![,]) {
        input.parse::<Token![,]>()?;
    }

    Ok(expr)
}

/// 展开服务工厂生成配置
fn expand_service_factory(input: ServiceFactoryInput) -> syn::Result<TokenStream> {
    let factory_type = input.factory_type;
    let state_type = input.state_type;
    let impl_generics = input
        .impl_generics
        .unwrap_or_else(|| syn::parse_quote!(<'a, #state_type>));

    let impl_blocks = input.groups.into_iter().map(|group| {
        let provider_trait = group.provider_trait;
        let context_expr = group.context_expr;
        let mut group_generics = impl_generics.clone();
        group_generics
            .make_where_clause()
            .predicates
            .push(syn::parse_quote!(#state_type: #provider_trait));
        let (impl_generics, _, where_clause) = group_generics.split_for_impl();
        let registrations = group.registrations.into_iter().map(|registration| {
            let method_name = registration.method_name;
            let service_type = registration.service_type;
            let context_expr = context_expr.clone();
            let doc_text = format!("创建 {} 应用服务", method_name);

            quote! {
                #[doc = #doc_text]
                pub fn #method_name(&self) -> #service_type {
                    <#service_type>::new(#context_expr)
                }
            }
        });

        quote! {
            impl #impl_generics #factory_type #where_clause {
                #(#registrations)*
            }
        }
    });

    Ok(quote! {
        #(#impl_blocks)*
    }
    .into())
}

/// 展开直接服务工厂生成配置
fn expand_direct_service_factory(input: DirectServiceFactoryInput) -> syn::Result<TokenStream> {
    let impl_generics = input.impl_generics;
    let (impl_generics, _, where_clause) = impl_generics.split_for_impl();
    let factory_type = input.factory_type;

    let methods = input.groups.into_iter().flat_map(|group| {
        let context_expr = group.context_expr;
        group.registrations.into_iter().map(move |registration| {
            let method_name = registration.method_name;
            let service_type = registration.service_type;
            let context_expr = context_expr.clone();
            let doc_text = format!("创建 {} 应用服务", method_name);

            quote! {
                #[doc = #doc_text]
                pub fn #method_name(&self) -> #service_type {
                    <#service_type>::new(#context_expr)
                }
            }
        })
    });

    Ok(quote! {
        impl #impl_generics #factory_type #where_clause {
            #(#methods)*
        }
    }
    .into())
}

/// 展开任务注册表生成配置
fn expand_task_registry_registration(
    input: TaskRegistryRegistrationInput,
) -> syn::Result<TokenStream> {
    let registry_expr = input.registry_expr;
    let is_direct_path = matches!(strip_transparent_expr(&registry_expr), Expr::Path(_));
    let registry_target = if is_direct_path {
        quote! { (#registry_expr) }
    } else {
        quote! { __task_registry }
    };
    let mut task_statements = Vec::new();
    let mut alias_statements = Vec::new();
    for entry in input.entries {
        match entry {
            TaskRegistryRegistrationEntry::Queue {
                definition,
                handler,
            } => {
                task_statements.push(quote! {
                    #registry_target.register_queue_with_definition(#definition, #handler)?;
                });
            }
            TaskRegistryRegistrationEntry::Schedule {
                definition,
                handler,
            } => {
                task_statements.push(quote! {
                    #registry_target.register_schedule_with_definition(#definition, #handler)?;
                });
            }
            TaskRegistryRegistrationEntry::QueueAlias { alias, target } => {
                alias_statements.push(quote! {
                    #registry_target.register_queue_alias(#alias, #target)?;
                });
            }
            TaskRegistryRegistrationEntry::ScheduleAlias { alias, target } => {
                alias_statements.push(quote! {
                    #registry_target.register_schedule_alias(#alias, #target)?;
                });
            }
        }
    }

    let registry_binding = if is_direct_path {
        quote! {}
    } else {
        quote! {
            let __task_registry = &mut (#registry_expr);
        }
    };

    Ok(quote! {
        {
            (|| -> ::core::result::Result<(), ::std::string::String> {
                #registry_binding
                #(#task_statements)*
                #(#alias_statements)*
                ::core::result::Result::Ok(())
            })()
        }
    }
    .into())
}

/// 剥离不改变表达式语义的括号与 Group 包装
fn strip_transparent_expr(mut expression: &Expr) -> &Expr {
    loop {
        expression = match expression {
            Expr::Paren(paren) => &paren.expr,
            Expr::Group(group) => &group.expr,
            _ => return expression,
        };
    }
}

/// #[retry] 的属性参数
struct RetryArgs {
    /// 最大重试次数，默认 3
    max_retries: Option<Expr>,

    /// 每次重试前的等待配置
    delay: RetryDelay,

    /// 显式稳定方法作用域
    key_scope: Option<syn::LitStr>,

    /// 宏调用处显式声明的额外类型维度
    key_types: Vec<Type>,

    /// 宏调用处显式声明的额外常量维度
    key_consts: Vec<Expr>,

    /// 显式作用域是否包含 receiver 的 Self 类型
    key_include_self: Option<bool>,
}

/// 重试等待配置
enum RetryDelay {
    /// 使用运行时默认秒数
    Default,

    /// 使用毫秒表达式
    Milliseconds(Expr),

    /// 使用秒表达式
    Seconds(Expr),
}

impl Default for RetryArgs {
    /// 返回默认重试配置：最多重试 3 次，不等待
    fn default() -> Self {
        Self {
            max_retries: None,
            delay: RetryDelay::Default,
            key_scope: None,
            key_types: Vec::new(),
            key_consts: Vec::new(),
            key_include_self: None,
        }
    }
}

impl Parse for RetryArgs {
    /// 从属性参数列表解析 max_retries 与 delay_seconds
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut args = RetryArgs::default();
        let mut key_scope = None;
        let mut key_types = None;
        let mut key_consts = None;
        let mut key_include_self = None;
        let mut has_max_retries = false;
        let mut has_delay_ms = false;
        let mut has_delay_seconds = false;

        // 无参数时直接使用默认值
        if input.is_empty() {
            return Ok(args);
        }

        // 解析形如 `key = value, key = value` 的参数列表
        let metas = Punctuated::<syn::Meta, Token![,]>::parse_terminated(input)?;

        for meta in &metas {
            let name = meta
                .path()
                .get_ident()
                .ok_or_else(|| syn::Error::new_spanned(meta.path(), "期望标识符"))?
                .to_string();

            match name.as_str() {
                "max_retries" => {
                    if has_max_retries {
                        return Err(syn::Error::new_spanned(
                            meta.path(),
                            "参数 max_retries 不能重复配置",
                        ));
                    }
                    has_max_retries = true;
                    args.max_retries = Some(require_name_value(meta, "max_retries")?.value.clone());
                }
                "delay_ms" => {
                    if has_delay_ms || has_delay_seconds {
                        return Err(syn::Error::new_spanned(
                            meta.path(),
                            "delay_ms 与 delay_seconds 只能配置一个且不能重复",
                        ));
                    }
                    has_delay_ms = true;
                    args.delay = RetryDelay::Milliseconds(
                        require_name_value(meta, "delay_ms")?.value.clone(),
                    );
                }
                "delay_seconds" => {
                    if has_delay_ms || has_delay_seconds {
                        return Err(syn::Error::new_spanned(
                            meta.path(),
                            "delay_ms 与 delay_seconds 只能配置一个且不能重复",
                        ));
                    }
                    has_delay_seconds = true;
                    args.delay = RetryDelay::Seconds(
                        require_name_value(meta, "delay_seconds")?.value.clone(),
                    );
                }
                other => {
                    if !parse_scoped_key_meta(
                        meta,
                        other,
                        &mut key_scope,
                        &mut key_types,
                        &mut key_consts,
                        &mut key_include_self,
                    )? {
                        return Err(syn::Error::new_spanned(
                            meta.path(),
                            format!(
                                "未知参数 `{other}`，支持 max_retries / delay_seconds / delay_ms / key_scope / key_types(...) / key_consts(...) / key_include_self"
                            ),
                        ));
                    }
                }
            }
        }

        validate_scoped_key_options(
            input,
            key_scope.as_ref(),
            key_types.as_deref(),
            key_consts.as_deref(),
            key_include_self,
        )?;
        args.key_scope = key_scope;
        args.key_types = key_types.unwrap_or_default();
        args.key_consts = key_consts.unwrap_or_default();
        args.key_include_self = key_include_self;

        Ok(args)
    }
}

/// 展开 #[retry] 宏，生成包含重试循环的完整方法体
fn expand_retry(args: RetryArgs, mut input: ItemFn) -> syn::Result<TokenStream> {
    bind_wildcard_impl_trait_parameters(&mut input);
    let runtime = runtime_crate_path()?;

    // 仅支持 async fn，同步方法不适用此宏
    if input.sig.asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            &input.sig,
            "#[retry] 只能用于 async fn",
        ));
    }

    validate_receiver_key_strategy(&input.sig, args.key_include_self, "retry")?;

    validate_retry_parameters(&input.sig)?;
    let sig = &input.sig;
    let retry_copy_assertions = build_retry_copy_assertions(sig, &runtime)?;

    let attrs = &input.attrs;
    let vis = &input.vis;
    let body = &input.block;
    let retry_body = format_ident!(
        "__source_generator_retry_body",
        span = proc_macro2::Span::mixed_site()
    );
    let key_scope = args
        .key_scope
        .as_ref()
        .ok_or_else(|| syn::Error::new_spanned(sig, "缺少必填参数 key_scope"))?;
    let method_key = build_scoped_method_key(
        sig,
        key_scope,
        &args.key_types,
        &args.key_consts,
        args.key_include_self,
        &runtime,
    )?;

    // 将宏参数转为编译期字面量，嵌入生成代码
    let max_retries = args.max_retries.map_or_else(
        || quote! { #runtime::RetryOptions::default().max_retries },
        |value| quote! { #value },
    );
    let delay_ms = match args.delay {
        RetryDelay::Default => quote! {
            #runtime::retry_delay_ms_from_seconds(
                #runtime::RetryOptions::default().delay_seconds,
            )
        },
        RetryDelay::Milliseconds(value) => quote! { #value },
        RetryDelay::Seconds(value) => {
            quote! { #runtime::retry_delay_ms_from_seconds(#value) }
        }
    };
    let expanded: TokenStream2 = quote! {

        #(#attrs)*
        #vis #sig {

            let __retry_max: u32 = #max_retries;
            let __retry_delay_ms: u64 = #delay_ms;
            #(#retry_copy_assertions)*
            let #retry_body = async || #body;

            if __retry_max == 0 {
                return #retry_body().await;
            }

            // 记录已经执行的重试次数，首次执行前为 0
            let mut __retry_count: u64 = 0;
            let __retry_max = u64::from(__retry_max);
            let mut __retry_context = ::core::option::Option::None;

            loop {

                // 每次迭代重新创建 async 块，使方法体可多次执行
                // 参数签名保持不可变，需要修改时由方法体为当前尝试创建局部副本
                let __retry_result = #retry_body().await;

                match __retry_result {
                    ::core::result::Result::Ok(__val) => {
                        // 执行成功，立即返回结果
                        return ::core::result::Result::Ok(__val);
                    }
                    ::core::result::Result::Err(__err) => {
                        let __retry_context = __retry_context.get_or_insert_with(|| {
                            (#method_key, #runtime::new_trace_id())
                        });
                        if __retry_count >= __retry_max {
                            #runtime::log_retry_exhausted_debug(
                                &__retry_context.0,
                                &__retry_context.1,
                                __retry_count,
                                &__err,
                            );
                            return ::core::result::Result::Err(__err);
                        }
                        __retry_count += 1;
                        #runtime::log_retry_scheduled_debug(
                            &__retry_context.0,
                            &__retry_context.1,
                            __retry_count,
                            __retry_delay_ms,
                            &__err,
                        );
                        // 等待指定间隔后进行下一次重试，delay_ms 为 0 时直接跳过
                        #runtime::retry_sleep(__retry_delay_ms).await;
                    }
                }

            }

        }

    };

    Ok(expanded.into())
}

/// 校验重试方法参数不会在多次尝试之间保留签名级可变状态
fn validate_retry_parameters(sig: &syn::Signature) -> syn::Result<()> {
    for argument in &sig.inputs {
        match argument {
            syn::FnArg::Receiver(receiver) => {
                if !receiver_is_immutable_reference(receiver) {
                    return Err(syn::Error::new_spanned(
                        receiver,
                        "#[retry] 的 receiver 必须是不可变引用，避免重试之间共享可变 receiver 状态",
                    ));
                }
            }
            syn::FnArg::Typed(argument) => {
                if pattern_contains_mutable_binding(argument.pat.as_ref()) {
                    return Err(syn::Error::new_spanned(
                        &argument.pat,
                        "#[retry] 不支持可变参数绑定，请使用不可变参数并在方法体内为单次尝试创建局部副本",
                    ));
                }

                if contains_mutable_reference(&argument.ty) {
                    return Err(syn::Error::new_spanned(
                        &argument.ty,
                        "#[retry] 不支持可变引用或可变裸指针参数，避免重试之间共享可变参数状态",
                    ));
                }
            }
        }
    }

    Ok(())
}

/// 生成非 receiver 参数必须实现 Copy 的编译期约束
fn build_retry_copy_assertions(
    sig: &syn::Signature,
    runtime: &TokenStream2,
) -> syn::Result<Vec<TokenStream2>> {
    sig.inputs
        .iter()
        .filter_map(|argument| {
            let syn::FnArg::Typed(argument) = argument else {
                return None;
            };

            if contains_impl_trait(&argument.ty) {
                let Pat::Ident(pattern) = argument.pat.as_ref() else {
                    return Some(Err(syn::Error::new_spanned(
                        &argument.pat,
                        "#[retry] 的 impl Trait 参数必须使用标识符或通配符模式",
                    )));
                };
                let ident = &pattern.ident;
                let value = if pattern.by_ref.is_some() {
                    quote! { &*#ident }
                } else {
                    quote! { &#ident }
                };

                return Some(Ok(quote! {
                    #runtime::assert_retry_parameter_copy(#value);
                }));
            }

            let ty = &argument.ty;
            Some(Ok(quote! {
                #runtime::assert_retry_parameter_type_copy::<#ty>();
            }))
        })
        .collect()
}

/// 判断参数模式是否直接或嵌套包含可变绑定
fn pattern_contains_mutable_binding(pattern: &Pat) -> bool {
    match pattern {
        Pat::Ident(pattern) => {
            pattern.mutability.is_some()
                || pattern
                    .subpat
                    .as_ref()
                    .is_some_and(|(_, pattern)| pattern_contains_mutable_binding(pattern))
        }
        Pat::Or(pattern) => pattern.cases.iter().any(pattern_contains_mutable_binding),
        Pat::Paren(pattern) => pattern_contains_mutable_binding(&pattern.pat),
        Pat::Reference(pattern) => {
            pattern.mutability.is_some() || pattern_contains_mutable_binding(&pattern.pat)
        }
        Pat::Slice(pattern) => pattern.elems.iter().any(pattern_contains_mutable_binding),
        Pat::Struct(pattern) => pattern
            .fields
            .iter()
            .any(|field| pattern_contains_mutable_binding(&field.pat)),
        Pat::Tuple(pattern) => pattern.elems.iter().any(pattern_contains_mutable_binding),
        Pat::TupleStruct(pattern) => pattern.elems.iter().any(pattern_contains_mutable_binding),
        Pat::Type(pattern) => pattern_contains_mutable_binding(&pattern.pat),
        Pat::Macro(_) | Pat::Verbatim(_) => true,
        _ => false,
    }
}

/// #[cacheable] 的属性参数
struct CacheableArgs {
    /// 缓存实例表达式
    cache: Expr,

    /// L1 与 L2 共同遵守的缓存有效期秒数
    ttl_seconds: Option<Expr>,

    /// 可选的分布式锁实例表达式
    lock: Option<Expr>,

    /// 防击穿锁有效期秒数
    lock_expiry_seconds: Option<Expr>,

    /// 无接收者函数的显式键作用域
    key_scope: Option<syn::LitStr>,

    /// 宏调用处显式声明的额外类型键维度
    key_types: Vec<Type>,

    /// 宏调用处显式声明的额外常量键维度
    key_consts: Vec<Expr>,

    /// 显式作用域是否包含 receiver 的 Self 类型
    key_include_self: Option<bool>,

    /// 显式参与缓存键生成的参数表达式
    key: Option<Expr>,

    /// 无法静态分析返回类型时显式声明是否缓存成功值
    cache_value: Option<bool>,
}

impl Parse for CacheableArgs {
    /// 解析 cache 与 ttl_seconds 参数
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let metas = Punctuated::<syn::Meta, Token![,]>::parse_terminated(input)?;

        let mut cache: Option<Expr> = None;
        let mut ttl_seconds = None;
        let mut lock: Option<Expr> = None;
        let mut lock_expiry_seconds = None;
        let mut key_scope: Option<syn::LitStr> = None;
        let mut key_types: Option<Vec<Type>> = None;
        let mut key_consts: Option<Vec<Expr>> = None;
        let mut key_include_self = None;
        let mut key = None;
        let mut cache_value = None;
        let mut has_cache = false;
        let mut has_ttl_seconds = false;
        let mut has_lock = false;
        let mut has_lock_expiry_seconds = false;
        let mut has_key = false;
        let mut has_cache_value = false;

        for meta in &metas {
            let name = meta
                .path()
                .get_ident()
                .ok_or_else(|| syn::Error::new_spanned(meta.path(), "期望标识符"))?
                .to_string();

            match name.as_str() {
                "cache" => {
                    if has_cache {
                        return Err(syn::Error::new_spanned(
                            meta.path(),
                            "参数 cache 不能重复配置",
                        ));
                    }
                    has_cache = true;
                    cache = Some(require_name_value(meta, "cache")?.value.clone());
                }
                "ttl_seconds" => {
                    if has_ttl_seconds {
                        return Err(syn::Error::new_spanned(
                            meta.path(),
                            "参数 ttl_seconds 不能重复配置",
                        ));
                    }
                    has_ttl_seconds = true;
                    ttl_seconds = Some(require_name_value(meta, "ttl_seconds")?.value.clone());
                }
                "lock" => {
                    if has_lock {
                        return Err(syn::Error::new_spanned(
                            meta.path(),
                            "参数 lock 不能重复配置",
                        ));
                    }
                    has_lock = true;
                    lock = Some(require_name_value(meta, "lock")?.value.clone());
                }
                "lock_expiry_seconds" => {
                    if has_lock_expiry_seconds {
                        return Err(syn::Error::new_spanned(
                            meta.path(),
                            "参数 lock_expiry_seconds 不能重复配置",
                        ));
                    }
                    has_lock_expiry_seconds = true;
                    lock_expiry_seconds = Some(
                        require_name_value(meta, "lock_expiry_seconds")?
                            .value
                            .clone(),
                    );
                }
                "key" => {
                    if has_key {
                        return Err(syn::Error::new_spanned(
                            meta.path(),
                            "参数 key 不能重复配置",
                        ));
                    }
                    has_key = true;
                    key = Some(require_name_value(meta, "key")?.value.clone());
                }
                "cache_value" => {
                    if has_cache_value {
                        return Err(syn::Error::new_spanned(
                            meta.path(),
                            "参数 cache_value 不能重复配置",
                        ));
                    }
                    has_cache_value = true;
                    cache_value = Some(parse_bool_literal(
                        &require_name_value(meta, "cache_value")?.value,
                        "cache_value",
                    )?);
                }
                other => {
                    if !parse_scoped_key_meta(
                        meta,
                        other,
                        &mut key_scope,
                        &mut key_types,
                        &mut key_consts,
                        &mut key_include_self,
                    )? {
                        return Err(syn::Error::new_spanned(
                            meta.path(),
                            format!(
                                "未知参数 `{other}`，支持 cache / ttl_seconds / lock / lock_expiry_seconds / key_scope / key_types(...) / key_consts(...) / key_include_self / key / cache_value"
                            ),
                        ));
                    }
                }
            }
        }

        let cache = cache.ok_or_else(|| syn::Error::new(input.span(), "缺少必填参数 cache"))?;

        if has_lock_expiry_seconds && !has_lock {
            return Err(syn::Error::new(
                input.span(),
                "配置 lock_expiry_seconds 时必须同时配置 lock",
            ));
        }

        validate_scoped_key_options(
            input,
            key_scope.as_ref(),
            key_types.as_deref(),
            key_consts.as_deref(),
            key_include_self,
        )?;

        Ok(Self {
            cache,
            ttl_seconds,
            lock,
            lock_expiry_seconds,
            key_scope,
            key_types: key_types.unwrap_or_default(),
            key_consts: key_consts.unwrap_or_default(),
            key_include_self,
            key,
            cache_value,
        })
    }
}

/// #[concurrency_limit] 的属性参数
struct ConcurrencyLimitArgs {
    /// 分布式锁实例表达式
    lock: Expr,

    /// 是否使用参数参与 key 计算
    is_use_parameter: Option<bool>,

    /// 获取不到锁时是否立即失败
    is_block: Option<bool>,

    /// 允许的最大并发数
    semaphore: Option<u32>,

    /// 锁过期秒数
    expiry_seconds: Option<u64>,

    /// 无接收者函数的显式键作用域
    key_scope: Option<syn::LitStr>,

    /// 宏调用处显式声明的额外类型键维度
    key_types: Vec<Type>,

    /// 宏调用处显式声明的额外常量键维度
    key_consts: Vec<Expr>,

    /// 显式作用域是否包含 receiver 的 Self 类型
    key_include_self: Option<bool>,

    /// 显式参与锁键生成的参数表达式
    key: Option<Expr>,
}

impl Parse for ConcurrencyLimitArgs {
    /// 解析并发限制宏参数
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let metas = Punctuated::<syn::Meta, Token![,]>::parse_terminated(input)?;
        let mut lock: Option<Expr> = None;
        let mut is_use_parameter = None;
        let mut is_block = None;
        let mut semaphore = None;
        let mut expiry_seconds = None;
        let mut key_scope: Option<syn::LitStr> = None;
        let mut key_types: Option<Vec<Type>> = None;
        let mut key_consts: Option<Vec<Expr>> = None;
        let mut key_include_self = None;
        let mut key = None;
        let mut has_lock = false;
        let mut has_is_use_parameter = false;
        let mut has_is_block = false;
        let mut has_semaphore = false;
        let mut has_expiry_seconds = false;
        let mut has_key = false;

        for meta in &metas {
            let name = meta
                .path()
                .get_ident()
                .ok_or_else(|| syn::Error::new_spanned(meta.path(), "期望标识符"))?
                .to_string();

            match name.as_str() {
                "lock" => {
                    if has_lock {
                        return Err(syn::Error::new_spanned(
                            meta.path(),
                            "参数 lock 不能重复配置",
                        ));
                    }
                    lock = Some(require_name_value(meta, "lock")?.value.clone());
                    has_lock = true;
                }
                "is_use_parameter" => {
                    if has_is_use_parameter {
                        return Err(syn::Error::new_spanned(
                            meta.path(),
                            "参数 is_use_parameter 不能重复配置",
                        ));
                    }
                    has_is_use_parameter = true;
                    is_use_parameter = Some(parse_bool_literal(
                        &require_name_value(meta, "is_use_parameter")?.value,
                        "is_use_parameter",
                    )?);
                }
                "is_block" => {
                    if has_is_block {
                        return Err(syn::Error::new_spanned(
                            meta.path(),
                            "参数 is_block 不能重复配置",
                        ));
                    }
                    has_is_block = true;
                    is_block = Some(parse_bool_literal(
                        &require_name_value(meta, "is_block")?.value,
                        "is_block",
                    )?);
                }
                "semaphore" => {
                    if has_semaphore {
                        return Err(syn::Error::new_spanned(
                            meta.path(),
                            "参数 semaphore 不能重复配置",
                        ));
                    }
                    has_semaphore = true;
                    semaphore = Some(parse_u32_literal(
                        &require_name_value(meta, "semaphore")?.value,
                        "semaphore",
                    )?);
                }
                "expiry_seconds" => {
                    if has_expiry_seconds {
                        return Err(syn::Error::new_spanned(
                            meta.path(),
                            "参数 expiry_seconds 不能重复配置",
                        ));
                    }
                    has_expiry_seconds = true;
                    expiry_seconds = Some(parse_u64_literal(
                        &require_name_value(meta, "expiry_seconds")?.value,
                        "expiry_seconds",
                    )?);
                }
                "key" => {
                    if has_key {
                        return Err(syn::Error::new_spanned(
                            meta.path(),
                            "参数 key 不能重复配置",
                        ));
                    }
                    has_key = true;
                    key = Some(require_name_value(meta, "key")?.value.clone());
                }
                other => {
                    if !parse_scoped_key_meta(
                        meta,
                        other,
                        &mut key_scope,
                        &mut key_types,
                        &mut key_consts,
                        &mut key_include_self,
                    )? {
                        return Err(syn::Error::new_spanned(
                            meta.path(),
                            format!(
                                "未知参数 `{other}`，支持 lock / is_use_parameter / is_block / semaphore / expiry_seconds / key_scope / key_types(...) / key_consts(...) / key_include_self / key"
                            ),
                        ));
                    }
                }
            }
        }

        let lock = lock.ok_or_else(|| syn::Error::new(input.span(), "缺少必填参数 lock"))?;

        validate_scoped_key_options(
            input,
            key_scope.as_ref(),
            key_types.as_deref(),
            key_consts.as_deref(),
            key_include_self,
        )?;

        if key.is_some() && !is_use_parameter.unwrap_or(false) {
            return Err(syn::Error::new(
                input.span(),
                "配置 key 时必须同时设置 is_use_parameter = true",
            ));
        }

        Ok(Self {
            lock,
            is_use_parameter,
            is_block,
            semaphore,
            expiry_seconds,
            key_scope,
            key_types: key_types.unwrap_or_default(),
            key_consts: key_consts.unwrap_or_default(),
            key_include_self,
            key,
        })
    }
}

/// 取得必须使用 name = value 形式声明的属性参数
fn require_name_value<'a>(meta: &'a syn::Meta, name: &str) -> syn::Result<&'a MetaNameValue> {
    let syn::Meta::NameValue(value) = meta else {
        return Err(syn::Error::new_spanned(
            meta,
            format!("参数 {name} 必须使用 `{name} = ...` 形式"),
        ));
    };

    Ok(value)
}

/// 解析显式作用域共用的类型、常量和 Self 键维度
fn parse_scoped_key_meta(
    meta: &syn::Meta,
    name: &str,
    key_scope: &mut Option<syn::LitStr>,
    key_types: &mut Option<Vec<Type>>,
    key_consts: &mut Option<Vec<Expr>>,
    key_include_self: &mut Option<bool>,
) -> syn::Result<bool> {
    match name {
        "key_scope" => {
            if key_scope.is_some() {
                return Err(syn::Error::new_spanned(
                    meta.path(),
                    "参数 key_scope 不能重复配置",
                ));
            }
            let value = require_name_value(meta, "key_scope")?;
            *key_scope = Some(parse_key_scope_literal(&value.value)?);
        }
        "key_types" => {
            if key_types.is_some() {
                return Err(syn::Error::new_spanned(
                    meta.path(),
                    "参数 key_types 不能重复配置",
                ));
            }
            let syn::Meta::List(list) = meta else {
                return Err(syn::Error::new_spanned(
                    meta,
                    "参数 key_types 必须使用 `key_types(Type, ...)` 形式",
                ));
            };
            let values = list
                .parse_args_with(Punctuated::<Type, Token![,]>::parse_terminated)?
                .into_iter()
                .collect::<Vec<_>>();
            if values.is_empty() {
                return Err(syn::Error::new_spanned(
                    list,
                    "参数 key_types 至少需要一个类型",
                ));
            }
            *key_types = Some(values);
        }
        "key_consts" => {
            if key_consts.is_some() {
                return Err(syn::Error::new_spanned(
                    meta.path(),
                    "参数 key_consts 不能重复配置",
                ));
            }
            let syn::Meta::List(list) = meta else {
                return Err(syn::Error::new_spanned(
                    meta,
                    "参数 key_consts 必须使用 `key_consts(CONST, ...)` 形式",
                ));
            };
            let values = list
                .parse_args_with(Punctuated::<Expr, Token![,]>::parse_terminated)?
                .into_iter()
                .collect::<Vec<_>>();
            if values.is_empty() {
                return Err(syn::Error::new_spanned(
                    list,
                    "参数 key_consts 至少需要一个常量表达式",
                ));
            }
            if let Some(value) = values.iter().find(|value| !matches!(value, Expr::Path(_))) {
                return Err(syn::Error::new_spanned(
                    value,
                    "参数 key_consts 只接受 const 泛型或关联常量路径",
                ));
            }
            *key_consts = Some(values);
        }
        "key_include_self" => {
            if key_include_self.is_some() {
                return Err(syn::Error::new_spanned(
                    meta.path(),
                    "参数 key_include_self 不能重复配置",
                ));
            }
            let value = require_name_value(meta, "key_include_self")?;
            *key_include_self = Some(parse_bool_literal(&value.value, "key_include_self")?);
        }
        _ => return Ok(false),
    }

    Ok(true)
}

/// 校验稳定键必须提供显式作用域
fn validate_scoped_key_options(
    input: ParseStream,
    key_scope: Option<&syn::LitStr>,
    _key_types: Option<&[Type]>,
    _key_consts: Option<&[Expr]>,
    _key_include_self: Option<bool>,
) -> syn::Result<()> {
    if key_scope.is_none() {
        return Err(syn::Error::new(
            input.span(),
            "必须配置跨构建稳定且全局唯一的 key_scope",
        ));
    }

    Ok(())
}

/// 解析布尔字面量配置
fn parse_bool_literal(value: &Expr, name: &str) -> syn::Result<bool> {
    let Expr::Lit(expr_lit) = value else {
        return Err(syn::Error::new_spanned(
            value,
            format!("参数 {name} 必须是布尔字面量"),
        ));
    };
    let syn::Lit::Bool(value) = &expr_lit.lit else {
        return Err(syn::Error::new_spanned(
            value,
            format!("参数 {name} 必须是布尔字面量"),
        ));
    };

    Ok(value.value)
}

/// 解析 u32 整数字面量配置
fn parse_u32_literal(value: &Expr, name: &str) -> syn::Result<u32> {
    parse_integer_literal(value, name)
}

/// 解析 u64 整数字面量配置
fn parse_u64_literal(value: &Expr, name: &str) -> syn::Result<u64> {
    parse_integer_literal(value, name)
}

/// 解析指定范围内的无符号整数字面量配置
fn parse_integer_literal<T>(value: &Expr, name: &str) -> syn::Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    let Expr::Lit(expr_lit) = value else {
        return Err(syn::Error::new_spanned(
            value,
            format!("参数 {name} 必须是无符号整数字面量"),
        ));
    };
    let syn::Lit::Int(value) = &expr_lit.lit else {
        return Err(syn::Error::new_spanned(
            value,
            format!("参数 {name} 必须是无符号整数字面量"),
        ));
    };

    value.base10_parse::<T>().map_err(|error| {
        syn::Error::new_spanned(value, format!("参数 {name} 超出允许范围: {error}"))
    })
}

/// 展开 #[cacheable] 宏，生成缓存命中包装逻辑
fn expand_cacheable(args: CacheableArgs, mut input: ItemFn) -> syn::Result<TokenStream> {
    if input.sig.asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            &input.sig,
            "#[cacheable] 只能用于 async fn",
        ));
    }

    let return_type = match &input.sig.output {
        syn::ReturnType::Type(_, return_type) => return_type,
        syn::ReturnType::Default => {
            return Err(syn::Error::new_spanned(
                &input.sig,
                "#[cacheable] 只能用于返回 Result<T, E> 的方法",
            ));
        }
    };
    let cache_value = resolve_cache_value(return_type, args.cache_value)?;
    let key_scope = args
        .key_scope
        .as_ref()
        .ok_or_else(|| syn::Error::new_spanned(&input.sig, "缺少必填参数 key_scope"))?;

    if !cache_value {
        return Ok(quote! { #input }.into());
    }

    bind_wildcard_impl_trait_parameters(&mut input);
    let sig = &input.sig;
    let runtime = runtime_crate_path()?;

    if let Some(receiver) = sig.inputs.iter().find_map(|input| match input {
        syn::FnArg::Receiver(receiver) => Some(receiver),
        syn::FnArg::Typed(_) => None,
    }) && !receiver_is_immutable_reference(receiver)
    {
        return Err(syn::Error::new_spanned(
            receiver,
            "#[cacheable] 的 receiver 必须是不可变引用，避免方法体闭包提前移动或独占 receiver",
        ));
    }

    if args.key.is_none()
        && let Some(input) = sig.inputs.iter().find(|input| match input {
            syn::FnArg::Receiver(_) => false,
            syn::FnArg::Typed(typed) => contains_trait_object(&typed.ty),
        })
    {
        return Err(syn::Error::new_spanned(
            input,
            "#[cacheable] 的默认参数键不支持包含 dyn Trait 的参数，请配置 key = (...) 只选择实现 StableInvocationKey 的稳定业务参数",
        ));
    }

    if let Some(input) = sig.inputs.iter().find(|input| match input {
        syn::FnArg::Receiver(receiver) => receiver_contains_mutable_reference(receiver),
        syn::FnArg::Typed(typed) => contains_mutable_reference(&typed.ty),
    }) {
        return Err(syn::Error::new_spanned(
            input,
            "#[cacheable] 不支持 &mut self 或包含可变引用的参数",
        ));
    }

    validate_receiver_key_strategy(sig, args.key_include_self, "cacheable")?;

    let uses_explicit_key = args.key.is_some();
    let argument_value = build_argument_value(sig, args.key.as_ref())?;
    let argument_value_ident = format_ident!(
        "__source_generator_cache_argument_value",
        span = proc_macro2::Span::mixed_site()
    );
    let argument_key_assertions = build_argument_key_assertions(
        sig,
        uses_explicit_key,
        &runtime,
        "cache",
        &argument_value_ident,
    )?;
    let method_key = build_scoped_method_key(
        sig,
        key_scope,
        &args.key_types,
        &args.key_consts,
        args.key_include_self,
        &runtime,
    )?;

    let attrs = &input.attrs;
    let vis = &input.vis;
    let body = &input.block;
    let cache_body = format_ident!(
        "__source_generator_cache_body",
        span = proc_macro2::Span::mixed_site()
    );
    let (cache_body_declaration, cache_body_arguments) =
        build_cacheable_body_declaration(sig, body, &cache_body)?;
    let parameter_safety_checks = sig
        .inputs
        .iter()
        .filter_map(|input| {
            let syn::FnArg::Typed(typed) = input else {
                return None;
            };
            let Pat::Ident(pattern) = typed.pat.as_ref() else {
                return None;
            };
            let ident = &pattern.ident;
            Some(quote! {
                if let ::core::result::Result::Err(error) =
                    #runtime::validate_cacheable_parameter_type(&#ident)
                {
                    #runtime::log_cache_degraded(
                        &__cache_method_key,
                        "parameter_type",
                        &error,
                    );
                    return #cache_body(#(#cache_body_arguments),*).await;
                }
            })
        })
        .collect::<Vec<_>>();
    let cache_expr = args.cache;
    let ttl_seconds = args.ttl_seconds.map_or_else(
        || quote! { #runtime::CacheableOptions::default().ttl_seconds },
        |value| quote! { #value },
    );
    let lock_expiry_seconds = args.lock_expiry_seconds.map_or_else(
        || quote! { #runtime::DEFAULT_CACHEABLE_LOCK_EXPIRY_SECONDS },
        |value| quote! { #value },
    );
    let lock_behavior = if let Some(lock_expr) = args.lock {
        quote! {
            let __cache_lock_source = &(#lock_expr);
            let __cache_lock_service =
                #runtime::distributed_lock_ref(__cache_lock_source);
            let __cache_lock_expiry_seconds: u64 = #lock_expiry_seconds;
            let __cache_lock_guard = match __cache_lock_service
                .lock(
                    &#runtime::cacheable_lock_key(
                        &__cache_method_key,
                        &__cache_arguments_key,
                    ),
                    ::std::time::Duration::from_secs(__cache_lock_expiry_seconds),
                    1,
                )
                .await
            {
                ::core::result::Result::Ok(guard) => ::core::option::Option::Some(guard),
                ::core::result::Result::Err(error) => {
                    #runtime::log_cache_degraded(
                        &__cache_method_key,
                        "stampede_lock",
                        &error,
                    );
                    ::core::option::Option::None
                }
            };

            if __cache_lock_guard.is_some() {
                match __cache.get_json(&__cache_key).await {
                    ::core::result::Result::Ok(::core::option::Option::Some(value)) => {
                        return ::core::result::Result::Ok(value);
                    }
                    ::core::result::Result::Ok(::core::option::Option::None) => {}
                    ::core::result::Result::Err(error) => {
                        #runtime::log_cache_degraded(
                            &__cache_method_key,
                            "read_after_lock",
                            &error,
                        );
                    }
                }
            }
        }
    } else {
        quote! {}
    };

    let expanded: TokenStream2 = quote! {
        #(#attrs)*
        #vis #sig {

            #cache_body_declaration

            let __cache_method_key = #method_key;
            #(#parameter_safety_checks)*

            let #argument_value_ident = #argument_value;
            #argument_key_assertions

            let __cache_arguments_key = match #runtime::cacheable_invocation_arguments_key(
                &#runtime::InvocationArguments(#argument_value_ident),
            ) {
                ::core::result::Result::Ok(key) => key,
                ::core::result::Result::Err(error) => {
                    #runtime::log_cache_degraded(
                        &__cache_method_key,
                        "arguments_key",
                        &error,
                    );
                    return #cache_body(#(#cache_body_arguments),*).await;
                }
            };
            let __cache_key = #runtime::cacheable_key(
                &__cache_method_key,
                &__cache_arguments_key,
            );
            let __cache_source = &(#cache_expr);
            let __cache = #runtime::hybrid_cache_ref(__cache_source);
            let __cache_ttl_seconds: u64 = #ttl_seconds;

            if __cache_ttl_seconds == 0 {
                #runtime::log_cache_degraded(
                    &__cache_method_key,
                    "configuration",
                    &"ttl_seconds 必须大于 0",
                );
                return #cache_body(#(#cache_body_arguments),*).await;
            }

            match __cache.get_json(&__cache_key).await {
                ::core::result::Result::Ok(::core::option::Option::Some(value)) => {
                    return ::core::result::Result::Ok(value);
                }
                ::core::result::Result::Ok(::core::option::Option::None) => {}
                ::core::result::Result::Err(error) => {
                    #runtime::log_cache_degraded(
                        &__cache_method_key,
                        "read",
                        &error,
                    );
                }
            }

            #lock_behavior

            let __cache_value = match #cache_body(#(#cache_body_arguments),*).await {
                ::core::result::Result::Ok(value) => value,
                ::core::result::Result::Err(error) => {
                    return ::core::result::Result::Err(error);
                }
            };

            if let ::core::result::Result::Err(error) = #runtime::set_cacheable_json(
                __cache,
                &__cache_key,
                &__cache_value,
                __cache_ttl_seconds,
            ).await
            {
                #runtime::log_cache_degraded(
                    &__cache_method_key,
                    "write",
                    &error,
                );
            }

            ::core::result::Result::Ok(__cache_value)

        }
    };

    Ok(expanded.into())
}

/// 生成只包含一份 cacheable 原方法体的异步闭包声明
fn build_cacheable_body_declaration(
    sig: &syn::Signature,
    body: &syn::Block,
    cache_body: &syn::Ident,
) -> syn::Result<(TokenStream2, Vec<TokenStream2>)> {
    let mut parameters = Vec::new();
    let mut arguments = Vec::new();
    let mut outer_parameter_uses = Vec::new();

    for input in &sig.inputs {
        let syn::FnArg::Typed(typed) = input else {
            continue;
        };
        let Pat::Ident(pattern) = typed.pat.as_ref() else {
            return Err(syn::Error::new_spanned(
                &typed.pat,
                "#[cacheable] 只支持标识符参数",
            ));
        };
        if pattern.subpat.is_some() {
            return Err(syn::Error::new_spanned(
                &typed.pat,
                "#[cacheable] 不支持带子模式的参数绑定",
            ));
        }

        let attrs = &pattern.attrs;
        let mutability = if pattern.by_ref.is_none() {
            pattern.mutability.as_ref()
        } else {
            None
        };
        let ident = &pattern.ident;
        parameters.push(quote! { #(#attrs)* #mutability #ident });
        arguments.push(quote! { #ident });
        if pattern.by_ref.is_none() && pattern.mutability.is_some() {
            outer_parameter_uses.push(quote! { let _ = &mut #ident; });
        }
    }

    if arguments.is_empty() {
        return Ok((
            quote! {
                let #cache_body = async || #body;
            },
            arguments,
        ));
    }

    let constrain_body = format_ident!(
        "__source_generator_constrain_cache_body",
        span = proc_macro2::Span::mixed_site()
    );
    let function_type = format_ident!(
        "__SourceGeneratorCacheBody",
        span = proc_macro2::Span::mixed_site()
    );
    let output_type = format_ident!(
        "__SourceGeneratorCacheOutput",
        span = proc_macro2::Span::mixed_site()
    );
    let argument_types = (0..arguments.len())
        .map(|index| {
            format_ident!(
                "__SourceGeneratorCacheArgument{index}",
                span = proc_macro2::Span::mixed_site()
            )
        })
        .collect::<Vec<_>>();

    let declaration = quote! {
        #(#outer_parameter_uses)*
        fn #constrain_body<#(#argument_types,)* #function_type, #output_type>(
            _: (#(&#argument_types,)*),
            function: #function_type,
        ) -> #function_type
        where
            #function_type: ::core::ops::AsyncFn(#(#argument_types),*) -> #output_type,
        {
            function
        }

        let #cache_body = #constrain_body(
            (#(&#arguments,)*),
            async |#(#parameters),*| {
                #(let _ = &#arguments;)*
                #body
            },
        );
    };

    Ok((declaration, arguments))
}

/// 展开 #[concurrency_limit] 宏，生成分布式锁包装逻辑
fn expand_concurrency_limit(
    args: ConcurrencyLimitArgs,
    mut input: ItemFn,
) -> syn::Result<TokenStream> {
    let use_arguments_key = args.is_use_parameter.unwrap_or(false);
    bind_wildcard_impl_trait_parameters(&mut input);
    let sig = &input.sig;
    let runtime = runtime_crate_path()?;

    if sig.asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            sig,
            "#[concurrency_limit] 只能用于 async fn",
        ));
    }

    if use_arguments_key
        && args.key.is_none()
        && let Some(input) = sig.inputs.iter().find(|input| match input {
            syn::FnArg::Receiver(_) => false,
            syn::FnArg::Typed(typed) => contains_trait_object(&typed.ty),
        })
    {
        return Err(syn::Error::new_spanned(
            input,
            "#[concurrency_limit(is_use_parameter = true)] 的默认参数键不支持包含 dyn Trait 的参数，请配置 key = (...) 只选择实现 StableInvocationKey 的稳定业务参数",
        ));
    }

    validate_receiver_key_strategy(sig, args.key_include_self, "concurrency_limit")?;

    let key_scope = args
        .key_scope
        .as_ref()
        .ok_or_else(|| syn::Error::new_spanned(sig, "缺少必填参数 key_scope"))?;
    let method_key = build_scoped_method_key(
        sig,
        key_scope,
        &args.key_types,
        &args.key_consts,
        args.key_include_self,
        &runtime,
    )?;

    let uses_explicit_key = args.key.is_some();
    let argument_value = if use_arguments_key {
        Some(build_argument_value(sig, args.key.as_ref())?)
    } else {
        None
    };
    let attrs = &input.attrs;
    let vis = &input.vis;
    let body = &input.block;
    let lock_expr = args.lock;
    let default_use_parameter_assertion = if args.is_use_parameter.is_none() {
        quote! {
            const _: () = assert!(
                !#runtime::DEFAULT_CONCURRENCY_LIMIT_USE_PARAMETER,
                "默认参数键策略改变后必须同步调整 concurrency_limit 宏展开"
            );
        }
    } else {
        quote! {}
    };
    let is_block = args.is_block.map_or_else(
        || {
            quote! {
                #runtime::ConcurrencyLimitOptions::default().is_block
            }
        },
        |value| quote! { #value },
    );
    let semaphore = args.semaphore.map_or_else(
        || {
            quote! {
                #runtime::ConcurrencyLimitOptions::default().semaphore
            }
        },
        |value| quote! { #value },
    );
    let expiry_seconds = args.expiry_seconds.map_or_else(
        || {
            quote! {
                #runtime::ConcurrencyLimitOptions::default().expiry_seconds
            }
        },
        |value| quote! { #value },
    );
    let arguments_key_behavior = if use_arguments_key {
        let argument_value = argument_value
            .as_ref()
            .ok_or_else(|| syn::Error::new_spanned(sig, "启用参数锁键时必须生成参数表达式"))?;
        let argument_value_ident = format_ident!(
            "__source_generator_lock_argument_value",
            span = proc_macro2::Span::mixed_site()
        );
        let argument_key_assertions = build_argument_key_assertions(
            sig,
            uses_explicit_key,
            &runtime,
            "lock",
            &argument_value_ident,
        )?;
        quote! {

            let #argument_value_ident = #argument_value;
            #argument_key_assertions

            let __lock_arguments_key = match #runtime::invocation_arguments_key(
                &#runtime::InvocationArguments(#argument_value_ident),
            ) {
                ::core::result::Result::Ok(key) => ::core::option::Option::Some(key),
                ::core::result::Result::Err(error) => {
                    return ::core::result::Result::Err(
                        ::std::format!("并发限制参数键生成失败: {error}").into()
                    );
                }
            };
        }
    } else {
        quote! {
            let __lock_arguments_key: ::core::option::Option<::std::string::String> =
                ::core::option::Option::None;
        }
    };

    let acquire_lock = quote! {
        let __lock_guard = if #is_block {
            match __lock_service
                .try_lock(&__lock_key, __lock_expiry, __lock_semaphore)
                .await?
            {
                ::core::option::Option::Some(guard) => guard,
                ::core::option::Option::None => {
                    return ::core::result::Result::Err("请勿频繁操作".to_string().into());
                }
            }
        } else {
            __lock_service
                .lock(&__lock_key, __lock_expiry, __lock_semaphore)
                .await?
        };
    };

    let expanded: TokenStream2 = quote! {
        #(#attrs)*
        #vis #sig {

            #default_use_parameter_assertion
            let __lock_semaphore: u32 = #semaphore;

            if __lock_semaphore == 0 {
                return ::core::result::Result::Err(
                    "并发限制 semaphore 必须大于 0".to_string().into()
                );
            }

            let __lock_expiry_seconds: u64 = #expiry_seconds;
            let __lock_expiry = ::std::time::Duration::from_secs(__lock_expiry_seconds);
            let __lock_method_key = #method_key;
            #arguments_key_behavior
            let __lock_key = #runtime::concurrency_limit_key(
                &__lock_method_key,
                __lock_arguments_key.as_deref(),
            );
            let __lock_source = &(#lock_expr);
            let __lock_service = #runtime::distributed_lock_ref(__lock_source);
            #acquire_lock

            let __result = (async #body).await;
            ::std::mem::drop(__lock_guard);
            __result

        }
    };

    Ok(expanded.into())
}

/// 生成显式参数键表达式或默认参数引用元组
fn build_argument_value(
    sig: &syn::Signature,
    explicit_key: Option<&Expr>,
) -> syn::Result<TokenStream2> {
    if let Some(explicit_key) = explicit_key {
        return Ok(quote! { &(#explicit_key) });
    }

    let mut args = Vec::new();

    for input in &sig.inputs {
        if let syn::FnArg::Typed(typed) = input {
            let Pat::Ident(pat_ident) = typed.pat.as_ref() else {
                return Err(syn::Error::new_spanned(&typed.pat, "仅支持标识符参数"));
            };
            let ident = &pat_ident.ident;
            args.push(quote! { &#ident });
        }
    }

    if args.is_empty() {
        Ok(quote! { &() })
    } else {
        Ok(quote! { &( #(#args,)* ) })
    }
}

/// 为默认参数或显式键生成可定位到调用参数的 StableInvocationKey 约束
fn build_argument_key_assertions(
    sig: &syn::Signature,
    uses_explicit_key: bool,
    runtime: &TokenStream2,
    context: &str,
    argument_value_ident: &Ident,
) -> syn::Result<TokenStream2> {
    if uses_explicit_key {
        let helper = format_ident!(
            "__source_generator_{}_explicit_key_must_implement_stable_invocation_key",
            context,
            span = proc_macro2::Span::mixed_site()
        );
        return Ok(quote! {
            {
                fn #helper<T>(_: &T)
                where
                    T: #runtime::StableInvocationKey + ?Sized,
                {
                }

                #helper(#argument_value_ident);
            }
        });
    }

    let mut assertions = Vec::new();
    for (index, input) in sig.inputs.iter().enumerate() {
        let syn::FnArg::Typed(typed) = input else {
            continue;
        };
        let Pat::Ident(pat_ident) = typed.pat.as_ref() else {
            return Err(syn::Error::new_spanned(&typed.pat, "仅支持标识符参数"));
        };
        let ident = &pat_ident.ident;
        let parameter_name = ident.to_string();
        let parameter_name = parameter_name.trim_start_matches("r#");
        let helper = format_ident!(
            "__source_generator_{}_key_parameter_{}_{}_must_implement_stable_invocation_key_or_configure_key",
            context,
            index,
            parameter_name,
            span = ident.span()
        );
        assertions.push(quote_spanned! {ident.span()=>
            {
                fn #helper<T>(_: &T)
                where
                    T: #runtime::StableInvocationKey + ?Sized,
                {
                }

                #helper(&#ident);
            }
        });
    }

    Ok(quote! { #(#assertions)* })
}

/// 根据返回类型语法与显式配置决定是否缓存成功值
fn resolve_cache_value(return_type: &Type, configured: Option<bool>) -> syn::Result<bool> {
    let Some(success_type) = extract_result_success_type(return_type) else {
        return configured.ok_or_else(|| {
            syn::Error::new_spanned(
                return_type,
                "无法静态识别 Result 成功类型，请通过 cache_value = true 或 false 明确声明",
            )
        });
    };

    let is_unit = type_is_unit(success_type);
    if is_unit && configured == Some(true) {
        return Err(syn::Error::new_spanned(
            success_type,
            "Result<(), E> 命令方法不能配置 cache_value = true",
        ));
    }

    Ok(configured.unwrap_or(!is_unit))
}

/// 提取 Result 返回类型的成功值类型
fn extract_result_success_type(return_type: &Type) -> Option<&Type> {
    match return_type {
        Type::Group(group) => extract_result_success_type(&group.elem),
        Type::Paren(paren) => extract_result_success_type(&paren.elem),
        Type::Path(path) => {
            let segment = path.path.segments.last()?;
            if segment.ident != "Result" {
                return None;
            }

            let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
                return None;
            };
            arguments.args.iter().find_map(|argument| match argument {
                GenericArgument::Type(ty) => Some(ty),
                _ => None,
            })
        }
        _ => None,
    }
}

/// 判断类型语法是否为单元类型
fn type_is_unit(ty: &Type) -> bool {
    match ty {
        Type::Group(group) => type_is_unit(&group.elem),
        Type::Paren(paren) => type_is_unit(&paren.elem),
        Type::Tuple(tuple) => tuple.elems.is_empty(),
        _ => false,
    }
}

/// 判断声明类型是否直接或嵌套包含 trait object
fn contains_trait_object(ty: &Type) -> bool {
    match ty {
        Type::TraitObject(_) => true,
        Type::Array(value) => contains_trait_object(&value.elem),
        Type::FnPtr(value) => {
            value
                .inputs
                .iter()
                .any(|input| contains_trait_object(&input.ty))
                || match &value.output {
                    syn::ReturnType::Default => false,
                    syn::ReturnType::Type(_, ty) => contains_trait_object(ty.as_ref()),
                }
        }
        Type::Group(value) => contains_trait_object(&value.elem),
        Type::Paren(value) => contains_trait_object(&value.elem),
        Type::Path(value) => {
            value
                .qself
                .as_ref()
                .is_some_and(|qself| contains_trait_object(&qself.ty))
                || value
                    .path
                    .segments
                    .iter()
                    .any(|segment| path_arguments_contain_trait_object(&segment.arguments))
        }
        Type::Ptr(value) => contains_trait_object(&value.elem),
        Type::Reference(value) => contains_trait_object(&value.elem),
        Type::Slice(value) => contains_trait_object(&value.elem),
        Type::Tuple(value) => value.elems.iter().any(contains_trait_object),
        _ => false,
    }
}

/// 判断路径泛型参数是否包含 trait object
fn path_arguments_contain_trait_object(arguments: &PathArguments) -> bool {
    match arguments {
        PathArguments::None => false,
        PathArguments::AngleBracketed(arguments) => arguments.args.iter().any(|argument| {
            matches!(argument, GenericArgument::Type(ty) if contains_trait_object(ty))
                || matches!(argument, GenericArgument::AssocType(value) if contains_trait_object(&value.ty))
        }),
        PathArguments::Parenthesized(arguments) => {
            arguments
                .inputs
                .iter()
                .any(|input| contains_trait_object(&input.ty))
                || match &arguments.output {
                    syn::ReturnType::Default => false,
                    syn::ReturnType::Type(_, ty) => contains_trait_object(ty.as_ref()),
                }
        }
    }
}

/// 判断声明类型是否直接或嵌套包含 impl Trait
fn contains_impl_trait(ty: &Type) -> bool {
    match ty {
        Type::ImplTrait(_) => true,
        Type::Array(value) => contains_impl_trait(&value.elem),
        Type::FnPtr(value) => {
            value
                .inputs
                .iter()
                .any(|input| contains_impl_trait(&input.ty))
                || match &value.output {
                    syn::ReturnType::Default => false,
                    syn::ReturnType::Type(_, ty) => contains_impl_trait(ty.as_ref()),
                }
        }
        Type::Group(value) => contains_impl_trait(&value.elem),
        Type::Paren(value) => contains_impl_trait(&value.elem),
        Type::Path(value) => {
            value
                .qself
                .as_ref()
                .is_some_and(|qself| contains_impl_trait(&qself.ty))
                || value
                    .path
                    .segments
                    .iter()
                    .any(|segment| path_arguments_contain_impl_trait(&segment.arguments))
        }
        Type::Ptr(value) => contains_impl_trait(&value.elem),
        Type::Reference(value) => contains_impl_trait(&value.elem),
        Type::Slice(value) => contains_impl_trait(&value.elem),
        Type::Tuple(value) => value.elems.iter().any(contains_impl_trait),
        _ => false,
    }
}

/// 判断路径泛型参数是否包含 impl Trait
fn path_arguments_contain_impl_trait(arguments: &PathArguments) -> bool {
    match arguments {
        PathArguments::None => false,
        PathArguments::AngleBracketed(arguments) => arguments.args.iter().any(|argument| {
            matches!(argument, GenericArgument::Type(ty) if contains_impl_trait(ty))
                || matches!(argument, GenericArgument::AssocType(value) if contains_impl_trait(&value.ty))
        }),
        PathArguments::Parenthesized(arguments) => {
            arguments
                .inputs
                .iter()
                .any(|input| contains_impl_trait(&input.ty))
                || match &arguments.output {
                    syn::ReturnType::Default => false,
                    syn::ReturnType::Type(_, ty) => contains_impl_trait(ty.as_ref()),
                }
        }
    }
}

/// 根据显式作用域、方法泛型和稳定类型标签生成方法标识表达式
fn build_scoped_method_key(
    sig: &syn::Signature,
    key_scope: &syn::LitStr,
    explicit_key_types: &[Type],
    explicit_key_consts: &[Expr],
    key_include_self: Option<bool>,
    runtime: &TokenStream2,
) -> syn::Result<TokenStream2> {
    validate_explicit_method_generic_dimensions(sig, explicit_key_types, explicit_key_consts)?;

    let mut stable_types = sig
        .generics
        .type_params()
        .map(|parameter| {
            let ident = &parameter.ident;
            quote! { #runtime::stable_key_type::<#ident>() }
        })
        .collect::<Vec<_>>();
    stable_types.extend(build_impl_trait_stable_types(sig, runtime)?);
    stable_types.extend(
        explicit_key_types
            .iter()
            .map(|value| quote! { #runtime::stable_key_type::<#value>() }),
    );

    let mut stable_consts = sig
        .generics
        .const_params()
        .map(|parameter| {
            let ident = &parameter.ident;
            quote! {
                ::std::format!("{}", (#ident))
            }
        })
        .collect::<Vec<_>>();
    stable_consts.extend(explicit_key_consts.iter().map(|value| {
        quote! {
            ::std::format!("{}", (#value))
        }
    }));
    let self_type = if key_include_self.unwrap_or(false) {
        quote! { ::core::option::Option::Some(#runtime::stable_key_type::<Self>()) }
    } else {
        quote! { ::core::option::Option::<::std::string::String>::None }
    };

    Ok(quote! {
        {
            let __source_generator_stable_types = ::std::vec![#(#stable_types),*];
            let __source_generator_stable_consts = ::std::vec![#(#stable_consts),*];
            let __source_generator_self_type = #self_type;
            #runtime::stable_method_key(
                #key_scope,
                __source_generator_self_type.as_deref(),
                &__source_generator_stable_types,
                &__source_generator_stable_consts,
            )
        }
    })
}

/// 为参数位置直接或嵌套的 impl Trait 生成实际单态化类型标签
fn build_impl_trait_stable_types(
    sig: &syn::Signature,
    runtime: &TokenStream2,
) -> syn::Result<Vec<TokenStream2>> {
    sig.inputs
        .iter()
        .filter_map(|argument| {
            let syn::FnArg::Typed(argument) = argument else {
                return None;
            };
            if !contains_impl_trait(&argument.ty) {
                return None;
            }

            let Pat::Ident(pattern) = argument.pat.as_ref() else {
                return Some(Err(syn::Error::new_spanned(
                    &argument.pat,
                    "impl Trait 参数必须使用标识符或通配符模式才能生成稳定方法键",
                )));
            };
            let ident = &pattern.ident;
            let value = if pattern.by_ref.is_some() {
                quote! { &*#ident }
            } else {
                quote! { &#ident }
            };

            Some(Ok(quote! {
                #runtime::stable_key_type_of(#value)
            }))
        })
        .collect()
}

/// 拒绝重复显式声明已经自动加入方法键的方法泛型维度
fn validate_explicit_method_generic_dimensions(
    sig: &syn::Signature,
    explicit_key_types: &[Type],
    explicit_key_consts: &[Expr],
) -> syn::Result<()> {
    for value in explicit_key_types {
        let Some(ident) = transparent_type_path_ident(value) else {
            continue;
        };

        if sig
            .generics
            .type_params()
            .any(|parameter| parameter.ident == *ident)
        {
            return Err(syn::Error::new_spanned(
                value,
                format!("方法类型泛型 `{ident}` 已自动加入方法键，不能在 key_types 中重复配置"),
            ));
        }
    }

    for value in explicit_key_consts {
        let Some(ident) = transparent_expr_path_ident(value) else {
            continue;
        };

        if sig
            .generics
            .const_params()
            .any(|parameter| parameter.ident == *ident)
        {
            return Err(syn::Error::new_spanned(
                value,
                format!("方法 const 泛型 `{ident}` 已自动加入方法键，不能在 key_consts 中重复配置"),
            ));
        }
    }

    Ok(())
}

/// 取得透明类型包装中的单段路径标识符
fn transparent_type_path_ident(ty: &Type) -> Option<&Ident> {
    match ty {
        Type::Group(group) => transparent_type_path_ident(&group.elem),
        Type::Paren(paren) => transparent_type_path_ident(&paren.elem),
        Type::Path(path)
            if path.qself.is_none()
                && path.path.leading_colon.is_none()
                && path.path.segments.len() == 1 =>
        {
            path.path.segments.first().map(|segment| &segment.ident)
        }
        _ => None,
    }
}

/// 取得透明表达式包装中的单段路径标识符
fn transparent_expr_path_ident(expression: &Expr) -> Option<&Ident> {
    let Expr::Path(path) = strip_transparent_expr(expression) else {
        return None;
    };

    if path.qself.is_some() || path.path.leading_colon.is_some() || path.path.segments.len() != 1 {
        return None;
    }

    path.path.segments.first().map(|segment| &segment.ident)
}

/// 解析并校验稳定键作用域字符串
fn parse_key_scope_literal(value: &Expr) -> syn::Result<syn::LitStr> {
    let Expr::Lit(expr_lit) = value else {
        return Err(syn::Error::new_spanned(
            value,
            "key_scope 必须是非空字符串字面量",
        ));
    };
    let syn::Lit::Str(value) = &expr_lit.lit else {
        return Err(syn::Error::new_spanned(
            value,
            "key_scope 必须是非空字符串字面量",
        ));
    };

    if value.value().trim().is_empty() {
        return Err(syn::Error::new_spanned(
            value,
            "key_scope 不能为空或只包含空白",
        ));
    }

    if value.value().chars().any(char::is_control) {
        return Err(syn::Error::new_spanned(
            value,
            "key_scope 不能包含换行符或其他控制字符",
        ));
    }

    Ok(value.clone())
}

/// 为没有绑定名的 impl Trait 通配符参数生成宏内部绑定
fn bind_wildcard_impl_trait_parameters(input: &mut ItemFn) {
    for (index, argument) in input.sig.inputs.iter_mut().enumerate() {
        let syn::FnArg::Typed(argument) = argument else {
            continue;
        };
        if !contains_impl_trait(&argument.ty) || !is_wildcard_pattern(argument.pat.as_ref()) {
            continue;
        }

        let ident = format_ident!(
            "__source_generator_impl_trait_parameter_{index}",
            span = proc_macro2::Span::mixed_site()
        );
        *argument.pat = Pat::Ident(syn::PatIdent {
            attrs: Vec::new(),
            by_ref: None,
            mutability: None,
            ident,
            subpat: None,
        });
    }
}

/// 判断参数模式是否只是透明括号包裹的通配符
fn is_wildcard_pattern(pattern: &Pat) -> bool {
    match pattern {
        Pat::Wild(_) => true,
        Pat::Paren(pattern) => is_wildcard_pattern(&pattern.pat),
        _ => false,
    }
}

/// 判断方法签名是否包含 self 接收者
fn has_receiver(sig: &syn::Signature) -> bool {
    sig.inputs
        .iter()
        .any(|input| matches!(input, syn::FnArg::Receiver(_)))
}

/// 校验 receiver 方法显式声明是否将 Self 加入稳定方法键
fn validate_receiver_key_strategy(
    sig: &syn::Signature,
    key_include_self: Option<bool>,
    attribute_name: &str,
) -> syn::Result<()> {
    if has_receiver(sig) {
        if key_include_self.is_none() {
            return Err(syn::Error::new_spanned(
                sig,
                format!(
                    "#[{attribute_name}] 的 receiver 方法必须显式配置 key_include_self = true 或 false"
                ),
            ));
        }
    } else if key_include_self.is_some() {
        return Err(syn::Error::new_spanned(
            sig,
            format!("无 self 接收者的 #[{attribute_name}] 函数不能配置 key_include_self"),
        ));
    }

    Ok(())
}

/// 判断类型是否直接或嵌套包含可变引用
fn contains_mutable_reference(ty: &Type) -> bool {
    match ty {
        Type::Reference(reference) => {
            reference.mutability.is_some() || contains_mutable_reference(&reference.elem)
        }
        Type::Array(array) => contains_mutable_reference(&array.elem),
        Type::Group(group) => contains_mutable_reference(&group.elem),
        Type::Paren(paren) => contains_mutable_reference(&paren.elem),
        Type::Ptr(pointer) => {
            matches!(pointer.mutability, syn::PointerMutability::Mut(_))
                || contains_mutable_reference(&pointer.elem)
        }
        Type::Slice(slice) => contains_mutable_reference(&slice.elem),
        Type::Tuple(tuple) => tuple.elems.iter().any(contains_mutable_reference),
        Type::Path(path) => path.path.segments.iter().any(|segment| {
            let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
                return false;
            };

            arguments.args.iter().any(|argument| match argument {
                GenericArgument::Type(ty) => contains_mutable_reference(ty),
                GenericArgument::AssocType(binding) => contains_mutable_reference(&binding.ty),
                _ => false,
            })
        }),
        _ => false,
    }
}

/// 判断 receiver 是否为不可变引用，包括 `&self` 与 `self: &Self`。
fn receiver_is_immutable_reference(receiver: &syn::Receiver) -> bool {
    match &receiver.kind {
        syn::ReceiverKind::Reference(_, _, mutability) => mutability.is_none(),
        syn::ReceiverKind::Typed(_, ty) => {
            matches!(ty.as_ref(), Type::Reference(reference) if reference.mutability.is_none())
        }
        syn::ReceiverKind::Value => false,
        _ => false,
    }
}

/// 判断 receiver 是否直接或通过显式类型包含可变引用。
fn receiver_contains_mutable_reference(receiver: &syn::Receiver) -> bool {
    receiver.mutability.is_some()
        || match &receiver.kind {
            syn::ReceiverKind::Reference(_, _, mutability) => mutability.is_some(),
            syn::ReceiverKind::Typed(_, ty) => contains_mutable_reference(ty),
            syn::ReceiverKind::Value => false,
            _ => false,
        }
}
