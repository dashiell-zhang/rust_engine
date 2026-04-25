use application_model::shared::models::PageRequest;
use application_model::shared::models::validate_page_request;
use axum::{
    Json as AxumJson,
    extract::{FromRequest, FromRequestParts, Query as AxumQuery, Request},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Serialize, de::DeserializeOwned};

use crate::error::ErrBody;

/// 统一 JSON 提取器
pub struct Json<T>(pub T);

/// 统一 Query 提取器
pub struct Query<T>(pub T);

/// 统一分页 Query 提取器
pub struct PageQuery<T>(pub T);

impl<T> IntoResponse for Json<T>
where
    T: Serialize,
{
    /// 复用 axum 的 JSON 响应输出
    fn into_response(self) -> Response {
        AxumJson(self.0).into_response()
    }
}

impl<S, T> FromRequest<S> for Json<T>
where
    AxumJson<T>: FromRequest<S, Rejection = axum::extract::rejection::JsonRejection>,
    S: Send + Sync,
{
    type Rejection = Response;

    /// 将 JSON 提取失败转换为统一错误响应
    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        match AxumJson::<T>::from_request(request, state).await {
            Ok(value) => Ok(Self(value.0)),
            Err(rejection) => Err(build_extractor_error_response(
                rejection.status(),
                rejection.to_string(),
            )),
        }
    }
}

impl<S, T> FromRequestParts<S> for Query<T>
where
    T: DeserializeOwned + Send,
    S: Send + Sync,
{
    type Rejection = Response;

    /// 将 Query 提取失败转换为统一错误响应
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        match AxumQuery::<T>::from_request_parts(parts, state).await {
            Ok(value) => Ok(Self(value.0)),
            Err(rejection) => Err(build_extractor_error_response(
                rejection.status(),
                rejection.to_string(),
            )),
        }
    }
}

impl<S, T> FromRequestParts<S> for PageQuery<T>
where
    T: DeserializeOwned + PageRequest + Send,
    S: Send + Sync,
{
    type Rejection = Response;

    /// 将分页 Query 提取和分页参数校验统一收敛到 WebAPI 层
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        let Query(value) = Query::<T>::from_request_parts(parts, state).await?;

        if let Err(error) = validate_page_request(value.page(), value.page_size()) {
            return Err(build_extractor_error_response(
                StatusCode::BAD_REQUEST,
                error.message().to_string(),
            ));
        }

        Ok(Self(value))
    }
}

/// 构建统一提取器错误响应
fn build_extractor_error_response(status: StatusCode, err_msg: String) -> Response {
    (status, AxumJson(ErrBody { err_msg })).into_response()
}
