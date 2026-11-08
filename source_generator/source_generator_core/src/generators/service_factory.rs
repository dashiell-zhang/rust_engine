use crate::*;

/// 服务工厂生成输入
pub(crate) struct ServiceFactoryInput {
    /// impl 泛型参数，省略时保持旧版 `<'a, State>` 兼容行为
    impl_generics: Option<syn::Generics>,

    /// 工厂类型
    factory_type: Type,

    /// 状态类型标识符
    state_type: Ident,

    /// 服务分组
    groups: Vec<ServiceFactoryGroup>,
}

/// 直接服务工厂生成输入
pub(crate) struct DirectServiceFactoryInput {
    /// impl 泛型参数
    impl_generics: syn::Generics,

    /// 工厂类型
    factory_type: Type,

    /// 服务分组
    groups: Vec<DirectServiceFactoryGroup>,
}

/// 直接服务工厂分组
pub(crate) struct DirectServiceFactoryGroup {
    /// 构建服务时使用的上下文表达式
    context_expr: Expr,

    /// 当前分组下的服务清单
    registrations: Vec<ServiceFactoryRegistration>,
}

/// 单个服务分组
pub(crate) struct ServiceFactoryGroup {
    /// 访问该分组所需的上下文提供器约束
    provider_trait: Path,

    /// 构建服务时使用的上下文表达式
    context_expr: Expr,

    /// 当前分组下的服务清单
    registrations: Vec<ServiceFactoryRegistration>,
}

/// 单个服务注册项
pub(crate) struct ServiceFactoryRegistration {
    /// 工厂方法名
    method_name: Ident,

    /// 返回的服务类型
    service_type: Type,

    /// 对调用方公开的动态服务契约，省略时返回实现类型。
    contract_type: Option<Type>,
}

impl Parse for ServiceFactoryInput {
    /// 解析服务工厂生成配置
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let impl_generics =
            if input.peek(Ident) && input.fork().parse::<Ident>()? == "impl_generics" {
                input.parse::<Ident>()?;
                input.parse::<Token![=]>()?;
                let mut impl_generics = input.parse::<syn::Generics>()?;
                if input.peek(Token![where]) {
                    impl_generics.where_clause = Some(input.parse::<syn::WhereClause>()?);
                }
                input.parse::<Token![;]>()?;
                Some(impl_generics)
            } else {
                None
            };

        let factory_label = input.parse::<Ident>()?;
        if factory_label != "factory" {
            return Err(syn::Error::new(factory_label.span(), "期望 factory"));
        }
        input.parse::<Token![=]>()?;
        let factory_type = input.parse::<Type>()?;
        input.parse::<Token![;]>()?;

        let state_label = input.parse::<Ident>()?;
        if state_label != "state" {
            return Err(syn::Error::new(state_label.span(), "期望 state"));
        }
        input.parse::<Token![=]>()?;
        let state_type = input.parse::<Ident>()?;
        input.parse::<Token![;]>()?;

        let mut groups = Vec::new();
        while !input.is_empty() {
            groups.push(input.parse::<ServiceFactoryGroup>()?);
        }

        Ok(Self {
            impl_generics,
            factory_type,
            state_type,
            groups,
        })
    }
}

impl Parse for DirectServiceFactoryInput {
    /// 解析直接服务工厂生成配置
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let impl_generics_label = input.parse::<Ident>()?;
        if impl_generics_label != "impl_generics" {
            return Err(syn::Error::new(
                impl_generics_label.span(),
                "期望 impl_generics",
            ));
        }
        input.parse::<Token![=]>()?;
        let mut impl_generics = input.parse::<syn::Generics>()?;
        if input.peek(Token![where]) {
            impl_generics.where_clause = Some(input.parse::<syn::WhereClause>()?);
        }
        input.parse::<Token![;]>()?;

        let factory_label = input.parse::<Ident>()?;
        if factory_label != "factory" {
            return Err(syn::Error::new(factory_label.span(), "期望 factory"));
        }
        input.parse::<Token![=]>()?;
        let factory_type = input.parse::<Type>()?;
        input.parse::<Token![;]>()?;

        let mut groups = Vec::new();
        while !input.is_empty() {
            groups.push(input.parse::<DirectServiceFactoryGroup>()?);
        }

        Ok(Self {
            impl_generics,
            factory_type,
            groups,
        })
    }
}

impl Parse for ServiceFactoryGroup {
    /// 解析单个上下文分组
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let provider_trait = input.parse::<Path>()?;
        input.parse::<Token![=>]>()?;

        let context_content;
        parenthesized!(context_content in input);
        let context_expr = context_content.parse::<Expr>()?;

        let registrations_content;
        braced!(registrations_content in input);
        let registrations = Punctuated::<ServiceFactoryRegistration, Token![,]>::parse_terminated(
            &registrations_content,
        )?
        .into_iter()
        .collect::<Vec<_>>();

        Ok(Self {
            provider_trait,
            context_expr,
            registrations,
        })
    }
}

impl Parse for DirectServiceFactoryGroup {
    /// 解析直接服务工厂分组
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let context_content;
        parenthesized!(context_content in input);
        let context_expr = context_content.parse::<Expr>()?;

        let registrations_content;
        braced!(registrations_content in input);
        let registrations = Punctuated::<ServiceFactoryRegistration, Token![,]>::parse_terminated(
            &registrations_content,
        )?
        .into_iter()
        .collect::<Vec<_>>();

        Ok(Self {
            context_expr,
            registrations,
        })
    }
}

impl Parse for ServiceFactoryRegistration {
    /// 解析单个服务注册项
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let method_name = input.parse::<Ident>()?;
        input.parse::<Token![:]>()?;

        let service_type = input.parse::<Type>()?;
        let contract_type = if input.peek(Token![=>]) {
            input.parse::<Token![=>]>()?;
            let contract = input.parse::<Type>()?;
            if !matches!(contract, Type::TraitObject(_)) {
                return Err(syn::Error::new_spanned(
                    contract,
                    "服务契约应为 dyn Trait，可显式声明借用生命周期",
                ));
            }
            Some(contract)
        } else {
            None
        };
        Ok(Self {
            method_name,
            service_type,
            contract_type,
        })
    }
}

/// 展开服务工厂生成配置
pub(crate) fn expand_service_factory(input: ServiceFactoryInput) -> syn::Result<TokenStream> {
    let factory_type = input.factory_type;
    let state_type = input.state_type;
    let impl_generics = input
        .impl_generics
        .unwrap_or_else(|| syn::parse_quote!(<'a, #state_type>));

    let impl_blocks = input.groups.into_iter().map(|group| {
        let provider_trait = group.provider_trait;
        let context_expr = group.context_expr;
        let mut group_generics = impl_generics.clone();
        group_generics
            .make_where_clause()
            .predicates
            .push(syn::parse_quote!(#state_type: #provider_trait));
        let (impl_generics, _, where_clause) = group_generics.split_for_impl();
        let registrations = group.registrations.into_iter().map(|registration| {
            let method_name = registration.method_name;
            let service_type = registration.service_type;
            let (return_type, constructor) =
                service_factory_return(&service_type, registration.contract_type);
            let context_expr = context_expr.clone();
            let doc_text = format!("创建 {} 应用服务", method_name);

            quote! {
                #[doc = #doc_text]
                pub fn #method_name(&self) -> #return_type {
                    #constructor(<#service_type>::new(#context_expr))
                }
            }
        });

        quote! {
            impl #impl_generics #factory_type #where_clause {
                #(#registrations)*
            }
        }
    });

    Ok(quote! {
        #(#impl_blocks)*
    }
    .into())
}

/// 展开直接服务工厂生成配置
pub(crate) fn expand_direct_service_factory(
    input: DirectServiceFactoryInput,
) -> syn::Result<TokenStream> {
    let impl_generics = input.impl_generics;
    let (impl_generics, _, where_clause) = impl_generics.split_for_impl();
    let factory_type = input.factory_type;

    let methods = input.groups.into_iter().flat_map(|group| {
        let context_expr = group.context_expr;
        group.registrations.into_iter().map(move |registration| {
            let method_name = registration.method_name;
            let service_type = registration.service_type;
            let (return_type, constructor) =
                service_factory_return(&service_type, registration.contract_type);
            let context_expr = context_expr.clone();
            let doc_text = format!("创建 {} 应用服务", method_name);

            quote! {
                #[doc = #doc_text]
                pub fn #method_name(&self) -> #return_type {
                    #constructor(<#service_type>::new(#context_expr))
                }
            }
        })
    });

    Ok(quote! {
        impl #impl_generics #factory_type #where_clause {
            #(#methods)*
        }
    }
    .into())
}

fn service_factory_return(
    service: &Type,
    contract: Option<Type>,
) -> (proc_macro2::TokenStream, proc_macro2::TokenStream) {
    match contract {
        Some(contract) => (
            quote!(::std::boxed::Box<#contract>),
            quote!(::std::boxed::Box::new),
        ),
        None => (quote!(#service), quote!(::std::convert::identity)),
    }
}
