use crate::*;

/// 剥离不改变表达式语义的括号与 Group 包装
pub(crate) fn strip_transparent_expr(mut expression: &Expr) -> &Expr {
    loop {
        expression = match expression {
            Expr::Paren(paren) => &paren.expr,
            Expr::Group(group) => &group.expr,
            _ => return expression,
        };
    }
}

/// 判断声明类型是否直接或嵌套包含 trait object
pub(crate) fn contains_trait_object(ty: &Type) -> bool {
    match ty {
        Type::TraitObject(_) => true,
        Type::Array(value) => contains_trait_object(&value.elem),
        Type::FnPtr(value) => {
            value
                .inputs
                .iter()
                .any(|input| contains_trait_object(&input.ty))
                || match &value.output {
                    syn::ReturnType::Default => false,
                    syn::ReturnType::Type(_, ty) => contains_trait_object(ty.as_ref()),
                }
        }
        Type::Group(value) => contains_trait_object(&value.elem),
        Type::Paren(value) => contains_trait_object(&value.elem),
        Type::Path(value) => {
            value
                .qself
                .as_ref()
                .is_some_and(|qself| contains_trait_object(&qself.ty))
                || value
                    .path
                    .segments
                    .iter()
                    .any(|segment| path_arguments_contain_trait_object(&segment.arguments))
        }
        Type::Ptr(value) => contains_trait_object(&value.elem),
        Type::Reference(value) => contains_trait_object(&value.elem),
        Type::Slice(value) => contains_trait_object(&value.elem),
        Type::Tuple(value) => value.elems.iter().any(contains_trait_object),
        _ => false,
    }
}

/// 判断路径泛型参数是否包含 trait object
pub(crate) fn path_arguments_contain_trait_object(arguments: &PathArguments) -> bool {
    match arguments {
        PathArguments::None => false,
        PathArguments::AngleBracketed(arguments) => arguments.args.iter().any(|argument| {
            matches!(argument, GenericArgument::Type(ty) if contains_trait_object(ty))
                || matches!(argument, GenericArgument::AssocType(value) if contains_trait_object(&value.ty))
        }),
        PathArguments::Parenthesized(arguments) => {
            arguments
                .inputs
                .iter()
                .any(|input| contains_trait_object(&input.ty))
                || match &arguments.output {
                    syn::ReturnType::Default => false,
                    syn::ReturnType::Type(_, ty) => contains_trait_object(ty.as_ref()),
                }
        }
    }
}

/// 判断声明类型是否直接或嵌套包含 impl Trait
pub(crate) fn contains_impl_trait(ty: &Type) -> bool {
    match ty {
        Type::ImplTrait(_) => true,
        Type::Array(value) => contains_impl_trait(&value.elem),
        Type::FnPtr(value) => {
            value
                .inputs
                .iter()
                .any(|input| contains_impl_trait(&input.ty))
                || match &value.output {
                    syn::ReturnType::Default => false,
                    syn::ReturnType::Type(_, ty) => contains_impl_trait(ty.as_ref()),
                }
        }
        Type::Group(value) => contains_impl_trait(&value.elem),
        Type::Paren(value) => contains_impl_trait(&value.elem),
        Type::Path(value) => {
            value
                .qself
                .as_ref()
                .is_some_and(|qself| contains_impl_trait(&qself.ty))
                || value
                    .path
                    .segments
                    .iter()
                    .any(|segment| path_arguments_contain_impl_trait(&segment.arguments))
        }
        Type::Ptr(value) => contains_impl_trait(&value.elem),
        Type::Reference(value) => contains_impl_trait(&value.elem),
        Type::Slice(value) => contains_impl_trait(&value.elem),
        Type::Tuple(value) => value.elems.iter().any(contains_impl_trait),
        _ => false,
    }
}

/// 判断路径泛型参数是否包含 impl Trait
pub(crate) fn path_arguments_contain_impl_trait(arguments: &PathArguments) -> bool {
    match arguments {
        PathArguments::None => false,
        PathArguments::AngleBracketed(arguments) => arguments.args.iter().any(|argument| {
            matches!(argument, GenericArgument::Type(ty) if contains_impl_trait(ty))
                || matches!(argument, GenericArgument::AssocType(value) if contains_impl_trait(&value.ty))
        }),
        PathArguments::Parenthesized(arguments) => {
            arguments
                .inputs
                .iter()
                .any(|input| contains_impl_trait(&input.ty))
                || match &arguments.output {
                    syn::ReturnType::Default => false,
                    syn::ReturnType::Type(_, ty) => contains_impl_trait(ty.as_ref()),
                }
        }
    }
}

/// 取得透明类型包装中的单段路径标识符
pub(crate) fn transparent_type_path_ident(ty: &Type) -> Option<&Ident> {
    match ty {
        Type::Group(group) => transparent_type_path_ident(&group.elem),
        Type::Paren(paren) => transparent_type_path_ident(&paren.elem),
        Type::Path(path)
            if path.qself.is_none()
                && path.path.leading_colon.is_none()
                && path.path.segments.len() == 1 =>
        {
            path.path.segments.first().map(|segment| &segment.ident)
        }
        _ => None,
    }
}

/// 取得透明表达式包装中的单段路径标识符
pub(crate) fn transparent_expr_path_ident(expression: &Expr) -> Option<&Ident> {
    let Expr::Path(path) = strip_transparent_expr(expression) else {
        return None;
    };

    if path.qself.is_some() || path.path.leading_colon.is_some() || path.path.segments.len() != 1 {
        return None;
    }

    path.path.segments.first().map(|segment| &segment.ident)
}

/// 解析并校验稳定键作用域字符串
pub(crate) fn parse_key_scope_literal(value: &Expr) -> syn::Result<syn::LitStr> {
    let Expr::Lit(expr_lit) = value else {
        return Err(syn::Error::new_spanned(
            value,
            "key_scope 必须是非空字符串字面量",
        ));
    };
    let syn::Lit::Str(value) = &expr_lit.lit else {
        return Err(syn::Error::new_spanned(
            value,
            "key_scope 必须是非空字符串字面量",
        ));
    };

    if value.value().trim().is_empty() {
        return Err(syn::Error::new_spanned(
            value,
            "key_scope 不能为空或只包含空白",
        ));
    }

    if value.value().chars().any(char::is_control) {
        return Err(syn::Error::new_spanned(
            value,
            "key_scope 不能包含换行符或其他控制字符",
        ));
    }

    Ok(value.clone())
}

/// 为没有绑定名的 impl Trait 通配符参数生成宏内部绑定
pub(crate) fn bind_wildcard_impl_trait_parameters(input: &mut ItemFn) {
    for (index, argument) in input.sig.inputs.iter_mut().enumerate() {
        let syn::FnArg::Typed(argument) = argument else {
            continue;
        };
        if !contains_impl_trait(&argument.ty) || !is_wildcard_pattern(argument.pat.as_ref()) {
            continue;
        }

        let ident = format_ident!(
            "__source_generator_impl_trait_parameter_{index}",
            span = proc_macro2::Span::mixed_site()
        );
        *argument.pat = Pat::Ident(syn::PatIdent {
            attrs: Vec::new(),
            by_ref: None,
            mutability: None,
            ident,
            subpat: None,
        });
    }
}

/// 判断参数模式是否只是透明括号包裹的通配符
pub(crate) fn is_wildcard_pattern(pattern: &Pat) -> bool {
    match pattern {
        Pat::Wild(_) => true,
        Pat::Paren(pattern) => is_wildcard_pattern(&pattern.pat),
        _ => false,
    }
}

/// 判断方法签名是否包含 self 接收者
pub(crate) fn has_receiver(sig: &syn::Signature) -> bool {
    sig.inputs
        .iter()
        .any(|input| matches!(input, syn::FnArg::Receiver(_)))
}

/// 校验 receiver 方法显式声明是否将 Self 加入稳定方法键
pub(crate) fn validate_receiver_key_strategy(
    sig: &syn::Signature,
    key_include_self: Option<bool>,
    attribute_name: &str,
) -> syn::Result<()> {
    if has_receiver(sig) {
        if key_include_self.is_none() {
            return Err(syn::Error::new_spanned(
                sig,
                format!(
                    "#[{attribute_name}] 的 receiver 方法必须显式配置 key_include_self = true 或 false"
                ),
            ));
        }
    } else if key_include_self.is_some() {
        return Err(syn::Error::new_spanned(
            sig,
            format!("无 self 接收者的 #[{attribute_name}] 函数不能配置 key_include_self"),
        ));
    }

    Ok(())
}

/// 判断类型是否直接或嵌套包含可变引用
pub(crate) fn contains_mutable_reference(ty: &Type) -> bool {
    match ty {
        Type::Reference(reference) => {
            reference.mutability.is_some() || contains_mutable_reference(&reference.elem)
        }
        Type::Array(array) => contains_mutable_reference(&array.elem),
        Type::Group(group) => contains_mutable_reference(&group.elem),
        Type::Paren(paren) => contains_mutable_reference(&paren.elem),
        Type::Ptr(pointer) => {
            matches!(pointer.mutability, syn::PointerMutability::Mut(_))
                || contains_mutable_reference(&pointer.elem)
        }
        Type::Slice(slice) => contains_mutable_reference(&slice.elem),
        Type::Tuple(tuple) => tuple.elems.iter().any(contains_mutable_reference),
        Type::Path(path) => path.path.segments.iter().any(|segment| {
            let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
                return false;
            };

            arguments.args.iter().any(|argument| match argument {
                GenericArgument::Type(ty) => contains_mutable_reference(ty),
                GenericArgument::AssocType(binding) => contains_mutable_reference(&binding.ty),
                _ => false,
            })
        }),
        _ => false,
    }
}

/// 判断 receiver 是否为不可变引用，包括 `&self` 与 `self: &Self`。
pub(crate) fn receiver_is_immutable_reference(receiver: &syn::Receiver) -> bool {
    match &receiver.kind {
        syn::ReceiverKind::Reference(_, _, mutability) => mutability.is_none(),
        syn::ReceiverKind::Typed(_, ty) => {
            matches!(ty.as_ref(), Type::Reference(reference) if reference.mutability.is_none())
        }
        syn::ReceiverKind::Value => false,
        _ => false,
    }
}

/// 判断 receiver 是否直接或通过显式类型包含可变引用。
pub(crate) fn receiver_contains_mutable_reference(receiver: &syn::Receiver) -> bool {
    receiver.mutability.is_some()
        || match &receiver.kind {
            syn::ReceiverKind::Reference(_, _, mutability) => mutability.is_some(),
            syn::ReceiverKind::Typed(_, ty) => contains_mutable_reference(ty),
            syn::ReceiverKind::Value => false,
            _ => false,
        }
}
