use crate::shared::models::{PageListDto, default_page, default_page_size};
use common::json_helper::deserialize_i64_from_string_or_number;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// 角色分页请求参数
#[derive(Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct RolePageRequest {
    /// 页码
    #[param(minimum = 1)]
    #[serde(default = "default_page")]
    pub page: u64,

    /// 每页条数
    #[param(minimum = 1, maximum = 100)]
    #[serde(default = "default_page_size", rename = "pageSize")]
    pub page_size: u64,
}

impl crate::shared::models::PageRequest for RolePageRequest {
    fn page(&self) -> u64 {
        self.page
    }

    fn page_size(&self) -> u64 {
        self.page_size
    }
}

/// 角色查询参数
#[derive(Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct RoleIdQuery {
    /// 角色 ID
    #[serde(rename = "roleId")]
    pub role_id: i64,
}

/// 删除角色查询参数
#[derive(Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct DeleteRoleQuery {
    /// 角色 ID
    pub id: i64,
}

/// 角色信息返回模型
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleDto {
    /// 角色 ID
    pub id: String,

    /// 角色编码
    pub code: String,

    /// 角色名称
    pub name: String,

    /// 备注信息
    pub remark: Option<String>,

    /// 创建时间
    pub create_time: String,
}

/// 编辑角色请求体
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EditRoleDto {
    /// 角色编码
    pub code: String,

    /// 角色名称
    pub name: String,

    /// 备注信息
    pub remark: Option<String>,
}

/// 角色功能返回模型
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleFunctionDto {
    /// 功能 ID
    pub id: String,

    /// 功能名称
    pub name: String,

    /// 是否选中
    pub is_check: bool,

    /// 功能标记
    pub sign: String,

    /// 子模块列表
    #[schema(no_recursion)]
    pub child_list: Vec<RoleFunctionDto>,

    /// 当前模块下的功能列表
    #[schema(no_recursion)]
    pub function_list: Vec<RoleFunctionDto>,
}

/// 角色下拉选择项
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleSelectDto {
    /// 角色 ID
    pub id: String,

    /// 角色名称
    pub name: String,
}

/// 设置角色功能请求体
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetRoleFunctionDto {
    /// 功能 ID
    #[serde(rename = "functionId")]
    #[serde(deserialize_with = "deserialize_i64_from_string_or_number")]
    pub function_id: i64,

    /// 角色 ID
    #[serde(rename = "roleId")]
    #[serde(deserialize_with = "deserialize_i64_from_string_or_number")]
    pub role_id: i64,

    /// 是否选中
    #[serde(rename = "isCheck")]
    pub is_check: bool,
}

/// 角色分页返回类型
pub type RolePageListDto = PageListDto<RoleDto>;
