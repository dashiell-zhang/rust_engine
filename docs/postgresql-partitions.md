# PostgreSQL 时间分区

`repository_core::partition` 为预分配雪花 ID 的表提供 RANGE 分区声明、建表语句和后台维护。当前日志组件的 `Log` 表按一个自然月分区。

## 分区声明与迁移

实体引用 `repository_core` 和 `source_generator_core`，在 SeaORM 模型上声明：

```rust,ignore
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, source_generator_core::PartitionTable)]
#[sea_orm(table_name = "Log")]
#[partition(interval = 1, unit = "Month")]
pub struct Model {
    #[sea_orm(column_name = "Id", primary_key, auto_increment = false)]
    pub id: i64,
    // 其他持久化字段……
}
```

周期支持 `Hour`、`Day`、`Month`、`Year`，`interval` 必须是正整数。模型必须使用单列 `i64` 主键，显式声明数据库列名，并关闭自增。主键值由雪花 ID 生成器在写入前分配。表名和可选 `schema_name` 沿用 SeaORM 映射。

在模型所属的迁移定义中调用 `repository_core::partition::initial_schema_statements(Entity, chrono::Utc::now())`，生成父表和当前时刻所在的一个初始子分区；再追加模型需要的索引。唯一约束和唯一索引必须包含分区键，PostgreSQL 在迁移事务中校验这一要求。运行时维护使用同一模型的 `PartitionedModel::partition_definition()`。

日志迁移通过独立目标执行：

```powershell
docker compose exec -T dev cargo run --locked -p repository_tool -- --target log migrate
```

工具连接由 `repository_tool/appsettings.toml` 的日志目标决定，历史表为 `__EFMigrationsHistory_Log`。宿主不自动执行迁移。已有普通 `Log` 表不会自动转换为分区表，也不会通过 `IF NOT EXISTS` 跳过结构不一致；已有数据的结构调整应先设计并执行显式数据库迁移。

## 时间与边界

分区使用固定 UTC+8，不读取宿主时区。小时、天和多周期按 `1970-01-01 00:00:00 +08:00` 对齐；月和年按自然日历计算。边界为左闭右开 `[start_id, end_id)`。

雪花时间布局由 `identifier_generator::layout` 统一提供：纪元是 `2022-01-01 00:00:00 UTC`，低 21 位用于节点和序列。边界的低 21 位必须全部为零。自然月或自然年的起点可能早于雪花纪元，这种边界允许为负数；当前时间本身早于纪元时禁止创建初始分区，超出有符号 ID 表示范围时返回错误。

子表名为父表名加 `_pYYYYMMDDHH`。名称超过 PostgreSQL 的 63 字节上限时，按 UTF-8 字符边界截短前缀，并添加父表名称的 SHA-256 前八位十六进制摘要。

## 日志维护配置

三个宿主选择数据库日志时，在启动阶段执行首轮分区维护，随后每十分钟执行一次。每次保持当前分区及三个连续的未来分区；整个计划超过 32 个新分区时，在执行 DDL 前终止本轮。维护错误会记录到标准错误输出，随后仍会重试。

```toml
[logger]
component = "database"
database_identity = "RustEngine.Log"
maintenance_lock_connection = ""
maintenance_lock_prefix = "NetEngine:PartitionMaintenance:"
```

维护锁的类型沿用宿主 `[lock].component`。Redis 连接优先使用 `maintenance_lock_connection`，为空时使用 `[lock].connection`。维护锁前缀独立于宿主业务锁前缀；所有维护同一日志库的实例必须使用相同的 Redis 目标、维护前缀和 `database_identity`。内存锁仅用于单进程运行。

锁名包含数据库标识、实际数据库名、解析后的 schema 和父表名。锁租期十分钟，每三分钟续期，未取得锁时跳过本轮；续期失败取消维护事务。每轮建表在事务内提交，同名对象只有在实际父表、schema、名称及边界都与预期一致时才视为已完成。

已有分区边界保持不变。调整周期后，从最右侧真实边界开始衔接新策略，首个新分区可以是短周期。维护不会删除、归档、拆分或合并分区，不会填补历史空洞；不支持 DEFAULT、LIST、HASH、表达式、多列键或多级分区。日志保留天数通过按项目删除记录执行，不通过删除整个分区执行。

## 其他模型接入

业务模型在自身 crate 声明分区并生成迁移，运行入口将这些声明和对应写连接、共享锁、数据库标识传入 `PartitionMaintenance::new(...)`，等待 `start()` 返回并在宿主生命周期内持有守卫。不要为未声明策略的数据库表自动注册维护。

分区声明与并发版本是两个独立能力。实体的新增、更新和删除使用 [PostgreSQL 行版本与并发保存](optimistic-concurrency.md) 中的公共入口；插入和真实 `xmin` 回读必须处于同一事务，不能改成分区 INSERT 直接返回 `xmin`。
