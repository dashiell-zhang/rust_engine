use async_trait::async_trait;
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DbBackend, DbErr, ExecResult, QueryResult, Statement,
};

/// 数据库只读连接
///
/// 该类型只向上层开放查询能力并拒绝 SeaORM 的写执行入口
/// 宿主同时将连接会话配置为默认只读以覆盖带返回值的写语句
/// 生产环境仍应为读连接配置数据库侧只读账号形成双重保护
pub struct ReadDatabaseConnection {
    /// 底层数据库连接
    connection: DatabaseConnection,
}

impl ReadDatabaseConnection {
    /// 创建独立的 PostgreSQL 只读连接池，同时在会话层禁止写入。
    pub async fn connect(mut options: sea_orm::ConnectOptions) -> Result<Self, DbErr> {
        options.map_sqlx_postgres_opts(|options| {
            options.options([("default_transaction_read_only", "on")])
        });
        sea_orm::Database::connect(options).await.map(Self::new)
    }

    /// 创建数据库只读连接
    pub fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }
}

#[async_trait]
impl ConnectionTrait for ReadDatabaseConnection {
    /// 获取数据库后端类型
    fn get_database_backend(&self) -> DbBackend {
        self.connection.get_database_backend()
    }

    /// 拒绝执行写命令
    async fn execute_raw(&self, _statement: Statement) -> Result<ExecResult, DbErr> {
        Err(read_only_error())
    }

    /// 拒绝执行未预编译写命令
    async fn execute_unprepared(&self, _sql: &str) -> Result<ExecResult, DbErr> {
        Err(read_only_error())
    }

    /// 查询单条数据
    async fn query_one_raw(&self, statement: Statement) -> Result<Option<QueryResult>, DbErr> {
        self.connection.query_one_raw(statement).await
    }

    /// 查询多条数据
    async fn query_all_raw(&self, statement: Statement) -> Result<Vec<QueryResult>, DbErr> {
        self.connection.query_all_raw(statement).await
    }
}

/// 只读连接参与健康检查，不向调用方泄露底层写连接。
#[async_trait]
impl crate::database_health::DatabaseHealthConnection for ReadDatabaseConnection {
    async fn check_connection(
        &self,
        stopping: &cancellation::CancellationToken,
    ) -> Result<(), DbErr> {
        crate::database_health::DatabaseHealthConnection::check_connection(
            &self.connection,
            stopping,
        )
        .await
    }
}

/// 创建只读连接写入异常
fn read_only_error() -> DbErr {
    DbErr::Custom("ReadDatabaseConnection 为只读连接，不允许执行写命令".to_string())
}
