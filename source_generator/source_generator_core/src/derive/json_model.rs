use crate::*;

/// 正文验证元数据复用字段命名与默认值声明，不生成第二套反序列化器。
pub(crate) fn expand(input: DeriveInput) -> syn::Result<TokenStream> {
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "JsonModel 暂不支持泛型模型",
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
            "JsonModel 需要具名字段结构体",
        ));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "JsonModel 需要具名字段结构体",
        ));
    };
    let mut rename_all = None;
    let mut type_name = None;
    for attr in input
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("json"))
    {
        attr.parse_nested_meta(|meta| {
            if !meta.path.is_ident("type_name") || type_name.is_some() {
                return Err(meta.error("模型需要单个 json(type_name = \"协议类型名\")"));
            }
            type_name = Some(meta.value()?.parse::<syn::LitStr>()?.value());
            Ok(())
        })?;
    }
    let type_name = type_name.filter(|name| !name.is_empty()).ok_or_else(|| {
        syn::Error::new_spanned(
            &input.ident,
            "JsonModel 需要显式 json(type_name = \"协议类型名\")",
        )
    })?;
    for attr in input
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("serde"))
    {
        let options = attr.parse_args_with(Punctuated::<syn::Meta, Token![,]>::parse_terminated)?;
        for option in options {
            if let syn::Meta::NameValue(value) = &option
                && value.path.is_ident("rename_all")
            {
                rename_all = Some(string_value(&value.value)?);
            } else {
                return Err(syn::Error::new_spanned(
                    option,
                    "JsonModel 的模型级 Serde 配置仅支持 rename_all",
                ));
            }
        }
    }
    let mut declarations = Vec::new();
    for field in &fields.named {
        let member = field.ident.as_ref().expect("已检查具名字段");
        let mut name = rename(&member.to_string(), rename_all.as_deref())?;
        let mut aliases = Vec::new();
        let mut default = false;
        let mut deserialize_with = None::<syn::Path>;
        for attr in field
            .attrs
            .iter()
            .filter(|attr| attr.path().is_ident("serde"))
        {
            for option in
                attr.parse_args_with(Punctuated::<syn::Meta, Token![,]>::parse_terminated)?
            {
                if option.path().is_ident("flatten")
                    || option.path().is_ident("skip")
                    || option.path().is_ident("skip_deserializing")
                {
                    return Err(syn::Error::new_spanned(
                        option,
                        "JsonModel 不支持 flatten 或跳过字段",
                    ));
                }
                if option.path().is_ident("default") {
                    default = true;
                }
                if let syn::Meta::NameValue(value) = &option {
                    if value.path.is_ident("deserialize_with") {
                        deserialize_with = Some(syn::parse_str(&string_value(&value.value)?)?);
                    }
                    if value.path.is_ident("rename") {
                        name = string_value(&value.value)?;
                    }
                    if value.path.is_ident("alias") {
                        aliases.push(string_value(&value.value)?);
                    }
                }
            }
        }
        let mut property = uppercase_first(&name);
        let mut required = None::<String>;
        let mut value_type = false;
        let mut scalar_type = None::<String>;
        let mut minimum = None::<usize>;
        let mut minimum_message = None::<String>;
        let mut nested = false;
        let mut array = false;
        let mut dictionary_value_name = None::<String>;
        for attr in field
            .attrs
            .iter()
            .filter(|attr| attr.path().is_ident("json"))
        {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("required") {
                    required = Some(if meta.input.peek(Token![=]) {
                        meta.value()?.parse::<syn::LitStr>()?.value()
                    } else {
                        String::new()
                    });
                } else if meta.path.is_ident("name") {
                    property = meta.value()?.parse::<syn::LitStr>()?.value();
                } else if meta.path.is_ident("value") {
                    value_type = true;
                } else if meta.path.is_ident("scalar_type") && scalar_type.is_none() {
                    scalar_type = Some(meta.value()?.parse::<syn::LitStr>()?.value());
                } else if meta.path.is_ident("nested") {
                    nested = true;
                } else if meta.path.is_ident("array") {
                    array = true;
                } else if meta.path.is_ident("dictionary_value_name") {
                    dictionary_value_name = Some(meta.value()?.parse::<syn::LitStr>()?.value());
                } else if meta.path.is_ident("min_length") {
                    minimum = Some(meta.value()?.parse::<syn::LitInt>()?.base10_parse()?);
                } else if meta.path.is_ident("min_length_message") {
                    minimum_message = Some(meta.value()?.parse::<syn::LitStr>()?.value());
                } else {
                    return Err(meta.error("不支持的 JSON 验证声明"));
                }
                Ok(())
            })?;
        }
        let optional = inner_type(&field.ty, "Option");
        let ty = optional.unwrap_or(&field.ty);
        let collection = inner_type(ty, "Vec");
        let dictionary = dictionary_value(ty)?;
        if scalar_type.is_some()
            && (!value_type
                || deserialize_with.is_none()
                || !matches!(field.ty, Type::Path(_))
                || optional.is_some()
                || collection.is_some()
                || dictionary.is_some()
                || nested
                || scalar_type.as_ref().is_some_and(String::is_empty))
        {
            return Err(syn::Error::new_spanned(
                field,
                "scalar_type 需要非可空标量的 json(value) 和 deserialize_with 声明",
            ));
        }
        if dictionary_value_name.is_some() && dictionary.is_none() {
            return Err(syn::Error::new_spanned(
                field,
                "dictionary_value_name 仅适用于字典",
            ));
        }
        let element = collection.or(dictionary);
        if array && collection.is_none() {
            return Err(syn::Error::new_spanned(field, "json(array) 仅适用于集合"));
        }
        let element_string = element.is_some_and(|child| {
            let child = inner_type(child, "Option").unwrap_or(child);
            matches!(child, syn::Type::Path(path) if path.path.segments.last().is_some_and(|part| part.ident == "String"))
        });
        let element_kind = if element_string {
            quote!(String)
        } else if element.is_some_and(
            |child| matches!(child, syn::Type::Path(path) if path.path.is_ident("bool")),
        ) {
            quote!(Boolean)
        } else {
            quote!(Value)
        };
        let string = matches!(ty, syn::Type::Path(path) if path.path.segments.last().is_some_and(|part| part.ident == "String"));
        // 可空值的空白规则由其显式转换器决定，不套用非可空 bool 的 token 限制。
        let boolean = optional.is_none()
            && matches!(ty, syn::Type::Path(path) if path.path.segments.last().is_some_and(|part| part.ident == "bool"));
        let kind = if value_type {
            quote!(Value)
        } else if string {
            quote!(String)
        } else if boolean {
            quote!(Boolean)
        } else if dictionary.is_some() {
            quote!(Dictionary)
        } else if collection.is_some() {
            quote!(Collection)
        } else if nested {
            quote!(Object)
        } else {
            quote!(Value)
        };
        let explicit = required.is_some();
        if required.is_none()
            && optional.is_none()
            && !value_type
            && (string || collection.is_some() || dictionary.is_some() || nested)
        {
            required = Some(String::new());
        }
        let required = required
            .map(|message| quote!(Some(#message)))
            .unwrap_or_else(|| quote!(None));
        let required_on_missing = explicit || !default;
        let min_length = match (minimum, minimum_message) {
            (Some(minimum), Some(message)) => quote!(Some((#minimum, #message))),
            (None, None) => quote!(None),
            _ => {
                return Err(syn::Error::new_spanned(
                    field,
                    "min_length 与 min_length_message 必须同时声明",
                ));
            }
        };
        let field_type = if let Some(name) = dictionary_value_name {
            quote!(#name.to_owned())
        } else if nested {
            let child = element.unwrap_or(ty);
            let child = inner_type(child, "Option").unwrap_or(child);
            quote!(<#child as #common::json_helper::input::JsonModel>::json_type_name().to_owned())
        } else if string {
            quote!("System.String".to_owned())
        } else if let Some(child) = element {
            let child = inner_type(child, "Option").unwrap_or(child);
            let name = match child {
                syn::Type::Path(path) => path
                    .path
                    .segments
                    .last()
                    .map(|segment| segment.ident.to_string()),
                _ => None,
            };
            let name = match name.as_deref() {
                Some("i64") => "System.Int64",
                Some("i32") => "System.Int32",
                Some("String") => "System.String",
                Some("bool") => "System.Boolean",
                Some("f32") => "System.Single",
                Some("f64") => "System.Double",
                Some("Value" | "RawValue") => "System.Text.Json.JsonElement",
                _ => {
                    return Err(syn::Error::new_spanned(
                        child,
                        "集合或字典元素需要 json(nested) 或已支持的标量类型",
                    ));
                }
            };
            quote!(#name.to_owned())
        } else {
            quote!(::std::string::String::new())
        };
        let field_type = if array {
            quote!(Some(|| format!("{}[]", #field_type)))
        } else if dictionary.is_some() {
            quote!(Some(
                || format!("System.Collections.Generic.Dictionary`2[System.String,{}]", #field_type)
            ))
        } else if collection.is_some() {
            quote!(Some(
                || format!("System.Collections.Generic.List`1[{}]", #field_type)
            ))
        } else if nested || string {
            quote!(Some(|| #field_type))
        } else {
            quote!(None)
        };
        let nested = if nested {
            let child = element.unwrap_or(ty);
            let child = inner_type(child, "Option").unwrap_or(child);
            quote!(Some(|| #common::json_helper::input::JsonObject {
                name: <#child as #common::json_helper::input::JsonModel>::json_type_name(),
                fields: <#child as #common::json_helper::input::JsonModel>::json_fields(),
            }))
        } else {
            quote!(None)
        };
        let scalar = scalar_declaration(
            &common,
            &field.ty,
            deserialize_with.as_ref(),
            scalar_type.as_deref(),
        );
        let element_scalar = element
            .map(|child| scalar_declaration(&common, child, None, None))
            .unwrap_or_else(|| quote!(None));
        declarations.push(quote! {
            #common::json_helper::input::JsonField {
                names: &[#name, #(#aliases),*], property: #property,
                kind: #common::json_helper::input::JsonFieldKind::#kind,
                required: #required, required_on_missing: #required_on_missing,
                min_length: #min_length, nested: #nested, type_name: #field_type,
                element_kind: #common::json_helper::input::JsonFieldKind::#element_kind,
                scalar: #scalar, element_scalar: #element_scalar,
            }
        });
    }
    let name = &input.ident;
    Ok(quote! {
        impl #common::json_helper::input::JsonModel for #name {
            fn json_type_name() -> &'static str { #type_name }
            fn json_fields() -> &'static [#common::json_helper::input::JsonField] {
                &[#(#declarations),*]
            }
        }
    }
    .into())
}

fn scalar_declaration(
    common: &TokenStream2,
    ty: &Type,
    deserialize_with: Option<&syn::Path>,
    scalar_type: Option<&str>,
) -> TokenStream2 {
    let nullable = inner_type(ty, "Option");
    if nullable.is_some()
        && !deserialize_with.is_some_and(|path| {
            path.segments.last().is_some_and(|part| {
                part.ident == "deserialize_optional_json_scalar"
                    || part.ident == "deserialize_option_i64_from_string_or_number"
            })
        })
    {
        return quote!(None);
    }
    let Type::Path(path) = nullable.unwrap_or(ty) else {
        return quote!(None);
    };
    let name = path.path.segments.last().map(|part| part.ident.to_string());
    let type_name = if let Some(type_name) = scalar_type {
        type_name
    } else {
        match name.as_deref() {
            Some("i8") => "System.SByte",
            Some("u8") => "System.Byte",
            Some("i16") => "System.Int16",
            Some("u16") => "System.UInt16",
            Some("i32") => "System.Int32",
            Some("u32") => "System.UInt32",
            Some("i64") => "System.Int64",
            Some("u64") => "System.UInt64",
            Some("f32") => "System.Single",
            Some("f64") => "System.Double",
            Some("bool") => "System.Boolean",
            _ => return quote!(None),
        }
    };
    let read = deserialize_with
        .map(|path| quote!(#path(value.into_deserializer())))
        .unwrap_or_else(
            || quote!(<#ty as ::serde::Deserialize>::deserialize(value.into_deserializer())),
        );
    let scoped = nullable.is_some();
    quote!(Some(#common::json_helper::input::JsonScalarField {
        type_name: #type_name,
        scoped: #scoped,
        read: |value| {
            let result: ::std::result::Result<#ty, _> = #read;
            result.map(|_| ())
        },
    }))
}

fn string_value(value: &syn::Expr) -> syn::Result<String> {
    if let syn::Expr::Lit(value) = value
        && let syn::Lit::Str(value) = &value.lit
    {
        return Ok(value.value());
    }
    Err(syn::Error::new_spanned(value, "需要字符串名称"))
}

fn uppercase_first(value: &str) -> String {
    let mut chars = value.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

pub(super) fn rename(name: &str, rule: Option<&str>) -> syn::Result<String> {
    match rule {
        None | Some("snake_case") => Ok(name.to_owned()),
        Some("camelCase" | "PascalCase") => {
            let mut parts = name.split('_');
            let first = parts.next().unwrap_or_default();
            let mut result = if rule == Some("PascalCase") {
                uppercase_first(first)
            } else {
                first.to_owned()
            };
            for part in parts {
                result.push_str(&uppercase_first(part));
            }
            Ok(result)
        }
        _ => Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "JsonModel 不支持该 rename_all 规则",
        )),
    }
}

fn inner_type<'a>(ty: &'a syn::Type, name: &str) -> Option<&'a syn::Type> {
    let syn::Type::Path(path) = ty else {
        return None;
    };
    let segment = path.path.segments.last()?;
    if segment.ident != name {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };
    args.args.iter().find_map(|arg| {
        if let syn::GenericArgument::Type(ty) = arg {
            Some(ty)
        } else {
            None
        }
    })
}

fn dictionary_value(ty: &syn::Type) -> syn::Result<Option<&syn::Type>> {
    let syn::Type::Path(path) = ty else {
        return Ok(None);
    };
    let Some(segment) = path.path.segments.last() else {
        return Ok(None);
    };
    if !matches!(
        segment.ident.to_string().as_str(),
        "HashMap" | "BTreeMap" | "IndexMap" | "Map"
    ) {
        return Ok(None);
    }
    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
        return Ok(None);
    };
    let mut types = args.args.iter().filter_map(|arg| match arg {
        syn::GenericArgument::Type(ty) => Some(ty),
        _ => None,
    });
    let key = types.next();
    if !matches!(key, Some(syn::Type::Path(path)) if path.path.segments.last().is_some_and(|part| part.ident == "String"))
    {
        return Err(syn::Error::new_spanned(
            ty,
            "JsonModel 字典目前只支持 String 键",
        ));
    }
    Ok(types.next())
}
