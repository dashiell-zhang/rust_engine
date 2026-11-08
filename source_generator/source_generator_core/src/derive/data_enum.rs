use crate::*;
use std::collections::HashSet;

pub(crate) fn expand(input: DeriveInput) -> syn::Result<TokenStream> {
    let common = match crate_name("common") {
        Ok(FoundCrate::Itself) => quote!(crate),
        Ok(FoundCrate::Name(name)) => {
            let name = format_ident!("{name}");
            quote!(::#name)
        }
        Err(error) => {
            return Err(syn::Error::new_spanned(
                &input.ident,
                format!("DataEnum 需要 common 依赖：{error}"),
            ));
        }
    };
    let Data::Enum(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "DataEnum 只支持数值枚举",
        ));
    };
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "DataEnum 不支持泛型",
        ));
    }
    let mut repr = None;
    let mut flags = false;
    for attribute in &input.attrs {
        if attribute.path().is_ident("data_enum") {
            attribute.parse_nested_meta(|meta| {
                if !meta.path.is_ident("flags") || flags {
                    return Err(meta.error("枚举类型只支持单个 data_enum(flags)"));
                }
                flags = true;
                Ok(())
            })?;
        }
        if attribute.path().is_ident("repr") {
            attribute.parse_nested_meta(|meta| {
                let is_integer = ["i8", "u8", "i16", "u16", "i32", "u32", "i64", "u64"]
                    .iter()
                    .any(|name| meta.path.is_ident(name));
                if !is_integer || repr.is_some() {
                    return Err(meta.error("DataEnum 要求单个 8/16/32/64 位整数 repr"));
                }
                repr = Some(meta.path.clone());
                Ok(())
            })?;
        }
    }
    let repr = repr.ok_or_else(|| {
        syn::Error::new_spanned(&input.ident, "DataEnum 需要显式底层类型，例如 #[repr(i32)]")
    })?;
    let mut names = HashSet::new();
    let mut members = Vec::new();
    let mut conversions = Vec::new();
    for variant in &data.variants {
        if !matches!(variant.fields, Fields::Unit) {
            return Err(syn::Error::new_spanned(
                variant,
                "DataEnum 成员不能携带字段",
            ));
        }
        let ident = &variant.ident;
        let mut name = None::<syn::LitStr>;
        let mut description = None::<syn::LitStr>;
        for attribute in variant
            .attrs
            .iter()
            .filter(|attr| attr.path().is_ident("data_enum"))
        {
            attribute.parse_nested_meta(|meta| {
                if meta.path.is_ident("name") && name.is_none() {
                    name = Some(meta.value()?.parse()?);
                } else if meta.path.is_ident("description") && description.is_none() {
                    description = Some(meta.value()?.parse()?);
                } else {
                    return Err(meta.error("未知或重复的枚举参数"));
                }
                Ok(())
            })?;
        }
        let name = name.unwrap_or_else(|| {
            let name = ident.to_string();
            syn::LitStr::new(name.strip_prefix("r#").unwrap_or(&name), ident.span())
        });
        let text = name.value();
        if text.is_empty() || text.trim() != text || text.contains(',') || !names.insert(text) {
            return Err(syn::Error::new_spanned(
                name,
                "枚举名称不能为空、重复、包含逗号或首尾空白",
            ));
        }
        let description = match description {
            Some(description) => quote!(::core::option::Option::Some(#description)),
            None => quote!(::core::option::Option::None),
        };
        members.push(quote! {
            #common::data_helper::EnumMember {
                name: #name,
                description: #description,
                value: Self::#ident as #repr,
            }
        });
        conversions.push(quote! {
            if value == Self::#ident as #repr {
                return ::core::option::Option::Some(Self::#ident);
            }
        });
    }
    let ident = &input.ident;
    Ok(quote! {
        impl #common::data_helper::DataEnum for #ident {
            type Repr = #repr;
            const MEMBERS: &'static [#common::data_helper::EnumMember<#repr>] = &[#(#members),*];
            const FLAGS: bool = #flags;

            fn to_repr(self) -> #repr { self as #repr }

            fn from_repr(value: #repr) -> ::core::option::Option<Self> {
                #(#conversions)*
                ::core::option::Option::None
            }
        }

        impl #common::data_helper::EnumProperty for #ident {
            fn column_type() -> #common::data_helper::DataColumnType {
                #common::data_helper::DataColumnType::enumeration::<Self>()
            }
            fn enum_snapshot(&self) -> #common::property_helper::PropertyResult<
                ::core::option::Option<#common::data_helper::EnumSnapshot>
            > {
                #common::data_helper::EnumProperty::enum_snapshot(
                    &#common::data_helper::EnumValue::from(*self)
                )
            }
        }
    }
    .into())
}
