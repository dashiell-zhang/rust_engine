use application_interface::context::AppContext;
use chrono::{Duration, Utc};
use common::app_error::AppError;
use repository::extensions::SoftDeleteQueryExt;
use repository::{queue_task, queue_task::QueueTaskStatus, user_token};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter,
    QueryOrder, QuerySelect,
};
use serde::Deserialize;

/// 系统任务应用服务
pub struct SystemTaskService<'a> {
    /// 应用层上下文
    context: AppContext<'a>,
}

/// 用户 Token 清理结果
pub struct CleanUserTokenResult {
    /// 实际清理数量
    pub deleted_count: usize,

    /// 保留天数
    pub retain_days: i64,

    /// 批量大小
    pub batch_size: u64,
}

/// 用户 Token 清理参数
#[derive(Deserialize, Clone, Default)]
struct CleanUserTokenParameter {
    /// 保留天数
    retain_days: Option<i64>,

    /// 单次清理批量大小
    batch_size: Option<u64>,
}

impl<'a> SystemTaskService<'a> {
    /// 创建系统任务应用服务
    pub fn new(context: AppContext<'a>) -> Self {
        Self { context }
    }

    /// 清理超过保留天数的用户 Token
    ///
    /// 按保留天数与批量大小筛选旧 Token 并执行软删除
    pub async fn clean_user_tokens(
        &self,
        parameter: Option<&str>,
    ) -> Result<CleanUserTokenResult, AppError> {
        let parameter = parse_clean_user_token_parameter(parameter)?;
        let retain_days = parameter.retain_days.unwrap_or(30).max(1);
        let batch_size = parameter.batch_size.unwrap_or(500).clamp(1, 5_000);
        let expire_before = Utc::now() - Duration::days(retain_days);

        let token_list = user_token::Entity::find()
            .not_deleted(user_token::Column::DeleteTime)
            .filter(user_token::Column::CreateTime.lt(expire_before))
            .order_by_asc(user_token::Column::CreateTime)
            .limit(batch_size)
            .all(self.context.db)
            .await
            .map_err(|error| AppError::internal(format!("加载待清理用户 Token 失败: {error}")))?;

        let deleted_count = token_list.len();

        for token in token_list {
            let mut active = token.into_active_model();
            active.delete_time = Set(Some(Utc::now().into()));
            active.update(self.context.db).await.map_err(|error| {
                AppError::internal(format!("更新用户 Token 删除状态失败: {error}"))
            })?;
        }

        Ok(CleanUserTokenResult {
            deleted_count,
            retain_days,
            batch_size,
        })
    }

    /// 创建并投递队列任务
    ///
    /// 写入待执行队列任务记录供任务宿主后续拉取执行
    pub async fn enqueue_queue_task(
        &self,
        queue_task_name: &str,
        parameter: Option<String>,
    ) -> Result<(), AppError> {
        if queue_task_name.trim().is_empty() {
            return Err(AppError::bad_request("队列任务名称不可以为空"));
        }

        let queue_task_id = self
            .context
            .id_generator
            .get_id()
            .await
            .map_err(|error| AppError::internal(format!("生成队列任务主键失败: {error}")))?;

        queue_task::ActiveModel {
            id: Set(queue_task_id),
            name: Set(queue_task_name.to_string()),
            parameter: Set(parameter),
            status: Set(QueueTaskStatus::Pending),
            count: Set(0),
            first_time: Set(None),
            last_time: Set(None),
            success_time: Set(None),
            create_time: Set(Utc::now().into()),
            delete_time: Set(None),
            plan_time: Set(None),
            worker_id: Set(None),
            lease_expire_time: Set(None),
            callback_name: Set(None),
            callback_parameter: Set(None),
            child_success_time: Set(None),
            parent_task_id: Set(None),
        }
        .insert(self.context.db)
        .await
        .map_err(|error| AppError::internal(format!("投递队列任务失败: {error}")))?;

        Ok(())
    }
}

/// 解析用户 Token 清理参数
fn parse_clean_user_token_parameter(
    parameter: Option<&str>,
) -> Result<CleanUserTokenParameter, AppError> {
    let Some(parameter) = parameter.map(str::trim).filter(|item| !item.is_empty()) else {
        return Ok(CleanUserTokenParameter::default());
    };

    serde_json::from_str::<CleanUserTokenParameter>(parameter)
        .map_err(|error| AppError::bad_request(format!("用户 Token 清理任务参数格式无效: {error}")))
}
