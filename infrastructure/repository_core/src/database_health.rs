use async_trait::async_trait;
use cancellation::CancellationToken;
use sea_orm::{DatabaseConnection, DbErr, sqlx};

use crate::postgres_operation::PostgresOperation;

/// 仅暴露连接探测能力，避免为了检查只读库而公开其底层写连接。
#[async_trait]
pub trait DatabaseHealthConnection: Send + Sync {
    async fn check_connection(&self, stopping: &CancellationToken) -> Result<(), DbErr>;
}

#[async_trait]
impl DatabaseHealthConnection for DatabaseConnection {
    async fn check_connection(&self, stopping: &CancellationToken) -> Result<(), DbErr> {
        let mut operation =
            PostgresOperation::acquire(self.get_postgres_connection_pool(), stopping)
                .await
                .map_err(DbErr::Custom)?;
        operation
            .query(|connection| {
                Box::pin(async move { sqlx::query("SELECT 1").fetch_one(connection).await })
            })
            .await
            .map_err(DbErr::Custom)?;
        operation.commit().await.map_err(DbErr::Custom)
    }
}

/// 检查选定连接的可用性，不依赖业务表，也不修改数据库结构。
pub async fn check_database_health<C: DatabaseHealthConnection>(db: &C) -> Result<(), DbErr> {
    check_database_health_with_cancellation(db, &CancellationToken::new()).await
}

/// 超时和调用方取消均通知数据库操作，并等待其清理；清理仍受宿主停机预算约束。
pub async fn check_database_health_with_cancellation<C: DatabaseHealthConnection>(
    db: &C,
    cancellation: &CancellationToken,
) -> Result<(), DbErr> {
    let stopping = cancellation.child_token();
    let check = db.check_connection(&stopping);
    tokio::pin!(check);
    tokio::select! {
        result = &mut check => result,
        _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {
            stopping.cancel();
            let _ = check.await;
            Err(DbErr::Custom("数据库健康检查超时".into()))
        }
    }
}
