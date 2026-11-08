use common::json_helper::deserialize_i64_from_string_or_number;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

pub use crate::shared::models::{PageListDto, default_page, default_page_size};

/// 用户列表分页请求参数
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct UserPageRequest {
    /// 页码，默认 1
    #[param(default = default_page)]
    #[serde(default = "default_page")]
    pub page: i32,

    /// 每页条数
    #[param(minimum = 1, maximum = 100, default = default_page_size)]
    #[serde(default = "default_page_size")]
    #[query(validate = "crate::shared::models::validate_page_size_query")]
    pub page_size: i32,

    /// 关键词，支持按名称、用户名、手机、邮箱搜索
    #[serde(
        default,
        deserialize_with = "common::json_helper::deserialize_optional_query_scalar"
    )]
    pub keyword: Option<String>,
}

impl crate::shared::models::PageRequest for UserPageRequest {
    fn page(&self) -> i32 {
        self.page
    }

    fn page_size(&self) -> i32 {
        self.page_size
    }
}

/// 用户信息返回模型
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserDto {
    /// 用户 ID
    pub id: String,

    /// 姓名
    pub name: String,

    /// 用户名
    pub user_name: String,

    /// 手机号
    pub phone: String,

    /// 邮箱
    pub email: Option<String>,

    /// 角色编码列表
    pub roles: Option<String>,

    /// 角色 ID 列表
    pub role_ids: Option<Vec<String>>,

    /// 创建时间
    #[serde(serialize_with = "common::json_helper::date_time::serialize_offset")]
    pub create_time: chrono::DateTime<chrono::FixedOffset>,
}

/// 创建或编辑用户请求体
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::JsonModel)]
#[json(type_name = "Application.Model.User.User.EditUserDto")]
pub struct EditUserDto {
    /// 姓名
    #[json(required = "名称不可以空")]
    pub name: String,

    /// 用户名
    #[json(required = "用户名不可以空")]
    pub user_name: String,

    /// 手机号
    #[json(required = "手机号不可以空")]
    pub phone: String,

    /// 邮箱
    #[serde(
        default,
        deserialize_with = "common::json_helper::deserialize_optional_json_scalar"
    )]
    pub email: Option<String>,

    /// 密码
    #[json(required = "密码不可以为空")]
    pub password: String,

    /// 角色 ID 列表
    #[serde(default, deserialize_with = "deserialize_role_ids")]
    #[schema(min_items = 1)]
    #[json(min_length = 1, min_length_message = "至少选择一个角色")]
    #[json(array)]
    pub role_ids: Vec<Option<String>>,
}

/// 元素允许空值，由应用服务在用户名查重之后解析；数组本身仍为必填集合。
fn deserialize_role_ids<'de, D>(deserializer: D) -> Result<Vec<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Vec::<serde_json::Value>::deserialize(deserializer)?
        .into_iter()
        .map(|value| match value {
            serde_json::Value::Null => Ok(None),
            serde_json::Value::String(value) => {
                if value.trim().is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(value))
                }
            }
            _ => Err(serde::de::Error::custom("角色 ID 应提供字符串或 null")),
        })
        .collect()
}

/// 查询单个用户的参数
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct UserIdQuery {
    /// 用户 ID
    #[serde(
        default,
        deserialize_with = "common::json_helper::deserialize_optional_query_scalar"
    )]
    pub user_id: Option<i64>,
}

/// 更新用户的查询参数
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct UpdateUserQuery {
    /// 目标用户 ID
    #[serde(default)]
    pub user_id: i64,
}

/// 删除用户的查询参数
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct DeleteUserQuery {
    /// 目标用户 ID
    #[serde(default)]
    pub id: i64,
}

/// 用户资料详情返回模型
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserProfileDto {
    /// 用户 ID
    pub user_id: String,

    /// 用户资料记录 ID
    pub id: Option<String>,

    /// 区县 ID
    pub region_area_id: Option<i32>,

    /// 省份 ID
    pub region_province_id: Option<i32>,

    /// 城市 ID
    pub region_city_id: Option<i32>,

    /// 省份名称
    pub region_province: Option<String>,

    /// 城市名称
    pub region_city: Option<String>,

    /// 区县名称
    pub region_area: Option<String>,

    /// 详细地址
    pub address: Option<String>,

    /// 性别
    pub sex: Option<bool>,

    /// 创建时间
    pub create_time: Option<String>,

    /// 更新时间
    pub update_time: Option<String>,
}

/// 编辑用户资料请求模型
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EditUserProfileDto {
    /// 区县 ID
    pub region_area_id: Option<i32>,

    /// 详细地址
    pub address: Option<String>,

    /// 性别
    pub sex: Option<bool>,
}

/// 用户角色返回模型
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserRoleDto {
    /// 角色 ID
    pub id: String,

    /// 角色名称
    pub name: String,

    /// 备注信息
    pub remark: Option<String>,

    /// 是否选中
    pub is_check: bool,
}

/// 设置用户角色请求模型
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::JsonModel)]
#[json(type_name = "Application.Model.User.User.SetUserRoleDto")]
pub struct SetUserRoleDto {
    /// 角色 ID
    #[serde(rename = "roleId")]
    #[serde(deserialize_with = "deserialize_i64_from_string_or_number")]
    #[serde(default)]
    pub role_id: i64,

    /// 用户 ID
    #[serde(rename = "userId")]
    #[serde(deserialize_with = "deserialize_i64_from_string_or_number")]
    #[serde(default)]
    pub user_id: i64,

    /// 是否选中
    #[serde(rename = "isCheck")]
    #[serde(default)]
    pub is_check: bool,
}

/// 用户功能返回模型
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserFunctionDto {
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
    pub child_list: Option<Vec<UserFunctionDto>>,

    /// 当前模块下的功能列表
    #[schema(no_recursion)]
    pub function_list: Option<Vec<UserFunctionDto>>,
}

/// 设置用户功能请求模型
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::JsonModel)]
#[json(type_name = "Application.Model.User.User.SetUserFunctionDto")]
pub struct SetUserFunctionDto {
    /// 功能 ID
    #[serde(rename = "functionId")]
    #[serde(deserialize_with = "deserialize_i64_from_string_or_number")]
    #[serde(default)]
    pub function_id: i64,

    /// 用户 ID
    #[serde(rename = "userId")]
    #[serde(deserialize_with = "deserialize_i64_from_string_or_number")]
    #[serde(default)]
    pub user_id: i64,

    /// 是否选中
    #[serde(rename = "isCheck")]
    #[serde(default)]
    pub is_check: bool,
}

/// 短信修改手机号请求模型
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::JsonModel)]
#[json(type_name = "Application.Model.User.User.EditUserPhoneBySmsDto")]
pub struct EditUserPhoneBySmsDto {
    /// 新手机号
    pub new_phone: String,

    /// 短信验证码
    pub sms_code: String,
}
