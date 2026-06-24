use common::json_helper::{
    deserialize_i64_from_string_or_number, deserialize_option_i64_from_string_or_number,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// 授权模块对外请求与响应模型
///
/// 这些结构同时用于接口反序列化与 OpenAPI 文档生成
/// ECDSA JWT 密钥对返回模型
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "PascalCase")]
pub struct EcdsaKeyPairDto {
    /// SEC1 DER 私钥的 Base64 文本
    pub private_key: String,

    /// SPKI DER 公钥的 Base64 文本
    pub public_key: String,
}

/// JWT Claims 结构
#[derive(Serialize, Deserialize, Clone)]
pub struct JwtClaims {
    /// Token 标识
    pub token_id: String,

    /// 用户标识
    pub user_id: String,

    /// 颁发者
    pub iss: String,

    /// 受众
    pub aud: String,

    /// 签发时间
    pub iat: i64,

    /// 过期时间
    pub exp: i64,

    /// 生效时间
    pub nbf: i64,
}

/// 登录请求参数模型
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetTokenRequest {
    /// 用户名
    pub user_name: String,

    /// 密码
    pub password: String,
}

/// 短信登录请求参数模型
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetTokenBySmsRequest {
    /// 手机号
    pub phone: String,

    /// 短信验证码
    pub verify_code: String,
}

/// 发送短信验证码请求参数模型
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SendSmsVerifyCodeRequest {
    /// 手机号
    pub phone: String,
}

/// 短信修改密码请求参数模型
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePasswordBySmsRequest {
    /// 短信验证码
    pub sms_code: String,

    /// 新密码
    pub new_password: String,
}

/// 通过旧密码修改密码请求参数模型
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePasswordByOldPasswordRequest {
    /// 旧密码
    #[serde(rename = "oldPassword")]
    pub old_password: String,

    /// 新密码
    #[serde(rename = "newPassword")]
    pub new_password: String,
}

/// 微信小程序登录请求参数模型
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetTokenByWxMiniAppRequest {
    /// 微信小程序 AppId
    pub app_id: String,

    /// 微信小程序 wx.login 获取的 code
    pub code: String,
}

/// 微信公众号网页登录请求参数模型
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetTokenByWxAppRequest {
    /// 微信公众号应用标识
    #[serde(rename = "appId")]
    pub app_id: String,

    /// 网页授权回调返回的授权码
    #[serde(rename = "code")]
    pub code: String,
}

/// 微信公众号用户信息返回模型
#[derive(Deserialize)]
pub struct GetWeiXinAppUserInfoDto {
    /// 用户开放标识
    #[serde(rename = "openid")]
    pub open_id: Option<String>,

    /// 昵称
    #[serde(rename = "nickname")]
    pub nick_name: Option<String>,

    /// 性别
    #[serde(default, rename = "sex")]
    pub sex: i32,

    /// 省份
    #[serde(rename = "province")]
    pub province: Option<String>,

    /// 城市
    #[serde(rename = "city")]
    pub city: Option<String>,

    /// 国家
    #[serde(rename = "country")]
    pub country: Option<String>,

    /// 头像地址
    #[serde(rename = "headimgurl")]
    pub headimg_url: Option<String>,

    /// 权限列表
    #[serde(default, rename = "privilege")]
    pub privilege: Vec<String>,

    /// 用户统一标识
    #[serde(rename = "unionid")]
    pub union_id: Option<String>,
}

/// 获取授权功能列表请求
#[derive(Deserialize, IntoParams, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetFunctionListRequest {
    /// 功能标记
    pub sign: Option<String>,
}

/// 获取路由权限绑定列表请求
#[derive(Deserialize, IntoParams, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetFunctionRouteListRequest {
    /// 模块名
    pub module: Option<String>,

    /// 关键词
    pub keyword: Option<String>,
}

/// 路由权限绑定返回模型
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FunctionRouteDto {
    /// 路由 ID
    pub id: String,

    /// 模块名
    pub module: String,

    /// 路由路径
    pub route: String,

    /// 路由说明
    pub remark: Option<String>,

    /// 绑定的功能 ID
    pub function_id: Option<String>,

    /// 绑定的功能名称
    pub function_name: Option<String>,

    /// 绑定的功能标记
    pub function_sign: Option<String>,

    /// 创建时间
    pub create_time: String,
}

/// 功能树返回模型
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FunctionTreeDto {
    /// 功能 ID
    pub id: String,

    /// 功能名称
    pub name: String,

    /// 功能标记
    pub sign: String,

    /// 备注信息
    pub remark: Option<String>,

    /// 子模块列表
    #[schema(no_recursion)]
    pub child_list: Vec<FunctionTreeDto>,

    /// 当前模块下的功能列表
    #[schema(no_recursion)]
    pub function_list: Vec<FunctionTreeDto>,
}

/// 设置路由权限绑定请求
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetFunctionRouteDto {
    /// 路由 ID
    #[serde(rename = "routeId")]
    #[serde(deserialize_with = "deserialize_i64_from_string_or_number")]
    pub route_id: i64,

    /// 功能 ID
    #[serde(rename = "functionId")]
    #[serde(
        default,
        deserialize_with = "deserialize_option_i64_from_string_or_number"
    )]
    pub function_id: Option<i64>,
}
