mod migrator;
mod schema;
mod settings;

use clap::{Parser, Subcommand};
use sea_orm::{ConnectOptions, Database};
use settings::Settings;
use std::time::Duration;

/// 数据库工具命令行参数
#[derive(Parser)]
#[command(author, version, about = "rust_engine 数据库迁移工具")]
struct Cli {
    /// 配置文件路径，不含扩展名
    #[arg(long, global = true)]
    config_path: Option<String>,

    /// 数据库连接字符串
    #[arg(long, global = true)]
    database_url: Option<String>,

    /// 需要执行的命令
    #[command(subcommand)]
    command: Command,
}

/// 数据库工具子命令
#[derive(Subcommand)]
enum Command {
    /// 执行数据库迁移
    Migrate,
}

#[tokio::main]
/// 启动数据库工具
async fn main() {
    if let Err(error) = run().await {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

/// 执行数据库工具命令
async fn run() -> Result<(), String> {
    let cli = Cli::parse();

    match cli.command {
        Command::Migrate => {
            let db = connect_database(cli.config_path, cli.database_url).await?;
            migrator::migrate(&db).await?;
            println!("数据库迁移完成");
        }
    }

    Ok(())
}

/// 建立数据库连接
async fn connect_database(
    config_path: Option<String>,
    database_url: Option<String>,
) -> Result<sea_orm::DatabaseConnection, String> {
    let settings = Settings::load(config_path, database_url)?;
    Database::connect(build_postgres_connect_options(settings.database.connection))
        .await
        .map_err(|error| format!("数据库连接失败: {error}"))
}

/// 创建数据库工具默认 PostgreSQL 连接配置
fn build_postgres_connect_options(connection: impl Into<String>) -> ConnectOptions {
    let mut options = ConnectOptions::new(connection.into());

    options.test_before_acquire(false);

    options
        .max_connections(30)
        .min_connections(1)
        .connect_timeout(Duration::from_secs(50))
        .idle_timeout(Duration::from_secs(300))
        .sqlx_logging(false);

    options
}
