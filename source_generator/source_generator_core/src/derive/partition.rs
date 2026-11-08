use crate::*;

fn metadata(attrs: &[syn::Attribute], name: &str) -> syn::Result<Vec<syn::Meta>> {
    let mut values = Vec::new();
    for attr in attrs.iter().filter(|a| a.path().is_ident(name)) {
        values.extend(attr.parse_args_with(Punctuated::<syn::Meta, Token![,]>::parse_terminated)?);
    }
    Ok(values)
}

fn string_value(meta: &syn::Meta) -> syn::Result<syn::LitStr> {
    if let syn::Meta::NameValue(value) = meta
        && let Expr::Lit(expr) = &value.value
        && let syn::Lit::Str(value) = &expr.lit
    {
        return Ok(value.clone());
    }
    Err(syn::Error::new_spanned(meta, "必须使用字符串字面量"))
}

pub(crate) fn expand_partition(input: DeriveInput) -> syn::Result<TokenStream> {
    let fail = |message| syn::Error::new_spanned(&input.ident, message);
    let Data::Struct(data) = &input.data else {
        return Err(fail("分区声明只支持实体结构体"));
    };
    if !input.generics.params.is_empty() {
        return Err(fail("分区实体不能是泛型结构体"));
    }
    let mut table = None;
    let mut schema = None;
    for meta in metadata(&input.attrs, "sea_orm")? {
        if meta.path().is_ident("table_name") {
            table = Some(string_value(&meta)?);
        }
        if meta.path().is_ident("schema_name") {
            schema = Some(string_value(&meta)?);
        }
    }
    let table = table.ok_or_else(|| fail("必须通过 sea_orm(table_name) 显式声明表名"))?;
    for name in std::iter::once(&table).chain(schema.iter()) {
        if name.value().is_empty() || name.value().len() > 63 || name.value().contains('\0') {
            return Err(syn::Error::new_spanned(
                name,
                "表名和 schema 必须为 1～63 个 UTF-8 字节",
            ));
        }
    }
    let mut interval = None;
    let mut unit = None;
    for meta in metadata(&input.attrs, "partition")? {
        if meta.path().is_ident("interval") && interval.is_none() {
            if let syn::Meta::NameValue(value) = &meta
                && let Expr::Lit(expr) = &value.value
                && let syn::Lit::Int(value) = &expr.lit
            {
                let n = value.base10_parse::<u32>()?;
                if n == 0 || n > i32::MAX as u32 {
                    return Err(fail("分区周期必须为正的 i32 整数"));
                }
                interval = Some(n);
            } else {
                return Err(syn::Error::new_spanned(meta, "interval 必须为正整数字面量"));
            }
        } else if meta.path().is_ident("unit") && unit.is_none() {
            let value = string_value(&meta)?;
            if !matches!(value.value().as_str(), "Hour" | "Day" | "Month" | "Year") {
                return Err(syn::Error::new_spanned(
                    value,
                    "unit 仅支持 Hour、Day、Month、Year",
                ));
            }
            unit = Some(format_ident!("{}", value.value()));
        } else {
            return Err(syn::Error::new_spanned(meta, "未知或重复的分区参数"));
        }
    }
    let interval = interval.ok_or_else(|| fail("缺少 partition(interval)"))?;
    let unit = unit.ok_or_else(|| fail("缺少 partition(unit)"))?;
    let mut key_column = None;
    for field in &data.fields {
        let attributes = metadata(&field.attrs, "sea_orm")?;
        if !attributes.iter().any(|m| m.path().is_ident("primary_key")) {
            continue;
        }
        if key_column.is_some() {
            return Err(fail("分区实体必须只有一个主键列"));
        }
        if !matches!(&field.ty, Type::Path(path) if path.qself.is_none() && path.path.is_ident("i64"))
        {
            return Err(syn::Error::new_spanned(&field.ty, "分区主键必须为 i64"));
        }
        let preassigned = attributes.iter().any(|m| matches!(m,
            syn::Meta::NameValue(value) if value.path.is_ident("auto_increment") && matches!(&value.value,
                Expr::Lit(expr) if matches!(&expr.lit, syn::Lit::Bool(value) if !value.value))));
        if !preassigned {
            return Err(syn::Error::new_spanned(
                field,
                "分区主键必须显式 auto_increment = false，由雪花 ID 生成器预分配",
            ));
        }
        let column = attributes
            .iter()
            .find(|m| m.path().is_ident("column_name"))
            .ok_or_else(|| syn::Error::new_spanned(field, "分区键必须显式声明 column_name"))?;
        let name = string_value(column)?;
        if name.value().is_empty() || name.value().len() > 63 || name.value().contains('\0') {
            return Err(syn::Error::new_spanned(name, "分区列名无效"));
        }
        key_column = Some(name);
    }
    let key_column = key_column.ok_or_else(|| fail("分区实体缺少主键"))?;
    let core = match crate_name("repository_core") {
        Ok(FoundCrate::Itself) => quote! { crate },
        Ok(FoundCrate::Name(name)) => {
            let ident = format_ident!("{}", name);
            quote! { ::#ident }
        }
        Err(error) => return Err(fail(&format!("分区实体必须引用 repository_core: {error}"))),
    };
    let schema = match schema {
        Some(value) => quote! { Some(#value.to_string()) },
        None => quote! { None },
    };
    let ident = &input.ident;
    Ok(quote! {
        impl #core::partition::PartitionedModel for #ident {
            fn partition_definition() -> #core::partition::PartitionTableDefinition {
                #core::partition::PartitionTableDefinition {
                    schema: #schema, table: #table.to_string(), key_column: #key_column.to_string(),
                    interval: #interval, unit: #core::partition::PartitionUnit::#unit,
                }
            }
        }
    }
    .into())
}
