# 数据库日志

三个宿主通过 `[logger]` 选择日志组件。`component = "database"` 使用调用现场快照、有界内存、文件积压和数据库入库流程；`local_file` 使用[本地文件提供者](local-file-logger.md)。业务和日志共用宿主 ID 生成器。tracing 事件保存为 `Trace`、`Debug`、`Information`、`Warning`、`Error`，`logger::critical!` 保存为 `Critical`，日志级别查询使用这些文本。最低级别支持 `critical` 和 `none`。

`[logger.log_level]` 与本地文件提供者共用级别解析、类别匹配和启动校验。非法级别或多通配符规则会在启动分区维护及日志写入任务前返回错误；空字符串不产生规则。类别忽略大小写，环境配置中的同名类别覆盖基础配置。名称、简写、数字字符串及类别优先级规则见[级别和停机](local-file-logger.md#级别和停机)。

通过 `[logger.providers."Logger.Database.DatabaseLoggerProvider".log_level]` 单独设置数据库日志级别，控制台使用 `[logger.providers.Console.log_level]`。两者独立过滤，提供者专属默认规则也优先于全局类别规则；未配置时沿用全局级别。完整配置和环境变量示例见[分别设置控制台与存储级别](local-file-logger.md#分别设置控制台与存储级别)。

运行时更新过滤规则使用 `LoggerRuntime::reload_filter`，不会重建数据库连接或改变正在处理的日志批次。三个宿主已接入默认开启的文件监听，停机先停止监听再排空日志；可以通过 `reload_on_change = false` 关闭。来源覆盖、错误恢复及容器轮询选项详见[配置文件自动更新](local-file-logger.md#配置文件自动更新)。

## 配置

`connection` 是日志写连接，留空时选用业务写库地址并建立独立连接池。`read_connection` 留空时只回退日志写连接，不使用业务读库。`database_identity` 是稳定的日志存储标识，`project` 区分日志所属项目。

日志表与分区通过数据库工具的 `--target log` 准备。维护锁配置见 [PostgreSQL 时间分区](postgresql-partitions.md)，ID 配置见 [ID 生成器](identifier-generator.md)。同一日志库的实例应使用共同维护锁；同一项目的实例应使用一致保留策略。

数据库日志缓冲默认配置如下，修改后重启宿主生效：

```toml
[logger.database_buffer]
queue_capacity = 8192
max_buffered_bytes = 50331648
important_queue_capacity = 2048
important_max_buffered_bytes = 16777216
batch_size = 1000
flush_interval_milliseconds = 1000
```

所有级别先使用普通队列，普通队列或字节预算耗尽后，Warning 及更高级别才使用预留容量。预留队列优先消费，因此不保证跨队列时间顺序。入队不等待磁盘、数据库或可用容量；两组预算均耗尽时计入舍弃数量，超大单条日志不会截断正文。

字节预算包含队列和正在写入或重试的批次，按字符串 UTF-16 长度与固定开销估算，不是进程总内存上限。队列外最多有一个当前批次。容量和批量大小必须为正的 i32 整数，字节上限必须为正的 i64 整数，落盘间隔为 1～4294967294 毫秒。

`save_days` 默认 14，允许正数或 -1；-1 禁用清理。清理仅删除当前项目中未软删除的过期日志，不删除分区。保留时间超出公元 1 年下界时，截止条件使用 PostgreSQL 的 `-infinity`，与源 Npgsql 对最小日期的编码一致；不会生成超出数据库范围的日期，也不会将有限的 `0001-01-01` 当成该边界而误删。

## 文件积压与恢复

日志保存在 `Logs/Database/<目标哈希>`。目标包含存储标识、项目、实际数据库主机或套接字、端口、库名、账号和连接选项；密码、TLS 证书和连接池配置不参与计算。切换目标后不会自动消费旧目录，切换前应处理现有积压。每个进程应使用独立的工作目录和日志目录。

文件消费者只处理目标目录顶层的普通文件，使用以下完整命名规则，固定前后缀区分大小写：

- 正常批次：`batch-yyyyMMddHHmmssfff-<32 位十六进制 GUID>.log`，日期必须有效。
- 临时文件：正常批次名加 `.tmp`。
- 重名恢复批次：`batch-recovered-<32 位十六进制 GUID>.log`。
- 损坏保留文件：正常或恢复批次名加 `.rejected`。

每条日志在后台首次写出时获得 ID，重试保持同一 ID，时间戳保留调用现场时间。落盘失败保留当前批次，按 1、2、4、8、16、30 秒重试，之后维持 30 秒。失败产生的临时文件保留；超过五分钟且能够独占锁定时恢复，不覆盖已有批次。

文件 JSON 使用驼峰字段，`id` 写成字符串，`createTime` 使用 `yyyy/MM/dd HH:mm:ss +00:00`，并包含 `deleteTime` 和 `rowVersion` 字段。读取不区分字段大小写，ID 接受字符串或整数，时间也接受 RFC 3339。行版本只校验文件值类型，实际入库版本由数据库产生。

文件逐行读取，支持 UTF-8 BOM、CR/LF/CRLF、末行无换行和无效 UTF-8 的替换字符处理。按 ID 去重时包含已软删除记录，同一文件中的重复 ID 只保存一次。有效日志在同一事务中提交后才删除文件；含损坏行的完整原文件保留为 `.rejected`，不自动清理。

若同名 `.rejected` 已存在，消费者报告失败并保留两份原文件，不覆盖旧的损坏样本。核对并将旧样本归档到消费目录之外后，后续扫描会继续处理；已经提交的有效记录按原 ID 去重，不重复写入。数据库不可用时，包含有效记录的批次保持待入库状态，不能提前移动成 `.rejected`。

有积压时连续入库，空闲或失败后等待五秒。磁盘上的积压不依赖 ID 服务恢复发号即可继续入库。

## 停机与诊断

正常宿主退出先停止业务和事件处理，再关闭日志接收，使用剩余停机预算尽量排空到文件，不等待积压全部入库。WebAPI 与事件关闭、日志排空共享 30 秒预算。任务宿主从收到停止信号开始计时，已领取任务、事件关闭与日志排空共享同一份 30 秒预算，不在任务结束后重新计算。仅丢弃运行时守卫会取消后台操作，调用方应显式等待 `LoggerRuntime::shutdown`。

通过 `logger_database::diagnostics::snapshot()` 拉取进程级诊断快照；未初始化数据库日志时返回 `None`。采集端可将这些值接入监控系统：

| 字段 | 含义 |
| --- | --- |
| `dropped_by_level` | Trace、Debug、Information、Warning、Error、Critical 的累计舍弃条数 |
| `failures_by_operation` | spool、database、recovery、shutdown、quarantine、cleanup 的累计失败次数 |
| `buffered`、`buffered_bytes` | 队列及当前批次的条数与估算字节数 |
| `spool_files`、`spool_bytes` | 待入库文件及临时文件数量、总字节数 |
| `rejected_files` | 保留的损坏文件数量 |

组件初始化后通过 OpenTelemetry 的 `Logger.Database` meter 提供标准拉取式指标：

| 指标 | 类型 | 单位 | 标签 |
| --- | --- | --- | --- |
| `logger.database.dropped` | ObservableCounter | `{log}` | `level` |
| `logger.database.failures` | ObservableCounter | `{failure}` | `operation` |
| `logger.database.buffered` | ObservableGauge | `{log}` | 无 |
| `logger.database.buffered.bytes` | ObservableGauge | `By` | 无 |
| `logger.database.spool.files` | ObservableGauge | `{file}` | 无 |
| `logger.database.spool.bytes` | ObservableGauge | `By` | 无 |
| `logger.database.rejected.files` | ObservableGauge | `{file}` | 无 |

`level` 使用 Trace、Debug、Information、Warning、Error、Critical、None，None 为零值；`operation` 使用 spool、database、recovery、shutdown、quarantine、cleanup。计数器累计失败或舍弃次数，仪表使用既有后台统计的当前值；采集不触发磁盘扫描或数据库查询。

宿主在初始化日志之前通过 `opentelemetry::global::set_meter_provider` 安装采集端，并使用与组件一致的 OpenTelemetry API 版本。SDK、采集周期和导出目标由宿主监控集成选择，日志库仅注册指标；未配置采集端时快照接口仍可用。退出时先排空日志，再刷新和关闭宿主拥有的采集端。

计数在重启后归零。业务线程只更新原子计数；磁盘统计在后台刷新，最短间隔 30 秒。后台失败按阶段每 30 秒最多输出一次简要诊断，不输出日志正文和连接凭据，也不回写数据库日志链路。

这是有限缓冲的尽力持久化日志。ID 租约失效、持续磁盘故障或停机超时可能导致舍弃；强制终止进程会丢失尚在内存中的记录。文件发布不代表抗断电持久化，需保证绝对可靠的审计事件应使用独立业务持久化机制。

## 独立查询

`logger_database::query::LogQueryService` 只依赖日志只读连接，不启动写盘、ID 或维护任务。请求支持项目、机器、内容、级别和时间范围，分页 `take` 为 1～100。项目、机器和内容按文字包含匹配，`%`、`_` 与反斜杠不会变成检索通配符。未传项目时可查询整个日志库，访问权限由调用方控制。

结果使用 `LogQueryItem`，不向应用层暴露数据库实体或行版本。管理端应用服务负责业务 DTO 转换。
