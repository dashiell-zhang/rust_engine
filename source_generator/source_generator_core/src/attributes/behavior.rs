use crate::*;

pub(crate) struct BehaviorArgs {
    behaviors: Vec<Expr>,
    key_scope: syn::LitStr,
    key_types: Vec<Type>,
    key_consts: Vec<Expr>,
    key_include_self: Option<bool>,
    stream: bool,
    result: bool,
    stream_boxed: Option<bool>,
}

impl Parse for BehaviorArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let entries = Punctuated::<syn::Meta, Token![,]>::parse_terminated(input)?;
        let (mut key_scope, mut key_types, mut key_consts, mut key_include_self) =
            (None, None, None, None);
        let (mut behaviors, mut stream, mut result, mut stream_boxed) = (None, None, None, None);
        for meta in entries {
            let name = meta
                .path()
                .get_ident()
                .map(ToString::to_string)
                .unwrap_or_default();
            match name.as_str() {
                "behavior" | "behaviors" => {
                    if behaviors.is_some() {
                        return Err(syn::Error::new_spanned(meta, "行为不能重复配置"));
                    }
                    behaviors = Some(if name == "behavior" {
                        vec![require_name_value(&meta, "behavior")?.value.clone()]
                    } else if let syn::Meta::List(list) = &meta {
                        list.parse_args_with(Punctuated::<Expr, Token![,]>::parse_terminated)?
                            .into_iter()
                            .collect()
                    } else {
                        return Err(syn::Error::new_spanned(meta, "使用 behaviors(表达式, ...)"));
                    });
                }
                "stream" | "result" | "stream_boxed" => {
                    let target = match name.as_str() {
                        "stream" => &mut stream,
                        "result" => &mut result,
                        _ => &mut stream_boxed,
                    };
                    if target.is_some() {
                        return Err(syn::Error::new_spanned(meta, "参数不能重复配置"));
                    }
                    *target = Some(parse_bool_literal(
                        &require_name_value(&meta, &name)?.value,
                        &name,
                    )?);
                }
                _ => {
                    if !parse_scoped_key_meta(
                        &meta,
                        &name,
                        &mut key_scope,
                        &mut key_types,
                        &mut key_consts,
                        &mut key_include_self,
                    )? {
                        return Err(syn::Error::new_spanned(meta, "未知的 behavior 参数"));
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
        let behaviors = behaviors
            .filter(|items: &Vec<Expr>| !items.is_empty())
            .ok_or_else(|| {
                syn::Error::new(
                    input.span(),
                    "必须声明 behavior = 表达式 或 behaviors(表达式, ...)",
                )
            })?;
        if !stream.unwrap_or(false) && (result.is_some() || stream_boxed.is_some()) {
            return Err(syn::Error::new(
                input.span(),
                "result/stream_boxed 仅用于 stream = true",
            ));
        }
        Ok(Self {
            behaviors,
            key_scope: key_scope.unwrap(),
            key_types: key_types.unwrap_or_default(),
            key_consts: key_consts.unwrap_or_default(),
            key_include_self,
            stream: stream.unwrap_or(false),
            result: result.unwrap_or(false),
            stream_boxed,
        })
    }
}

pub(crate) fn expand(args: BehaviorArgs, mut input: ItemFn) -> syn::Result<TokenStream2> {
    bind_wildcard_impl_trait_parameters(&mut input);
    validate_receiver_key_strategy(&input.sig, args.key_include_self, "behavior")?;
    if input.sig.constness.is_some() {
        return Err(syn::Error::new_spanned(
            &input.sig,
            "行为管道不支持 const fn",
        ));
    }
    let runtime = runtime_crate_path()?;
    let key = build_scoped_method_key(
        &input.sig,
        &args.key_scope,
        &args.key_types,
        &args.key_consts,
        args.key_include_self,
        &runtime,
    )?;
    let context = format_ident!(
        "__source_generator_behavior_context",
        span = proc_macro2::Span::mixed_site()
    );
    let names = (0..args.behaviors.len())
        .map(|i| {
            format_ident!(
                "__source_generator_behavior_{i}",
                span = proc_macro2::Span::mixed_site()
            )
        })
        .collect::<Vec<_>>();
    let expressions = &args.behaviors;
    let attrs = &input.attrs;
    let vis = &input.vis;
    let sig = &input.sig;
    let body = &input.block;
    let mut execution = quote! { #body };
    if args.stream {
        execution = if sig.asyncness.is_some() {
            quote! { (async move #body).await }
        } else {
            quote! { (move || #body)() }
        };
        if args.result {
            execution = quote! { #execution? };
        }
        for name in names.iter().rev() {
            execution = quote! { #runtime::invoke_stream(#context.clone(), #name, #execution) };
        }
        if args
            .stream_boxed
            .unwrap_or_else(|| super::logging::logging_stream_is_boxed(&sig.output, args.result))
        {
            execution = quote! { ::std::boxed::Box::pin(#execution) };
        }
        if args.result {
            execution = quote! { ::core::result::Result::Ok(#execution) };
        }
    } else {
        for name in names.iter().rev() {
            execution = if sig.asyncness.is_some() {
                quote! { #runtime::invoke_async(#context.clone(), #name, async move { #execution }).await }
            } else {
                quote! { #runtime::invoke_sync(#context.clone(), #name, move || { #execution }) }
            };
        }
    }
    Ok(quote! {
        #(#attrs)*
        #vis #sig {
            let #context = #runtime::InvocationContext::new(#key);
            #(let #names = #expressions;)*
            #execution
        }
    })
}
