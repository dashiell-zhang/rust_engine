mod migrator;
mod schema;
mod settings;

use clap::{CommandFactory, Parser, Subcommand};
use sea_orm::{ConnectOptions, Database};
use settings::{DatabaseTarget, Settings};

/// 数据库工具命令行参数
#[derive(Parser)]
#[command(author, version, about = "rust_engine 数据库迁移工具")]
struct Cli {
    /// 迁移目标，每个写模型仅操作自己的表和迁移历史。
    #[arg(long, global = true, value_enum, ignore_case = true)]
    target: Option<DatabaseTarget>,

    /// 选择工具配置中的命名数据库模块，模型必须与 target 匹配。
    #[arg(long, global = true)]
    module: Option<String>,

    /// 需要执行的命令
    #[command(subcommand)]
    command: Option<Command>,
}

/// 数据库工具子命令
#[derive(Subcommand)]
enum Command {
    /// 离线生成增量迁移、反向操作与目标模型；不修改数据库
    MigrationAdd {
        name: String,
        /// 自定义生成目录；默认写入所选模型的源码目录
        #[arg(long)]
        output_dir: Option<std::path::PathBuf>,
    },
    /// 检查当前模型是否存在尚未生成迁移的变化，不连接数据库
    MigrationPending,
    /// 离线展示当前模型（不会替换历史快照）
    ModelSnapshot {
        /// 输出当前模型的建库 SQL 供审查，不包含迁移历史
        #[arg(long)]
        sql: bool,
    },
    /// 将所选数据库模型迁移到指定版本；省略版本时升级到最新
    Migrate {
        /// 目标版本、名称或完整标识；0 撤销全部迁移（可能删除数据）
        #[arg(long)]
        to: Option<String>,
    },
    /// 离线导出迁移 SQL，不连接数据库或执行结构变更
    MigrationScript {
        /// 起点为已应用迁移的版本、名称或完整标识；默认 0
        #[arg(long)]
        from: Option<String>,
        /// 终点为要应用到的迁移；默认最新
        #[arg(long)]
        to: Option<String>,
        /// 按迁移历史跳过已应用的升级或已撤销的回退步骤
        #[arg(long)]
        idempotent: bool,
        /// 输出文件（容器内路径）；省略时写入标准输出
        #[arg(short, long)]
        output: Option<std::path::PathBuf>,
    },
    /// 只读取所选模型的索引目录，输出 JSON 索引差异 SQL，不执行 DDL
    JsonIndex,
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

    let Some(command) = cli.command else {
        Cli::command()
            .print_help()
            .map_err(|error| error.to_string())?;
        println!();
        return Ok(());
    };
    let target = cli
        .target
        .ok_or("请通过 --target 明确选择 business、log 或 event-bus")?;
    match command {
        Command::MigrationAdd { name, output_dir } => {
            Settings::load(target, cli.module.as_deref())?;
            let path = migrator::scaffold::add(target, &name, output_dir.as_deref())?;
            println!("{}", path.display());
        }
        Command::MigrationPending => {
            Settings::load(target, cli.module.as_deref())?;
            if migrator::scaffold::has_pending_changes(target)? {
                return Err("当前模型存在尚未生成迁移的变化".into());
            }
            println!("当前模型与最后迁移快照一致");
        }
        Command::ModelSnapshot { sql } => {
            Settings::load(target, cli.module.as_deref())?;
            print!(
                "{}",
                if sql {
                    migrator::scaffold::model_script(target)?
                } else {
                    migrator::scaffold::snapshot(target)?
                }
            );
        }
        Command::Migrate { to } => {
            migrator::validate_target(target, to.as_deref())?;
            let db = connect_database(target, cli.module.as_deref()).await?;
            migrator::migrate(&db, target, to.as_deref()).await?;
            println!("数据库迁移完成");
        }
        Command::MigrationScript {
            from,
            to,
            idempotent,
            output,
        } => {
            // 与设计时入口一致：读取所选工具配置，但不打开数据库连接。
            Settings::load(target, cli.module.as_deref())?;
            let script = migrator::script(
                target,
                repository_core::migration::ScriptOptions {
                    from: from.as_deref(),
                    to: to.as_deref(),
                    idempotent,
                },
            )?;
            if let Some(path) = output {
                if let Some(directory) = path.parent().filter(|path| !path.as_os_str().is_empty()) {
                    std::fs::create_dir_all(directory).map_err(|error| {
                        format!("创建输出目录失败（{}）：{error}", directory.display())
                    })?;
                }
                std::fs::write(&path, script)
                    .map_err(|error| format!("写入迁移脚本失败（{}）：{error}", path.display()))?;
                eprintln!("已导出 {target:?} 迁移脚本：{}", path.display());
            } else {
                print!("{script}");
            }
        }
        Command::JsonIndex => {
            let tables = match target {
                DatabaseTarget::Business => schema::json_index_tables(),
                DatabaseTarget::Log => logger_database::schema::json_index_tables(),
                DatabaseTarget::EventBus => event_bus_database::schema::json_index_tables(),
            };
            let db = connect_database(target, cli.module.as_deref()).await?;
            eprintln!("JSON 索引差异目标：{:?}；仅查询目录，不执行 DDL", target);
            let script = repository_core::json_index::create_script(&db, &tables).await?;
            print!("{script}");
        }
    }

    Ok(())
}

/// 建立数据库连接
async fn connect_database(
    target: DatabaseTarget,
    module: Option<&str>,
) -> Result<sea_orm::DatabaseConnection, String> {
    let settings = Settings::load(target, module)?;
    Database::connect(build_postgres_connect_options(
        settings.connection(target, module)?,
    ))
    .await
    .map_err(|error| format!("数据库连接失败: {error}"))
}

/// 创建数据库工具默认 PostgreSQL 连接配置
fn build_postgres_connect_options(connection: impl Into<String>) -> ConnectOptions {
    repository_core::connection::build_postgres_connect_options(
        connection,
        repository_core::connection::PostgresPoolOptions {
            test_before_acquire: false,
            max_connections: 30,
            min_connections: 1,
            connect_timeout_seconds: 50,
            idle_timeout_seconds: 300,
            acquire_timeout_seconds: None,
            max_lifetime_seconds: None,
            sqlx_logging: false,
        },
    )
}
