use crate::*;

pub(crate) fn expand_event(input: DeriveInput) -> syn::Result<TokenStream> {
    if !matches!(input.data, Data::Struct(_)) || !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "事件必须是非泛型具体结构体",
        ));
    }
    let mut name: Option<syn::LitStr> = None;
    for attribute in &input.attrs {
        if !attribute.path().is_ident("event") {
            continue;
        }
        attribute.parse_nested_meta(|meta| {
            if !meta.path.is_ident("name") {
                return Err(meta.error("仅支持 name 参数"));
            }
            if name.is_some() {
                return Err(meta.error("事件 name 不能重复配置"));
            }
            name = Some(meta.value()?.parse()?);
            Ok(())
        })?;
    }
    let name = name.ok_or_else(|| {
        syn::Error::new_spanned(
            &input.ident,
            "必须配置 #[event(name = \"稳定的完整事件名\")]",
        )
    })?;
    if name.value().trim().is_empty() {
        return Err(syn::Error::new_spanned(name, "事件名称不能为空"));
    }
    let runtime = runtime_crate_path()?;
    let ident = &input.ident;
    Ok(quote! {
        impl #runtime::event_bus::Event for #ident {
            const NAME: &'static str = #name;
        }
    }
    .into())
}
