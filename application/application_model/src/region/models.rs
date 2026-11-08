use serde::{Deserialize, Serialize};

use utoipa::ToSchema;

/// 地区返回模型
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RegionDto {
    /// 地区 ID
    pub id: String,

    /// 地区名称
    pub name: String,

    /// 下级地区列表
    #[schema(no_recursion)]
    pub child_list: Option<Vec<RegionDto>>,
}

/// 乡镇返回模型
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TownDto {
    /// 乡镇 ID
    pub id: i32,

    /// 乡镇名称
    pub name: String,

    /// 所属区县 ID
    pub area_id: i32,
}
