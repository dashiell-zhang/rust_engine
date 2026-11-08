use crate::*;

/// 声明必须贴近实体，遗漏/改名删除字段时在编译期报错。
pub(crate) fn expand(input: DeriveInput) -> syn::Result<TokenStream2> {
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            input,
            "SoftDelete 仅支持 SeaORM Model",
        ));
    };
    if input.ident != "Model" || !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            input,
            "SoftDelete 需要非泛型 Model",
        ));
    }
    if !data.fields.iter().any(|field| {
        field
            .ident
            .as_ref()
            .is_some_and(|name| name == "delete_time")
    }) {
        return Err(syn::Error::new_spanned(
            input,
            "SoftDelete 需要 delete_time 字段",
        ));
    }
    let core = match crate_name("repository_core") {
        Ok(FoundCrate::Itself) => quote!(crate),
        Ok(FoundCrate::Name(name)) => {
            let name = Ident::new(&name, proc_macro2::Span::call_site());
            quote!(::#name)
        }
        Err(error) => return Err(syn::Error::new_spanned(input, error.to_string())),
    };
    Ok(quote! {
        impl #core::SoftDeleteEntityExt for Entity {
            fn delete_time_col() -> Column { Column::DeleteTime }
        }
        impl Entity {
            /// 默认只查询活跃记录；历史/恢复查询必须显式选择 find_with_deleted。
            pub fn find() -> sea_orm::Select<Self> {
                <Self as #core::SoftDeleteEntityExt>::find_active()
            }
            pub fn find_by_id<V>(id: V) -> sea_orm::Select<Self>
            where V: Into<<<Self as sea_orm::EntityTrait>::PrimaryKey as sea_orm::PrimaryKeyTrait>::ValueType> {
                <Self as #core::SoftDeleteEntityExt>::find_by_id_active(id)
            }
            pub fn find_with_deleted() -> sea_orm::Select<Self> {
                <Self as sea_orm::EntityTrait>::find()
            }
            pub fn find_by_id_with_deleted<V>(id: V) -> sea_orm::Select<Self>
            where V: Into<<<Self as sea_orm::EntityTrait>::PrimaryKey as sea_orm::PrimaryKeyTrait>::ValueType> {
                <Self as sea_orm::EntityTrait>::find_by_id(id)
            }
        }
    })
}
