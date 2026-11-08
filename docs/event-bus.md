# 跨进程事件广播

`event_bus` 提供公共协议、发布器和监听运行时，`event_bus_redis` 使用 Redis Streams，`event_bus_database` 使用独立 PostgreSQL 写连接。三个宿主均从 `[event_bus]` 选择实现，公共宿主不引用具体事件后端。

同一事件会被所有已订阅实例各自处理，包括发布方。只应由一个 Worker 执行的工作仍使用队列任务。事件发布成功仅表示后端确认保存，不表示监听完成，也不提供业务写入与广播的原子提交。

## 配置和初始化

```toml
[event_bus]
provider = "redis"
connection = "redis://127.0.0.1:6379/0"
scope = "NetEngine:Development"
retention_milliseconds = 3600000
poll_interval_milliseconds = 1000
retry_interval_milliseconds = 1000
cleanup_interval_milliseconds = 60000
batch_size = 100
```

互通实例必须使用相同后端、存储目标、Scope 和保留时间。Redis 后端需要支持精确 `XTRIM MINID` 的 Redis 6.2 或更新版本；不使用竞争消费组，不删除流来清理历史。未指定事件连接时，运行入口为 Redis 后端选用业务缓存的 Redis 连接，为数据库后端选用业务写库地址；事件组件仍持有独立连接，不沿用业务事务或只读副本。

Scope 非空且最多 200 个 UTF-16 码元，与源项目的字符串长度规则一致。保留时间以完整毫秒配置，范围为 1～922337203685477；长保留时间的清理截止日期最低为公元 1 年的 UTC 零点。数据库适配器将该最小日期按源 Npgsql 默认规则映射为 PostgreSQL `-infinity`，严格早于该值的消息为空，因此不会误删已有的负无穷或更早年份记录；仍执行数据库语句，连接故障仍进入正常重试流程。

切换数据库后端时，把 `provider` 改为 `database`，将 `connection` 配置为 PostgreSQL 写库，并在工具专用配置中设置 `database.event_bus_connection`，然后执行：

```powershell
docker compose exec -T dev cargo run --locked -p repository_tool -- migrate --target event-bus
```

迁移只创建 `EventBusStream`、`EventBusMessage` 及其索引，使用 `__EFMigrationsHistory_EventBus`，不会创建业务或日志表。宿主不自动迁移。数据库后端依赖宿主已有的 ID 服务；Redis 事件后端本身不依赖数据库或 ID 服务。

## 契约和发布

共享事件定义放在应用模型中，使用明确的稳定协议名。非泛型结构体可通过 `source_generator_runtime::Event` 派生，或实现 `event_bus::Event`。需要与 .NET 互通时，名称取对应事件的完整类型名，字段序列化也须一致。

公共契约和派生宏要求名称非空白，不统一施加数据库字段的长度上限。Redis 读取允许超过 1024 字符的非空白名称；数据库后端仍由 `EventName` 的 `varchar(1024)` 约束拒绝超长写入。传输层直接追加信封不额外验证名称，Redis 读取时会将空白名称标记为无效记录。

```rust
use serde::Serialize;
use source_generator_runtime::{Event, ProtocolModel};

#[derive(Serialize, ProtocolModel, Event)]
#[serde(rename_all = "camelCase")]
#[event(name = "Application.Model.Events.ProductChangedEvent")]
pub struct ProductChangedEvent {
    #[serde(default)]
    pub product_id: i64,
}
```

应用服务显式接收 `application_interface::events::EventPublisher`。WebAPI 状态和 TaskService 特性上下文提供 `event_publisher`，也实现 `EventPublisherProvider`。业务提交成功后调用 `publisher.publish(&message).await`。发布未来被取消或返回错误不证明消息一定没有写入。

需要响应调用方取消时使用 `publisher.publish_with_cancellation(&message, &cancellation).await`。入口先检查取消，再校验事件名称并序列化；同一个令牌传到后端的范围初始化、获取连接、状态更新、消息写入和提交。发送取消后仍应等待调用返回，直接丢弃 future 无法等待服务器端清理。Redis 已发送的命令不能撤回；数据库提交与取消相遇时也不能仅凭取消结果认定消息未写入，重试仍需业务幂等。

需要业务与广播任务一起提交时，先从所属 `ModuleDatabase` 开启 `ModuleTransaction`，用同模块的 `QueueTaskProducer::create` 在该事务中加入广播任务；后台任务再等待 `publish` 完成。重试可能产生重复广播，因此监听必须幂等。

## 监听注册

在业务 Feature 的 `register_event_listeners` 中调用 `registry.subscribe::<事件类型, _, _>("稳定监听标识", handler)`。运行入口先建立后端位置，再完成特性初始化，进入运行态后开始消费。名称冲突或同一事件重复监听标识会在启动时报告错误。

监听闭包接收反序列化事件及 `CancellationToken`，返回 `Result<(), String>` 的异步结果。闭包每次重试重新调用；跨调用共享状态使用宿主注入的 `Arc`。处理器必须配合取消，禁止启动脱离监听生命周期的后台任务。

需要独立服务及异步资源释放时，实现 `EventHandler<事件类型>`，通过 `registry.subscribe_factory::<事件类型, _, _>("稳定监听标识", factory)` 注册。同步工厂返回 `EventResult<处理器类型>`；每次尝试先调用工厂创建处理器，再解析负载并调用 `handle`。工厂返回失败前已取得的资源由工厂负责释放，不将未构造成功的对象交给运行时。普通闭包入口也使用同一调用流程，闭包捕获的共享实例仍归注册方管理。

工厂成功返回后，运行时在参数解析失败、处理失败或成功后都等待 `close`；展开型 panic 也先完成释放再交给监听重试流程。`close` 的失败优先于原处理结果，与源异步作用域释放的行为一致；释放成功后才视为本次监听成功，因此释放失败可能导致处理重跑，业务仍需幂等。正常完成不会主动取消传给处理器的令牌；到期或停机时发送取消并等待当前处理与释放。释放没有独立的后台任务或额外停机期限，宿主预算耗尽后强制结束时不能保证异步释放完成。

事件负载使用独立 Web JSON 读取入口：模型字段名不区分大小写，数值字段接受数字字符串，字典键和普通字符串保持原样，最大嵌套深度为 64。不运行 HTTP 的必填验证、空白转 null 或内部 JsonHelper 的日期与 long 转换器；数字字符串不能带外部空白，浮点常量仅接受 `NaN`、`Infinity` 和 `-Infinity` 的固定拼写。

发布使用独立 Web JSON 输出入口，保留数字形式的 long 和模型声明的日期序列化，不套用内部 JsonHelper 转换器。原始浮点字段为 NaN 或正负无穷时返回错误，不会静默变成 null，也不会写入事件后端；嵌套字段和集合元素遵守同一规则。若模型的自定义序列化器已经把数据转成字符串或 JSON 文档值，则由该模型负责转换语义。

事件及嵌套协议模型使用 `ProtocolModel`，不要同时派生 `Deserialize`。它按输入顺序转换每次已知字段赋值，成功重复字段取最后值，错误值不能被后续同名字段掩盖；支持模型级 `rename_all` 和字段级 `rename`、无参数 `default`。可空字段使用 `Option<T>`，与源模型缺省值对应的数值等字段显式添加 `#[serde(default)]`。自定义 Serde 反序列化器仍由模型声明负责，包括缺省值、日期、枚举及重复字段语义。

## 运行规则

| 情况 | 行为 |
| --- | --- |
| 启动或重启 | 从当前已提交末尾开始，不恢复前一进程的游标或未完成重试 |
| 正常读取 | 各进程维护自己的内存游标，按后端位置依次处理事件，监听方法依次执行 |
| 监听失败 | 继续尝试其他方法，成功方法不再重跑，仅重试失败方法；可恢复错误以 `Err` 返回，展开型 panic 也转为失败重试 |
| 当前事件重试 | 阻塞本进程的后续事件，其他实例独立运行；历史清理单独运行 |
| 断线恢复 | 保留当前进程的游标重新读取；超过保留时间的事件可能已经被清理 |
| 到期或停止 | 发送取消令牌，等待当前监听配合退出；正常退出响应 Ctrl+C 和容器 SIGTERM |
| 无效 Redis 记录 | 缺失或重复字段、无效标识与超出日期范围的记录产生告警，并保留位置标记推进游标，不派发；未知字段也不得重复 |
| 后端尚未恢复 | 宿主保持启动等待并重试，可通过停止信号取消 |

数据库启动位置、发布、后台读取和清理通过可取消的传输方法执行。每个操作保留独立事务连接，并记录后端 PID、启动时间和事务内随机标记；正常提交后连接回池，取消或未完成时禁止复用。取消使用沿用相同认证/TLS 配置的独立控制连接，精确终止该操作的后端，不按用户或 Scope 批量处理，也不等待已满的业务连接池。

控制操作最多等待十秒，其中 PostgreSQL 终止确认最多五秒，且始终受宿主剩余停机预算约束；失败输出固定诊断，不包含连接信息。根据 [PostgreSQL 服务器信号函数说明](https://www.postgresql.org/docs/current/functions-admin.html#FUNCTIONS-ADMIN-SIGNAL)，同一数据库角色可以终止自身后端；此实现使用带等待参数的 `pg_terminate_backend`（PostgreSQL 14+）。如果服务器禁止该函数、无法建立控制连接或停机预算已耗尽，不能保证查询已在服务器端结束。事件健康检查通过 `health_with_cancellation` 接收调用方令牌；数据库探测与事件操作共用 `repository_core::postgres_operation::PostgresOperation`，Redis 取消等待 PING 确认。
| 后台循环意外退出 | 报告给宿主并结束运行，避免宿主继续服务但不再接收广播 |

数据库状态行按 Scope 唯一，更新 Position 的写锁持续到消息事务提交，后续位置不能越过尚未提交的事件。状态行即使软删除仍继续使用；消息读取过滤软删除，过期清理包含软删除消息，不删除状态行。游标使用 Position，发布时间使用 CreateTime，不按雪花 ID 猜测提交顺序。

Redis 信封中的 `event-id` 复用 `common::data_helper::parse_guid`，与数据导入和请求绑定采用同一套 Guid.Parse 格式规则，支持 N/D/B/P/X 格式及其外部空白，拒绝 `urn:uuid:` 前缀。原始名称和消息正文保持不变，不因读取而裁剪或重新序列化。

数据库信封读取按实际查询列名映射。PostgreSQL 的正负无穷时间分别映射为源 DateTimeOffset 的最大、最小 UTC 时间；有限日期必须处于公元 1～9999 年。日期越界时整批读取失败并保留游标重试，不跳过坏记录，也不让日期驱动溢出终止宿主。修复存量记录后可从原位置继续读取。

WebAPI 的 `event_bus` 健康项检查选定后端连接，不发布消息、不建立范围状态、不改变消费位置。

截至 B7 基线 `acd147d`，Rust 与所对照的 NetEngine 均没有具体业务事件发布调用或监听声明；目前提供的是框架能力及宿主接入。不能把空监听宿主的启动、健康检查或退出当作业务事件发布/重试/释放的运行证明。有限源码核对、组件编译结果与所需运行条件见 [B7 记录](event-b7-review.md)，其余待办见 [阶段清单](netengine-next-stage.md)。
