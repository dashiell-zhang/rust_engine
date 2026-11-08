use application_model::user::models::{
    EditUserDto, PageListDto, SetUserFunctionDto, SetUserRoleDto, UserDto, UserFunctionDto,
    UserRoleDto,
};
use utoipa::OpenApi;

/// 用户模块 OpenAPI 文档
#[derive(OpenApi)]
#[openapi(
    components(
        schemas(
            PageListDto<UserDto>,
            UserDto,
            UserRoleDto,
            SetUserRoleDto,
            UserFunctionDto,
            SetUserFunctionDto,
            EditUserDto,
        )
    ),
    tags(
        (name = "用户管理", description = "用户增删改查接口"),
    )
)]
pub struct UserApiDoc;
