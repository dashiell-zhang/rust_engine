use application_model::task_center::ARGS_DEFAULT_PARAMETER;
use common::identifier_generator::IdentifierGenerator;
use repository::{extensions::SoftDeleteQueryExt, task_setting};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection, EntityTrait};

use crate::registry::TaskRegistry;

/// 任务定义同步器
pub struct TaskRegistrySynchronizer;

impl TaskRegistrySynchronizer {
    /// 同步任务注册表到任务配置表
    pub async fn sync_task_settings(
        db: &DatabaseConnection,
        registry: &TaskRegistry,
        identifier_generator: &IdentifierGenerator,
    ) -> Result<TaskRegistrySyncResult, String> {
        let existing_models = task_setting::Entity::find()
            .not_deleted(task_setting::Column::DeleteTime)
            .all(db)
            .await
            .map_err(|error| format!("加载任务配置失败: {error}"))?;

        let mut result = TaskRegistrySyncResult::default();

        for definition in registry.queue_definitions() {
            let exists = existing_models
                .iter()
                .any(|item| item.category == "QueueTask" && item.name == definition.name);

            if exists {
                result.existing_queue_names.push(definition.name);
                continue;
            }

            let task_setting_id = identifier_generator
                .get_id()
                .await
                .map_err(|error| format!("生成任务配置 ID 失败: {error}"))?;

            task_setting::ActiveModel {
                id: Set(task_setting_id),
                name: Set(definition.name.clone()),
                category: Set("QueueTask".to_string()),
                semaphore: Set(Some(definition.semaphore)),
                cron: Set(None),
                remark: Set(definition.remark.clone()),
                create_time: Set(chrono::Utc::now().into()),
                delete_time: Set(None),
                update_time: Set(None),
                create_user_id: Set(None),
                update_user_id: Set(None),
                delete_user_id: Set(None),
                is_enable: Set(true),
                parameter: Set(None),
            }
            .insert(db)
            .await
            .map_err(|error| format!("写入队列任务配置失败: {error}"))?;

            result.created_queue_names.push(definition.name);
        }

        for definition in registry.schedule_definitions() {
            let exists = existing_models.iter().any(|item| {
                item.category == "ScheduleTask"
                    && item.name == definition.name
                    && (!definition.supports_parameter
                        || item.parameter.as_deref() == Some(ARGS_DEFAULT_PARAMETER))
            });

            if exists {
                result.existing_schedule_names.push(definition.name);
                continue;
            }

            let task_setting_id = identifier_generator
                .get_id()
                .await
                .map_err(|error| format!("生成任务配置 ID 失败: {error}"))?;

            task_setting::ActiveModel {
                id: Set(task_setting_id),
                name: Set(definition.name.clone()),
                category: Set("ScheduleTask".to_string()),
                semaphore: Set(None),
                cron: Set(definition.cron.clone()),
                remark: Set(definition.remark.clone()),
                create_time: Set(chrono::Utc::now().into()),
                delete_time: Set(None),
                update_time: Set(None),
                create_user_id: Set(None),
                update_user_id: Set(None),
                delete_user_id: Set(None),
                is_enable: Set(!definition.supports_parameter),
                parameter: Set(if definition.supports_parameter {
                    Some(ARGS_DEFAULT_PARAMETER.to_string())
                } else {
                    None
                }),
            }
            .insert(db)
            .await
            .map_err(|error| format!("写入定时任务配置失败: {error}"))?;

            result.created_schedule_names.push(definition.name);
        }

        Ok(result)
    }
}

/// 任务定义同步结果
#[derive(Default)]
pub struct TaskRegistrySyncResult {
    /// 已创建的队列任务名称
    pub created_queue_names: Vec<String>,

    /// 已存在的队列任务名称
    pub existing_queue_names: Vec<String>,

    /// 已创建的定时任务名称
    pub created_schedule_names: Vec<String>,

    /// 已存在的定时任务名称
    pub existing_schedule_names: Vec<String>,
}
