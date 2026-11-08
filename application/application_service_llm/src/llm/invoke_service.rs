use application_model::llm::parameters::LlmParameters;
use repository_core::versioned::VersionedActiveModel;

use chrono::Utc;
use futures_util::{StreamExt, stream::BoxStream};
use repository::extensions::SoftDeleteQueryExt;
use repository::{llm_app, llm_conversation};
use sea_orm::{ColumnTrait, QueryFilter, Set};

use application_interface::{
    error::AppError,
    llm::{
        AppLlmChatRequest, AppLlmChatResponse, AppLlmClient, AppLlmMessage, AppLlmRole,
        AppLlmStreamChunk,
    },
};
use application_runtime::context::LlmContext;

use super::{
    app_service::{render_template, validate_required_parameters},
    client_adapter::ModelConfigAppLlmClient,
};

/// LLM 运行时调用服务
///
/// 按应用 Code 调用已配置模型
pub struct LlmInvokeService<'a> {
    /// 应用层上下文
    context: LlmContext<'a>,
}

impl<'a> LlmInvokeService<'a> {
    /// 创建 LLM 运行时调用服务
    ///
    /// 使用统一 LLM 上下文访问数据库和客户端工厂
    pub fn new(context: LlmContext<'a>) -> Self {
        Self { context }
    }

    /// 按 LLM 应用 Code 调用对话接口
    ///
    /// 通过应用层客户端工厂执行一次非流式业务对话并返回应用层响应
    pub async fn chat(
        &self,
        code: &str,
        parameters: LlmParameters,
        create_user_id: Option<i64>,
    ) -> Result<AppLlmChatResponse, AppError> {
        let prepared = self.prepare_chat(code, parameters).await?;
        let client = ModelConfigAppLlmClient::new(
            prepared.model_config.clone(),
            self.context.client_factory,
        )?;
        let response = client.chat(prepared.request).await?;

        let assistant_content = response
            .choices
            .first()
            .map(|choice| choice.message.content.clone())
            .unwrap_or_default();
        self.save_conversation(
            prepared.llm_app_id,
            prepared.system_prompt.unwrap_or_default(),
            prepared.user_prompt,
            assistant_content,
            create_user_id,
        )
        .await;

        Ok(response)
    }

    /// 按 LLM 应用 Code 调用并返回首条文本内容
    /// 从应用层响应消息中提取第一条回复文本
    pub async fn chat_content(
        &self,
        code: &str,
        parameters: LlmParameters,
        create_user_id: Option<i64>,
    ) -> Result<Option<String>, AppError> {
        let response = self.chat(code, parameters, create_user_id).await?;
        Ok(response
            .choices
            .first()
            .map(|choice| choice.message.content.clone()))
    }

    /// 按 LLM 应用 Code 调用流式对话接口
    ///
    /// 通过应用层客户端工厂执行一次流式业务对话并返回应用层分片
    /// 当前保留手工记录逻辑，不直接套用 logging 宏以避免把流尚未消费完成视为执行结束
    pub async fn chat_stream(
        &self,
        code: &str,
        parameters: LlmParameters,
        create_user_id: Option<i64>,
    ) -> Result<BoxStream<'a, Result<AppLlmStreamChunk, AppError>>, AppError> {
        let code = code.to_owned();
        // 复制轻量上下文用于流结束后异步保存会话记录
        let service = Self {
            context: self.context,
        };

        Ok(Box::pin(async_stream::try_stream! {
            let prepared = service.prepare_chat(&code, parameters).await?;
            let client = ModelConfigAppLlmClient::new(prepared.model_config.clone(), service.context.client_factory)?;

            let mut assistant_content = String::new();
            let mut is_completed = false;
            // 在流式返回过程中持续聚合文本 便于结束后落库完整回复
            let mut stream = client.chat_stream(prepared.request);

            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;

                if let Some(content) = chunk.choices.first().and_then(|choice| choice.delta.content.as_ref()) {
                    assistant_content.push_str(content);
                }

                if chunk.choices.iter().any(|choice| choice.finish_reason.as_ref().is_some_and(|reason| !reason.trim().is_empty())) {
                    is_completed = true;
                }

                yield chunk;
            }

            if is_completed {
                service
                    .save_conversation(
                        prepared.llm_app_id,
                        prepared.system_prompt.unwrap_or_default(),
                        prepared.user_prompt,
                        assistant_content,
                        create_user_id,
                    )
                    .await;
            }
        }))
    }

    /// 保存对话记录
    async fn save_conversation(
        &self,
        llm_app_id: i64,
        system_content: String,
        user_content: String,
        assistant_content: String,
        create_user_id: Option<i64>,
    ) {
        let Ok(conversation_id) = self.context.app.id_generator.get_id().await else {
            tracing::error!(target: "llm.invoke", "生成对话记录 ID 失败，跳过保存");
            return;
        };

        let result = llm_conversation::ActiveModel {
            row_version: Default::default(),
            id: Set(conversation_id),
            llm_app_id: Set(llm_app_id),
            system_content: Set(system_content),
            user_content: Set(user_content),
            assistant_content: Set(assistant_content),
            create_user_id: Set(create_user_id),
            delete_user_id: Set(None),
            create_time: Set(Utc::now().into()),
            delete_time: Set(None),
        }
        .insert_versioned(self.context.app.db)
        .await;

        if let Err(error) = result {
            tracing::error!(target: "llm.invoke", %error, "保存对话记录失败");
        }
    }

    /// 构建运行时调用所需上下文
    async fn prepare_chat(
        &self,
        code: &str,
        parameters: LlmParameters,
    ) -> Result<PreparedChat, AppError> {
        let app = llm_app::Entity::find()
            .filter(llm_app::Column::Code.eq(code))
            .filter(llm_app::Column::IsEnable.eq(true))
            .not_deleted(llm_app::Column::DeleteTime)
            .one(self.context.app.db)
            .await
            .map_err(|e| AppError::internal(format!("读取 LLM 应用配置失败: {e}")))?
            .ok_or_else(|| AppError::internal(format!("LLM app not found or disabled: {code}")))?;

        let prompt_template = app.prompt_template.as_str();
        if prompt_template.trim().is_empty() {
            return Err(AppError::internal(format!(
                "LLM app prompt template is required: {code}"
            )));
        }

        validate_required_parameters(
            app.system_prompt_template.as_deref(),
            Some(prompt_template),
            &parameters,
        )?;

        let system_prompt = render_template(
            app.system_prompt_template
                .as_deref()
                .filter(|text| !text.trim().is_empty()),
            &parameters,
        );
        let user_prompt = render_template(Some(prompt_template), &parameters).ok_or_else(|| {
            AppError::internal(format!("LLM app prompt template is required: {code}"))
        })?;

        let mut messages = Vec::new();
        if let Some(system_prompt) = system_prompt
            .as_deref()
            .filter(|item| !item.trim().is_empty())
        {
            messages.push(AppLlmMessage {
                role: AppLlmRole::System,
                content: system_prompt.to_string(),
            });
        }
        messages.push(AppLlmMessage {
            role: AppLlmRole::User,
            content: user_prompt.clone(),
        });

        let extra_body = parse_runtime_extra_body(app.extra_body_json.as_deref(), code)?;
        let model_config = self
            .context
            .model_config_resolver
            .get_config(app.llm_model_id)
            .await?
            .ok_or_else(|| {
                AppError::internal(format!(
                    "LLM model config not found or disabled for app: {code}"
                ))
            })?;

        let request = AppLlmChatRequest {
            model: model_config.model_id.clone(),
            messages,
            user: None,
            // 透传模型协议扩展参数 以兼容 Thinking 开关等能力
            extra_body,
        };

        Ok(PreparedChat {
            llm_app_id: app.id,
            model_config,
            request,
            system_prompt,
            user_prompt,
        })
    }
}

/// 运行时调用准备结果
struct PreparedChat {
    /// LLM 应用 ID
    llm_app_id: i64,

    /// 模型运行时配置
    model_config: llm::LlmModelConfig,

    /// 对话请求
    request: AppLlmChatRequest,

    /// System 提示词
    system_prompt: Option<String>,

    /// User 提示词
    user_prompt: String,
}

/// 运行时配置损坏属于内部错误；空键和 null 值不进入协议请求。
fn parse_runtime_extra_body(
    value: Option<&str>,
    code: &str,
) -> Result<Option<application_model::llm::invocation::LlmExtraBody>, AppError> {
    let Some(value) = value.filter(|value| !value.trim().is_empty()) else {
        return Ok(None);
    };
    let entries = common::json_helper::input::parse_json_object_entries(value).map_err(|_| {
        AppError::internal(format!("LLM app ExtraBodyJson is invalid JSON: {code}"))
    })?;
    let Some(entries) = entries else {
        return Err(AppError::internal(format!(
            "LLM app ExtraBodyJson must be a JSON object: {code}"
        )));
    };
    let mut object = application_model::llm::invocation::LlmExtraBody::new();
    for (key, value) in entries {
        // JsonObject 首次枚举会先建立完整字典，空键/null 的过滤不能绕过重复键错误。
        if object.contains_key(&key) {
            return Err(AppError::internal(format!(
                "ExtraBodyJson 包含重复属性: {key}"
            )));
        }
        object.insert(key, value);
    }
    object.retain(|key, value| !key.trim().is_empty() && value.get() != "null");
    Ok((!object.is_empty()).then_some(object))
}
