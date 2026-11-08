use quote::quote;
use syn::{Attribute, Expr, Fields, ItemStruct, Lit, Meta, Token, punctuated::Punctuated};

fn options(attrs: &[Attribute]) -> syn::Result<Vec<Meta>> {
    let mut options = Vec::new();
    for attr in attrs.iter().filter(|attr| attr.path().is_ident("sea_orm")) {
        options.extend(attr.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)?);
    }
    Ok(options)
}

fn string_option(options: &[Meta], name: &str) -> Option<String> {
    options.iter().find_map(|option| match option {
        Meta::NameValue(value) if value.path.is_ident(name) => match &value.value {
            Expr::Lit(value) => match &value.lit {
                Lit::Str(value) => Some(value.value()),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    })
}

fn add_comment(attrs: &mut Vec<Attribute>, options: &[Meta], fallback: &str) -> syn::Result<()> {
    // 显式 SeaORM 注释优先；文档与数据库注释不需要重复维护。
    if options
        .iter()
        .any(|option| option.path().is_ident("comment"))
    {
        return Ok(());
    }
    let mut lines = Vec::new();
    for attr in attrs.iter().filter(|attr| attr.path().is_ident("doc")) {
        if let Meta::NameValue(value) = &attr.meta
            && let Expr::Lit(value) = &value.value
            && let Lit::Str(value) = &value.lit
        {
            lines.push(value.value().trim().to_owned());
        } else {
            return Err(syn::Error::new_spanned(
                attr,
                "模型文档必须使用字符串字面量",
            ));
        }
    }
    let documentation = lines.join("\n");
    let comment = if documentation.trim().is_empty() {
        fallback
    } else {
        documentation.trim()
    };
    attrs.push(syn::parse_quote!(#[sea_orm(comment = #comment)]));
    Ok(())
}

pub(crate) fn expand(mut item: ItemStruct) -> syn::Result<proc_macro2::TokenStream> {
    let options = options(&item.attrs)?;
    let table = string_option(&options, "table_name").ok_or_else(|| {
        syn::Error::new_spanned(&item.ident, "模型文档需要显式 sea_orm(table_name)")
    })?;
    add_comment(&mut item.attrs, &options, &table)?;
    let Fields::Named(fields) = &mut item.fields else {
        return Err(syn::Error::new_spanned(&item, "模型文档只支持命名字段实体"));
    };
    for field in &mut fields.named {
        let options = self::options(&field.attrs)?;
        let column = string_option(&options, "column_name")
            .unwrap_or_else(|| field.ident.as_ref().expect("命名字段").to_string());
        // xmin 是系统列，ignore 字段不属于数据库模型。
        if column == "xmin"
            || options
                .iter()
                .any(|option| option.path().is_ident("ignore"))
        {
            continue;
        }
        add_comment(&mut field.attrs, &options, &column)?;
    }
    Ok(quote!(#item))
}
