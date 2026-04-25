use sea_orm::{DatabaseConnection, DbErr, EntityTrait, QuerySelect};

use crate::user;

/// 检查数据库连接健康状态
/// 查询 User 表首条记录，验证连接可用且表结构正常
pub async fn check_database_health(db: &DatabaseConnection) -> Result<(), DbErr> {
    user::Entity::find()
        .select_only()
        .column(user::Column::Id)
        .one(db)
        .await
        .map(|_| ())
}
