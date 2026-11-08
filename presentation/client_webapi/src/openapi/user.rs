use application_model::user::models::{EditUserPhoneBySmsDto, UserDto};
use utoipa::OpenApi;

/// 用户模块 OpenAPI 文档
#[derive(OpenApi)]
#[openapi(
    // 用户端文档只暴露当前场景需要的两个接口
    components(
        schemas(
            UserDto,
            EditUserPhoneBySmsDto,
        )
    ),
    tags(
        (name = "用户管理", description = "用户信息与手机号修改接口"),
    )
)]
pub struct UserApiDoc;
