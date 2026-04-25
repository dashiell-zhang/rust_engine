use crate::shared::models::{PageListDto, default_page, default_page_size};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// 友情链接分页请求参数
#[derive(Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct LinkPageRequest {
    /// 页码
    #[param(minimum = 1)]
    #[serde(default = "default_page")]
    pub page: u64,

    /// 每页条数
    #[param(minimum = 1, maximum = 100)]
    #[serde(default = "default_page_size", rename = "pageSize")]
    pub page_size: u64,

    /// 关键词
    pub keyword: Option<String>,
}

impl crate::shared::models::PageRequest for LinkPageRequest {
    fn page(&self) -> u64 {
        self.page
    }

    fn page_size(&self) -> u64 {
        self.page_size
    }
}

/// 友情链接标识参数
#[derive(Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct LinkIdQuery {
    /// 友情链接 ID
    #[serde(rename = "linkId")]
    pub link_id: i64,
}

/// 删除友情链接参数
#[derive(Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct DeleteLinkQuery {
    /// 友情链接 ID
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

    /// 备注
    pub remark: Option<String>,

    /// 创建时间
    pub create_time: String,
}

/// 编辑友情链接请求体
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EditLinkDto {
    /// 名称
    pub name: String,

    /// 链接地址
    pub url: String,

    /// 排序值
    #[serde(default)]
    pub sort: i32,

    /// 备注
    pub remark: Option<String>,
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

    /// 备注
    pub remark: Option<String>,
}
