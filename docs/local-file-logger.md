# 本地文件日志

本地文件日志适合不需要集中数据库查询的部署。日志按调用现场 UTC 小时写入工作目录的 `Logs/yyyyMMddHH.log`，每行是包含 category、level、content 的 JSON。

## 启用

```toml
[logger]
component = "local_file"
save_days = 14

[logger.log_level]
default = "info"
"http.request" = "warning"
```

组件本身不需要数据库或 ID 服务。宿主其他能力仍可能需要它们；管理端日志查询和 WebAPI 健康检查的依赖见各自文档。save_days 为 -1 时不清理过期文件。

```rust
tracing::info!(target: "app.orders", "订单处理完成");
logger::critical!(target: "app.startup", "关键服务不可用");
```

## 缓冲配置

```toml
[logger.local_file_buffer]
queue_capacity = 8192
max_buffered_bytes = 50331648
important_queue_capacity = 2048
important_max_buffered_bytes = 16777216
batch_size = 1000
flush_interval_milliseconds = 1000
```

普通队列满后，Warning、Error、Critical 可使用重要日志预留队列。业务线程不等待磁盘，容量耗尽时整条丢弃；字节预算包含队列和当前批次。

容量与字节预算须为正数，落盘间隔为 1～4294967294 毫秒。缓冲和存储设置修改后重启生效。

## 级别和停机

级别使用 trace、debug、information、warning、error、critical、none，也接受 info、warn、off 简写，名称不区分大小写。配置错误会阻止启动。

类别按大小写无关前缀匹配，也允许一个 `*` 通配符。命中多个规则时优先较长规则；default 作为后备。同一文件不要声明仅大小写不同的重复类别。

正常停机停止接收日志并在宿主剩余期限内排空，超时丢弃剩余内存记录。强制结束进程和持续磁盘故障仍可能丢日志，因此它不替代必须可靠提交的业务审计存储。

### 分别设置控制台与存储级别

```toml
[logger.providers.Console.log_level]
default = "error"

[logger.providers."Logger.LocalFile.LocalFileLoggerProvider".log_level]
default = "debug"
```

上例控制台只输出 Error/Critical，本地文件从 Debug 开始记录。提供者专属规则优先于全局规则；该设置不切换 logger.component。

| 输出 | 提供者名称 |
| --- | --- |
| 控制台 | Console |
| 本地文件 | Logger.LocalFile.LocalFileLoggerProvider |
| 数据库 | Logger.Database.DatabaseLoggerProvider |

环境变量示例：`APP__LOGGER__PROVIDERS__Console__LOG_LEVEL__DEFAULT=error`。提供者名称保留大小写与字面点号。

### 配置文件自动更新

默认监听基础 TOML 和当前环境 TOML 的日志过滤规则。配置合并顺序为基础文件、环境文件、启动时环境变量。可以设置 `[logger] reload_on_change = false` 关闭监听。

只热更新级别和类别，不切换存储、连接、缓冲、项目或保留天数。非法更新保留当前有效规则，修复文件后重新加载。环境变量使用启动快照。

Windows 挂载目录或网络文件系统无法传递文件通知时，可在启动前设置 `DOTNET_USE_POLLING_FILE_WATCHER=1`，改用四秒轮询。自定义宿主也可调用 `LoggerRuntime::reload_filter` 更新规则。

## 磁盘故障与恢复

写入失败保留批次，按 1、2、4、8、16、30 秒退避重试。每个进程使用独立目录，不支持多个进程共同追加同一小时文件。

磁盘写满时先释放空间，保留正在重试的日志文件；不要删除或截短它。写入器需要原文件内容才能从未确认位置继续。

未完成的末行会保存为 `*.tail-<GUID>.rejected` 后再修复尾部。这类文件不自动删除，需人工核对。过期清理只处理有效小时文件，不处理子目录、数据库批次或 rejected 文件。

## 诊断

`logger_local_file::diagnostics::snapshot()` 提供累计舍弃数、按操作失败数、当前缓冲条数和字节数；未启用时为 None。

OpenTelemetry meter 为 `Logger.LocalFile`，提供：

- `logger.localfile.dropped`
- `logger.localfile.failures`
- `logger.localfile.buffered`
- `logger.localfile.buffered.bytes`

宿主在日志初始化前设置 meter provider。计数器重启归零，指标采集不触发磁盘扫描。
