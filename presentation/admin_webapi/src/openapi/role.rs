use application_model::role::models::{
    EditRoleDto, RoleDto, RoleFunctionDto, RolePageListDto, RoleSelectDto, SetRoleFunctionDto,
};
use utoipa::OpenApi;

use crate::handlers::role;

/// 角色模块 OpenAPI 文档
#[derive(OpenApi)]
#[openapi(
    paths(
        role::get_role_list,
        role::get_role,
        role::create_role,
        role::update_role,
        role::delete_role,
        role::get_role_function,
        role::set_role_function,
        role::get_role_select,
    ),
    components(
        schemas(
            RolePageListDto,
            RoleDto,
            EditRoleDto,
            RoleFunctionDto,
            RoleSelectDto,
            SetRoleFunctionDto,
        )
    ),
    tags(
        (name = "角色管理", description = "角色与角色权限接口"),
    )
)]
pub struct RoleApiDoc;
