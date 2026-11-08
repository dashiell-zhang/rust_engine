use application_model::{app_setting::models::GetValueListQuery, region::models::RegionDto};
use axum::{
    extract::State,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use common::app_error::AppError as ApplicationError;
use common::img_helper;
use serde::Deserialize;
use std::sync::Arc;
use utoipa::IntoParams;
use webapi_core::error::AppError;
use webapi_core::extract::{Json, Query};
use webapi_support::WebApiApplicationExt;

use crate::state::AppState;

/// 二维码生成查询参数
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct GetQrCodeQuery {
    /// 要编码的文本内容
    pub text: String,
}

/// 获取地区列表查询参数
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct GetRegionQuery {
    /// 省份 ID
    #[serde(rename = "provinceId")]
    pub province_id: Option<i32>,

    /// 城市 ID
    #[serde(rename = "cityId")]
    pub city_id: Option<i32>,
}

/// 生成二维码图片
#[utoipa::path(
    get,
    path = "/Base/GetQrCode",
    params(GetQrCodeQuery),
    responses(
        (status = 200, description = "生成成功，返回 PNG 图片"),
        (status = 400, description = "参数无效或二维码生成失败"),
        (status = 500, description = "服务器内部错误"),
    ),
    tag = "基础工具"
)]
pub async fn get_qr_code(
    State(_app_state): State<Arc<AppState>>,
    Query(query): Query<GetQrCodeQuery>,
) -> Result<Response, AppError> {
    let image = img_helper::get_qr_code(&query.text).map_err(|error| {
        AppError::from(ApplicationError::internal(format!(
            "二维码生成失败: {error}"
        )))
    })?;

    Ok((StatusCode::OK, [(header::CONTENT_TYPE, "image/png")], image).into_response())
}

/// 获取省市级联地址数据
#[utoipa::path(
    get,
    path = "/Base/GetRegion",
    params(GetRegionQuery),
    responses(
        (status = 200, description = "查询成功", body = Vec<RegionDto>),
        (status = 500, description = "服务器内部错误"),
    ),
    tag = "基础工具"
)]
pub async fn get_region(
    State(app_state): State<Arc<AppState>>,
    Query(query): Query<GetRegionQuery>,
) -> Result<Response, AppError> {
    let result = app_state
        .services()
        .base()?
        .get_region(
            query.province_id.unwrap_or_default(),
            query.city_id.unwrap_or_default(),
        )
        .await;

    match result {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 获取全部省市级联地址数据
#[utoipa::path(
    get,
    path = "/Base/GetRegionAll",
    responses(
        (status = 200, description = "查询成功", body = Vec<RegionDto>),
        (status = 500, description = "服务器内部错误"),
    ),
    tag = "基础工具"
)]
pub async fn get_region_all(State(app_state): State<Arc<AppState>>) -> Result<Response, AppError> {
    match app_state.services().base()?.get_region_all().await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 获取指定组 ID 的键值对
#[utoipa::path(
    get,
    path = "/Base/GetValueList",
    params(GetValueListQuery),
    responses(
        (status = 200, description = "查询成功", body = Object),
        (status = 400, description = "请求参数无效"),
        (status = 500, description = "服务器内部错误"),
    ),
    tag = "基础工具"
)]
pub async fn get_value_list(
    State(app_state): State<Arc<AppState>>,
    Query(query): Query<GetValueListQuery>,
) -> Result<Response, AppError> {
    match app_state
        .services()
        .base()?
        .get_value_list(query.group_id)
        .await
    {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 获取一个雪花 ID
#[utoipa::path(
    get,
    path = "/Base/GetSnowflakeId",
    responses(
        (status = 200, description = "获取成功，返回雪花 ID", body = String),
        (status = 500, description = "服务器内部错误"),
    ),
    tag = "基础工具"
)]
pub async fn get_snowflake_id(
    State(app_state): State<Arc<AppState>>,
) -> Result<Response, AppError> {
    let id = app_state.id_generator.get_id().await.map_err(|error| {
        AppError::from(ApplicationError::internal(format!("ID 生成失败: {error}")))
    })?;

    Ok((StatusCode::OK, Json(id.to_string())).into_response())
}
