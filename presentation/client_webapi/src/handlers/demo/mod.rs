use std::{collections::HashMap, sync::Arc, time::Duration};

use axum::{
    extract::State,
    response::{IntoResponse, Response},
};
use futures_util::StreamExt;
use serde::Deserialize;
use tokio::time::sleep;
use webapi_core::error::AppError;
use webapi_core::extract::{Json, Query};

use crate::llm_support::ClientLlmApplicationAdapter;
use crate::state::AppState;

/// 调试环境下的 Demo 处理器
/// Echo 查询参数
#[derive(Deserialize)]
pub struct EchoQuery {
    /// 回显名称
    pub name: Option<String>,
}

/// Add 查询参数
#[derive(Deserialize)]
pub struct AddQuery {
    /// 第一个加数
    pub a: i32,

    /// 第二个加数
    pub b: i32,
}

/// Stream 查询参数
#[derive(Deserialize)]
pub struct StreamNumbersQuery {
    /// 返回数量
    pub count: Option<usize>,
}

/// Add2 请求体
#[derive(Deserialize)]
pub struct Add2Body {
    /// 文章标题
    pub title: Option<String>,
}

/// 开发测试回显接口
pub async fn echo(Query(query): Query<EchoQuery>) -> String {
    format!(
        "hello, {}",
        query.name.unwrap_or_else(|| "world".to_string())
    )
}

/// 开发测试加法接口
pub async fn add(Query(query): Query<AddQuery>) -> Json<i32> {
    sleep(Duration::from_millis(10)).await;
    Json(query.a + query.b)
}

/// 第二组开发测试回显接口
pub async fn echo2(Query(query): Query<EchoQuery>) -> String {
    format!(
        "hello, {}",
        query.name.unwrap_or_else(|| "world".to_string())
    )
}

/// 第二组开发测试加法接口
pub async fn add2(Query(query): Query<AddQuery>, Json(_body): Json<Add2Body>) -> Json<i32> {
    sleep(Duration::from_millis(10)).await;
    Json(query.a + query.b)
}

/// 开发测试引用参数加法接口
pub async fn a_ref(Query(query): Query<AddQuery>) -> Json<i32> {
    Json(query.a + query.b)
}

/// 开发测试数字流接口
pub async fn stream_numbers(Query(query): Query<StreamNumbersQuery>) -> Json<Vec<i32>> {
    Json(build_number_list(query.count).await)
}

/// 开发测试异步数字流接口
pub async fn stream_numbers2(Query(query): Query<StreamNumbersQuery>) -> Json<Vec<i32>> {
    sleep(Duration::from_millis(100)).await;
    Json(build_number_list(query.count).await)
}

/// 测试普通 LLM 调用
pub async fn test_llm(State(app_state): State<Arc<AppState>>) -> Result<Response, AppError> {
    let adapter = ClientLlmApplicationAdapter::new(app_state.as_ref());
    let service = adapter.llm_invoke();
    // 使用固定演示参数调用默认样例应用 便于快速验证链路是否可用
    let content = service
        .chat_content("sumqw", build_demo_parameters(), None)
        .await
        .map_err(AppError::from)?
        .unwrap_or_default();

    Ok(content.into_response())
}

/// 测试流式 LLM 调用并聚合返回
pub async fn test_llm_stream(State(app_state): State<Arc<AppState>>) -> Result<Response, AppError> {
    let adapter = ClientLlmApplicationAdapter::new(app_state.as_ref());
    let service = adapter.llm_invoke();
    let mut stream = service
        .chat_stream("sumqw", build_demo_parameters(), None)
        .await
        .map_err(AppError::from)?;
    let mut content = String::new();

    // 流式结果在调试接口中统一拼接 方便直接查看完整输出
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(AppError::from)?;
        if let Some(delta) = chunk.delta {
            content.push_str(&delta);
        }
    }

    Ok(content.into_response())
}

/// 构建 Demo 默认参数
fn build_demo_parameters() -> HashMap<String, String> {
    HashMap::from([
        ("a".to_string(), "5".to_string()),
        ("b".to_string(), "9".to_string()),
        ("c".to_string(), "13".to_string()),
    ])
}

/// 构建 Demo 数字列表
async fn build_number_list(count: Option<usize>) -> Vec<i32> {
    let count = count.unwrap_or(5).min(100);
    let mut numbers = Vec::with_capacity(count);

    for index in 0..count {
        sleep(Duration::from_millis(30)).await;
        numbers.push(index as i32);
    }

    numbers
}
