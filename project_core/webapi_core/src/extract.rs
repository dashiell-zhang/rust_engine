use application_model::shared::models::PageRequest;
use axum::{
    extract::{FromRequest, FromRequestParts, Request},
    response::{IntoResponse, Response},
};
use common::json_helper::input::{JsonModel, QueryModel};
use serde::Serialize;

/// 统一 JSON 提取器
pub struct Json<T>(pub T);

/// 使用共享 JSON 绑定并保留请求文本，供提供商回调验签，不重新序列化模型。
pub struct JsonWithBody<T> {
    /// 经共享绑定规则解析的对象
    pub value: T,
    /// 按共享 GetRequestBody 规则读取的原始请求文本，处理 gzip 与 BOM。
    pub body: String,
}

impl<S, T> FromRequest<S> for JsonWithBody<T>
where
    T: JsonModel,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        let encoding = request
            .headers()
            .get(axum::http::header::CONTENT_ENCODING)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let (value, bytes) = crate::json_body::read::<T, S>(request, state)
            .await
            .map_err(|error| *error)?;
        let body = crate::request_body::read_signature_text(encoding.as_deref(), &bytes)
            .map_err(|error| crate::error::internal_error_response(&error))?;
        Ok(Self { value, body })
    }
}

/// 统一 Query 提取器
pub struct Query<T>(pub T);

/// 统一分页 Query 提取器
pub struct PageQuery<T>(pub T);

/// 表单字段集合：保留首次字段名，同名字段按出现顺序合并，不包含文件或 Query。
pub struct FormFields(pub std::collections::HashMap<String, String>);

impl<S: Send + Sync> FromRequest<S> for FormFields {
    type Rejection = Response;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        let content_type = request
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        let media_type = content_type.split(';').next().unwrap_or_default().trim();
        let mut fields = Vec::new();
        if media_type.eq_ignore_ascii_case("application/x-www-form-urlencoded") {
            let axum::Form(values) =
                axum::Form::<Vec<(String, String)>>::from_request(request, state)
                    .await
                    .map_err(|error| crate::error::internal_error_response(&error.to_string()))?;
            for (name, value) in values {
                crate::request_parameters::append_joined(&mut fields, name, value);
            }
        } else if media_type.eq_ignore_ascii_case("multipart/form-data") {
            let mut multipart = axum::extract::Multipart::from_request(request, state)
                .await
                .map_err(|error| crate::error::internal_error_response(&error.to_string()))?;
            while let Some(mut field) = multipart
                .next_field()
                .await
                .map_err(|error| crate::error::internal_error_response(&error.to_string()))?
            {
                let Some(disposition) = crate::form_disposition::read(field.headers())
                    .map_err(|error| crate::error::internal_error_response(&error))?
                else {
                    continue;
                };
                if crate::form_disposition::file_name(&disposition).is_some() {
                    // Request.Form 不包含文件；流式消耗，不把文件作为字段参与签名。
                    while field
                        .chunk()
                        .await
                        .map_err(|error| crate::error::internal_error_response(&error.to_string()))?
                        .is_some()
                    {}
                } else {
                    let name = disposition.name.unwrap_or_default();
                    let value = field.text().await.map_err(|error| {
                        crate::error::internal_error_response(&error.to_string())
                    })?;
                    crate::request_parameters::append_joined(&mut fields, name, value);
                }
            }
        } else {
            return Err(crate::error::internal_error_response(
                "请求不是有效的表单类型",
            ));
        }
        Ok(Self(fields.into_iter().collect()))
    }
}

impl<T> IntoResponse for Json<T>
where
    T: Serialize,
{
    /// 保留 HTTP 对象结果；缓存或 ETag 启用时另按内部 JSON 规则生成快照。
    fn into_response(self) -> Response {
        let mut bytes = Vec::new();
        match common::json_helper::write_controller_json(&mut bytes, &self.0) {
            Ok(()) => crate::object_response::ObjectResponse {
                json: bytes.into(),
                internal_json: crate::object_response::ObjectResponse::internal_json(&self.0),
            }
            .into_response(),
            Err(error) => {
                crate::error::internal_error_response(&format!("响应对象序列化失败: {error}"))
            }
        }
    }
}

impl<S, T> FromRequest<S> for Json<T>
where
    T: JsonModel,
    S: Send + Sync,
{
    type Rejection = Response;

    /// 将 JSON 提取失败转换为统一错误响应
    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        crate::json_body::read::<T, S>(request, state)
            .await
            .map(|(value, _)| Self(value))
            .map_err(|error| *error)
    }
}

impl<S, T> FromRequestParts<S> for Query<T>
where
    T: QueryModel + Send,
    S: Send + Sync,
{
    type Rejection = Response;

    /// 将 Query 提取失败转换为统一错误响应
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        match crate::request_binding::query::<T>(parts.uri.query().unwrap_or_default()) {
            Ok(value) => Ok(Self(value)),
            Err(rejection) => Err(crate::request_binding::model_errors(&rejection).into_response()),
        }
    }
}

impl<S, T> FromRequestParts<S> for PageQuery<T>
where
    T: PageRequest + Send,
    S: Send + Sync,
{
    type Rejection = Response;

    /// 将分页 Query 提取和分页参数校验统一收敛到 WebAPI 层
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let (value, _prefix) =
            crate::request_binding::page_query::<T>(parts.uri.query().unwrap_or_default())
                .map_err(IntoResponse::into_response)?;

        Ok(Self(value))
    }
}
