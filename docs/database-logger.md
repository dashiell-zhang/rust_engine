# 数据库日志

数据库日志将运行日志异步写入 PostgreSQL，并通过本地文件暂存积压，降低数据库短暂故障对业务请求的影响。它适合集中查询与运维排查；必须随业务原子提交的审计数据应使用业务事务保存。

## 配置与初始化

```toml
[logger]
component = "database"
project = "Example"
database_identity = "Example.Log"
connection = "postgres://writer:password@log-db/logs"
read_connection = "postgres://reader:password@log-reader/logs"
save_days = 14

[logger.log_level]
default = "info"
```

写连接留空时使用业务写库地址，但创建独立连接池；读连接留空时使用日志写库地址，不使用业务只读库。每个进程使用独立工作目录，避免共享积压文件。

在工具配置中填写日志目标并迁移：

```powershell
docker compose exec -T dev cargo run --locked -p repository_tool -- migrate --target log
```

日志表按月分区，运行时自动维护未来分区，见[时间分区](postgresql-partitions.md)。多实例维护同一日志库应使用共同维护锁。

## 记录日志

```rust
tracing::info!(target: "app.orders", order_id = order_id, "订单处理完成");
logger::critical!(target: "app.startup", "关键服务不可用");
```

存储级别为 Trace、Debug、Information、Warning、Error、Critical。控制台与数据库可单独设置过滤规则，见[日志级别配置](local-file-logger.md#分别设置控制台与存储级别)。

## 缓冲配置

```toml
[logger.database_buffer]
queue_capacity = 8192
max_buffered_bytes = 50331648
important_queue_capacity = 2048
important_max_buffered_bytes = 16777216
batch_size = 1000
flush_interval_milliseconds = 1000
```

普通队列满后，Warning 及更高级别可使用预留容量。业务调用不等待队列空位，两组预算均耗尽时丢弃整条日志。字节预算是队列和正在处理批次的估算值，不是进程内存上限。

缓冲参数修改后重启生效。save_days 默认 14，设置 -1 禁止过期清理。清理只处理当前项目的有效日志，不删除整个分区。

## 积压与恢复

积压位于 `Logs/Database/<目标哈希>`，按存储标识、项目及数据库目标隔离。切换目标后不会消费旧目标目录。

- 数据库不可用时保留文件并重试，恢复后连续处理积压。
- 写盘失败保留当前批次，按 1、2、4、8、16、30 秒退避重试。
- 有效记录事务提交后才删除批次，重试按日志 ID 去重。
- 损坏行所在原文件保留为 `.rejected`，需人工核对，不自动清理。
- 同名 rejected 已存在时不会覆盖；将旧样本归档到消费目录之外后可继续处理。
- 临时批次超过五分钟且可独占锁定时尝试恢复。

不要把其他目标的积压文件直接复制进当前消费目录。工作目录应保留足够磁盘空间，并允许创建、改名和删除文件。

## 查询与监控

管理端通过独立日志只读连接查询。独立接入可使用 `logger_database::query::LogQueryService`，按项目、机器、内容、级别和时间筛选，分页为 1～100 条。省略项目可查询整个日志库，访问范围由调用方控制。

`logger_database::diagnostics::snapshot()` 返回当前进程快照，未初始化时为 None：

| 字段 | 含义 |
| --- | --- |
| dropped_by_level | 按级别累计丢弃数 |
| failures_by_operation | 按操作累计失败数 |
| buffered / buffered_bytes | 待处理条数与估算字节 |
| spool_files / spool_bytes | 待入库及临时文件数量、大小 |
| rejected_files | 损坏文件数量 |

OpenTelemetry meter 为 `Logger.Database`，指标前缀为 `logger.database`，包含 dropped、failures、buffered、buffered.bytes、spool.files、spool.bytes、rejected.files。宿主在初始化日志前安装 meter provider，并自行选择采集端。

## 停机

正常退出停止接收日志，使用宿主剩余期限尽量排空到文件，不等待全部积压入库。强制终止、持续磁盘故障、ID 租约失效或缓冲耗尽可能丢日志。退出前显式等待 LoggerRuntime 的 shutdown，不能用 Drop 代替异步排空。
