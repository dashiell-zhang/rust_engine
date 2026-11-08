use crate::registry::{
    QueueTaskContext, QueueTaskHandler, ScheduleTaskContext, ScheduleTaskHandler,
};
use async_trait::async_trait;
use serde::{Serialize, de::DeserializeOwned};
use std::future::Future;

/// 返回值只在需要填充回调参数时序列化，避免无回调任务因无关的序列化失败而重试。
pub struct QueueTaskResult {
    serialize: Option<SerializeTaskResult>,
}
type SerializeTaskResult = Box<dyn FnOnce() -> Result<Option<String>, String> + Send>;
impl QueueTaskResult {
    pub fn none() -> Self {
        Self { serialize: None }
    }
    pub fn value<T: Serialize + Send + 'static>(value: T) -> Self {
        Self {
            serialize: Some(Box::new(move || {
                let json = common::json_helper::object_to_json(&value)
                    .map_err(|error| format!("任务返回值序列化失败: {error}"))?;
                Ok((json != "null").then_some(json))
            })),
        }
    }
    pub fn into_callback_parameter(self) -> Result<Option<String>, String> {
        self.serialize.map_or(Ok(None), |serialize| serialize())
    }
}

impl QueueTaskContext {
    pub fn deserialize_parameter<T: DeserializeOwned>(&self) -> Result<T, String> {
        deserialize_parameter(self.parameter.as_deref())
    }
}
impl ScheduleTaskContext {
    pub fn deserialize_parameter<T: DeserializeOwned>(&self) -> Result<T, String> {
        deserialize_parameter(self.parameter.as_deref())
    }
}
fn deserialize_parameter<T: DeserializeOwned>(parameter: Option<&str>) -> Result<T, String> {
    // 读取参数表示该方法需要参数；数据库 NULL 是缺失记录，不能代替 JSON null。
    // 无参处理器不调用此入口，显式 JSON null 仍由目标类型（如 Option<T>）解析。
    let parameter = parameter.ok_or("任务方法要求有参数，但任务记录缺少参数")?;
    common::json_helper::json_clone_object(parameter)
        .map_err(|error| format!("任务参数反序列化失败: {error}"))
}

/// 将返回可序列化值的异步调用适配为队列处理器；() 和 Option::None 不生成回调参数。
pub fn queue_action<F, Fut, T>(action: F) -> impl QueueTaskHandler
where
    F: Fn(QueueTaskContext) -> Fut + Send + Sync,
    Fut: Future<Output = Result<T, String>> + Send,
    T: Serialize + Send + 'static,
{
    QueueAction(action)
}

struct QueueAction<F>(F);
#[async_trait]
impl<F, Fut, T> QueueTaskHandler for QueueAction<F>
where
    F: Fn(QueueTaskContext) -> Fut + Send + Sync,
    Fut: Future<Output = Result<T, String>> + Send,
    T: Serialize + Send + 'static,
{
    async fn execute(&self, context: QueueTaskContext) -> Result<(), String> {
        (self.0)(context).await.map(|_| ())
    }
    async fn execute_with_result(
        &self,
        context: QueueTaskContext,
    ) -> Result<QueueTaskResult, String> {
        (self.0)(context).await.map(QueueTaskResult::value)
    }
}

/// 将普通异步调用适配为定时处理器，作用域由注册工厂拥有。
pub fn schedule_action<F, Fut>(action: F) -> impl ScheduleTaskHandler
where
    F: Fn(ScheduleTaskContext) -> Fut + Send + Sync,
    Fut: Future<Output = Result<(), String>> + Send,
{
    ScheduleAction(action)
}
struct ScheduleAction<F>(F);
#[async_trait]
impl<F, Fut> ScheduleTaskHandler for ScheduleAction<F>
where
    F: Fn(ScheduleTaskContext) -> Fut + Send + Sync,
    Fut: Future<Output = Result<(), String>> + Send,
{
    async fn execute(&self, context: ScheduleTaskContext) -> Result<(), String> {
        (self.0)(context).await
    }
}
