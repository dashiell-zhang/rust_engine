use crate::*;

/// 每个字段只绑定一次，保留 Serde 的别名、默认值和显式转换器。
pub(crate) fn expand(input: DeriveInput) -> syn::Result<TokenStream> {
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "QueryModel 暂不支持泛型模型",
        ));
    }
    let common = match crate_name("common") {
        Ok(FoundCrate::Itself) => quote!(crate),
        Ok(FoundCrate::Name(name)) => {
            let name = format_ident!("{name}");
            quote!(::#name)
        }
        Err(error) => return Err(syn::Error::new_spanned(&input.ident, error)),
    };
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "QueryModel 需要具名字段结构体",
        ));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "QueryModel 需要具名字段结构体",
        ));
    };
    let container_attrs: Vec<_> = input
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("serde"))
        .collect();
    for attr in &container_attrs {
        let options = attr.parse_args_with(Punctuated::<syn::Meta, Token![,]>::parse_terminated)?;
        for option in options {
            if !option.path().is_ident("rename_all") {
                return Err(syn::Error::new_spanned(
                    option,
                    "QueryModel 的模型级 Serde 配置仅支持 rename_all",
                ));
            }
        }
    }
    let mut bindings = Vec::new();
    let mut members = Vec::new();
    for field in &fields.named {
        let member = field.ident.as_ref().expect("已检查具名字段");
        let slot = format_ident!("__query_value_{member}");
        let ty = &field.ty;
        let attrs: Vec<_> = field
            .attrs
            .iter()
            .filter(|attr| attr.path().is_ident("serde"))
            .collect();
        for attr in &attrs {
            let options =
                attr.parse_args_with(Punctuated::<syn::Meta, Token![,]>::parse_terminated)?;
            for option in options {
                if option.path().is_ident("flatten") {
                    return Err(syn::Error::new_spanned(
                        option,
                        "QueryModel 暂不支持 flatten，请显式声明 Query 字段",
                    ));
                }
            }
        }
        let mut validation = None::<syn::Path>;
        for attr in field
            .attrs
            .iter()
            .filter(|attr| attr.path().is_ident("query"))
        {
            attr.parse_nested_meta(|meta| {
                if !meta.path.is_ident("validate") || validation.is_some() {
                    return Err(meta.error("Query 字段仅支持单个 validate = \"函数路径\""));
                }
                let value: syn::LitStr = meta.value()?.parse()?;
                validation = Some(value.parse()?);
                Ok(())
            })?;
        }
        let validate = validation.map(|function| {
            quote! {
                if let ::std::result::Result::Err(error) = #function(&value.#member) {
                    __query_errors.push(error);
                }
            }
        });
        bindings.push(quote! {
            let #slot = {
                #[derive(::serde::Deserialize)]
                #(#container_attrs)*
                struct Field { #(#attrs)* #member: #ty }
                match #common::json_helper::input::query_field::<Field>(__query_fields) {
                    ::std::result::Result::Ok(value) => {
                        #validate
                        ::std::option::Option::Some(value.#member)
                    }
                    ::std::result::Result::Err(error) => {
                        __query_errors.push(error);
                        ::std::option::Option::None
                    }
                }
            };
        });
        members.push(quote!(#member: #slot.expect("Query 字段已通过绑定")));
    }
    let name = &input.ident;
    Ok(quote! {
        impl #common::json_helper::input::QueryModel for #name {
            fn bind_query(__query_fields: &[(::std::string::String, ::std::string::String)])
                -> ::std::result::Result<Self, ::std::vec::Vec<#common::json_helper::input::InputError>> {
                let mut __query_errors = ::std::vec::Vec::new();
                #(#bindings)*
                if !__query_errors.is_empty() {
                    return ::std::result::Result::Err(__query_errors);
                }
                ::std::result::Result::Ok(Self { #(#members),* })
            }
        }
    }.into())
}
