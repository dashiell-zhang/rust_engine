use crate::*;

/// 根据显式作用域、方法泛型和稳定类型标签生成方法标识表达式
pub(crate) fn build_scoped_method_key(
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
pub(crate) fn build_impl_trait_stable_types(
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
pub(crate) fn validate_explicit_method_generic_dimensions(
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
