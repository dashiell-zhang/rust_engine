use proc_macro2::{TokenStream, TokenTree};
use quote::{format_ident, quote};
use syn::{Ident, ItemFn, LitBool, LitStr, Path, Result, Token, parse::Parse, parse::ParseStream};

/// HTTP 方法和路径只声明一次；其余文档参数交给 utoipa。
pub struct EndpointArgs {
    parts: Vec<TokenStream>,
}
impl Parse for EndpointArgs {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let mut parts = Vec::new();
        while !input.is_empty() {
            let mut part = TokenStream::new();
            let mut angles = 0_usize;
            while !input.is_empty() && (angles > 0 || !input.peek(Token![,])) {
                let token = input.parse::<TokenTree>()?;
                if let TokenTree::Punct(punctuation) = &token {
                    match punctuation.as_char() {
                        '<' => angles += 1,
                        '>' => angles = angles.saturating_sub(1),
                        _ => {}
                    }
                }
                part.extend([token]);
            }
            parts.push(part);
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }
        Ok(Self { parts })
    }
}

pub fn expand(args: EndpointArgs, item: ItemFn) -> Result<TokenStream> {
    let mut doc = Vec::new();
    let mut methods = Vec::new();
    let mut path = None;
    let mut policy = None;
    let mut permission = None;
    let mut body = None;
    let mut documented = true;
    let mut seen = std::collections::BTreeSet::new();
    for part in args.parts {
        let tokens = part.clone().into_iter().collect::<Vec<_>>();
        let name = tokens.first().map(ToString::to_string).unwrap_or_default();
        if !seen.insert(name.clone()) {
            return Err(syn::Error::new_spanned(part, "端点参数不能重复"));
        }
        let value = || tokens.iter().skip(2).cloned().collect::<TokenStream>();
        match name.as_str() {
            "get" | "post" | "put" | "patch" | "delete" | "head" | "options" | "trace" => {
                syn::parse2::<Ident>(part.clone())?;
                methods.push(format_ident!("{}", name.to_uppercase()));
            }
            "path" => {
                path = Some(syn::parse2::<LitStr>(value())?);
                doc.push(part);
            }
            "policy" => {
                policy = Some(syn::parse2::<Ident>(value())?);
            }
            "permission" => {
                permission = Some(if value().to_string() == "none" {
                    None
                } else {
                    Some(syn::parse2::<LitStr>(value())?)
                });
            }
            "json_body" => {
                body = Some(syn::parse2::<LitStr>(value())?);
            }
            "document" => {
                documented = syn::parse2::<LitBool>(value())?.value;
            }
            "security" | "method" | "context_path" | "impl_for" => {
                return Err(syn::Error::new_spanned(
                    part,
                    "方法、路径及鉴权文档由端点声明派生，不能另行覆盖",
                ));
            }
            _ => doc.push(part),
        }
    }
    let path = path.ok_or_else(|| syn::Error::new_spanned(&item.sig, "端点必须声明 path"))?;
    if !path.value().starts_with('/') || methods.is_empty() {
        return Err(syn::Error::new_spanned(
            &item.sig,
            "端点需要绝对路径及至少一个 HTTP 方法",
        ));
    }
    let policy =
        policy.ok_or_else(|| syn::Error::new_spanned(&item.sig, "端点必须显式声明 policy"))?;
    let (variant, authorized) = match policy.to_string().as_str() {
        "anonymous" => (format_ident!("Anonymous"), false),
        "signed" => (format_ident!("Signed"), false),
        "authorized" => (format_ident!("Authorized"), true),
        "authorized_signed" => (format_ident!("AuthorizedSigned"), true),
        _ => return Err(syn::Error::new_spanned(policy, "未知端点 policy")),
    };
    let permission = permission.ok_or_else(|| {
        syn::Error::new_spanned(&item.sig, "端点必须声明 permission = 备注 或 none")
    })?;
    if permission.is_some() && !authorized {
        return Err(syn::Error::new_spanned(
            &item.sig,
            "匿名/仅签名端点不能进入权限同步清单",
        ));
    }
    let permission = match permission {
        Some(value) => quote!(Some(#value)),
        None => quote!(None),
    };
    let name = &item.sig.ident;
    let factory = format_ident!("__endpoint_{}", name);
    let visibility = &item.vis;
    let bind = body.map(|body| quote!(.json_body(#body)));
    let (attribute, document) = if documented {
        let doc_methods = methods
            .iter()
            .map(|method| format_ident!("{}", method.to_string().to_lowercase()));
        let security = authorized.then(|| quote!(security(("bearer_auth" = [])),));
        (
            quote!(#[::utoipa::path(method(#(#doc_methods),*), #(#doc,)* #security)]),
            quote!({
                #[derive(::utoipa::OpenApi)]
                #[openapi(paths(#name))]
                struct Document;
                Some(<Document as ::utoipa::OpenApi>::openapi())
            }),
        )
    } else {
        (quote!(), quote!(None))
    };
    Ok(quote! {
        #attribute
        #item
        #[doc(hidden)]
        #visibility fn #factory() -> ::webapi_core::endpoint::Endpoint {
            ::webapi_core::endpoint::Endpoint::new(
                #path,
                &[#(::axum::http::Method::#methods),*],
                ::webapi_core::endpoint::EndpointPolicy::#variant,
                #permission,
                || ::webapi_core::routes::on(&[#(::axum::http::Method::#methods),*], #name) #bind,
                || #document,
            )
        }
    })
}

pub fn registration(mut path: Path) -> TokenStream {
    let last = path.segments.last_mut().expect("syn Path 非空");
    last.ident = format_ident!("__endpoint_{}", last.ident);
    quote!(#path())
}
