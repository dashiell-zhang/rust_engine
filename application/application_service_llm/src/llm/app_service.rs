use application_model::llm::parameters::{LlmParameterComparison, LlmParameters};
use common::string_helper::eq_ordinal_ignore_case;
use repository_core::pagination::SignedPage;
use repository_core::versioned::VersionedActiveModel;
use std::sync::OnceLock;

use chrono::Utc;
use common::json_helper::{JsonObjectStringError, validate_optional_json_object_string};
use regex::Regex;
use repository::extensions::SoftDeleteQueryExt;
use repository::{llm_app, llm_model};
use sea_orm::{
    ColumnTrait, ExprTrait, IntoActiveModel, PaginatorTrait, QueryFilter, QueryOrder, Set,
};

use application_interface::{
    error::AppError,
    llm::{AppLlmChatRequest, AppLlmClient, AppLlmMessage, AppLlmRole, AppLlmUsage},
};
use application_model::shared::models::{page_offset, validate_page_request};
use application_model::user::models::PageListDto;
use application_runtime::context::{CurrentUser, LlmContext};

use super::client_adapter::ModelConfigAppLlmClient;
use super::models::{
    EditLlmAppDto, LlmAppDto, LlmAppPageListDto, LlmAppPageRequest, TestLlmAppRequest,
    TestLlmAppResult, TestLlmAppUsage,
};

/// LLM 应用配置服务
///
/// 负责 LLM 应用配置的增删改查与测试调用
pub struct LlmAppService<'a> {
    /// 应用层上下文
    context: LlmContext<'a>,
}

impl<'a> LlmAppService<'a> {
    /// 创建 LLM 应用配置服务
    ///
    /// 使用统一 LLM 上下文访问数据库和客户端工厂
    pub fn new(context: LlmContext<'a>) -> Self {
        Self { context }
    }

    /// 获取 LLM 应用配置分页列表
    pub async fn get_llm_app_list(
        &self,
        params: LlmAppPageRequest,
    ) -> Result<LlmAppPageListDto, AppError> {
        validate_page_request(params.page, params.page_size)?;

        let mut query = llm_app::Entity::find().not_deleted(llm_app::Column::DeleteTime);

        if let Some(keyword) = &params.keyword {
            let keyword = keyword.trim();
            if !keyword.is_empty() {
                let pattern = repository_core::extensions::contains_pattern(keyword);
                query = query.filter(
                    llm_app::Column::Code
                        .like(pattern.clone())
                        .or(llm_app::Column::Name.like(pattern.clone()))
                        .or(llm_app::Column::Remark.like(pattern)),
                );
            }
        }

        if let Some(is_enable) = params.is_enable {
            query = query.filter(llm_app::Column::IsEnable.eq(is_enable));
        }

        let total = query
            .clone()
            .count(self.context.app.db)
            .await
            .map_err(to_app_error)?;
        let total = repository_core::pagination::count_i32(total).map_err(to_app_error)?;

        if total == 0 {
            return Ok(PageListDto {
                total: 0,
                list: None,
            });
        }

        let skip = page_offset(params.page, params.page_size);
        let apps = query
            .order_by_desc(llm_app::Column::Id)
            .signed_page(skip, params.page_size)
            .all(self.context.app.db)
            .await
            .map_err(to_app_error)?;

        let model_names = load_model_names(
            self.context.app.db,
            apps.iter().map(|item| item.llm_model_id).collect(),
        )
        .await?;

        let list = apps
            .into_iter()
            .map(|app| to_llm_app_dto(app, &model_names))
            .collect();

        Ok(PageListDto {
            total,
            list: Some(list),
        })
    }

    /// 获取单个 LLM 应用配置
    pub async fn get_llm_app(&self, llm_app_id: i64) -> Result<Option<LlmAppDto>, AppError> {
        if llm_app_id <= 0 {
            return Err(AppError::bad_request("无效的 llmAppId"));
        }

        let model = llm_app::Entity::find()
            .filter(llm_app::Column::Id.eq(llm_app_id))
            .not_deleted(llm_app::Column::DeleteTime)
            .one(self.context.app.db)
            .await
            .map_err(to_app_error)?;

        let Some(model) = model else {
            return Ok(None);
        };

        let model_names = load_model_names(self.context.app.db, vec![model.llm_model_id]).await?;

        Ok(Some(to_llm_app_dto(model, &model_names)))
    }

    /// 创建 LLM 应用配置，返回新配置 ID
    pub async fn create_llm_app(
        &self,
        dto: EditLlmAppDto,
        current_user: &CurrentUser,
    ) -> Result<String, AppError> {
        validate_edit_llm_app(&dto)?;
        validate_extra_body_json(dto.extra_body_json.as_deref())?;

        let code = dto.code.trim().to_string();
        let code_lock = self
            .context
            .distributed_lock
            .try_lock(
                &format!("llm:app:code:{code}"),
                std::time::Duration::ZERO,
                1,
            )
            .await?
            .ok_or_else(|| AppError::bad_request("当前应用 Code 正在处理中，请稍后重试"))?;
        let result = async {
            let model_lock = self
                .context
                .distributed_lock
                .try_lock(
                    &format!("llm:model:id:{}", dto.llm_model_id),
                    std::time::Duration::ZERO,
                    1,
                )
                .await?
                .ok_or_else(|| AppError::bad_request("当前关联模型正在处理中，请稍后重试"))?;
            let result = async {
                let exists = llm_app::Entity::find()
                    .filter(llm_app::Column::Code.eq(code.clone()))
                    .not_deleted(llm_app::Column::DeleteTime)
                    .count(self.context.app.db)
                    .await
                    .map_err(to_app_error)?;

                if exists > 0 {
                    return Err(AppError::bad_request("Code 已存在"));
                }

                validate_llm_model_state(self.context.app.db, dto.llm_model_id, dto.is_enable)
                    .await?;

                let id = self
                    .context
                    .app
                    .id_generator
                    .get_id()
                    .await
                    .map_err(|error| {
                        AppError::internal(format!("生成 LLM 应用主键失败: {error}"))
                    })?;
                let model = llm_app::ActiveModel {
                    row_version: Default::default(),
                    id: Set(id),
                    code: Set(code),
                    name: Set(dto.name.trim().to_string()),
                    llm_model_id: Set(dto.llm_model_id),
                    system_prompt_template: Set(dto.system_prompt_template),
                    prompt_template: Set(dto.prompt_template.trim().to_string()),
                    extra_body_json: Set(dto.extra_body_json),
                    is_enable: Set(dto.is_enable),
                    remark: Set(dto.remark),
                    create_time: Set(Utc::now().into()),
                    delete_time: Set(None),
                    update_time: Set(None),
                    create_user_id: Set(Some(current_user.user_id)),
                    update_user_id: Set(None),
                    delete_user_id: Set(None),
                };

                model
                    .insert_versioned(self.context.app.db)
                    .await
                    .map_err(|e| AppError::internal(format!("创建 LLM 应用配置失败: {e}")))?;

                Ok(id.to_string())
            }
            .await;
            model_lock.release().await;
            result
        }
        .await;
        code_lock.release().await;
        result
    }

    /// 更新 LLM 应用配置
    pub async fn update_llm_app(
        &self,
        llm_app_id: i64,
        dto: EditLlmAppDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        validate_edit_llm_app(&dto)?;

        let model = llm_app::Entity::find()
            .filter(llm_app::Column::Id.eq(llm_app_id))
            .not_deleted(llm_app::Column::DeleteTime)
            .one(self.context.app.db)
            .await
            .map_err(to_app_error)?;

        let Some(model) = model else {
            return Err(AppError::bad_request("无效的 id"));
        };

        validate_extra_body_json(dto.extra_body_json.as_deref())?;

        let code = dto.code.trim().to_string();
        let code_lock = self
            .context
            .distributed_lock
            .try_lock(
                &format!("llm:app:code:{code}"),
                std::time::Duration::ZERO,
                1,
            )
            .await?
            .ok_or_else(|| AppError::bad_request("当前应用 Code 正在处理中，请稍后重试"))?;
        let result = async {
            let model_lock = self
                .context
                .distributed_lock
                .try_lock(
                    &format!("llm:model:id:{}", dto.llm_model_id),
                    std::time::Duration::ZERO,
                    1,
                )
                .await?
                .ok_or_else(|| AppError::bad_request("当前关联模型正在处理中，请稍后重试"))?;
            let result = async {
                let duplicate_count = llm_app::Entity::find()
                    .filter(llm_app::Column::Id.ne(llm_app_id))
                    .filter(llm_app::Column::Code.eq(code.clone()))
                    .not_deleted(llm_app::Column::DeleteTime)
                    .count(self.context.app.db)
                    .await
                    .map_err(to_app_error)?;

                if duplicate_count > 0 {
                    return Err(AppError::bad_request("Code 已存在"));
                }

                validate_llm_model_state(self.context.app.db, dto.llm_model_id, dto.is_enable)
                    .await?;

                if model.code == code
                    && model.name == dto.name.trim()
                    && model.llm_model_id == dto.llm_model_id
                    && model.system_prompt_template == dto.system_prompt_template
                    && model.prompt_template == dto.prompt_template.trim()
                    && model.extra_body_json == dto.extra_body_json
                    && model.is_enable == dto.is_enable
                    && model.remark == dto.remark
                    && model.update_user_id == Some(current_user.user_id)
                {
                    return Ok(true);
                }
                let mut active = model.into_active_model();
                active.code = Set(code);
                active.name = Set(dto.name.trim().to_string());
                active.llm_model_id = Set(dto.llm_model_id);
                active.system_prompt_template = Set(dto.system_prompt_template);
                active.prompt_template = Set(dto.prompt_template.trim().to_string());
                active.extra_body_json = Set(dto.extra_body_json);
                active.is_enable = Set(dto.is_enable);
                active.remark = Set(dto.remark);
                active.update_user_id = Set(Some(current_user.user_id));

                active
                    .update_versioned(self.context.app.db)
                    .await
                    .map_err(|e| AppError::internal(format!("更新 LLM 应用配置失败: {e}")))?;

                Ok(true)
            }
            .await;
            model_lock.release().await;
            result
        }
        .await;
        code_lock.release().await;
        result
    }

    /// 软删除 LLM 应用配置
    pub async fn delete_llm_app(
        &self,
        llm_app_id: i64,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        let model = llm_app::Entity::find()
            .filter(llm_app::Column::Id.eq(llm_app_id))
            .not_deleted(llm_app::Column::DeleteTime)
            .one(self.context.app.db)
            .await
            .map_err(to_app_error)?;

        let Some(model) = model else {
            return Ok(true);
        };

        let mut active = model.into_active_model();
        active.delete_time = Set(Some(Utc::now().into()));
        active.delete_user_id = Set(Some(current_user.user_id));

        active
            .update_versioned(self.context.app.db)
            .await
            .map_err(|e| AppError::internal(format!("删除 LLM 应用配置失败: {e}")))?;

        Ok(true)
    }

    /// 执行 LLM 应用测试调用
    ///
    /// 通过应用层客户端工厂执行一次模板渲染后的测试调用
    pub async fn test_llm_app(
        &self,
        actor_user_id: i64,
        request: TestLlmAppRequest,
    ) -> Result<TestLlmAppResult, AppError> {
        let model_config = self
            .context
            .model_config_resolver
            .get_config(request.llm_model_id)
            .await?
            .ok_or_else(|| AppError::bad_request("无效的 LlmModelId 或模型已禁用"))?;
        let client =
            ModelConfigAppLlmClient::new(model_config.clone(), self.context.client_factory)?;

        let parameters = LlmParameters::from_map(
            request.parameters.unwrap_or_default(),
            LlmParameterComparison::OrdinalIgnoreCase,
        )?;
        validate_required_parameters(
            request.system_prompt_template.as_deref(),
            Some(request.prompt_template.as_str()),
            &parameters,
        )?;

        let system_prompt = render_template(request.system_prompt_template.as_deref(), &parameters);
        let prompt = render_template(Some(request.prompt_template.as_str()), &parameters)
            .unwrap_or_default();

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
            content: prompt,
        });

        let chat_request = AppLlmChatRequest {
            model: model_config.model_id,
            messages,
            user: Some(actor_user_id.to_string()),
            extra_body: request.extra_body,
        };

        let response = client.chat(chat_request).await?;

        Ok(TestLlmAppResult {
            model: response.model,
            response_id: response.id,
            content: response
                .choices
                .first()
                .map(|choice| choice.message.content.clone()),
            usage: response.usage.map(to_test_usage),
        })
    }
}

/// 校验编辑 LLM 应用配置请求
fn validate_edit_llm_app(dto: &EditLlmAppDto) -> Result<(), AppError> {
    if dto.code.trim().is_empty() {
        return Err(AppError::bad_request("Code 不可以空"));
    }

    if dto.name.trim().is_empty() {
        return Err(AppError::bad_request("名称不可以空"));
    }

    if dto.prompt_template.trim().is_empty() {
        return Err(AppError::bad_request("PromptTemplate 不可以空"));
    }

    Ok(())
}

/// 校验额外请求参数 JSON
fn validate_extra_body_json(extra_body_json: Option<&str>) -> Result<(), AppError> {
    validate_optional_json_object_string(extra_body_json).map_err(map_extra_body_json_string_error)
}

/// 映射 ExtraBodyJson 字符串错误
fn map_extra_body_json_string_error(error: JsonObjectStringError) -> AppError {
    match error {
        JsonObjectStringError::InvalidJson => {
            AppError::bad_request("ExtraBodyJson 不是合法的 JSON")
        }
        JsonObjectStringError::NotObject => AppError::bad_request(
            "ExtraBodyJson 必须是 JSON 对象（例如 {\"enable_thinking\":true}）",
        ),
    }
}

/// 校验模板必填参数
pub(crate) fn validate_required_parameters(
    system_prompt_template: Option<&str>,
    prompt_template: Option<&str>,
    parameters: &LlmParameters,
) -> Result<(), AppError> {
    let mut required_keys: Vec<String> = Vec::new();
    for key in extract_required_keys(system_prompt_template)
        .into_iter()
        .chain(extract_required_keys(prompt_template))
    {
        if !required_keys
            .iter()
            .any(|existing| eq_ordinal_ignore_case(existing, &key))
        {
            required_keys.push(key);
        }
    }

    if required_keys.is_empty() {
        return Ok(());
    }

    let missing = required_keys
        .into_iter()
        .filter(|key| {
            parameters
                .get(key)
                .map(|value| value.as_deref().unwrap_or_default())
                .map(|value| value.trim().is_empty())
                .unwrap_or(true)
        })
        .collect::<Vec<_>>();

    if missing.is_empty() {
        Ok(())
    } else {
        Err(AppError::bad_request(format!(
            "必传参数未填写: {}",
            missing.join("、")
        )))
    }
}

/// 提取必填占位参数
fn extract_required_keys(template: Option<&str>) -> Vec<String> {
    let Some(template) = template.filter(|item| !item.trim().is_empty()) else {
        return Vec::new();
    };

    placeholder_regex()
        .captures_iter(template)
        .filter(|capture| capture.name("required").is_some())
        .filter_map(|capture| {
            capture
                .name("key")
                .map(|item| item.as_str().trim().to_string())
        })
        .filter(|item| !item.is_empty())
        .collect()
}

/// 渲染提示词模板
pub(crate) fn render_template(
    template: Option<&str>,
    parameters: &LlmParameters,
) -> Option<String> {
    let template = template?;

    Some(
        placeholder_regex()
            .replace_all(template, |captures: &regex::Captures<'_>| {
                let key = captures
                    .name("key")
                    .map(|item| item.as_str())
                    .unwrap_or_default();

                parameters
                    .get(key)
                    .map(|value| value.as_deref().unwrap_or_default())
                    .map(str::to_string)
                    .unwrap_or_else(|| {
                        captures
                            .get(0)
                            .map(|item| item.as_str())
                            .unwrap_or_default()
                            .to_string()
                    })
            })
            .to_string(),
    )
}

/// 获取占位符正则
fn placeholder_regex() -> &'static Regex {
    static PLACEHOLDER_REGEX: OnceLock<Regex> = OnceLock::new();

    PLACEHOLDER_REGEX.get_or_init(|| {
        Regex::new(r"\{\{\s*(?P<required>\*)?\s*(?P<key>[^{}|\s]+)\s*(?:\|\s*(?P<comment>[^{}]*?))?\s*\}\}")
            .expect("placeholder regex must be valid")
    })
}

/// 转换应用层用量为测试返回用量
fn to_test_usage(usage: AppLlmUsage) -> TestLlmAppUsage {
    TestLlmAppUsage {
        prompt_tokens: usage.prompt_tokens,
        completion_tokens: usage.completion_tokens,
        total_tokens: usage.total_tokens,
    }
}

/// 将实体模型转换为返回模型
fn to_llm_app_dto(
    model: llm_app::Model,
    model_names: &std::collections::HashMap<i64, String>,
) -> LlmAppDto {
    LlmAppDto {
        id: model.id.to_string(),
        code: model.code,
        name: model.name,
        llm_model_id: model.llm_model_id.to_string(),
        llm_model_name: model_names
            .get(&model.llm_model_id)
            .cloned()
            .unwrap_or_default(),
        system_prompt_template: model.system_prompt_template,
        prompt_template: model.prompt_template,
        extra_body_json: model.extra_body_json,
        is_enable: model.is_enable,
        remark: model.remark,
        create_time: model.create_time.to_rfc3339(),
        update_time: model.update_time.map(|item| item.to_rfc3339()),
    }
}

/// 加载模型名称映射
async fn load_model_names(
    db: &sea_orm::DatabaseConnection,
    model_ids: Vec<i64>,
) -> Result<std::collections::HashMap<i64, String>, AppError> {
    if model_ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }

    let models = llm_model::Entity::find()
        .filter(llm_model::Column::Id.is_in(model_ids))
        .all(db)
        .await
        .map_err(to_app_error)?;

    Ok(models
        .into_iter()
        .map(|model| (model.id, model.name))
        .collect())
}

/// 校验 LLM 应用关联模型的状态
async fn validate_llm_model_state(
    db: &sea_orm::DatabaseConnection,
    model_id: i64,
    app_is_enable: bool,
) -> Result<(), AppError> {
    let model = llm_model::Entity::find()
        .filter(llm_model::Column::Id.eq(model_id))
        .not_deleted(llm_model::Column::DeleteTime)
        .one(db)
        .await
        .map_err(to_app_error)?;

    let Some(model) = model else {
        return Err(AppError::bad_request("无效的 LlmModelId"));
    };

    if app_is_enable && !model.is_enable {
        return Err(AppError::bad_request("启用的 LLM 应用不可以关联已禁用模型"));
    }

    Ok(())
}

/// 将数据库错误转换为应用层错误
fn to_app_error(error: sea_orm::DbErr) -> AppError {
    AppError::internal(error.to_string())
}
