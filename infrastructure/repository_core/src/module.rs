//! 按业务模块类型固定数据库目标。注册后不允许覆盖；事务保留所属连接。
use crate::ReadDatabaseConnection;
use sea_orm::{ConnectOptions, DatabaseConnection, DatabaseTransaction, DbErr, TransactionTrait};
use std::{
    any::{Any, TypeId},
    collections::{HashMap, HashSet},
    marker::PhantomData,
    sync::Arc,
};

pub trait DatabaseModule: Send + Sync + 'static {
    const NAME: &'static str;
    const MIGRATION_HISTORY_TABLE: &'static str;
}

pub use common::database_settings::ModuleConnectionSettings;

pub struct ModuleDatabase<M: DatabaseModule> {
    write: Arc<DatabaseConnection>,
    read: Arc<ReadDatabaseConnection>,
    marker: PhantomData<fn() -> M>,
}
impl<M: DatabaseModule> Clone for ModuleDatabase<M> {
    fn clone(&self) -> Self {
        Self {
            write: self.write.clone(),
            read: self.read.clone(),
            marker: PhantomData,
        }
    }
}
impl<M: DatabaseModule> ModuleDatabase<M> {
    pub async fn connect(write: ConnectOptions, read: ConnectOptions) -> Result<Self, DbErr> {
        let write = Arc::new(sea_orm::Database::connect(write).await?);
        let read = Arc::new(ReadDatabaseConnection::connect(read).await?);
        Ok(Self {
            write,
            read,
            marker: PhantomData,
        })
    }
    pub fn write(&self) -> &DatabaseConnection {
        &self.write
    }
    pub fn read(&self) -> &ReadDatabaseConnection {
        &self.read
    }
    pub fn write_pool(&self) -> Arc<DatabaseConnection> {
        self.write.clone()
    }
    pub fn read_pool(&self) -> Arc<ReadDatabaseConnection> {
        self.read.clone()
    }
    pub async fn begin(&self) -> Result<ModuleTransaction<M>, DbErr> {
        Ok(ModuleTransaction {
            transaction: self.write.begin().await?,
            owner: self.write.clone(),
            marker: PhantomData,
        })
    }
}

/// 业务操作使用 connection()，事务入队验证 owner；只有调用方可以提交或回滚。
pub struct ModuleTransaction<M: DatabaseModule> {
    transaction: DatabaseTransaction,
    owner: Arc<DatabaseConnection>,
    marker: PhantomData<fn() -> M>,
}
impl<M: DatabaseModule> ModuleTransaction<M> {
    pub fn connection(&self) -> &DatabaseTransaction {
        &self.transaction
    }
    pub fn belongs_to(&self, database: &DatabaseConnection) -> bool {
        std::ptr::eq(self.owner.as_ref(), database)
    }
    pub async fn commit(self) -> Result<(), DbErr> {
        self.transaction.commit().await
    }
    pub async fn rollback(self) -> Result<(), DbErr> {
        self.transaction.rollback().await
    }
}

#[derive(Default)]
pub struct DatabaseModules {
    entries: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
    names: HashSet<&'static str>,
    connections: Vec<ModuleConnections>,
}
pub struct ModuleConnections {
    pub name: &'static str,
    pub write: Arc<DatabaseConnection>,
    pub read: Arc<ReadDatabaseConnection>,
}
impl DatabaseModules {
    pub fn connections(&self) -> &[ModuleConnections] {
        &self.connections
    }

    pub fn register<M: DatabaseModule>(&mut self, module: ModuleDatabase<M>) -> Result<(), String> {
        if M::NAME.trim().is_empty() || M::MIGRATION_HISTORY_TABLE.trim().is_empty() {
            return Err("模块名称和迁移历史名称不能为空".into());
        }
        if self.entries.contains_key(&TypeId::of::<M>()) || self.names.contains(M::NAME) {
            return Err(format!("数据库模块重复注册: {}", M::NAME));
        }
        self.connections.push(ModuleConnections {
            name: M::NAME,
            write: module.write_pool(),
            read: module.read_pool(),
        });
        self.names.insert(M::NAME);
        self.entries.insert(TypeId::of::<M>(), Box::new(module));
        Ok(())
    }
    pub fn get<M: DatabaseModule>(&self) -> Result<&ModuleDatabase<M>, String> {
        self.entries
            .get(&TypeId::of::<M>())
            .and_then(|value| value.downcast_ref())
            .ok_or_else(|| format!("数据库模块未注册: {}", M::NAME))
    }
}
