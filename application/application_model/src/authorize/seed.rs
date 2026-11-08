use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// 权限初始化文件。读取时与 NetEngine 一致，忽略 JSON 属性名大小写。
#[derive(Default, Deserialize)]
#[serde(default)]
pub struct AuthorizeSeedData {
    #[serde(rename = "adminuser")]
    pub admin_user: AdminUserSeedItem,
    #[serde(rename = "adminrole")]
    pub admin_role: AdminRoleSeedItem,
    pub functions: Vec<FunctionSeedItem>,
    #[serde(rename = "functionroutes")]
    pub function_routes: Vec<FunctionRouteSeedItem>,
}

impl AuthorizeSeedData {
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        fn normalize(value: serde_json::Value) -> serde_json::Value {
            match value {
                serde_json::Value::Object(fields) => serde_json::Value::Object(
                    fields
                        .into_iter()
                        .map(|(key, value)| (key.to_ascii_lowercase(), normalize(value)))
                        .collect(),
                ),
                serde_json::Value::Array(items) => {
                    serde_json::Value::Array(items.into_iter().map(normalize).collect())
                }
                value => value,
            }
        }
        serde_json::from_value(normalize(serde_json::from_str(
            json.trim_start_matches('\u{feff}'),
        )?))
    }
}

#[derive(Default, Deserialize)]
#[serde(default)]
pub struct AdminUserSeedItem {
    pub name: String,
    #[serde(rename = "username")]
    pub user_name: String,
    pub phone: String,
    pub email: Option<String>,
    pub password: String,
}

#[derive(Default, Deserialize)]
#[serde(default)]
pub struct AdminRoleSeedItem {
    pub code: String,
    pub name: String,
    pub remark: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
pub struct FunctionSeedItem {
    pub name: String,
    pub sign: String,
    pub remark: Option<String>,
    #[serde(rename = "parentsign")]
    pub parent_sign: Option<String>,
    pub r#type: i32,
}

#[derive(Default, Deserialize)]
#[serde(default)]
pub struct FunctionRouteSeedItem {
    pub module: String,
    pub route: String,
    pub remark: Option<String>,
    #[serde(rename = "functionsign")]
    pub function_sign: Option<String>,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "PascalCase")]
pub struct AuthorizeSeedResult {
    pub admin_user: String,
    pub admin_role: String,
    pub function_count: usize,
    pub function_route_count: usize,
    pub function_authorize_count: u64,
    pub user_role_count: u64,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "PascalCase")]
pub struct AuthorizeInitDataResult {
    #[serde(flatten)]
    pub result: AuthorizeSeedResult,
    pub file_path: String,
}
