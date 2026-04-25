use std::collections::HashMap;

use chrono::Utc;
use futures_util::{StreamExt, stream::BoxStream};
use repository::extensions::SoftDeleteQueryExt;
use repository::{llm_app, llm_conversation};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

use application_interface::{
    context::LlmContext,
    error::AppError,
    llm::{AppLlmChatRequest, AppLlmChatResponse, AppLlmMessage, AppLlmRole, AppLlmStreamChunk},
};

use super::app_service::{parse_extra_body_json, render_template, validate_required_parameters};

/// LLM 运行时调用服务
///
/// 按应用 Code 调用已注册 Provider
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
        parameters: HashMap<String, String>,
        create_user_id: Option<i64>,
    ) -> Result<AppLlmChatResponse, AppError> {
        let prepared = self.prepare_chat(code, parameters).await?;
        let client = self
            .context
            .llm_client_factory
            .require_client(&prepared.provider)?;
        let response = client.chat(prepared.request).await?;

        let assistant_content = response
            .messages
            .first()
            .map(|message| message.content.clone())
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
        parameters: HashMap<String, String>,
        create_user_id: Option<i64>,
    ) -> Result<Option<String>, AppError> {
        let response = self.chat(code, parameters, create_user_id).await?;
        Ok(response
            .messages
            .first()
            .map(|message| message.content.clone()))
    }

    /// 按 LLM 应用 Code 调用流式对话接口
    ///
    /// 通过应用层客户端工厂执行一次流式业务对话并返回应用层分片
    /// 当前保留手工记录逻辑，不直接套用 logging 宏以避免把流尚未消费完成视为执行结束
    pub async fn chat_stream(
        &self,
        code: &str,
        parameters: HashMap<String, String>,
        create_user_id: Option<i64>,
    ) -> Result<BoxStream<'a, Result<AppLlmStreamChunk, AppError>>, AppError> {
        let prepared = self.prepare_chat(code, parameters).await?;
        let client = self
            .context
            .llm_client_factory
            .require_client(&prepared.provider)?;

        // 复制轻量上下文用于流结束后异步保存会话记录
        let service = Self {
            context: self.context,
        };

        Ok(Box::pin(async_stream::try_stream! {
            let mut assistant_content = String::new();
            // 在流式返回过程中持续聚合文本 便于结束后落库完整回复
            let mut stream = client.chat_stream(prepared.request);

            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;

                if let Some(content) = &chunk.delta {
                    assistant_content.push_str(content);
                }

                yield chunk;
            }

            service
                .save_conversation(
                    prepared.llm_app_id,
                    prepared.system_prompt.unwrap_or_default(),
                    prepared.user_prompt,
                    assistant_content,
                    create_user_id,
                )
                .await;
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
            tracing::warn!(target: "llm.invoke", "生成对话记录 ID 失败，跳过保存");
            return;
        };

        let result = llm_conversation::ActiveModel {
            id: Set(conversation_id),
            llm_app_id: Set(llm_app_id),
            system_content: Set(system_content),
            user_content: Set(user_content),
            assistant_content: Set(assistant_content),
            create_user_id: Set(create_user_id),
            create_time: Set(Utc::now().into()),
            delete_time: Set(None),
        }
        .insert(self.context.app.db)
        .await;

        if let Err(error) = result {
            tracing::warn!(target: "llm.invoke", %error, "保存对话记录失败");
        }
    }

    /// 构建运行时调用所需上下文
    async fn prepare_chat(
        &self,
        code: &str,
        parameters: HashMap<String, String>,
    ) -> Result<PreparedChat, AppError> {
        let code = code.trim();
        if code.is_empty() {
            return Err(AppError::bad_request("Code 不可以为空"));
        }

        let app = llm_app::Entity::find()
            .filter(llm_app::Column::Code.eq(code))
            .filter(llm_app::Column::IsEnable.eq(true))
            .not_deleted(llm_app::Column::DeleteTime)
            .one(self.context.app.db)
            .await
            .map_err(|e| AppError::internal(format!("读取 LLM 应用配置失败: {e}")))?
            .ok_or_else(|| AppError::bad_request("LLM 应用配置不存在或已禁用"))?;

        if app.provider.trim().is_empty() {
            return Err(AppError::internal(format!(
                "LLM 应用 Provider 未配置: {code}"
            )));
        }

        if app.model.trim().is_empty() {
            return Err(AppError::internal(format!("LLM 应用模型未配置: {code}")));
        }

        let prompt_template = app.prompt_template.trim();
        if prompt_template.is_empty() {
            return Err(AppError::internal(format!(
                "LLM 应用 PromptTemplate 未配置: {code}"
            )));
        }

        validate_required_parameters(
            app.system_prompt_template.as_deref(),
            Some(prompt_template),
            &parameters,
        )?;

        let system_prompt = render_template(app.system_prompt_template.as_deref(), &parameters);
        let user_prompt = render_template(Some(prompt_template), &parameters)
            .ok_or_else(|| AppError::internal(format!("LLM 应用 PromptTemplate 未配置: {code}")))?;

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

        let request = AppLlmChatRequest {
            model: app.model.clone(),
            messages,
            temperature: app.temperature.map(|item| item as f32),
            max_tokens: app.max_tokens.and_then(|item| u32::try_from(item).ok()),
            // 透传 Provider 特有参数 以兼容 Thinking 开关等扩展能力
            extra_body: parse_extra_body_json(app.extra_body_json.as_deref())?,
        };

        Ok(PreparedChat {
            llm_app_id: app.id,
            provider: app.provider,
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

    /// Provider 标识
    provider: String,

    /// 对话请求
    request: AppLlmChatRequest,

    /// System 提示词
    system_prompt: Option<String>,

    /// User 提示词
    user_prompt: String,
}
