use crate::*;

/// 展开 StableInvocationKey 派生实现
pub(crate) fn expand_stable_invocation_key(input: DeriveInput) -> syn::Result<TokenStream> {
    let runtime = runtime_crate_path()?;

    let options = parse_invocation_key_options(&input)?;

    let scope = &options.scope;

    let type_name = &input.ident;

    validate_invocation_key_fields(&input.data)?;

    let mut invocation_generics = input.generics.clone();

    let invocation_bounds = options
        .invocation_bounds
        .unwrap_or_else(|| build_invocation_key_field_bounds(&input.data, type_name, &runtime));

    invocation_generics
        .make_where_clause()
        .predicates
        .extend(invocation_bounds);

    for parameter in input.generics.type_params() {
        let ident = &parameter.ident;

        invocation_generics
            .make_where_clause()
            .predicates
            .push(syn::parse_quote!(#ident: #runtime::StableKeyType));
    }

    let (impl_generics, type_generics, where_clause) = invocation_generics.split_for_impl();

    let generic_writes = build_invocation_key_generic_writes(&input.generics, &runtime);

    let generic_count = generic_writes.len();

    let body = match &input.data {
        Data::Struct(data) => build_struct_invocation_key_body(
            &data.fields,
            scope,
            &runtime,
            &generic_writes,
            generic_count,
        )?,

        Data::Enum(data) => {
            build_enum_invocation_key_body(data, scope, &runtime, &generic_writes, generic_count)?
        }

        Data::Union(data) => {
            return Err(syn::Error::new_spanned(
                data.union_token,
                "StableInvocationKey 不支持 union",
            ));
        }
    };

    let mut type_key_generics = input.generics.clone();

    for parameter in input.generics.type_params() {
        let ident = &parameter.ident;

        type_key_generics
            .make_where_clause()
            .predicates
            .push(syn::parse_quote!(#ident: #runtime::StableKeyType));
    }

    let (type_key_impl_generics, type_key_type_generics, type_key_where_clause) =
        type_key_generics.split_for_impl();

    let generic_type_parts = build_stable_key_type_generic_parts(&input.generics, &runtime);

    Ok(quote! {

        impl #impl_generics #runtime::StableInvocationKey for #type_name #type_generics #where_clause {

            fn write_stable_invocation_key(

                &self,

                writer: &mut #runtime::StableInvocationKeyWriter,

            ) -> ::core::result::Result<(), #runtime::InvocationKeyError> {

                #body

            }

        }


        impl #type_key_impl_generics #runtime::StableKeyType for #type_name #type_key_type_generics #type_key_where_clause {

            fn stable_key_type() -> ::std::string::String {

                #runtime::compose_stable_key_type(

                    "rust:user",

                    &[::std::string::ToString::to_string(#scope), #(#generic_type_parts),*],

                )

            }

        }

    }

    .into())
}

/// StableInvocationKey 派生配置
pub(crate) struct InvocationKeyDeriveOptions {
    /// 用户类型跨构建稳定作用域
    scope: syn::LitStr,

    /// 显式字段编码约束，配置后替换自动字段约束
    invocation_bounds: Option<Vec<syn::WherePredicate>>,
}

/// 解析用户类型显式声明的稳定键作用域和字段约束
pub(crate) fn parse_invocation_key_options(
    input: &DeriveInput,
) -> syn::Result<InvocationKeyDeriveOptions> {
    let mut scope = None;

    let mut invocation_bounds = None;

    for attribute in &input.attrs {
        if !attribute.path().is_ident("invocation_key") {
            continue;
        }

        attribute.parse_nested_meta(|meta| {
            if meta.path.is_ident("scope") {
                if scope.is_some() {
                    return Err(meta.error("参数 scope 不能重复配置"));
                }

                let value = meta.value()?.parse::<syn::LitStr>()?;

                if value.value().trim().is_empty() {
                    return Err(syn::Error::new_spanned(
                        &value,
                        "scope 不能为空或只包含空白",
                    ));
                }

                if value.value().chars().any(char::is_control) {
                    return Err(syn::Error::new_spanned(
                        &value,
                        "scope 不能包含换行符或其他控制字符",
                    ));
                }

                scope = Some(value);

                return Ok(());
            }

            if meta.path.is_ident("bound") {
                if invocation_bounds.is_some() {
                    return Err(meta.error("参数 bound 不能重复配置"));
                }

                let value = meta.value()?.parse::<syn::LitStr>()?;

                if value.value().trim().is_empty() {
                    return Err(syn::Error::new_spanned(
                        &value,
                        "bound 不能为空或只包含空白",
                    ));
                }

                let where_clause =
                    syn::parse_str::<syn::WhereClause>(&format!("where {}", value.value()))
                        .map_err(|error| {
                            syn::Error::new_spanned(&value, format!("bound 约束无法解析: {error}"))
                        })?;

                if where_clause.predicates.is_empty() {
                    return Err(syn::Error::new_spanned(
                        &value,
                        "bound 至少需要一个 where 约束",
                    ));
                }

                invocation_bounds = Some(where_clause.predicates.into_iter().collect());

                return Ok(());
            }

            Err(meta.error("未知参数，仅支持 scope / bound"))
        })?;
    }

    let scope = scope.ok_or_else(|| {
        syn::Error::new_spanned(
            &input.ident,
            "派生 StableInvocationKey 时必须配置 #[invocation_key(scope = \"...\")]",
        )
    })?;

    Ok(InvocationKeyDeriveOptions {
        scope,

        invocation_bounds,
    })
}

/// 根据非递归字段的完整声明类型生成稳定键约束
pub(crate) fn build_invocation_key_field_bounds(
    data: &Data,

    type_name: &Ident,

    runtime: &TokenStream2,
) -> Vec<syn::WherePredicate> {
    let fields = match data {
        Data::Struct(data) => data.fields.iter().collect::<Vec<_>>(),

        Data::Enum(data) => data
            .variants
            .iter()
            .flat_map(|variant| variant.fields.iter())
            .collect::<Vec<_>>(),

        Data::Union(_) => Vec::new(),
    };

    let mut seen = std::collections::BTreeSet::new();

    fields
        .into_iter()
        .filter(|field| !type_contains_unambiguous_derived_type(&field.ty, type_name))
        .filter_map(|field| {
            let ty = &field.ty;

            let key = quote! { #ty }.to_string();

            if !seen.insert(key) {
                return None;
            }

            Some(syn::parse_quote!(#ty: #runtime::StableInvocationKey))
        })
        .collect()
}

/// 判断字段是否通过无歧义路径直接递归包含当前派生类型
pub(crate) fn type_contains_unambiguous_derived_type(ty: &Type, type_name: &Ident) -> bool {
    let mut visitor = DerivedTypeUsageVisitor {
        type_name,

        found: false,
    };

    visitor.visit_type(ty);

    visitor.found
}

/// 当前派生类型递归引用访问器
pub(crate) struct DerivedTypeUsageVisitor<'a> {
    type_name: &'a Ident,

    found: bool,
}

impl<'ast> Visit<'ast> for DerivedTypeUsageVisitor<'_> {
    fn visit_type_path(&mut self, node: &'ast syn::TypePath) {
        if is_unambiguous_derived_type_path(node, self.type_name) {
            self.found = true;

            return;
        }

        visit::visit_type_path(self, node);
    }
}

/// 判断类型路径是否能够仅凭语法确认指向当前派生类型
pub(crate) fn is_unambiguous_derived_type_path(node: &syn::TypePath, type_name: &Ident) -> bool {
    if node.qself.is_some() || node.path.leading_colon.is_some() {
        return false;
    }

    let segments = &node.path.segments;

    match segments.len() {
        1 => segments[0].ident == *type_name,

        2 => segments[0].ident == "self" && segments[1].ident == *type_name,

        _ => false,
    }
}

/// 校验派生类型字段并拒绝未支持的字段配置
pub(crate) fn validate_invocation_key_fields(data: &Data) -> syn::Result<()> {
    if let Data::Enum(data) = data {
        for variant in &data.variants {
            if let Some(attribute) = variant
                .attrs
                .iter()
                .find(|attribute| attribute.path().is_ident("invocation_key"))
            {
                return Err(syn::Error::new_spanned(
                    attribute,
                    "StableInvocationKey 当前不支持枚举变体配置",
                ));
            }
        }
    }

    let fields = match data {
        Data::Struct(data) => data.fields.iter().collect::<Vec<_>>(),

        Data::Enum(data) => data
            .variants
            .iter()
            .flat_map(|variant| variant.fields.iter())
            .collect::<Vec<_>>(),

        Data::Union(_) => return Ok(()),
    };

    for field in &fields {
        if let Some(attribute) = field
            .attrs
            .iter()
            .find(|attribute| attribute.path().is_ident("invocation_key"))
        {
            return Err(syn::Error::new_spanned(
                attribute,
                "StableInvocationKey 当前不支持跳过字段，所有字段都必须进入稳定键",
            ));
        }
    }

    Ok(())
}

/// 生成泛型参数的稳定调用键维度
pub(crate) fn build_invocation_key_generic_writes(
    generics: &syn::Generics,

    runtime: &TokenStream2,
) -> Vec<TokenStream2> {
    generics
        .params
        .iter()
        .filter(|parameter| !matches!(parameter, syn::GenericParam::Lifetime(_)))
        .enumerate()
        .filter_map(|(index, parameter)| match parameter {
            syn::GenericParam::Type(parameter) => {
                let ident = &parameter.ident;

                let index = index.to_string();

                Some(quote! {

                    writer.write_node("rust:user-generic-type", 2, |writer| {

                        writer.write_scalar("rust:user-generic-index", #index)?;

                        writer.write_scalar(

                            "rust:user-generic-value",

                            &<#ident as #runtime::StableKeyType>::stable_key_type(),

                        )

                    })?;

                })
            }

            syn::GenericParam::Const(parameter) => {
                let ident = &parameter.ident;

                let index = index.to_string();

                Some(quote! {

                    writer.write_node("rust:user-generic-const", 2, |writer| {

                        writer.write_scalar("rust:user-generic-index", #index)?;

                        writer.write_scalar(

                            "rust:user-generic-value",

                            &::std::string::ToString::to_string(&#ident),

                        )

                    })?;

                })
            }

            syn::GenericParam::Lifetime(_) => None,
        })
        .collect()
}

/// 生成派生类型稳定标签中的泛型维度
pub(crate) fn build_stable_key_type_generic_parts(
    generics: &syn::Generics,

    runtime: &TokenStream2,
) -> Vec<TokenStream2> {
    generics
        .params
        .iter()
        .filter(|parameter| !matches!(parameter, syn::GenericParam::Lifetime(_)))
        .enumerate()
        .filter_map(|(index, parameter)| match parameter {
            syn::GenericParam::Type(parameter) => {
                let ident = &parameter.ident;

                Some(quote! {

                    ::std::format!(

                        "type:{}={}",

                        #index,

                        <#ident as #runtime::StableKeyType>::stable_key_type(),

                    )

                })
            }

            syn::GenericParam::Const(parameter) => {
                let ident = &parameter.ident;

                Some(quote! {

                    ::std::format!("const:{}={}", #index, #ident)

                })
            }

            syn::GenericParam::Lifetime(_) => None,
        })
        .collect()
}

/// 生成派生类型键中的泛型维度节点
pub(crate) fn build_invocation_key_generics_node(
    generic_writes: &[TokenStream2],

    generic_count: usize,
) -> TokenStream2 {
    quote! {

        writer.write_node("rust:user-generics", #generic_count, |writer| {

            #(#generic_writes)*

            ::core::result::Result::Ok(())

        })?;

    }
}

/// 生成结构体稳定键方法体
pub(crate) fn build_struct_invocation_key_body(
    fields: &Fields,

    scope: &syn::LitStr,

    runtime: &TokenStream2,

    generic_writes: &[TokenStream2],

    generic_count: usize,
) -> syn::Result<TokenStream2> {
    let field_writes = build_struct_field_writes(fields, runtime)?;

    let field_count = field_writes.len();

    let generics_node = build_invocation_key_generics_node(generic_writes, generic_count);

    Ok(quote! {

        writer.write_node("rust:user-struct", 3, |writer| {

            writer.write_scalar("rust:user-type", #scope)?;

            #generics_node

            writer.write_node("rust:user-fields", #field_count, |writer| {

                #(#field_writes)*

                ::core::result::Result::Ok(())

            })

        })

    })
}

/// 生成结构体字段稳定键写入语句
pub(crate) fn build_struct_field_writes(
    fields: &Fields,

    runtime: &TokenStream2,
) -> syn::Result<Vec<TokenStream2>> {
    fields
        .iter()
        .enumerate()
        .map(|(index, field)| {
            let (field_name, access) = if let Some(ident) = &field.ident {
                (ident.to_string(), quote! { &self.#ident })
            } else {
                let index = syn::Index::from(index);

                (index.index.to_string(), quote! { &self.#index })
            };

            Ok(quote! {

                writer.write_node("rust:user-field", 2, |writer| {

                    writer.write_scalar("rust:user-field-name", #field_name)?;

                    #runtime::StableInvocationKey::write_stable_invocation_key(#access, writer)

                })?;

            })
        })
        .collect()
}

/// 生成枚举稳定键方法体
pub(crate) fn build_enum_invocation_key_body(
    data: &syn::DataEnum,

    scope: &syn::LitStr,

    runtime: &TokenStream2,

    generic_writes: &[TokenStream2],

    generic_count: usize,
) -> syn::Result<TokenStream2> {
    let generics_node = build_invocation_key_generics_node(generic_writes, generic_count);

    let arms = data
        .variants
        .iter()
        .map(|variant| build_enum_variant_arm(variant, scope, runtime, &generics_node))
        .collect::<syn::Result<Vec<_>>>()?;

    Ok(quote! {

        match self {

            #(#arms),*

        }

    })
}

/// 生成单个枚举变体的稳定键匹配分支
pub(crate) fn build_enum_variant_arm(
    variant: &syn::Variant,

    scope: &syn::LitStr,

    runtime: &TokenStream2,

    generics_node: &TokenStream2,
) -> syn::Result<TokenStream2> {
    let variant_name = variant.ident.to_string();

    let variant_ident = &variant.ident;

    let mut field_writes = Vec::new();

    let pattern = match &variant.fields {
        Fields::Named(fields) => {
            let bindings = fields
                .named
                .iter()
                .map(|field| {
                    field
                        .ident
                        .as_ref()
                        .ok_or_else(|| syn::Error::new_spanned(field, "命名枚举字段缺少标识符"))
                })
                .collect::<syn::Result<Vec<_>>>()?;

            for binding in &bindings {
                let field_name = binding.to_string();

                field_writes.push(quote! {

                    writer.write_node("rust:user-field", 2, |writer| {

                        writer.write_scalar("rust:user-field-name", #field_name)?;

                        #runtime::StableInvocationKey::write_stable_invocation_key(#binding, writer)

                    })?;

                });
            }

            quote! { Self::#variant_ident { #(#bindings),* } }
        }

        Fields::Unnamed(fields) => {
            let bindings = (0..fields.unnamed.len())
                .map(|index| {
                    format_ident!(
                        "__source_generator_invocation_key_field_{index}",
                        span = proc_macro2::Span::mixed_site()
                    )
                })
                .collect::<Vec<_>>();

            for (index, binding) in bindings.iter().enumerate() {
                let field_name = index.to_string();

                field_writes.push(quote! {

                    writer.write_node("rust:user-field", 2, |writer| {

                        writer.write_scalar("rust:user-field-name", #field_name)?;

                        #runtime::StableInvocationKey::write_stable_invocation_key(#binding, writer)

                    })?;

                });
            }

            quote! { Self::#variant_ident(#(#bindings),*) }
        }

        Fields::Unit => quote! { Self::#variant_ident },
    };

    let field_count = field_writes.len();

    Ok(quote! {

        #pattern => writer.write_node("rust:user-enum", 4, |writer| {

            writer.write_scalar("rust:user-type", #scope)?;

            #generics_node

            writer.write_scalar("rust:user-variant", #variant_name)?;

            writer.write_node("rust:user-fields", #field_count, |writer| {

                #(#field_writes)*

                ::core::result::Result::Ok(())

            })

        })

    })
}
