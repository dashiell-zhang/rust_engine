# 本地文件日志

在宿主的 `[logger]` 中设置 `component = "local_file"`，日志按调用现场 UTC 时间写入工作目录下的 `Logs/yyyyMMddHH.log`。每行是包含 `category`、`level`、`content` 的 JSON 对象，使用 UTF-8、无 BOM 和换行分隔，不需要数据库或 ID 服务。

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

所有级别先进入普通队列，容量或字节预算耗尽后，Warning、Error、Critical 可使用重要日志预留队列。重要队列优先消费，不保证跨队列时间顺序；业务调用不等待磁盘或队列空位。

队列和当前批次共同占用字节预算，按内容、分类、级别的 UTF-16 字符串长度和每条固定开销估算。超过预算时舍弃整条日志，不截断正文。队列外最多有一个批次；预算不是进程总内存上限。

容量、批量大小和字节预算必须为正数，间隔为 1～4294967294 毫秒；修改后重启生效。`[logger].save_days` 默认 14，允许正数或 -1，-1 表示永久保存。

## 写入、重试和恢复

后台按批次或时间间隔写入。一个批次按连续 UTC 小时段追加，每段成功关闭文件后才释放内存预算。写入失败保留批次，按 1、2、4、8、16、30 秒退避重试，之后维持 30 秒。重试先退回该段写入前的文件长度，避免把已写入的一部分再次追加；已经成功的前面小时段不会重写。

等待重试的文件如果被外部删除或截短，会保留失败状态并报告，不自动重建或跳过。每个进程应使用独立日志目录，不支持多个进程共同追加同一个小时文件。

磁盘写满时应先恢复可用空间，并保留待重试文件；后台会从本次未确认段的起始位置重写。不要为腾出空间而截短正在等待重试的小时文件。若文件已被外部截短，单纯释放空间不能恢复此前已确认的字节，写入器会继续报告错误，需要先恢复原内容。

启动时检查本组件的小时文件，追加前也会核对文件尾部。末行没有换行时，先将原始尾部保存为 `yyyyMMddHH.log.tail-<GUID>.rejected`，关闭成功后才把原文件截回完整行边界。尾部保存失败时不截短原文件。保留文件不自动清理，应由维护人员核对。

过期清理依据最后写入时间，只处理有效日期的小时文件；不会处理数据库批次、子目录、相似名称和 `.rejected` 文件。追加、恢复和清理相互协调，清理跳过等待重试的文件。删除不要求先取得文件写锁或以读写方式打开；目录允许删除时，已过期的只读文件也会清理，未过期文件仍保留。启动恢复与清理独立调度，清理不强制先恢复过期文件；已经过期的未完成日志可能先被清理，不能依赖启动恢复延长其保留时间。

## 级别和停机

普通 tracing 事件保存为 `Trace`、`Debug`、`Information`、`Warning`、`Error`。严重故障使用独立的 Critical 宏：

```rust
logger::critical!(target: "app.startup", error = %error, "宿主启动失败");
```

`[logger.log_level]` 支持 `critical` 和 `none`，以及常规级别名称。设为 `critical` 时普通 Error 不会写入，Critical 仍保留当前 span 上下文。控制台显示 `CRITICAL`，文件保存 `Critical`。

级别名称不区分大小写，并保留 `info`、`warn`、`off` 简写。空字符串不产生规则；仅含空白、未知名称、越界整数及非法名称组合会使初始化失败，在启动日志后台任务前返回配置错误，不再忽略后继续启动。与源日志配置一致，也接受 Int32 数字字符串及逗号分隔的枚举名称组合（按位或）；通常应使用单个可读级别名称。

类别按 `OrdinalIgnoreCase` 语义匹配：不含 `*` 时匹配前缀，包含一个 `*` 时分别匹配其前缀和后缀，不要求中间至少有一个字符；有效规则包含多个 `*` 会使初始化失败。类别原有空白和分隔符参与匹配，不做裁剪。`default` 只在没有类别规则命中时使用；多个规则命中时，按 UTF-16 长度优先选择较长规则（包括通配符），等长按源配置子键枚举顺序选择，不依赖 TOML 的书写顺序或 HashMap 顺序。

WebAPI 和任务宿主通过 `logger::log_filter_source` 包装文件来源，通过 `logger::log_filter_environment` 读取 APP 环境变量，在每层配置合并前统一类别键：基础文件、环境文件和环境变量依次覆盖。同一文件同时定义 `http.request` 与 `HTTP.Request` 会报重复错误；不同配置源中的这两个名称代表同一类别，后面的值覆盖前面的值。自定义宿主也应使用这些入口加载日志配置。

### 分别设置控制台与存储级别

`logger.providers` 接受提供者的完整类型名；控制台也可使用 `Console` 别名。名称使用表中大小写，类别仍不区分大小写。

| 输出目标 | 提供者名称 |
| --- | --- |
| 控制台 | `Console` 或 `Microsoft.Extensions.Logging.Console.ConsoleLoggerProvider` |
| 本地文件 | `Logger.LocalFile.LocalFileLoggerProvider` |
| 数据库 | `Logger.Database.DatabaseLoggerProvider` |

```toml
[logger.log_level]
default = "info"
"http.request" = "warning"

[logger.providers.Console.log_level]
default = "error"

[logger.providers."Logger.LocalFile.LocalFileLoggerProvider".log_level]
default = "debug"
```

这个例子中，控制台只输出 Error/Critical，本地文件从 Debug 开始记录，包括 `http.request` 的 Debug 日志。提供者专属类别或默认规则都优先于全局类别规则；只有该提供者没有适用规则时才回退全局。别名和完整类型名同时配置时，先比较类别长度，等长按源配置节顺序选择；不会额外提高别名的优先级。配置存储提供者的规则不切换 `logger.component`，也不会创建另一种存储。

环境变量 `APP__LOGGER__PROVIDERS__Console__LOG_LEVEL__DEFAULT=error` 可覆盖控制台默认级别。提供者名称保留原大小写及字面点号，类别中的字面点号也不会变成嵌套对象；类别尾部的 `__` 按源环境配置映射为冒号。其他 APP 配置继续沿用原环境读取规则。

两个日志提供者均使用 `LoggerRuntime::shutdown` 限时排空，退出时先停止接收，超时后取消写入并将剩余内存条目计入舍弃。仅释放运行时守卫会取消后台操作，不会阻塞线程等待磁盘。

### 运行时更新入口

已初始化的运行时可以调用 `LoggerRuntime::reload_filter(&updated_logger_settings)` 更新全局及提供者专属过滤规则。控制台与当前存储使用一个整体替换的规则快照；编译失败返回错误，保留此前快照。调用点持续动态检查规则，降低最低级别后不会被先前缓存的拒绝结果阻断，已有 span 上下文也继续可用。

此入口只更新级别和类别规则，不切换存储提供者、连接、缓冲、项目标识或保留天数。`shutdown` 先停止监听和更新任务，再使用剩余停机期限排空日志；更新入口随之失效。

### 配置文件自动更新

三个宿主通过 `logger::load_host_configuration` 加载 TOML 并捕获各来源的过滤配置；默认监听基础文件和所选环境文件，按基础文件、环境文件、启动时环境变量的顺序合并。`[logger] reload_on_change = false` 关闭自动监听，对应源宿主的文件重载开关语义；此开关在启动时生效，过滤规则之外的配置修改仍需重启。

文件通知经过 250 毫秒等待后重读对应文件。监听父目录以支持编辑器替换文件及可选环境文件后续创建；首次注册后重新读取，补齐初始配置加载与监听之间的变化。消息通知使用有界队列与合并标记，不在文件回调里读文件或更新日志。文件读取在独立阻塞任务中进行，监听器不会长期保存连接配置。

删除文件会移除该来源的覆盖；TOML 解析失败会清空该来源缓存，但不发布本次更新，因此当前生效过滤规则保留，其他文件后续成功更新时会使用清空后的来源。读文件失败保留该来源缓存，级别校验失败返回固定诊断并保留当前过滤快照；文件修复后的通知会再次加载。诊断直接输出固定文字，不回写日志提供者或打印配置正文。

默认使用系统文件通知。Windows 挂载目录、网络文件系统等场景若无法传递通知，可在容器或进程启动前设置 `DOTNET_USE_POLLING_FILE_WATCHER=1`（也接受 `true`），使用四秒轮询。环境变量过滤规则本身采用启动快照，不因文件变化重新读取进程环境。

## 诊断

通过 `logger_local_file::diagnostics::snapshot()` 拉取当前进程快照，未启用该提供者时返回 `None`：

- `dropped_by_level`：按 Trace、Debug、Information、Warning、Error、Critical 排列的累计舍弃数量。
- `failures_by_operation`：按 write、recovery、shutdown、cleanup 排列的累计失败次数。
- `buffered`、`buffered_bytes`：尚未确认落盘的条数和估算字节数。

组件初始化后通过 OpenTelemetry 的 `Logger.LocalFile` meter 提供以下拉取式指标：

| 指标 | 类型 | 单位 | 标签 |
| --- | --- | --- | --- |
| `logger.localfile.dropped` | ObservableCounter | `{log}` | `level` |
| `logger.localfile.failures` | ObservableCounter | `{failure}` | `operation` |
| `logger.localfile.buffered` | ObservableGauge | `{log}` | 无 |
| `logger.localfile.buffered.bytes` | ObservableGauge | `By` | 无 |

`level` 使用 Trace、Debug、Information、Warning、Error、Critical、None；None 为零值。`operation` 使用 write、recovery、shutdown、cleanup。计数器从进程启动累计，仪表表示采集时的当前值。

宿主的监控集成应在初始化日志之前，通过 `opentelemetry::global::set_meter_provider` 安装采集端，使用与组件一致的 OpenTelemetry API 版本。日志库只注册指标，不负责选择 SDK、采集周期或导出目标；未配置采集端时不导出指标，快照接口仍可用。采集回调只读取原子计数；退出时先排空日志，再刷新和关闭宿主拥有的采集端。

业务线程只更新原子计数，后台故障按阶段限频输出，不通过自身日志链路回写。强制结束进程、缓冲耗尽和长期磁盘不可写仍可能丢失日志；文件关闭不等于抗断电持久化，不适合替代必须可靠提交的业务审计存储。
