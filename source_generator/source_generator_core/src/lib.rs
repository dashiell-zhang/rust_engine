use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
    Expr, GenericArgument, Ident, ItemFn, MetaNameValue, Pat, Path, PathArguments, ReturnType,
    Token, Type, braced, parenthesized,
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
};

// ─── logging ─────────────────────────────────────────────────────────────────

/// 为 async fn 方法自动注入结构化调用日志
///
/// 在方法执行前记录 executing 日志，执行后记录 executed 日志（含耗时），
/// 发生异常时记录 exception 日志（含耗时与错误描述）
///
/// 仅支持返回 Result<T, E> 的 async fn，E 需实现 Debug
///
/// # 示例
/// ```ignore
/// use source_generator_runtime::logging;
///
/// impl MyService {
///     #[logging]
///     pub async fn get_user(&self, id: i64) -> Result<UserDto, AppError> {
///         // 方法体
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn logging(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);

    expand_logging(input).unwrap_or_else(|e| e.to_compile_error().into())
}

/// 展开 #[logging] 宏，生成包含日志注入的完整方法体
fn expand_logging(input: ItemFn) -> syn::Result<TokenStream> {
    let sig = &input.sig;

    // 仅支持 async fn，同步方法不适用此宏
    if sig.asyncness.is_none() {
        return Err(syn::Error::new_spanned(sig, "#[logging] 只能用于 async fn"));
    }

    let attrs = &input.attrs;
    let vis = &input.vis;
    let body = &input.block;

    // 用于在日志 method 字段中标识方法名
    let fn_name_str = sig.ident.to_string();

    let expanded: TokenStream2 = quote! {

        #(#attrs)*
        #vis #sig {

            // 生成本次调用的唯一追踪 ID，用于关联 executing/executed/exception 三条日志
            let __log_trace_id = ::source_generator_runtime::new_trace_id();

            // 拼接模块路径与方法名，形成结构化日志中的 method 字段
            let __log_method = ::std::concat!(::std::module_path!(), "::", #fn_name_str);

            ::source_generator_runtime::log_executing(__log_method, &__log_trace_id);

            // 记录方法开始时间，用于计算 duration_ms
            let __log_start = ::std::time::Instant::now();

            // 将原始方法体移入 async 块执行，确保参数所有权随 future 传递
            let __log_result = (async move #body).await;

            let __log_duration_ms = __log_start.elapsed().as_millis();

            // 根据执行结果分别记录成功或异常日志
            match &__log_result {
                ::core::result::Result::Ok(_) => {
                    ::source_generator_runtime::log_executed(__log_method, &__log_trace_id, __log_duration_ms);
                }
                ::core::result::Result::Err(__log_err) => {
                    ::source_generator_runtime::log_exception(
                        __log_method,
                        &__log_trace_id,
                        __log_duration_ms,
                        &::std::format!("{:?}", __log_err),
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
/// - `max_attempts`：`max_retries` 的兼容别名
/// - `delay_ms`：Rust 侧扩展的毫秒级等待配置，优先级高于 `delay_seconds`
///
/// 仅支持返回 Result<T, E> 的 async fn
///
/// # 使用限制
///
/// 适合方法参数为 `Copy` 类型或引用（`&str`、`&T`）的场景。
/// 若方法体内在 `.await` 点前后移动了 owned 参数的所有权（如将 `String` 传入消耗所有权的调用），
/// 编译器会报错——这类方法本身即不适合重试，应修改为传引用或在调用侧管理重试逻辑。
///
/// # 示例
/// ```ignore
/// use source_generator_runtime::retry;
///
/// impl MyService {
///     // 参数均为 Copy 类型或引用，适合使用 retry
///     #[retry(max_retries = 3, delay_seconds = 1)]
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
/// 参数格式与 #[retry] 保持一致
///
/// - cache：缓存实例表达式，支持 Arc<HybridCache> 或 &HybridCache
/// - ttl_seconds：L2 缓存有效期秒数，默认 60
///
/// 仅支持返回 Result<T, E> 的 async fn
#[proc_macro_attribute]
pub fn cacheable(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as CacheableArgs);
    let input = parse_macro_input!(item as ItemFn);

    expand_cacheable(args, input).unwrap_or_else(|e| e.to_compile_error().into())
}

/// 为 async fn 方法开启并发限制包装，使用分布式锁控制同 key 并发数
///
/// 参数格式与 #[retry] 保持一致
///
/// - lock：分布式锁实例表达式
/// - is_use_parameter：是否将参数加入锁 key
/// - is_block：获取不到锁时是否立即返回错误
/// - semaphore：允许的最大并发数，默认 1
/// - expiry_seconds：锁过期秒数，默认 0 表示使用锁实现默认值
///
/// 仅支持返回 Result<T, E> 的 async fn
#[proc_macro_attribute]
pub fn concurrency_limit(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as ConcurrencyLimitArgs);
    let input = parse_macro_input!(item as ItemFn);

    expand_concurrency_limit(args, input).unwrap_or_else(|e| e.to_compile_error().into())
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
/// 通过声明式清单展开 queue schedule 与 alias 注册调用，减少重复样板代码
#[proc_macro]
pub fn register_task_registry(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as TaskRegistryRegistrationInput);

    expand_task_registry_registration(input).unwrap_or_else(|e| e.to_compile_error().into())
}

/// 服务工厂生成输入
struct ServiceFactoryInput {
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
        let impl_generics = input.parse::<syn::Generics>()?;
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

        match kind.to_string().as_str() {
            "queue" => Ok(Self::Queue {
                definition: parse_named_expr(&content, "definition")?,
                handler: parse_named_expr(&content, "handler")?,
            }),
            "schedule" => Ok(Self::Schedule {
                definition: parse_named_expr(&content, "definition")?,
                handler: parse_named_expr(&content, "handler")?,
            }),
            "queue_alias" => Ok(Self::QueueAlias {
                alias: parse_named_expr(&content, "alias")?,
                target: parse_named_expr(&content, "target")?,
            }),
            "schedule_alias" => Ok(Self::ScheduleAlias {
                alias: parse_named_expr(&content, "alias")?,
                target: parse_named_expr(&content, "target")?,
            }),
            _ => Err(syn::Error::new(kind.span(), "不支持的任务注册类型")),
        }
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

    let impl_blocks = input.groups.into_iter().map(|group| {
        let provider_trait = group.provider_trait;
        let context_expr = group.context_expr;
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
            impl<'a, #state_type> #factory_type
            where
                #state_type: #provider_trait,
            {
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
        impl #impl_generics #factory_type {
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
    let statements = input.entries.into_iter().map(|entry| match entry {
        TaskRegistryRegistrationEntry::Queue {
            definition,
            handler,
        } => {
            quote! {
                #registry_expr.register_queue_with_definition(#definition, #handler);
            }
        }
        TaskRegistryRegistrationEntry::Schedule {
            definition,
            handler,
        } => {
            quote! {
                #registry_expr.register_schedule_with_definition(#definition, #handler);
            }
        }
        TaskRegistryRegistrationEntry::QueueAlias { alias, target } => {
            quote! {
                #registry_expr.register_queue_alias(#alias, #target);
            }
        }
        TaskRegistryRegistrationEntry::ScheduleAlias { alias, target } => {
            quote! {
                #registry_expr.register_schedule_alias(#alias, #target);
            }
        }
    });

    Ok(quote! {
        #(#statements)*
    }
    .into())
}

/// #[retry] 的属性参数
struct RetryArgs {
    /// 最大重试次数，默认 3
    max_retries: Expr,

    /// 每次重试前等待毫秒数，默认 0
    delay_ms: Expr,
}

impl Default for RetryArgs {
    /// 返回默认重试配置：3 次尝试，不等待
    fn default() -> Self {
        Self {
            max_retries: syn::parse_quote!(3u32),
            delay_ms: syn::parse_quote!(0u64),
        }
    }
}

impl Parse for RetryArgs {
    /// 从属性参数列表解析 max_retries 与 delay_seconds
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut args = RetryArgs::default();

        // 无参数时直接使用默认值
        if input.is_empty() {
            return Ok(args);
        }

        // 解析形如 `key = value, key = value` 的参数列表
        let metas = Punctuated::<MetaNameValue, Token![,]>::parse_terminated(input)?;

        for meta in &metas {
            let name = meta
                .path
                .get_ident()
                .ok_or_else(|| syn::Error::new_spanned(&meta.path, "期望标识符"))?
                .to_string();

            match name.as_str() {
                "max_retries" | "max_attempts" => {
                    args.max_retries = meta.value.clone();
                }
                "delay_ms" => {
                    args.delay_ms = meta.value.clone();
                }
                "delay_seconds" => {
                    let delay_seconds = meta.value.clone();
                    args.delay_ms = syn::parse_quote! {
                        ::source_generator_runtime::retry_delay_ms_from_seconds((#delay_seconds) as u64)
                    };
                }
                other => {
                    return Err(syn::Error::new_spanned(
                        &meta.path,
                        format!(
                            "未知参数 `{other}`，支持 max_retries / delay_seconds / max_attempts / delay_ms"
                        ),
                    ));
                }
            }
        }

        Ok(args)
    }
}

/// 展开 #[retry] 宏，生成包含重试循环的完整方法体
fn expand_retry(args: RetryArgs, input: ItemFn) -> syn::Result<TokenStream> {
    let sig = &input.sig;

    // 仅支持 async fn，同步方法不适用此宏
    if sig.asyncness.is_none() {
        return Err(syn::Error::new_spanned(sig, "#[retry] 只能用于 async fn"));
    }

    let attrs = &input.attrs;
    let vis = &input.vis;
    let body = &input.block;

    // 将宏参数转为编译期字面量，嵌入生成代码
    let max_retries = args.max_retries;
    let delay_ms = args.delay_ms;

    let expanded: TokenStream2 = quote! {

        #(#attrs)*
        #vis #sig {

            let __retry_max: u32 = ::source_generator_runtime::normalize_retry_max_retries((#max_retries) as u32);
            let __retry_delay_ms: u64 = (#delay_ms) as u64;

            if __retry_max == 0 {
                return (async #body).await;
            }

            // 记录已消耗的尝试次数，首次执行前为 0
            let mut __retry_attempt: u32 = 0;

            loop {

                // 每次迭代重新创建 async 块，使方法体可多次执行
                // 使用 async（非 async move）以借用方式捕获参数，避免首次执行后参数被消耗
                let __retry_result = (async #body).await;

                match __retry_result {
                    ::core::result::Result::Ok(__val) => {
                        // 执行成功，立即返回结果
                        return ::core::result::Result::Ok(__val);
                    }
                    ::core::result::Result::Err(__err) => {
                        __retry_attempt += 1;
                        // 已耗尽所有重试次数，返回最后一次的错误
                        // __retry_max 表示最大重试次数，原始执行不计入，因此条件为 >
                        if __retry_attempt > __retry_max {
                            return ::core::result::Result::Err(__err);
                        }
                        // 等待指定间隔后进行下一次重试，delay_ms 为 0 时直接跳过
                        ::source_generator_runtime::retry_sleep(__retry_delay_ms).await;
                    }
                }

            }

        }

    };

    Ok(expanded.into())
}

/// #[cacheable] 的属性参数
struct CacheableArgs {
    /// 缓存实例表达式
    cache: Expr,

    /// L2 缓存有效期秒数
    ttl_seconds: Expr,
}

impl Parse for CacheableArgs {
    /// 解析 cache 与 ttl_seconds 参数
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let metas = Punctuated::<MetaNameValue, Token![,]>::parse_terminated(input)?;

        let mut cache: Option<Expr> = None;
        let mut ttl_seconds: Expr = syn::parse_quote!(60u64);

        for meta in &metas {
            let name = meta
                .path
                .get_ident()
                .ok_or_else(|| syn::Error::new_spanned(&meta.path, "期望标识符"))?
                .to_string();

            match name.as_str() {
                "cache" => {
                    cache = Some(meta.value.clone());
                }
                "ttl_seconds" => {
                    ttl_seconds = meta.value.clone();
                }
                other => {
                    return Err(syn::Error::new_spanned(
                        &meta.path,
                        format!("未知参数 `{other}`，支持 cache / ttl_seconds"),
                    ));
                }
            }
        }

        let cache = cache.ok_or_else(|| syn::Error::new(input.span(), "缺少必填参数 cache"))?;

        Ok(Self { cache, ttl_seconds })
    }
}

/// #[concurrency_limit] 的属性参数
struct ConcurrencyLimitArgs {
    /// 分布式锁实例表达式
    lock: Expr,

    /// 是否使用参数参与 key 计算
    is_use_parameter: Expr,

    /// 获取不到锁时是否立即失败
    is_block: Expr,

    /// 允许的最大并发数
    semaphore: Expr,

    /// 锁过期秒数
    expiry_seconds: Expr,
}

impl Default for ConcurrencyLimitArgs {
    /// 创建默认并发限制配置
    fn default() -> Self {
        Self {
            lock: syn::parse_quote!(self.lock),
            is_use_parameter: syn::parse_quote!(false),
            is_block: syn::parse_quote!(false),
            semaphore: syn::parse_quote!(1u32),
            expiry_seconds: syn::parse_quote!(0u64),
        }
    }
}

impl Parse for ConcurrencyLimitArgs {
    /// 解析并发限制宏参数
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let metas = Punctuated::<MetaNameValue, Token![,]>::parse_terminated(input)?;
        let mut args = ConcurrencyLimitArgs::default();
        let mut has_lock = false;

        for meta in &metas {
            let name = meta
                .path
                .get_ident()
                .ok_or_else(|| syn::Error::new_spanned(&meta.path, "期望标识符"))?
                .to_string();

            match name.as_str() {
                "lock" => {
                    args.lock = meta.value.clone();
                    has_lock = true;
                }
                "is_use_parameter" => {
                    args.is_use_parameter = meta.value.clone();
                }
                "is_block" => {
                    args.is_block = meta.value.clone();
                }
                "semaphore" => {
                    args.semaphore = meta.value.clone();
                }
                "expiry_seconds" => {
                    args.expiry_seconds = meta.value.clone();
                }
                other => {
                    return Err(syn::Error::new_spanned(
                        &meta.path,
                        format!(
                            "未知参数 `{other}`，支持 lock / is_use_parameter / is_block / semaphore / expiry_seconds"
                        ),
                    ));
                }
            }
        }

        if !has_lock {
            return Err(syn::Error::new(input.span(), "缺少必填参数 lock"));
        }

        Ok(args)
    }
}

/// 展开 #[cacheable] 宏，生成缓存命中包装逻辑
fn expand_cacheable(args: CacheableArgs, input: ItemFn) -> syn::Result<TokenStream> {
    let sig = &input.sig;

    if sig.asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            sig,
            "#[cacheable] 只能用于 async fn",
        ));
    }

    let (_, error_ty) = extract_result_types(sig)?;
    let arg_tuple = build_argument_tuple(sig)?;

    let attrs = &input.attrs;
    let vis = &input.vis;
    let body = &input.block;
    let fn_name_str = sig.ident.to_string();
    let cache_expr = args.cache;
    let ttl_seconds = args.ttl_seconds;

    let expanded: TokenStream2 = quote! {
        #(#attrs)*
        #vis #sig {

            let __cache_key = ::source_generator_runtime::cacheable_key(
                ::std::module_path!(),
                #fn_name_str,
                &#arg_tuple,
            );
            let __cache = ::source_generator_runtime::hybrid_cache_ref(&(#cache_expr));
            let __cache_result = __cache
                .get_or_create_json(&__cache_key, #ttl_seconds, || async {
                    let __cache_inner_result = (async #body).await;
                    __cache_inner_result.map_err(::source_generator_runtime::hybrid_cache_factory_error)
                })
                .await;

            __cache_result.map_err(|error| {
                <#error_ty as ::core::convert::From<::std::string::String>>::from(error.to_string())
            })

        }
    };

    Ok(expanded.into())
}

/// 展开 #[concurrency_limit] 宏，生成分布式锁包装逻辑
fn expand_concurrency_limit(args: ConcurrencyLimitArgs, input: ItemFn) -> syn::Result<TokenStream> {
    let sig = &input.sig;

    if sig.asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            sig,
            "#[concurrency_limit] 只能用于 async fn",
        ));
    }

    let (_, error_ty) = extract_result_types(sig)?;
    let arg_tuple = build_argument_tuple(sig)?;

    let attrs = &input.attrs;
    let vis = &input.vis;
    let body = &input.block;
    let fn_name_str = sig.ident.to_string();
    let lock_expr = args.lock;
    let is_use_parameter = args.is_use_parameter;
    let is_block = args.is_block;
    let semaphore = args.semaphore;
    let expiry_seconds = args.expiry_seconds;

    let acquire_lock = quote! {
        let __lock_guard = if #is_block {
            __lock_service
                .try_lock(&__lock_key, __lock_expiry, __lock_semaphore)
                .await
                .map_err(|_| {
                    <#error_ty as ::core::convert::From<::std::string::String>>::from(
                        "操作频繁，请稍后再试".to_string(),
                    )
                })?
                .ok_or_else(|| {
                    <#error_ty as ::core::convert::From<::std::string::String>>::from(
                        "请勿频繁操作".to_string(),
                    )
                })?
        } else {
            __lock_service
                .lock(&__lock_key, __lock_expiry, __lock_semaphore)
                .await
                .map_err(|_| {
                    <#error_ty as ::core::convert::From<::std::string::String>>::from(
                        "操作频繁，请稍后再试".to_string(),
                    )
                })?
        };
    };

    let expanded: TokenStream2 = quote! {
        #(#attrs)*
        #vis #sig {

            let __lock_key = ::source_generator_runtime::concurrency_limit_key(
                ::std::module_path!(),
                #fn_name_str,
                #is_use_parameter,
                &#arg_tuple,
            );
            let __lock_service = ::source_generator_runtime::distributed_lock_ref(&(#lock_expr));
            let __lock_semaphore = ::source_generator_runtime::normalize_concurrency_limit_semaphore((#semaphore) as u32);
            let __lock_expiry = ::std::time::Duration::from_secs((#expiry_seconds) as u64);
            #acquire_lock

            let __result = (async #body).await;
            ::std::mem::drop(__lock_guard);
            __result

        }
    };

    Ok(expanded.into())
}

/// 提取 Result 的 Ok 与 Err 类型
fn extract_result_types(sig: &syn::Signature) -> syn::Result<(&Type, &Type)> {
    let ReturnType::Type(_, ty) = &sig.output else {
        return Err(syn::Error::new_spanned(
            sig,
            "方法返回值必须是 Result<T, E>",
        ));
    };

    let Type::Path(type_path) = ty.as_ref() else {
        return Err(syn::Error::new_spanned(ty, "方法返回值必须是 Result<T, E>"));
    };

    let result_segment = type_path
        .path
        .segments
        .last()
        .ok_or_else(|| syn::Error::new_spanned(ty, "方法返回值必须是 Result<T, E>"))?;

    if result_segment.ident != "Result" {
        return Err(syn::Error::new_spanned(ty, "方法返回值必须是 Result<T, E>"));
    }

    let PathArguments::AngleBracketed(arguments) = &result_segment.arguments else {
        return Err(syn::Error::new_spanned(ty, "方法返回值必须是 Result<T, E>"));
    };

    if arguments.args.len() != 2 {
        return Err(syn::Error::new_spanned(ty, "方法返回值必须是 Result<T, E>"));
    }

    let ok_ty = match &arguments.args[0] {
        GenericArgument::Type(ty) => ty,
        other => {
            return Err(syn::Error::new_spanned(
                other,
                "Result<T, E> 的 T 必须是类型",
            ));
        }
    };
    let err_ty = match &arguments.args[1] {
        GenericArgument::Type(ty) => ty,
        other => {
            return Err(syn::Error::new_spanned(
                other,
                "Result<T, E> 的 E 必须是类型",
            ));
        }
    };

    Ok((ok_ty, err_ty))
}

/// 将方法参数整理为可序列化元组
fn build_argument_tuple(sig: &syn::Signature) -> syn::Result<TokenStream2> {
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
        Ok(quote! { () })
    } else {
        Ok(quote! { ( #(#args),* ) })
    }
}
