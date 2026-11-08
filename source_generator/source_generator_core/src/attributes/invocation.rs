use crate::*;

pub(crate) struct InvocationArgs {
    keys: Vec<syn::Meta>,
    scope: syn::LitStr,
    types: Vec<Type>,
    consts: Vec<Expr>,
    include_self: Option<bool>,
    cancellation: Option<Expr>,
}

impl Parse for InvocationArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let metas = Punctuated::<syn::Meta, Token![,]>::parse_terminated(input)?;
        let (mut scope, mut types, mut consts, mut include_self) = (None, None, None, None);
        let mut cancellation = None;
        let mut keys = Vec::new();
        for meta in metas {
            if meta.path().is_ident("cancellation") {
                if cancellation
                    .replace(require_name_value(&meta, "cancellation")?.value.clone())
                    .is_some()
                {
                    return Err(syn::Error::new_spanned(meta, "cancellation 不得重复配置"));
                }
            } else {
                let name = meta
                    .path()
                    .get_ident()
                    .map(ToString::to_string)
                    .unwrap_or_default();
                if !parse_scoped_key_meta(
                    &meta,
                    &name,
                    &mut scope,
                    &mut types,
                    &mut consts,
                    &mut include_self,
                )? {
                    return Err(syn::Error::new_spanned(meta, "未知的 invocation 参数"));
                }
                keys.push(meta);
            }
        }
        validate_scoped_key_options(
            input,
            scope.as_ref(),
            types.as_deref(),
            consts.as_deref(),
            include_self,
        )?;
        Ok(Self {
            keys,
            scope: scope.unwrap(),
            types: types.unwrap_or_default(),
            consts: consts.unwrap_or_default(),
            include_self,
            cancellation,
        })
    }
}

/// 一次显式调用边界统一展开其声明的行为；不依赖线程/任务局部变量或运行时反射。
pub(crate) fn expand(args: InvocationArgs, mut input: ItemFn) -> syn::Result<TokenStream2> {
    bind_wildcard_impl_trait_parameters(&mut input);
    validate_receiver_key_strategy(&input.sig, args.include_self, "invocation")?;
    if input.sig.constness.is_some() {
        return Err(syn::Error::new_spanned(
            &input.sig,
            "invocation 不支持 const fn",
        ));
    }
    let runtime = runtime_crate_path()?;
    let key = build_scoped_method_key(
        &input.sig,
        &args.scope,
        &args.types,
        &args.consts,
        args.include_self,
        &runtime,
    )?;
    let context = format_ident!(
        "__source_generator_invocation_context",
        span = proc_macro2::Span::mixed_site()
    );
    let shared: Expr = syn::parse_quote!(#context.clone());
    let mut behaviors = Vec::new();
    let mut retained = Vec::new();
    let mut retry_seen = false;
    let mut cache_seen = false;
    for attr in std::mem::take(&mut input.attrs) {
        let name = attr
            .path()
            .segments
            .last()
            .map(|s| s.ident.to_string())
            .unwrap_or_default();
        let known_path = attr.path().segments.len() == 1
            || (attr.path().segments.len() == 2
                && attr.path().segments[0].ident == "source_generator_runtime");
        if !known_path
            || !matches!(
                name.as_str(),
                "logging" | "cacheable" | "retry" | "behavior"
            )
        {
            // 未知属性宏不能在已消费的行为之间被悄悄提升为外层。
            if !behaviors.is_empty()
                && !matches!(
                    name.as_str(),
                    "doc"
                        | "cfg"
                        | "allow"
                        | "warn"
                        | "deny"
                        | "forbid"
                        | "expect"
                        | "inline"
                        | "cold"
                        | "must_use"
                        | "deprecated"
                        | "track_caller"
                )
            {
                return Err(syn::Error::new_spanned(
                    attr,
                    "其他属性宏请放到 invocation 外层，避免改变行为顺序",
                ));
            }
            retained.push(attr);
            continue;
        }
        if name == "retry" && std::mem::replace(&mut retry_seen, true) {
            return Err(syn::Error::new_spanned(
                attr,
                "一次调用只能声明一个重试行为",
            ));
        }
        if name == "cacheable" && std::mem::replace(&mut cache_seen, true) {
            return Err(syn::Error::new_spanned(
                attr,
                "一次调用只能声明一个缓存参数键",
            ));
        }
        let options = match &attr.meta {
            syn::Meta::Path(_) => Punctuated::<syn::Meta, Token![,]>::new(),
            _ => attr.parse_args_with(Punctuated::<syn::Meta, Token![,]>::parse_terminated)?,
        };
        for option in &options {
            if args.keys.iter().any(|key| key.path() == option.path())
                || option.path().get_ident().is_some_and(|id| {
                    matches!(
                        id.to_string().as_str(),
                        "key_scope" | "key_types" | "key_consts" | "key_include_self"
                    )
                })
            {
                return Err(syn::Error::new_spanned(
                    option,
                    "共享方法键只在 invocation 上声明",
                ));
            }
        }
        let options = options.iter().chain(args.keys.iter());
        let tokens = quote!(#(#options),*);
        behaviors.push((name, tokens));
    }
    if behaviors.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.sig,
            "invocation 需要至少一个 logging/cacheable/retry/behavior 声明",
        ));
    }
    for (name, tokens) in behaviors.into_iter().rev() {
        let expanded = match name.as_str() {
            "logging" => {
                let mut options: LoggingArgs = syn::parse2(tokens)?;
                options.context = Some(shared.clone());
                expand_logging(options, input)?.into()
            }
            "cacheable" => {
                let mut options: CacheableArgs = syn::parse2(tokens)?;
                options.context = Some(shared.clone());
                expand_cacheable(options, input)?.into()
            }
            "retry" => {
                let mut options: RetryArgs = syn::parse2(tokens)?;
                options.context = Some(shared.clone());
                if args.cancellation.is_some() {
                    if options.cancellation.is_some() {
                        return Err(syn::Error::new_spanned(
                            &input.sig,
                            "共享取消令牌只在 invocation 上声明",
                        ));
                    }
                    options.cancellation = Some(
                        syn::parse_quote!(#context.cancellation().expect("已声明调用取消令牌")),
                    );
                } else if options.cancellation.is_some() {
                    return Err(syn::Error::new_spanned(
                        &input.sig,
                        "请将 cancellation 移到 invocation，供全部行为共享",
                    ));
                }
                expand_retry(options, input)?.into()
            }
            _ => {
                let mut options: super::behavior::BehaviorArgs = syn::parse2(tokens)?;
                options.context = Some(shared.clone());
                super::behavior::expand(options, input)?
            }
        };
        input = syn::parse2(expanded)?;
        // 每层只借用上层句柄来克隆，再允许自己的 continuation 移动局部句柄。
        // 此绑定必须位于当前层内部：被 retry/cacheable 包裹时，每次调用闭包
        // 都重新创建局部克隆，不能让内部 async move 消耗闭包捕获的上层上下文。
        let body = input.block;
        input.block = syn::parse_quote!({
            let #context = #shared;
            #body
        });
    }
    let cancellation = args
        .cancellation
        .map_or_else(|| quote!(None), |token| quote!(Some((#token).clone())));
    let body = input.block;
    input.attrs = retained;
    input.block = syn::parse_quote!({
        let #context = #runtime::InvocationContext::with_cancellation(#key, #cancellation);
        #body
    });
    Ok(quote!(#input))
}
