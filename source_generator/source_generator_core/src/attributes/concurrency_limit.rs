use crate::*;

/// #[concurrency_limit] 的属性参数
pub(crate) struct ConcurrencyLimitArgs {
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

/// 展开 #[concurrency_limit] 宏，生成分布式锁包装逻辑
pub(crate) fn expand_concurrency_limit(
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
pub(crate) fn build_argument_value(
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
pub(crate) fn build_argument_key_assertions(
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
