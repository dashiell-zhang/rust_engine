use chrono::Utc;
use repository::extensions::SoftDeleteQueryExt;
use repository::{llm_app, llm_model};
use repository_core::pagination::SignedPage;
use repository_core::versioned::VersionedActiveModel;
use sea_orm::{
    ColumnTrait, EntityTrait, ExprTrait, IntoActiveModel, PaginatorTrait, QueryFilter, QueryOrder,
    Set,
};

use application_interface::error::AppError;
use application_model::shared::models::{page_offset, validate_page_request};
use application_model::user::models::PageListDto;
use application_runtime::context::{CurrentUser, LlmModelContext};

use super::models::{
    EditLlmModelDto, LlmModelDto, LlmModelPageListDto, LlmModelPageRequest, LlmModelSelectDto,
};

/// LLM 模型配置服务
pub struct LlmModelService<'a> {
    /// 应用层上下文
    context: LlmModelContext<'a>,
}

impl<'a> LlmModelService<'a> {
    /// 创建 LLM 模型配置服务
    pub fn new(context: LlmModelContext<'a>) -> Self {
        Self { context }
    }

    /// 获取 LLM 模型配置分页列表
    pub async fn get_llm_model_list(
        &self,
        params: LlmModelPageRequest,
    ) -> Result<LlmModelPageListDto, AppError> {
        validate_page_request(params.page, params.page_size)?;

        let mut query = llm_model::Entity::find().not_deleted(llm_model::Column::DeleteTime);

        if let Some(keyword) = &params.keyword {
            let keyword = keyword.trim();
            if !keyword.is_empty() {
                let pattern = repository_core::extensions::contains_pattern(keyword);
                query = query.filter(
                    llm_model::Column::Name
                        .like(pattern.clone())
                        .or(llm_model::Column::ModelId.like(pattern.clone()))
                        .or(llm_model::Column::Remark.like(pattern)),
                );
            }
        }

        if let Some(is_enable) = params.is_enable {
            query = query.filter(llm_model::Column::IsEnable.eq(is_enable));
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
        let list = query
            .order_by_desc(llm_model::Column::Id)
            .signed_page(skip, params.page_size)
            .all(self.context.app.db)
            .await
            .map_err(to_app_error)?
            .into_iter()
            .map(to_llm_model_dto)
            .collect();

        Ok(PageListDto {
            total,
            list: Some(list),
        })
    }

    /// 获取 LLM 模型下拉列表
    pub async fn get_llm_model_select(&self) -> Result<Vec<LlmModelSelectDto>, AppError> {
        llm_model::Entity::find()
            .filter(llm_model::Column::IsEnable.eq(true))
            .not_deleted(llm_model::Column::DeleteTime)
            .order_by_asc(llm_model::Column::Name)
            .all(self.context.app.db)
            .await
            .map_err(to_app_error)
            .map(|items| {
                items
                    .into_iter()
                    .map(|item| LlmModelSelectDto {
                        id: item.id.to_string(),
                        name: item.name,
                    })
                    .collect()
            })
    }

    /// 创建 LLM 模型配置
    pub async fn create_llm_model(
        &self,
        dto: EditLlmModelDto,
        current_user: &CurrentUser,
    ) -> Result<String, AppError> {
        validate_create_llm_model(&dto)?;

        let name = dto.name.trim().to_string();
        let name_lock = self
            .context
            .distributed_lock
            .try_lock(
                &format!("llm:model:name:{name}"),
                std::time::Duration::ZERO,
                1,
            )
            .await?
            .ok_or_else(|| AppError::bad_request("当前模型名称正在处理中，请稍后重试"))?;
        let result = async {
            ensure_name_unique(self.context.app.db, &name, None).await?;

            let id = self
                .context
                .app
                .id_generator
                .get_id()
                .await
                .map_err(|error| AppError::internal(format!("生成 LLM 模型主键失败: {error}")))?;

            let model = llm_model::ActiveModel {
                row_version: Default::default(),
                id: Set(id),
                name: Set(name),
                model_id: Set(dto.model_id.trim().to_string()),
                endpoint: Set(dto.endpoint.trim().to_string()),
                api_key: Set(dto.api_key.unwrap_or_default().trim().to_string()),
                protocol_type: Set(dto.protocol_type),
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
                .map_err(|e| AppError::internal(format!("创建 LLM 模型配置失败: {e}")))?;

            Ok(id.to_string())
        }
        .await;
        name_lock.release().await;
        result
    }

    /// 更新 LLM 模型配置
    pub async fn update_llm_model(
        &self,
        id: i64,
        dto: EditLlmModelDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        validate_update_llm_model(&dto)?;

        let name = dto.name.trim().to_string();
        let name_lock = self
            .context
            .distributed_lock
            .try_lock(
                &format!("llm:model:name:{name}"),
                std::time::Duration::ZERO,
                1,
            )
            .await?
            .ok_or_else(|| AppError::bad_request("当前模型名称正在处理中，请稍后重试"))?;
        let result = async {
            let model_lock = self
                .context
                .distributed_lock
                .try_lock(&format!("llm:model:id:{id}"), std::time::Duration::ZERO, 1)
                .await?
                .ok_or_else(|| AppError::bad_request("当前模型正在处理中，请稍后重试"))?;
            let result = async {
                let model = llm_model::Entity::find()
                    .filter(llm_model::Column::Id.eq(id))
                    .not_deleted(llm_model::Column::DeleteTime)
                    .one(self.context.app.db)
                    .await
                    .map_err(to_app_error)?;

                let Some(model) = model else {
                    return Err(AppError::bad_request("无效的 id"));
                };

                ensure_name_unique(self.context.app.db, &name, Some(id)).await?;

                if model.is_enable && !dto.is_enable {
                    let enabled_app_count = llm_app::Entity::find()
                        .filter(llm_app::Column::LlmModelId.eq(id))
                        .filter(llm_app::Column::IsEnable.eq(true))
                        .not_deleted(llm_app::Column::DeleteTime)
                        .count(self.context.app.db)
                        .await
                        .map_err(to_app_error)?;

                    if enabled_app_count > 0 {
                        return Err(AppError::bad_request(
                            "当前模型下存在已启用的 LLM 应用，无法禁用",
                        ));
                    }
                }

                // EF 只有实际字段或修改者变化时才更新行版本与更新时间。
                if model.name == name
                    && model.model_id == dto.model_id.trim()
                    && model.endpoint == dto.endpoint.trim()
                    && normalize_optional_string(dto.api_key.as_deref())
                        .is_none_or(|key| model.api_key == key)
                    && model.protocol_type == dto.protocol_type
                    && model.is_enable == dto.is_enable
                    && model.remark == dto.remark
                    && model.update_user_id == Some(current_user.user_id)
                {
                    return Ok(true);
                }
                let mut active = model.into_active_model();
                active.name = Set(name);
                active.model_id = Set(dto.model_id.trim().to_string());
                active.endpoint = Set(dto.endpoint.trim().to_string());
                if let Some(api_key) = normalize_optional_string(dto.api_key.as_deref()) {
                    active.api_key = Set(api_key);
                }
                active.protocol_type = Set(dto.protocol_type);
                active.is_enable = Set(dto.is_enable);
                active.remark = Set(dto.remark);
                active.update_user_id = Set(Some(current_user.user_id));

                active
                    .update_versioned(self.context.app.db)
                    .await
                    .map_err(|e| AppError::internal(format!("更新 LLM 模型配置失败: {e}")))?;

                Ok(true)
            }
            .await;
            model_lock.release().await;
            result
        }
        .await;
        name_lock.release().await;
        result
    }

    /// 删除 LLM 模型配置
    pub async fn delete_llm_model(
        &self,
        id: i64,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        let model_lock = self
            .context
            .distributed_lock
            .try_lock(&format!("llm:model:id:{id}"), std::time::Duration::ZERO, 1)
            .await?
            .ok_or_else(|| AppError::bad_request("当前模型正在处理中，请稍后重试"))?;
        let result = async {
            let model = llm_model::Entity::find()
                .filter(llm_model::Column::Id.eq(id))
                .not_deleted(llm_model::Column::DeleteTime)
                .one(self.context.app.db)
                .await
                .map_err(to_app_error)?;

            let Some(model) = model else {
                return Ok(true);
            };

            let app_count = llm_app::Entity::find()
                .filter(llm_app::Column::LlmModelId.eq(id))
                .not_deleted(llm_app::Column::DeleteTime)
                .count(self.context.app.db)
                .await
                .map_err(to_app_error)?;

            if app_count > 0 {
                return Err(AppError::bad_request("当前模型下存在 LLM 应用，无法删除"));
            }

            let mut active = model.into_active_model();
            active.delete_time = Set(Some(Utc::now().into()));
            active.delete_user_id = Set(Some(current_user.user_id));

            active
                .update_versioned(self.context.app.db)
                .await
                .map_err(|e| AppError::internal(format!("删除 LLM 模型配置失败: {e}")))?;

            Ok(true)
        }
        .await;
        model_lock.release().await;
        result
    }
}

/// 校验创建请求
fn validate_create_llm_model(dto: &EditLlmModelDto) -> Result<(), AppError> {
    validate_common_llm_model(dto)?;

    if dto
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none()
    {
        return Err(AppError::bad_request("接口密钥不可以空"));
    }

    Ok(())
}

/// 校验更新请求
fn validate_update_llm_model(dto: &EditLlmModelDto) -> Result<(), AppError> {
    validate_common_llm_model(dto)
}

/// 校验通用字段
fn validate_common_llm_model(dto: &EditLlmModelDto) -> Result<(), AppError> {
    if dto.name.trim().is_empty() {
        return Err(AppError::bad_request("名称不可以空"));
    }

    if dto.model_id.trim().is_empty() {
        return Err(AppError::bad_request("模型标识不可以空"));
    }

    if dto.endpoint.trim().is_empty() {
        return Err(AppError::bad_request("端点地址不可以空"));
    }

    if llm::LlmProtocolType::try_from(dto.protocol_type).is_err() {
        return Err(AppError::bad_request("无效的协议类型"));
    }

    Ok(())
}

/// 确认名称唯一
async fn ensure_name_unique(
    db: &sea_orm::DatabaseConnection,
    name: &str,
    exclude_id: Option<i64>,
) -> Result<(), AppError> {
    let mut query = llm_model::Entity::find()
        .filter(llm_model::Column::Name.eq(name.to_string()))
        .not_deleted(llm_model::Column::DeleteTime);

    if let Some(exclude_id) = exclude_id {
        query = query.filter(llm_model::Column::Id.ne(exclude_id));
    }

    let exists = query.count(db).await.map_err(to_app_error)?;
    if exists > 0 {
        return Err(AppError::bad_request("名称已存在"));
    }

    Ok(())
}

/// 转换实体为 DTO
fn to_llm_model_dto(model: llm_model::Model) -> LlmModelDto {
    LlmModelDto {
        id: model.id.to_string(),
        name: model.name,
        model_id: model.model_id,
        endpoint: model.endpoint,
        protocol_type: model.protocol_type,
        is_enable: model.is_enable,
        remark: model.remark,
        create_time: model.create_time.to_rfc3339(),
        update_time: model.update_time.map(|item| item.to_rfc3339()),
    }
}

/// 规整可选字符串
fn normalize_optional_string(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
}

/// 将数据库错误转换为应用层错误
fn to_app_error(error: sea_orm::DbErr) -> AppError {
    AppError::internal(error.to_string())
}
