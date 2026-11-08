use crate::shared::models::{PageListDto, default_page, default_page_size};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// 友情链接分页请求参数
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct LinkPageRequest {
    /// 页码
    #[param(default = default_page)]
    #[serde(default = "default_page")]
    pub page: i32,

    /// 每页条数
    #[param(minimum = 1, maximum = 100, default = default_page_size)]
    #[serde(default = "default_page_size", rename = "pageSize")]
    #[query(validate = "crate::shared::models::validate_page_size_query")]
    pub page_size: i32,
}

impl crate::shared::models::PageRequest for LinkPageRequest {
    fn page(&self) -> i32 {
        self.page
    }

    fn page_size(&self) -> i32 {
        self.page_size
    }
}

/// 友情链接标识参数
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct LinkIdQuery {
    /// 友情链接 ID
    #[serde(rename = "linkId", default)]
    pub link_id: i64,
}

/// 删除友情链接参数
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct DeleteLinkQuery {
    /// 友情链接 ID
    #[serde(default)]
    pub id: i64,
}

/// 友情链接返回模型
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LinkDto {
    /// 友情链接 ID
    pub id: String,

    /// 名称
    pub name: String,

    /// 链接地址
    pub url: String,

    /// 排序值
    pub sort: i32,

    /// 创建时间
    #[serde(serialize_with = "common::json_helper::date_time::serialize_offset_string")]
    pub create_time: String,
}

/// 编辑友情链接请求体
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::JsonModel)]
#[json(type_name = "Application.Model.Site.Link.EditLinkDto")]
pub struct EditLinkDto {
    /// 名称
    #[json(required = "名称不可以空")]
    pub name: String,

    /// 链接地址
    #[json(required = "Url不可以空")]
    pub url: String,

    /// 排序值
    #[serde(default)]
    pub sort: i32,
}

/// 友情链接分页返回模型
pub type LinkPageListDto = PageListDto<LinkDto>;

/// 客户端友情链接列表项
#[derive(Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PublicLinkDto {
    /// 友情链接 ID
    pub id: String,

    /// 名称
    pub name: String,

    /// 链接地址
    pub url: String,

    /// 排序值
    pub sort: i32,
}
