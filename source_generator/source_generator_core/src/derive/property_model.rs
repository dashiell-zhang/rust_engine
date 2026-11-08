use crate::*;
use std::collections::HashSet;

pub(crate) fn expand(input: DeriveInput) -> syn::Result<TokenStream> {
    expand_model(input, false)
}

pub(crate) fn expand_data_model(input: DeriveInput) -> syn::Result<TokenStream> {
    expand_model(input, true)
}

fn expand_model(input: DeriveInput, import: bool) -> syn::Result<TokenStream> {
    let common = match crate_name("common") {
        Ok(FoundCrate::Itself) => quote!(crate),
        Ok(FoundCrate::Name(name)) => {
            let name = format_ident!("{name}");
            quote!(::#name)
        }
        Err(error) => {
            return Err(syn::Error::new_spanned(
                &input.ident,
                format!("PropertyModel 需要 common 依赖：{error}"),
            ));
        }
    };
    let mut model_name = None::<syn::LitStr>;
    for attribute in input
        .attrs
        .iter()
        .filter(|attribute| attribute.path().is_ident("property"))
    {
        attribute.parse_nested_meta(|meta| {
            if !meta.path.is_ident("model_name") || model_name.is_some() {
                return Err(meta.error("模型仅支持单个 property(model_name = \"...\")"));
            }
            let value: syn::LitStr = meta.value()?.parse()?;
            if value.value().is_empty() {
                return Err(meta.error("模型名称不能为空"));
            }
            model_name = Some(value);
            Ok(())
        })?;
    }
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "PropertyModel 只支持具名字段结构体",
        ));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "PropertyModel 只支持具名字段结构体",
        ));
    };
    let mut generics = input.generics.clone();
    let mut calls = Vec::new();
    let mut readers = Vec::new();
    let mut name_setters = Vec::new();
    let mut display_setters = Vec::new();
    let mut names = HashSet::new();
    for field in &fields.named {
        let member = field.ident.as_ref().expect("具名字段");
        let mut name = None::<syn::LitStr>;
        let mut display_name = None::<syn::LitStr>;
        let mut skip = false;
        let mut value_type = false;
        let mut read_only = false;
        let mut nested = false;
        let mut enumeration = false;
        for attribute in field
            .attrs
            .iter()
            .filter(|attribute| attribute.path().is_ident("property"))
        {
            attribute.parse_nested_meta(|meta| {
                if meta.path.is_ident("name") && name.is_none() {
                    name = Some(meta.value()?.parse()?);
                } else if meta.path.is_ident("display_name") && display_name.is_none() {
                    display_name = Some(meta.value()?.parse()?);
                } else if meta.path.is_ident("skip") && !skip {
                    skip = true;
                } else if meta.path.is_ident("value_type") && !value_type {
                    value_type = true;
                } else if meta.path.is_ident("read_only") && !read_only {
                    read_only = true;
                } else if meta.path.is_ident("nested") && !nested {
                    nested = true;
                } else if meta.path.is_ident("enumeration") && !enumeration {
                    enumeration = true;
                } else {
                    return Err(meta.error("未知或重复的属性参数"));
                }
                Ok(())
            })?;
        }
        if skip {
            if name.is_some()
                || display_name.is_some()
                || value_type
                || read_only
                || nested
                || enumeration
            {
                return Err(syn::Error::new_spanned(
                    field,
                    "skip 不能与其他属性参数组合",
                ));
            }
            continue;
        }
        if !matches!(field.vis, syn::Visibility::Public(_)) {
            if name.is_some()
                || display_name.is_some()
                || value_type
                || read_only
                || nested
                || enumeration
            {
                return Err(syn::Error::new_spanned(field, "属性声明只适用于 pub 字段"));
            }
            continue;
        }
        let raw_name = member.to_string();
        let name = name.unwrap_or_else(|| {
            syn::LitStr::new(
                raw_name.strip_prefix("r#").unwrap_or(&raw_name),
                member.span(),
            )
        });
        if name.value().is_empty() || !names.insert(name.value()) {
            return Err(syn::Error::new_spanned(name, "属性名称不能为空或重复"));
        }
        let ty = &field.ty;
        if enumeration && (nested || value_type) {
            return Err(syn::Error::new_spanned(
                field,
                "enumeration 不能与 nested 或 value_type 组合",
            ));
        }
        if import {
            let setter = if read_only {
                quote!()
            } else {
                generics
                    .make_where_clause()
                    .predicates
                    .push(syn::parse_quote!(#ty: #common::data_helper::FromDataCell));
                quote! {
                    if let ::std::option::Option::Some(converted) = <#ty as #common::data_helper::FromDataCell>::from_data_cell(_value)? {
                        self.#member = converted;
                    }
                }
            };
            name_setters
                .push(quote! { #name => { #setter return ::std::result::Result::Ok(true); } });
            if let Some(display) = &display_name {
                display_setters.push(quote! {
                    if column == #display { #setter return ::std::result::Result::Ok(true); }
                });
            }
            continue;
        }
        let display = match display_name {
            Some(name) => quote!(::std::option::Option::Some(#name)),
            None => quote!(::std::option::Option::None),
        };
        let snapshot = if enumeration {
            generics
                .make_where_clause()
                .predicates
                .push(syn::parse_quote!(#ty: #common::data_helper::EnumProperty));
            quote!(#common::property_helper::Property::new_enum(#name, #display, &self.#member)?)
        } else {
            generics
                .make_where_clause()
                .predicates
                .push(syn::parse_quote!(#ty: #common::property_helper::Serialize));
            quote!(#common::property_helper::Property::new(#name, #display, &self.#member, #value_type)?)
        };
        calls.push(snapshot.clone());
        let descend = if nested {
            if !type_contains_unambiguous_derived_type(ty, &input.ident) {
                generics
                    .make_where_clause()
                    .predicates
                    .push(syn::parse_quote!(#ty: #common::property_helper::PropertyModel));
            }
            quote!(#common::property_helper::PropertyModel::read_property(&self.#member, remaining))
        } else {
            quote!(::std::result::Result::Ok(::std::option::Option::None))
        };
        readers.push(quote! {
            if #common::string_helper::eq_ordinal_ignore_case(first, #name) {
                if remaining.is_empty() {
                    return ::std::result::Result::Ok(::std::option::Option::Some(
                        #snapshot
                    ));
                }
                return #descend;
            }
        });
    }
    let ident = &input.ident;
    let model_name = model_name.unwrap_or_else(|| {
        let name = ident.to_string();
        syn::LitStr::new(name.strip_prefix("r#").unwrap_or(&name), ident.span())
    });
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    if import {
        return Ok(quote! {
            impl #impl_generics #common::data_helper::DataModel for #ident #type_generics #where_clause {
                fn set_column(&mut self, column: &str, _value: &#common::data_helper::DataCell, display_names: bool) -> #common::property_helper::PropertyResult<bool> {
                    match column { #(#name_setters)* _ => {} }
                    if display_names { #(#display_setters)* }
                    ::std::result::Result::Ok(false)
                }
            }
        }.into());
    }
    Ok(quote! {
        impl #impl_generics #common::property_helper::PropertyModel for #ident #type_generics #where_clause {
            fn model_name() -> &'static str { #model_name }
            fn properties(&self) -> #common::property_helper::PropertyResult<::std::vec::Vec<#common::property_helper::Property>> {
                ::std::result::Result::Ok(::std::vec![#(#calls),*])
            }
            fn read_property(&self, path: &[&str]) -> #common::property_helper::PropertyResult<::std::option::Option<#common::property_helper::Property>> {
                if let ::std::option::Option::Some((first, remaining)) = path.split_first() {
                    // 空模型同样可派生，不为不存在的字段生成无效读取。
                    let _ = (first, remaining);
                    #(#readers)*
                }
                ::std::result::Result::Ok(::std::option::Option::None)
            }
        }
    }.into())
}
