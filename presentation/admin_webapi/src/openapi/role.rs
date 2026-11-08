use application_model::role::models::{
    EditRoleDto, RoleDto, RoleFunctionDto, RoleSelectDto, SetRoleFunctionDto,
};
use application_model::shared::models::PageListDto;
use utoipa::OpenApi;

/// 角色模块 OpenAPI 文档
#[derive(OpenApi)]
#[openapi(
    components(
        schemas(
            PageListDto<RoleDto>,
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
