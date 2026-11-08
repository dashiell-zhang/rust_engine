use crate::*;

pub(crate) fn expand_rsa_decrypt(input: DeriveInput) -> syn::Result<TokenStream> {
    reject_container_attributes(&input.attrs)?;
    let runtime = runtime_crate_path()?;
    let ident = &input.ident;
    let mut generics = input.generics.clone();
    for parameter in generics.type_params_mut() {
        parameter
            .bounds
            .push(syn::parse_quote!(#runtime::rsa::RsaDecrypt));
    }
    let body = match &input.data {
        Data::Struct(data) => {
            let calls = data
                .fields
                .iter()
                .enumerate()
                .map(|(index, field)| {
                    let member = field
                        .ident
                        .clone()
                        .map(syn::Member::Named)
                        .unwrap_or_else(|| syn::Member::Unnamed(syn::Index::from(index)));
                    field_call(field, quote!(&mut self.#member), &runtime)
                })
                .collect::<syn::Result<Vec<_>>>()?;
            quote! { #(#calls)* }
        }
        Data::Enum(data) => {
            let arms = data
                .variants
                .iter()
                .map(|variant| {
                    reject_container_attributes(&variant.attrs)?;
                    let name = &variant.ident;
                    let bindings = variant
                        .fields
                        .iter()
                        .enumerate()
                        .map(|(i, _)| format_ident!("__rsa_field_{i}"))
                        .collect::<Vec<_>>();
                    let calls = variant
                        .fields
                        .iter()
                        .zip(&bindings)
                        .map(|(field, binding)| field_call(field, quote!(#binding), &runtime))
                        .collect::<syn::Result<Vec<_>>>()?;
                    let pattern = match &variant.fields {
                        Fields::Named(fields) => {
                            let names = fields.named.iter().map(|field| &field.ident);
                            quote! { Self::#name { #(#names: #bindings),* } }
                        }
                        Fields::Unnamed(_) => quote! { Self::#name(#(#bindings),*) },
                        Fields::Unit => quote! { Self::#name },
                    };
                    Ok(quote! { #pattern => { #(#calls)* } })
                })
                .collect::<syn::Result<Vec<_>>>()?;
            quote! { match self { #(#arms),* } }
        }
        Data::Union(_) => return Err(syn::Error::new_spanned(ident, "RSA 请求模型不支持 union")),
    };
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    Ok(quote! {
        impl #impl_generics #runtime::rsa::RsaDecrypt for #ident #type_generics #where_clause {
            fn decrypt_fields(&mut self, __rsa_decrypt: &mut #runtime::rsa::DecryptField<'_>) -> ::std::result::Result<(), ::std::string::String> {
                #body
                Ok(())
            }
        }
    }.into())
}

fn reject_container_attributes(attributes: &[syn::Attribute]) -> syn::Result<()> {
    for attribute in attributes {
        if attribute.path().is_ident("rsa") || attribute.path().is_ident("rsa_encrypted") {
            return Err(syn::Error::new_spanned(
                attribute,
                "RSA 标记只能声明在字段上",
            ));
        }
    }
    Ok(())
}

fn field_call(
    field: &syn::Field,
    value: TokenStream2,
    runtime: &TokenStream2,
) -> syn::Result<TokenStream2> {
    let mut encrypted = false;
    let mut skip = false;
    for attr in &field.attrs {
        if attr.path().is_ident("rsa_encrypted") {
            if encrypted || !matches!(attr.meta, syn::Meta::Path(_)) {
                return Err(syn::Error::new_spanned(
                    attr,
                    "rsa_encrypted 只能声明一次且不接受参数",
                ));
            }
            encrypted = true;
        } else if attr.path().is_ident("rsa") {
            attr.parse_nested_meta(|meta| {
                if !meta.path.is_ident("skip") || skip {
                    return Err(meta.error("仅支持单个 skip 标记"));
                }
                skip = true;
                Ok(())
            })?;
            if !skip {
                return Err(syn::Error::new_spanned(attr, "rsa 必须指定 skip"));
            }
        }
    }
    if skip && encrypted {
        return Err(syn::Error::new_spanned(
            field,
            "rsa(skip) 不能与 rsa_encrypted 同时使用",
        ));
    }
    if skip {
        return Ok(quote!());
    }
    if encrypted {
        Ok(quote! { #runtime::rsa::EncryptedField::decrypt_field(#value, __rsa_decrypt)?; })
    } else {
        Ok(quote! { #runtime::rsa::RsaDecrypt::decrypt_fields(#value, __rsa_decrypt)?; })
    }
}
