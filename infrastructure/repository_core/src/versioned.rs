//! PostgreSQL xmin 乐观并发控制。版本来自读取的行，不能在写入前另查一个新版本替代。
use async_trait::async_trait;
use sea_orm::{
    ActiveModelBehavior, ActiveModelTrait, ActiveValue, ColumnTrait, ConnectionTrait, DbBackend,
    DbErr, DeleteResult, EntityTrait, IntoActiveModel, Iterable, ModelTrait, PrimaryKeyToColumn,
    QueryFilter, Schema, TransactionSession, TransactionTrait, Value,
    sea_query::{Condition, TableCreateStatement},
};

/// xmin 通过 PostgreSQL text 转换读取，避免驱动把 xid 当成普通有符号整型。
pub trait VersionedEntity: EntityTrait {
    fn row_version_column() -> Self::Column;
}

#[macro_export]
macro_rules! impl_versioned_entity {
    ($entity:ty, $column:expr) => {
        impl $crate::versioned::VersionedEntity for $entity {
            fn row_version_column() -> Self::Column {
                $column
            }
        }
    };
}

/// PostgreSQL 自动提供 xmin，建表时不得把它当作用户列创建。
/// 保留 SeaORM 生成的其他列、主键、外键和注释，在语句树上处理，不改写 SQL 文本。
pub fn create_table_from_entity<E: EntityTrait>(
    entity: E,
    backend: DbBackend,
) -> TableCreateStatement {
    let original = Schema::new(backend).create_table_from_entity(entity);
    let mut table = TableCreateStatement::new();
    table.table(entity.table_ref());
    for column in original.get_columns() {
        if column.get_column_name() != "xmin" {
            table.col(column.clone());
        }
    }
    for index in original.get_indexes() {
        table.index(&mut index.clone());
    }
    for foreign_key in original.get_foreign_key_create_stmts() {
        table.foreign_key(&mut foreign_key.clone());
    }
    if let Some(comment) = original.get_comment() {
        table.comment(comment);
    }
    table
}

fn primary_key_condition<A: ActiveModelTrait>(model: &A) -> Result<Condition, DbErr> {
    let mut condition = Condition::all();
    for key in <A::Entity as EntityTrait>::PrimaryKey::iter() {
        let column = key.into_column();
        let value = match model.get(column) {
            ActiveValue::Set(value) | ActiveValue::Unchanged(value) if value != value.as_null() => {
                value
            }
            _ => return Err(DbErr::Custom("保存前必须确定全部主键值".into())),
        };
        condition = condition.add(column.eq(value));
    }
    Ok(condition)
}

fn expected_version<A: ActiveModelTrait>(model: &A) -> Result<Value, DbErr>
where
    A::Entity: VersionedEntity,
{
    match model.get(A::Entity::row_version_column()) {
        ActiveValue::Set(Value::String(Some(value)))
        | ActiveValue::Unchanged(Value::String(Some(value))) => {
            let version = value
                .parse::<u32>()
                .map_err(|_| DbErr::Custom("xmin 行版本无效".into()))?;
            Ok(version.to_string().into())
        }
        _ => Err(DbErr::Custom(
            "更新或删除必须携带读取时的 xmin 行版本".into(),
        )),
    }
}

fn require_postgres(db: &impl ConnectionTrait) -> Result<(), DbErr> {
    if db.get_database_backend() != DbBackend::Postgres {
        return Err(DbErr::Custom("xmin 持久化仅支持 PostgreSQL".into()));
    }
    Ok(())
}

/// 批量保存及回读，按有界批次限制 PostgreSQL 参数数量，返回顺序与输入一致。
pub async fn insert_many_versioned<A, C>(
    models: Vec<A>,
    db: &C,
) -> Result<Vec<<A::Entity as EntityTrait>::Model>, DbErr>
where
    A: ActiveModelBehavior + Send,
    A::Entity: VersionedEntity,
    <A::Entity as EntityTrait>::Model: IntoActiveModel<A>,
    C: ConnectionTrait + TransactionTrait,
{
    require_postgres(db)?;
    if models.is_empty() {
        return Ok(Vec::new());
    }
    let transaction = db.begin().await?;
    let result = async {
        let mut saved = Vec::with_capacity(models.len());
        let columns = <A::Entity as EntityTrait>::Column::iter().count().max(1);
        let batch_size = (60_000 / columns).clamp(1, 256);
        let mut input = models.into_iter();
        loop {
            let batch: Vec<_> = input.by_ref().take(batch_size).collect();
            if batch.is_empty() {
                break;
            }
            let mut active_models = Vec::with_capacity(batch.len());
            let mut keys = Vec::with_capacity(batch.len());
            let mut condition = Condition::any();
            for active in batch {
                let mut active = A::before_save(active, &transaction, true).await?;
                active.not_set(A::Entity::row_version_column());
                condition = condition.add(primary_key_condition(&active)?);
                keys.push(
                    <A::Entity as EntityTrait>::PrimaryKey::iter()
                        .map(|key| {
                            let column = key.into_column();
                            (column, active.get(column).unwrap())
                        })
                        .collect::<Vec<_>>(),
                );
                active_models.push(active);
            }
            let affected = A::Entity::insert_many(active_models)
                .exec_without_returning(&transaction)
                .await?;
            if affected != keys.len() as u64 {
                return Err(DbErr::RecordNotInserted);
            }
            let mut rows = A::Entity::find()
                .filter(condition)
                .all(&transaction)
                .await?;
            if rows.len() != keys.len() {
                return Err(DbErr::Custom("批量新增行的 xmin 回读数量不一致".into()));
            }
            for key in keys {
                let index = rows
                    .iter()
                    .position(|row| key.iter().all(|(column, value)| row.get(*column) == *value))
                    .ok_or_else(|| DbErr::Custom("批量 xmin 回读缺少主键对应的行".into()))?;
                let model = rows.swap_remove(index);
                saved.push(A::after_save(model, &transaction, true).await?);
            }
        }
        Ok(saved)
    }
    .await;
    match result {
        Ok(models) => {
            transaction.commit().await?;
            Ok(models)
        }
        Err(error) => {
            transaction.rollback().await.map_err(|rollback| {
                DbErr::Custom(format!("保存失败: {error}；回滚失败: {rollback}"))
            })?;
            Err(error)
        }
    }
}

#[async_trait]
pub trait VersionedActiveModel: ActiveModelBehavior
where
    Self::Entity: VersionedEntity,
    <Self::Entity as EntityTrait>::Model: IntoActiveModel<Self>,
{
    /// 写入和版本回读共用事务；已有事务时使用保存点，不提交调用方事务。
    /// 所有表采用同一路径，因此已有分区无需额外策略声明也不会 RETURNING xmin。
    async fn insert_versioned<C>(
        self,
        db: &C,
    ) -> Result<<Self::Entity as EntityTrait>::Model, DbErr>
    where
        C: ConnectionTrait + TransactionTrait,
    {
        insert_many_versioned(vec![self], db)
            .await?
            .pop()
            .ok_or(DbErr::RecordNotInserted)
    }

    /// 原始版本加入 UPDATE 条件，成功后返回数据库生成的新版本。
    async fn update_versioned<C>(
        self,
        db: &C,
    ) -> Result<<Self::Entity as EntityTrait>::Model, DbErr>
    where
        C: ConnectionTrait,
    {
        require_postgres(db)?;
        let version_column = Self::Entity::row_version_column();
        let expected = expected_version(&self)?;
        let mut active = Self::before_save(self, db, false).await?;
        active.not_set(version_column);
        let has_changes = <Self::Entity as EntityTrait>::Column::iter().any(|column| {
            <Self::Entity as EntityTrait>::PrimaryKey::from_column(column).is_none()
                && active.get(column).is_set()
        });
        let model = if has_changes {
            Self::Entity::update(active)
                .validate()?
                .filter(version_column.eq(expected))
                .exec(db)
                .await?
        } else {
            // SeaORM 空更新默认只按主键重读；仍需验证原始版本，不能静默接受被别人修改的行。
            Self::Entity::find()
                .filter(primary_key_condition(&active)?)
                .filter(version_column.eq(expected))
                .one(db)
                .await?
                .ok_or(DbErr::RecordNotUpdated)?
        };
        Self::after_save(model, db, false).await
    }

    async fn delete_versioned<C>(self, db: &C) -> Result<DeleteResult, DbErr>
    where
        C: ConnectionTrait,
    {
        require_postgres(db)?;
        let expected = expected_version(&self)?;
        let active = Self::before_delete(self, db).await?;
        let result = Self::Entity::delete_many()
            .filter(primary_key_condition(&active)?)
            .filter(Self::Entity::row_version_column().eq(expected))
            .exec(db)
            .await?;
        if result.rows_affected != 1 {
            return Err(DbErr::RecordNotUpdated);
        }
        Self::after_delete(active, db).await?;
        Ok(result)
    }
}

impl<A> VersionedActiveModel for A
where
    A: ActiveModelBehavior,
    A::Entity: VersionedEntity,
    <A::Entity as EntityTrait>::Model: IntoActiveModel<A>,
{
}
