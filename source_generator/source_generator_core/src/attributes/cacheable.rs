use crate::*;

/// #[cacheable] 的属性参数
pub(crate) struct CacheableArgs {
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

/// 展开 #[cacheable] 宏，生成缓存命中包装逻辑
pub(crate) fn expand_cacheable(args: CacheableArgs, mut input: ItemFn) -> syn::Result<TokenStream> {
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
pub(crate) fn build_cacheable_body_declaration(
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

/// 根据返回类型语法与显式配置决定是否缓存成功值
pub(crate) fn resolve_cache_value(
    return_type: &Type,
    configured: Option<bool>,
) -> syn::Result<bool> {
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
pub(crate) fn extract_result_success_type(return_type: &Type) -> Option<&Type> {
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
pub(crate) fn type_is_unit(ty: &Type) -> bool {
    match ty {
        Type::Group(group) => type_is_unit(&group.elem),
        Type::Paren(paren) => type_is_unit(&paren.elem),
        Type::Tuple(tuple) => tuple.elems.is_empty(),
        _ => false,
    }
}
