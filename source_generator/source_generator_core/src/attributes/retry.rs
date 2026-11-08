use crate::*;

/// #[retry] 的属性参数
pub(crate) struct RetryArgs {
    pub(crate) context: Option<Expr>,
    /// 最大重试次数，默认 3
    max_retries: Option<Expr>,

    /// 每次重试前的等待配置
    delay: RetryDelay,

    /// 可选的调用方取消令牌表达式。
    pub(crate) cancellation: Option<Expr>,

    /// 自定义错误类型的取消判定；缺省使用公共取消契约。
    is_cancelled: Option<Expr>,

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
pub(crate) enum RetryDelay {
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
            context: None,
            max_retries: None,

            delay: RetryDelay::Default,

            cancellation: None,

            is_cancelled: None,

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
                "cancellation" => {
                    if args.cancellation.is_some() {
                        return Err(syn::Error::new_spanned(
                            meta.path(),
                            "参数 cancellation 不能重复配置",
                        ));
                    }
                    args.cancellation =
                        Some(require_name_value(meta, "cancellation")?.value.clone());
                }

                "is_cancelled" => {
                    if args.is_cancelled.is_some() {
                        return Err(syn::Error::new_spanned(
                            meta.path(),
                            "参数 is_cancelled 不能重复配置",
                        ));
                    }
                    args.is_cancelled =
                        Some(require_name_value(meta, "is_cancelled")?.value.clone());
                }

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
                                "未知参数 `{other}`，支持 max_retries / delay_seconds / delay_ms / cancellation / is_cancelled / key_scope / key_types(...) / key_consts(...) / key_include_self"
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
pub(crate) fn expand_retry(args: RetryArgs, mut input: ItemFn) -> syn::Result<TokenStream> {
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

    let is_cancelled = args.is_cancelled.map_or_else(
        || quote! { #runtime::CancellationError::is_cancelled(&__err) },
        |predicate| quote! { (#predicate)(&__err) },
    );

    let (cancellation_setup, cancellation_check, retry_wait) = match args.cancellation {
        Some(token) => (
            quote! { let __retry_cancellation: &#runtime::CancellationToken = &(#token); },
            quote! { #runtime::check_retry_cancellation(__retry_cancellation)?; },
            quote! {
                #runtime::retry_sleep_with_cancellation(
                    __retry_delay_ms,
                    __retry_cancellation,
                ).await?;
            },
        ),
        None => (
            quote! {},
            quote! {},
            quote! { #runtime::retry_sleep(__retry_delay_ms).await; },
        ),
    };

    let context = args.context.map_or_else(
        || quote! { #runtime::InvocationContext::new(#method_key) },
        |value| quote! { #value },
    );

    let expanded: TokenStream2 = quote! {


        #(#attrs)*

        #vis #sig {


            let __retry_max: u32 = #max_retries;

            let __retry_delay_ms: u64 = #delay_ms;

            let __retry_context = #context;
            __retry_context.set_retry_count(0);

            #(#retry_copy_assertions)*

            let #retry_body = async || #body;


            if __retry_max == 0 {

                return #retry_body().await;

            }

            #cancellation_setup


            // 记录已经执行的重试次数，首次执行前为 0

            let mut __retry_count: u64 = 0;

            let __retry_max = u64::from(__retry_max);



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

                        // 取消直接返回原错误，不产生重试或耗尽日志。
                        if #is_cancelled {
                            return ::core::result::Result::Err(__err);
                        }


                        if __retry_count >= __retry_max {

                            #runtime::log_retry_exhausted_debug(

                                &__retry_context.method,

                                __retry_context.trace_id(),

                                __retry_count,

                                &__err,

                            );

                            return ::core::result::Result::Err(__err);

                        }

                        // 与源管道相同：仅在仍允许下一次尝试时观察令牌。
                        #cancellation_check

                        __retry_count += 1;

                        #runtime::log_retry_scheduled_debug(

                            &__retry_context.method,

                            __retry_context.trace_id(),

                            __retry_count,

                            __retry_delay_ms,

                            &__err,

                        );

                        // 等待指定间隔后进行下一次重试，delay_ms 为 0 时直接跳过

                        #retry_wait

                        __retry_context.set_retry_count(__retry_count);

                    }

                }


            }


        }


    };

    Ok(expanded.into())
}

/// 校验重试方法参数不会在多次尝试之间保留签名级可变状态
pub(crate) fn validate_retry_parameters(sig: &syn::Signature) -> syn::Result<()> {
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
pub(crate) fn build_retry_copy_assertions(
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
pub(crate) fn pattern_contains_mutable_binding(pattern: &Pat) -> bool {
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
