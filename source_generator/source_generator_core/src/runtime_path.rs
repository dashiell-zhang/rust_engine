use crate::*;

/// 解析调用方实际使用的运行时 crate 路径
pub(crate) fn runtime_crate_path() -> syn::Result<TokenStream2> {
    match crate_name("source_generator_runtime") {
        Ok(FoundCrate::Itself) => Ok(quote! { crate }),

        Ok(FoundCrate::Name(name)) => {
            let ident = Ident::new(&name, proc_macro2::Span::call_site());

            Ok(quote! { ::#ident })
        }

        Err(error) => Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            format!("无法解析 source_generator_runtime 依赖名称: {error}"),
        )),
    }
}
