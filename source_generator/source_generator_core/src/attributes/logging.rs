use crate::*;

/// #[logging] 的方法标识参数
pub(crate) struct LoggingArgs {
    /// 是否按 Result 返回值记录成功或失败，别名同样适用。
    result: bool,
    /// 借用返回值仍持有参数时，可显式关闭调用后的参数刷新。
    refresh_arguments: bool,
    stream: bool,
    stream_item_result: bool,
    stream_boxed: Option<bool>,
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
        let mut result = None;
        let mut refresh_arguments = None;
        let mut stream = None;
        let mut stream_item_result = None;
        let mut stream_boxed = None;

        for meta in &metas {
            let name = meta
                .path()
                .get_ident()
                .ok_or_else(|| syn::Error::new_spanned(meta.path(), "期望标识符"))?
                .to_string();

            if matches!(
                name.as_str(),
                "result" | "refresh_arguments" | "stream" | "stream_item_result" | "stream_boxed"
            ) {
                let slot = match name.as_str() {
                    "result" => &mut result,
                    "refresh_arguments" => &mut refresh_arguments,
                    "stream" => &mut stream,
                    "stream_item_result" => &mut stream_item_result,
                    _ => &mut stream_boxed,
                };
                let value = parse_bool_literal(&require_name_value(meta, &name)?.value, &name)?;
                if slot.replace(value).is_some() {
                    return Err(syn::Error::new_spanned(
                        meta,
                        format!("{name} 不得重复配置"),
                    ));
                }
                continue;
            }

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
                        "未知参数 `{name}`，支持 key_scope / key_types(...) / key_consts(...) / key_include_self / result / refresh_arguments / stream / stream_item_result / stream_boxed"
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

        if stream != Some(true) && (stream_item_result.is_some() || stream_boxed.is_some()) {
            return Err(syn::Error::new(
                input.span(),
                "stream_item_result 和 stream_boxed 仅用于 stream = true",
            ));
        }
        Ok(Self {
            result: result.unwrap_or(true),
            refresh_arguments: refresh_arguments.unwrap_or(true),
            stream: stream.unwrap_or(false),
            stream_item_result: stream_item_result.unwrap_or(true),
            stream_boxed,
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
    for (index, argument) in input.sig.inputs.iter_mut().enumerate() {
        if let syn::FnArg::Typed(argument) = argument
            && matches!(argument.pat.as_ref(), Pat::Wild(_))
        {
            let ident = format_ident!(
                "__log_argument_{index}",
                span = proc_macro2::Span::mixed_site()
            );
            *argument.pat = syn::parse_quote!(#ident);
        }
    }

    let sig = &input.sig;

    let runtime = runtime_crate_path()?;

    validate_receiver_key_strategy(sig, args.key_include_self, "logging")?;

    let attrs = &input.attrs;

    let vis = &input.vis;

    let body = &input.block;
    let mut snapshots = Vec::new();
    let mut stream_snapshots = Vec::new();
    let mut refresh = Vec::new();
    let mut reborrows = Vec::new();
    let mut mutability_uses = Vec::new();
    for argument in &sig.inputs {
        let syn::FnArg::Typed(argument) = argument else {
            continue;
        };
        let Pat::Ident(pattern) = argument.pat.as_ref() else {
            return Err(syn::Error::new_spanned(
                &argument.pat,
                "日志参数需使用标识符或 _，请将解构移到方法体",
            ));
        };
        let ident = &pattern.ident;
        let name = ident.to_string();
        let name = name.trim_start_matches("r#");
        let value = if matches!(argument.ty.as_ref(), Type::Reference(_)) {
            quote! { &*#ident }
        } else {
            quote! { &#ident }
        };
        let snapshot = quote! {
            __log_invocation.set_argument(#name, || {
                (&&&&&#runtime::LogValue(#value)).capture_log_value()
            });
        };
        snapshots.push(snapshot.clone());
        stream_snapshots.push(quote! {
            __log_arguments.push((#name, (&&&&&#runtime::LogValue(#value)).capture_log_value()));
        });
        if args.refresh_arguments
            && let Type::Reference(reference) = argument.ty.as_ref()
        {
            refresh.push(snapshot);
            if reference.mutability.is_some() {
                let mutability = &pattern.mutability;
                reborrows.push(quote! { let #mutability #ident = &mut *#ident; });
                if mutability.is_some() {
                    mutability_uses.push(quote! { let _ = &mut #ident; });
                }
            }
        }
    }
    let execution = if sig.asyncness.is_some() {
        quote! { #runtime::catch_log_unwind(async { #(#reborrows)* #body }).await }
    } else {
        // 独立闭包使 return 和 ? 回到包装层，重借用保留调用后可读取的引用参数。
        quote! { ::std::panic::catch_unwind(::std::panic::AssertUnwindSafe(|| { #(#reborrows)* #body })) }
    };
    let completion = if args.result {
        quote! {
            match &__log_result {
                ::core::result::Result::Ok(value) => __log_invocation.complete_value(value, || (&&&&&#runtime::LogValue(value)).capture_log_value()),
                ::core::result::Result::Err(error) => {
                    use #runtime::CaptureLogError as _;
                    __log_invocation.fail(|| (&&#runtime::LogError(error)).capture_log_error());
                }
            }
        }
    } else {
        quote! { __log_invocation.complete_value(&__log_result, || (&&&&&#runtime::LogValue(&__log_result)).capture_log_value()); }
    };

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

    if args.stream {
        let prepare = if sig.asyncness.is_some() {
            quote! { (async move #body).await }
        } else {
            quote! { (move || #body)() }
        };
        let prepare = if args.result {
            quote! { #prepare? }
        } else {
            prepare
        };
        let capture = if args.stream_item_result {
            quote! {
                match item {
                    ::core::result::Result::Ok(value) => #runtime::StreamItemLog::Value(
                        capture.then(|| (&&&&&#runtime::LogValue(value)).capture_log_value())
                    ),
                    ::core::result::Result::Err(error) => {
                        use #runtime::CaptureLogError as _;
                        #runtime::StreamItemLog::Error(capture_error.then(|| (&&#runtime::LogError(error)).capture_log_error()))
                    }
                }
            }
        } else {
            quote! {
                let _ = capture_error;
                #runtime::StreamItemLog::Value(capture.then(|| (&&&&&#runtime::LogValue(item)).capture_log_value()))
            }
        };
        let boxed = args
            .stream_boxed
            .unwrap_or_else(|| logging_stream_is_boxed(&sig.output, args.result));
        let output = if boxed {
            quote! { ::std::boxed::Box::pin(__log_stream) }
        } else {
            quote! { __log_stream }
        };
        let output = if args.result {
            quote! { ::core::result::Result::Ok(#output) }
        } else {
            output
        };
        return Ok(quote! {
            #(#attrs)*
            #vis #sig {
                use #runtime::CaptureLogValue as _;
                let __log_method = #method_key;
                let mut __log_arguments = ::std::vec::Vec::new();
                #(#stream_snapshots)*
                // 准备阶段失败直接传播，日志从返回流的首次 poll 开始。
                let __log_source = #prepare;
                let __log_stream = #runtime::LoggedStream::new(
                    __log_source,
                    __log_method,
                    __log_arguments,
                    |item, capture, capture_error| { #capture },
                );
                #output
            }
        }
        .into());
    }

    let expanded: TokenStream2 = quote! {


        #(#attrs)*

        #vis #sig {


            let mut __log_invocation = #runtime::InvocationLog::new(#method_key);
            use #runtime::CaptureLogValue as _;
            #(#snapshots)*
            __log_invocation.start();
            #(#mutability_uses)*
            let __log_outcome = #execution;
            #(#refresh)*
            let __log_result = match __log_outcome {
                ::core::result::Result::Ok(value) => value,
                ::core::result::Result::Err(payload) => {
                    __log_invocation.panic(payload.as_ref());
                    ::std::panic::resume_unwind(payload);
                }
            };
            #completion


            __log_result


        }


    };

    Ok(expanded.into())
}

/// 常见流别名自动恢复装箱；自定义别名可显式使用 stream_boxed。
fn logging_stream_is_boxed(output: &syn::ReturnType, result: bool) -> bool {
    let syn::ReturnType::Type(_, ty) = output else {
        return false;
    };
    let ty = if result {
        extract_result_success_type(ty).unwrap_or(ty)
    } else {
        ty
    };
    let Type::Path(path) = ty else {
        return false;
    };
    let Some(segment) = path.path.segments.last() else {
        return false;
    };
    if segment.ident == "BoxStream" || segment.ident == "LocalBoxStream" {
        return true;
    }
    if segment.ident == "Pin"
        && let PathArguments::AngleBracketed(arguments) = &segment.arguments
    {
        return arguments.args.iter().any(|argument| matches!(argument, GenericArgument::Type(Type::Path(path)) if path.path.segments.last().is_some_and(|segment| segment.ident == "Box")));
    }
    false
}
