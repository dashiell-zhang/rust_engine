use crate::shared::models::{PageListDto, default_page, default_page_size};
use common::json_helper::deserialize_i64_from_string_or_number;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// 角色分页请求参数
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct RolePageRequest {
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

impl crate::shared::models::PageRequest for RolePageRequest {
    fn page(&self) -> i32 {
        self.page
    }

    fn page_size(&self) -> i32 {
        self.page_size
    }
}

/// 角色查询参数
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct RoleIdQuery {
    /// 角色 ID
    #[serde(rename = "roleId", default)]
    pub role_id: i64,
}

/// 删除角色查询参数
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct DeleteRoleQuery {
    /// 角色 ID
    #[serde(default)]
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
    #[serde(serialize_with = "common::json_helper::date_time::serialize_offset_string")]
    pub create_time: String,
}

/// 编辑角色请求体
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::JsonModel)]
#[json(type_name = "Application.Model.User.Role.EditRoleDto")]
pub struct EditRoleDto {
    /// 角色编码
    #[json(required = "角色编码不可以空")]
    pub code: String,

    /// 角色名称
    #[json(required = "名称不可以空")]
    pub name: String,

    /// 备注信息
    #[serde(
        default,
        deserialize_with = "common::json_helper::deserialize_optional_json_scalar"
    )]
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
    pub child_list: Option<Vec<RoleFunctionDto>>,

    /// 当前模块下的功能列表
    #[schema(no_recursion)]
    pub function_list: Option<Vec<RoleFunctionDto>>,
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
#[derive(common::json_helper::input::JsonModel)]
#[json(type_name = "Application.Model.User.Role.SetRoleFunctionDto")]
pub struct SetRoleFunctionDto {
    /// 功能 ID
    #[serde(rename = "functionId")]
    #[serde(deserialize_with = "deserialize_i64_from_string_or_number")]
    #[serde(default)]
    pub function_id: i64,

    /// 角色 ID
    #[serde(rename = "roleId")]
    #[serde(deserialize_with = "deserialize_i64_from_string_or_number")]
    #[serde(default)]
    pub role_id: i64,

    /// 是否选中
    #[serde(rename = "isCheck")]
    #[serde(default)]
    pub is_check: bool,
}

/// 角色分页返回类型
pub type RolePageListDto = PageListDto<RoleDto>;
