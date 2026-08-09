use crate::*;

/// #[logging] 的方法标识参数
pub(crate) struct LoggingArgs {
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
pub(crate) fn expand_logging(args: LoggingArgs, mut input: ItemFn) -> syn::Result<TokenStream> {
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
