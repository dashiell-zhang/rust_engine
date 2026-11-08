use application_model::article::models::ArticleDto;
use application_model::llm::parameters::LlmParameters;
use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::State,
    response::{IntoResponse, Response},
};
use futures_util::StreamExt;
use serde::Deserialize;
use webapi_core::auth::OptionalUserContext;
use webapi_core::error::AppError;
use webapi_core::extract::{Json, Query};
use webapi_core::json_stream::JsonStream;
use webapi_core::text_response::Text;

use crate::llm_support::ClientLlmApplicationAdapter;
use crate::services::DemoServices;
use crate::state::AppState;

/// Echo 查询参数
#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
#[derive(common::json_helper::input::QueryModel)]
pub struct EchoQuery {
    /// 回显名称
    #[param(default = "world")]
    pub name: Option<String>,
}

/// Add 查询参数
#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
#[derive(common::json_helper::input::QueryModel)]
pub struct AddQuery {
    /// 第一个加数
    #[serde(default)]
    #[param(default = 0, required = false)]
    pub a: i32,

    /// 第二个加数
    #[serde(default)]
    #[param(default = 0, required = false)]
    pub b: i32,
}

/// Stream 查询参数
#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
#[derive(common::json_helper::input::QueryModel)]
pub struct StreamNumbersQuery {
    /// 返回数量
    #[serde(default = "default_count")]
    #[param(default = 5, required = false)]
    pub count: i32,
}

fn default_count() -> i32 {
    5
}

/// 演示回显接口
#[utoipa::path(
    get, path = "/api/Demo/Echo",
    params(EchoQuery),
responses((status = 200, description = "调用成功", body = String, content_type = "text/plain")),
    tag = "Demo"
)]
pub async fn echo(State(state): State<Arc<AppState>>, Query(query): Query<EchoQuery>) -> Text {
    Text(
        DemoServices::new(&state)
            .demo()
            .echo(query.name.as_deref().unwrap_or("world")),
    )
}

/// 演示加法接口
#[utoipa::path(
    get, path = "/api/Demo/Add",
    params(AddQuery),
responses((status = 200, description = "调用成功", body = i32, content_type = "application/json")),
    tag = "Demo"
)]
pub async fn add(
    State(state): State<Arc<AppState>>,
    Query(query): Query<AddQuery>,
) -> Result<Json<i32>, AppError> {
    Ok(Json(
        DemoServices::new(&state)
            .demo()
            .add(query.a, query.b)
            .await?,
    ))
}

/// 第二组演示回显接口
#[utoipa::path(
    get, path = "/api/Demo/Echo2",
    params(EchoQuery),
responses((status = 200, description = "调用成功", body = String, content_type = "text/plain")),
    tag = "Demo"
)]
pub async fn echo2(State(state): State<Arc<AppState>>, Query(query): Query<EchoQuery>) -> Text {
    Text(
        DemoServices::new(&state)
            .demo2()
            .echo(query.name.as_deref().unwrap_or("world")),
    )
}

/// 第二组演示加法接口
#[utoipa::path(
    post, path = "/api/Demo/Add2",
    params(AddQuery),
request_body = ArticleDto,
responses((status = 200, description = "调用成功", body = i32, content_type = "application/json")),
    tag = "Demo"
)]
pub async fn add2(
    State(state): State<Arc<AppState>>,
    Query(query): Query<AddQuery>,
    Json(body): Json<ArticleDto>,
) -> Result<Json<i32>, AppError> {
    Ok(Json(
        DemoServices::new(&state)
            .demo2()
            .add(query.a, query.b, &body)
            .await?,
    ))
}

/// 演示引用参数加法接口
#[utoipa::path(
    get, path = "/api/Demo/ARef",
    params(AddQuery),
responses((status = 200, description = "调用成功", body = i32, content_type = "application/json")),
    tag = "Demo"
)]
pub async fn a_ref(State(state): State<Arc<AppState>>, Query(query): Query<AddQuery>) -> Json<i32> {
    let mut a = query.a;
    Json(DemoServices::new(&state).demo2().add_ref(&mut a, query.b))
}

/// 演示数字流接口
#[utoipa::path(
    get, path = "/api/Demo/StreamNumbers",
    params(StreamNumbersQuery),
responses((status = 200, description = "调用成功", body = Vec<i32>, content_type = "application/json")),
    tag = "Demo"
)]
pub async fn stream_numbers(
    State(state): State<Arc<AppState>>,
    Query(query): Query<StreamNumbersQuery>,
) -> Response {
    JsonStream(
        DemoServices::new(&state)
            .demo2()
            .stream_numbers(query.count),
    )
    .into_response()
}

/// 演示异步数字流接口
#[utoipa::path(
    get, path = "/api/Demo/StreamNumbers2",
    params(StreamNumbersQuery),
responses((status = 200, description = "调用成功", body = Vec<i32>, content_type = "application/json")),
    tag = "Demo"
)]
pub async fn stream_numbers2(
    State(state): State<Arc<AppState>>,
    Query(query): Query<StreamNumbersQuery>,
) -> Response {
    JsonStream(
        DemoServices::new(&state)
            .demo2()
            .stream_numbers_async(query.count)
            .await,
    )
    .into_response()
}

/// 测试普通 LLM 调用
#[utoipa::path(
    get, path = "/api/Demo/TestLLM",
    responses((status = 200, description = "调用成功", body = String, content_type = "text/plain")),
    tag = "Demo"
)]
pub async fn test_llm(
    State(app_state): State<Arc<AppState>>,
    user: OptionalUserContext,
) -> Result<Response, AppError> {
    app_state.id_generator.get_id().await.map_err(|error| {
        AppError::from(common::app_error::AppError::internal(format!(
            "ID 生成失败: {error}"
        )))
    })?;
    let adapter = ClientLlmApplicationAdapter::new(app_state.as_ref());
    let service = adapter.llm_invoke();
    // 使用固定演示参数调用默认样例应用 便于快速验证链路是否可用
    let content = service
        .chat_content(
            "jiafa",
            build_demo_parameters(),
            user.0.map(|user| user.user_id),
        )
        .await
        .map_err(AppError::from)?
        .unwrap_or_default();

    Ok(Text(content).into_response())
}

/// 测试流式 LLM 调用并聚合返回
#[utoipa::path(
    get, path = "/api/Demo/TestLLMStream",
    responses((status = 200, description = "调用成功", body = String, content_type = "text/plain")),
    tag = "Demo"
)]
pub async fn test_llm_stream(
    State(app_state): State<Arc<AppState>>,
    user: OptionalUserContext,
    cancellation: webapi_core::request_cancellation::RequestCancellation,
) -> Result<Response, AppError> {
    let adapter = ClientLlmApplicationAdapter::new(app_state.as_ref());
    let service = adapter.llm_invoke();
    let mut stream = service
        .chat_stream_with_cancellation(
            "sumqw",
            build_demo_parameters(),
            user.0.map(|user| user.user_id),
            cancellation.0,
        )
        .await
        .map_err(AppError::from)?;
    let mut content = String::new();

    // 流式结果在演示接口中统一拼接 方便直接查看完整输出
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(AppError::from)?;
        if let Some(delta) = chunk
            .choices
            .first()
            .and_then(|choice| choice.delta.content.as_deref())
        {
            content.push_str(delta);
        }
    }

    Ok(Text(content).into_response())
}

/// 构建 Demo 默认参数
fn build_demo_parameters() -> LlmParameters {
    HashMap::from([
        ("a".to_string(), Some("5".to_string())),
        ("b".to_string(), Some("9".to_string())),
        ("c".to_string(), Some("13".to_string())),
    ])
    .into()
}
