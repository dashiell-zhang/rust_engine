use application_model::user::models::{
    EditUserDto, PageListDto, SetUserFunctionDto, SetUserRoleDto, UserDto, UserFunctionDto,
    UserRoleDto,
};
use utoipa::OpenApi;

use crate::handlers::user;

/// 用户模块 OpenAPI 文档
#[derive(OpenApi)]
#[openapi(
    paths(
        user::get_user_list,
        user::get_user,
        user::get_user_role_list,
        user::set_user_role,
        user::get_user_function,
        user::set_user_function,
        user::create_user,
        user::update_user,
        user::delete_user,
    ),
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
