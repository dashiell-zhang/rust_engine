use crate::*;

/// 取得必须使用 name = value 形式声明的属性参数
pub(crate) fn require_name_value<'a>(
    meta: &'a syn::Meta,
    name: &str,
) -> syn::Result<&'a MetaNameValue> {
    let syn::Meta::NameValue(value) = meta else {
        return Err(syn::Error::new_spanned(
            meta,
            format!("参数 {name} 必须使用 `{name} = ...` 形式"),
        ));
    };

    Ok(value)
}

/// 解析显式作用域共用的类型、常量和 Self 键维度
pub(crate) fn parse_scoped_key_meta(
    meta: &syn::Meta,

    name: &str,

    key_scope: &mut Option<syn::LitStr>,

    key_types: &mut Option<Vec<Type>>,

    key_consts: &mut Option<Vec<Expr>>,

    key_include_self: &mut Option<bool>,
) -> syn::Result<bool> {
    match name {
        "key_scope" => {
            if key_scope.is_some() {
                return Err(syn::Error::new_spanned(
                    meta.path(),
                    "参数 key_scope 不能重复配置",
                ));
            }

            let value = require_name_value(meta, "key_scope")?;

            *key_scope = Some(parse_key_scope_literal(&value.value)?);
        }

        "key_types" => {
            if key_types.is_some() {
                return Err(syn::Error::new_spanned(
                    meta.path(),
                    "参数 key_types 不能重复配置",
                ));
            }

            let syn::Meta::List(list) = meta else {
                return Err(syn::Error::new_spanned(
                    meta,
                    "参数 key_types 必须使用 `key_types(Type, ...)` 形式",
                ));
            };

            let values = list
                .parse_args_with(Punctuated::<Type, Token![,]>::parse_terminated)?
                .into_iter()
                .collect::<Vec<_>>();

            if values.is_empty() {
                return Err(syn::Error::new_spanned(
                    list,
                    "参数 key_types 至少需要一个类型",
                ));
            }

            *key_types = Some(values);
        }

        "key_consts" => {
            if key_consts.is_some() {
                return Err(syn::Error::new_spanned(
                    meta.path(),
                    "参数 key_consts 不能重复配置",
                ));
            }

            let syn::Meta::List(list) = meta else {
                return Err(syn::Error::new_spanned(
                    meta,
                    "参数 key_consts 必须使用 `key_consts(CONST, ...)` 形式",
                ));
            };

            let values = list
                .parse_args_with(Punctuated::<Expr, Token![,]>::parse_terminated)?
                .into_iter()
                .collect::<Vec<_>>();

            if values.is_empty() {
                return Err(syn::Error::new_spanned(
                    list,
                    "参数 key_consts 至少需要一个常量表达式",
                ));
            }

            if let Some(value) = values.iter().find(|value| !matches!(value, Expr::Path(_))) {
                return Err(syn::Error::new_spanned(
                    value,
                    "参数 key_consts 只接受 const 泛型或关联常量路径",
                ));
            }

            *key_consts = Some(values);
        }

        "key_include_self" => {
            if key_include_self.is_some() {
                return Err(syn::Error::new_spanned(
                    meta.path(),
                    "参数 key_include_self 不能重复配置",
                ));
            }

            let value = require_name_value(meta, "key_include_self")?;

            *key_include_self = Some(parse_bool_literal(&value.value, "key_include_self")?);
        }

        _ => return Ok(false),
    }

    Ok(true)
}

/// 校验稳定键必须提供显式作用域
pub(crate) fn validate_scoped_key_options(
    input: ParseStream,

    key_scope: Option<&syn::LitStr>,

    _key_types: Option<&[Type]>,

    _key_consts: Option<&[Expr]>,

    _key_include_self: Option<bool>,
) -> syn::Result<()> {
    if key_scope.is_none() {
        return Err(syn::Error::new(
            input.span(),
            "必须配置跨构建稳定且全局唯一的 key_scope",
        ));
    }

    Ok(())
}

/// 解析布尔字面量配置
pub(crate) fn parse_bool_literal(value: &Expr, name: &str) -> syn::Result<bool> {
    let Expr::Lit(expr_lit) = value else {
        return Err(syn::Error::new_spanned(
            value,
            format!("参数 {name} 必须是布尔字面量"),
        ));
    };

    let syn::Lit::Bool(value) = &expr_lit.lit else {
        return Err(syn::Error::new_spanned(
            value,
            format!("参数 {name} 必须是布尔字面量"),
        ));
    };

    Ok(value.value)
}

/// 解析 u32 整数字面量配置
pub(crate) fn parse_u32_literal(value: &Expr, name: &str) -> syn::Result<u32> {
    parse_integer_literal(value, name)
}

/// 解析 u64 整数字面量配置
pub(crate) fn parse_u64_literal(value: &Expr, name: &str) -> syn::Result<u64> {
    parse_integer_literal(value, name)
}

/// 解析指定范围内的无符号整数字面量配置
pub(crate) fn parse_integer_literal<T>(value: &Expr, name: &str) -> syn::Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    let Expr::Lit(expr_lit) = value else {
        return Err(syn::Error::new_spanned(
            value,
            format!("参数 {name} 必须是无符号整数字面量"),
        ));
    };

    let syn::Lit::Int(value) = &expr_lit.lit else {
        return Err(syn::Error::new_spanned(
            value,
            format!("参数 {name} 必须是无符号整数字面量"),
        ));
    };

    value.base10_parse::<T>().map_err(|error| {
        syn::Error::new_spanned(value, format!("参数 {name} 超出允许范围: {error}"))
    })
}
