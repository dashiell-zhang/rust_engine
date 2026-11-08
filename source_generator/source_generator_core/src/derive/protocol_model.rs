use crate::*;

/// 协议 DTO 按输入顺序读取每次属性赋值；成功的重复赋值取最后一次。
pub(crate) fn expand(input: DeriveInput) -> syn::Result<TokenStream> {
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "ProtocolModel 暂不支持泛型模型",
        ));
    }
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "ProtocolModel 需要具名字段结构体",
        ));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "ProtocolModel 需要具名字段结构体",
        ));
    };
    let mut rename_all = None;
    let mut model_default = false;
    let mut case_sensitive = false;
    for attr in input
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("protocol"))
    {
        attr.parse_nested_meta(|meta| {
            if !meta.path.is_ident("case_sensitive") || case_sensitive {
                return Err(meta.error("ProtocolModel 仅支持单个 case_sensitive 标记"));
            }
            case_sensitive = true;
            Ok(())
        })?;
    }
    for attr in input
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("serde"))
    {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("default") && !meta.input.peek(Token![=]) && !model_default {
                model_default = true;
                return Ok(());
            }
            if !meta.path.is_ident("rename_all") || rename_all.is_some() {
                return Err(
                    meta.error("ProtocolModel 的模型级配置仅支持 rename_all 和无参数 default")
                );
            }
            let rule = meta.value()?.parse::<syn::LitStr>()?;
            if !matches!(
                rule.value().as_str(),
                "snake_case" | "camelCase" | "PascalCase"
            ) {
                return Err(syn::Error::new_spanned(rule, "不支持该协议字段命名规则"));
            }
            rename_all = Some(rule.value());
            Ok(())
        })?;
    }
    let mut names = std::collections::BTreeSet::new();
    let mut field_names = Vec::new();
    let mut declarations = Vec::new();
    let mut assignments = Vec::new();
    let mut members = Vec::new();
    for field in &fields.named {
        let member = field.ident.as_ref().expect("已检查具名字段");
        let ty = &field.ty;
        let slot = format_ident!(
            "__protocol_value_{}",
            member.to_string().trim_start_matches("r#")
        );
        let mut name = super::json_model::rename(
            member.to_string().trim_start_matches("r#"),
            rename_all.as_deref(),
        )?;
        let mut default = model_default;
        for attr in field
            .attrs
            .iter()
            .filter(|attr| attr.path().is_ident("serde"))
        {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("rename") {
                    name = meta.value()?.parse::<syn::LitStr>()?.value();
                } else if meta.path.is_ident("default") && !meta.input.peek(Token![=]) {
                    default = true;
                } else {
                    return Err(meta.error("ProtocolModel 字段仅支持 rename 和无参数 default"));
                }
                Ok(())
            })?;
        }
        if !names.insert(if case_sensitive {
            name.clone()
        } else {
            name.to_ascii_lowercase()
        }) {
            return Err(syn::Error::new_spanned(
                field,
                "协议字段名称在所选大小写规则下不能重复",
            ));
        }
        let optional = matches!(ty, Type::Path(path) if path.path.segments.last().is_some_and(|part| part.ident == "Option"));
        declarations
            .push(quote!(let mut #slot: ::std::option::Option<#ty> = ::std::option::Option::None;));
        let matches = if case_sensitive {
            quote!(__protocol_key == #name)
        } else {
            quote!(__protocol_key.eq_ignore_ascii_case(#name))
        };
        assignments.push(quote! {
            if #matches {
                #slot = ::std::option::Option::Some(__protocol_map.next_value::<#ty>()?);
                continue;
            }
        });
        let value = if optional || default {
            quote!(#slot.unwrap_or_default())
        } else {
            quote!(#slot.ok_or_else(|| <A::Error as ::serde::de::Error>::missing_field(#name))?)
        };
        members.push(quote!(#member: #value));
        field_names.push(name);
    }
    let name = &input.ident;
    // 仅通知公共输入适配器保留每次赋值，不改变 JSON 对象的实际字段
    let marker = if case_sensitive {
        "$protocol:case_sensitive:"
    } else {
        "$protocol:"
    };
    Ok(quote! {
        impl<'__protocol_de> ::serde::Deserialize<'__protocol_de> for #name {
            fn deserialize<D: ::serde::Deserializer<'__protocol_de>>(__protocol_deserializer: D)
                -> ::std::result::Result<Self, D::Error> {
                struct ProtocolVisitor;
                impl<'__protocol_de> ::serde::de::Visitor<'__protocol_de> for ProtocolVisitor {
                    type Value = #name;
                    fn expecting(&self, formatter: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                        formatter.write_str("协议 JSON 对象")
                    }
                    fn visit_map<A: ::serde::de::MapAccess<'__protocol_de>>(self, mut __protocol_map: A)
                        -> ::std::result::Result<Self::Value, A::Error> {
                        #(#declarations)*
                        while let ::std::option::Option::Some(__protocol_key) = __protocol_map.next_key::<::std::string::String>()? {
                            #(#assignments)*
                            let _: ::serde::de::IgnoredAny = __protocol_map.next_value()?;
                        }
                        ::std::result::Result::Ok(#name { #(#members),* })
                    }
                }
                __protocol_deserializer.deserialize_struct(concat!(#marker, stringify!(#name)), &[#(#field_names),*], ProtocolVisitor)
            }
        }
    }.into())
}
