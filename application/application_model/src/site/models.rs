use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// 站点模块对外传输模型
///
/// 同时覆盖管理端读写站点信息和展示服务器状态的场景
/// 站点信息返回模型
#[derive(Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SiteDto {
    /// 站点地址
    pub web_url: Option<String>,

    /// 管理员名称
    pub manager_name: Option<String>,

    /// 管理员地址
    pub manager_address: Option<String>,

    /// 管理员电话
    pub manager_phone: Option<String>,

    /// 管理员邮箱
    pub manager_email: Option<String>,

    /// 备案号
    pub record_number: Option<String>,

    /// SEO 标题
    pub seo_title: Option<String>,

    /// SEO 关键字
    pub seo_keywords: Option<String>,

    /// SEO 描述
    pub seo_description: Option<String>,

    /// 页脚代码
    pub foot_code: Option<String>,
}

/// 服务器信息条目返回模型
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfoDto {
    /// 信息名称
    pub name: String,

    /// 信息值
    pub value: String,
}

/// 编辑站点信息请求体
///
/// 字段全部使用可选值以支持局部更新
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EditSiteDto {
    /// 站点地址
    pub web_url: Option<String>,

    /// 管理员名称
    pub manager_name: Option<String>,

    /// 管理员地址
    pub manager_address: Option<String>,

    /// 管理员电话
    pub manager_phone: Option<String>,

    /// 管理员邮箱
    pub manager_email: Option<String>,

    /// 备案号
    pub record_number: Option<String>,

    /// SEO 标题
    pub seo_title: Option<String>,

    /// SEO 关键字
    pub seo_keywords: Option<String>,

    /// SEO 描述
    pub seo_description: Option<String>,

    /// 页脚代码
    pub foot_code: Option<String>,
}
