# 宿主端点、资源、后台服务与自定义调用行为

本页对应架构清单 AR-10、AR-11、AR-13、AR-15、AR-16。使用显式注册和 Rust 类型约束，不依赖反射、程序集扫描或全局服务容器。

## 统一端点声明（AR-15）

处理器通过 `#[webapi_core::webapi_endpoint(...)]` 声明 HTTP 方法、`path`、`policy`、`permission` 和原有 utoipa 文档参数。Feature 的 `endpoints()` 只用 `webapi_core::endpoint!(crate::handlers::模块::处理器)` 列举一次；宿主从同一集合建立运行路由、权限同步清单及 OpenAPI 操作。可参考[文章处理器](../presentation/admin_webapi/src/handlers/article/mod.rs)与[文章特性](../presentation/admin_webapi/src/features/article.rs)。

| 参数/入口 | 约定 |
| --- | --- |
| `get`、`post` 等及 `path` | 同时生成运行方法映射与文档路径；可以声明多个方法，重复方法报编译错误 |
| `policy = anonymous` | 不挂认证、权限或签名；处理器自身的业务检查保持原样 |
| `policy = signed` | 只挂签名，保留管理端基础工具及文件读取策略 |
| `policy = authorized` | 挂认证、续签、权限；用于客户端受保护接口和管理端 Ueditor |
| `policy = authorized_signed` | 认证、续签、权限先执行，签名仍在 Action 阶段；用于管理端普通受保护接口 |
| `permission = "说明"` / `none` | 必须显式选择是否进入 UpdateRoute 清单；说明原样保留。匿名及仅签名端点不允许登记权限 |
| `json_body = "参数名"` | 生成正文绑定名称，沿用原有错误诊断与参数规则 |
| `document = false` | 显式不生成 OpenAPI；Ueditor 的 GET/POST 仍运行并同步为一条权限路径 |
| `configure_action` | 缓存、重复请求队列等装配，外层认证仍先执行；方法身份字符串保持原值 |
| `configure` | 保留上传正文限制、multipart 验证等外层配置 |

`responses`、`params`、`request_body`、`tag` 等继续交给 utoipa；`security` 根据 policy 生成，不能另行覆盖文档方法、路径前缀或鉴权。每个端点的文档使用 utoipa 的 OpenApi 派生收集关联 schema，避免只取 Operation 导致类型丢失。Feature 的 `openapi()` 仅补充类型和标签，宿主拒绝其中单独声明 paths。当前两端 112 个端点全部迁移；Ueditor 不公开文档，支付回调及 Demo 保持匿名。

同一宿主的路径按运行路由的 ASCII 大小写规则和方法检查重复；管理端、客户端分别装配。接口清单仍由 UpdateRoute 显式同步，宿主启动不写权限库。`WebApiFeature::register` 和 `permission_routes` 已移除，这是 Rust 扩展接口变化，不改变对外 HTTP 路由。

显式 `head` 使用声明的处理器；共享路由仅在该路径未声明 HEAD 时追加 405 拒绝处理，避免 GET 隐式开放 HEAD。该判断使用同一路径全部端点合并后的方法集合，Allow 头保持显式方法列表。

## 资源异步清理（AR-16）

公共契约为 [common::resource_cleanup::ResourceCleanup](../infrastructure/common/src/resource_cleanup.rs)，WebAPI 也从 `host::resources` 重导出。`BuildWebApiResources`、`BuildTaskServiceResources`、`BuildTaskServiceDatabases` 工厂新增同生命周期的 `&mut ResourceCleanup` 参数。登记簿由宿主独占，不能藏在资源集合内部直到工厂成功后才移交，否则无法清理部分装配结果。

- `acquire(name, 创建Future, 清理回调).await` 在创建前拒绝重复名称，成功后立即登记清理并返回资源；创建与登记之间没有新的 await 点。清理持有共享句柄的克隆，正常使用方仍获得原句柄。
- `manage(name, 已有句柄, 清理回调)` 用于移交已有资源；`register(name, 回调)` 用于单独登记。名称必须非空且宿主内唯一，开始停机后拒绝登记。被拒绝的已有值没有转移清理所有权，只做普通 Drop；新资源优先使用 acquire。
- 回调为 `FnOnce(Instant) -> Future<Output = Result<(), String>>`，接收同一绝对期限。先登记依赖、后登记消费者，退出时逆序调用；不建立自动依赖图。不要给已经由后台服务、日志或事件组件收尾的资源再登记第二次。
- 宿主先停止并排空请求/任务、后台服务和事件，再等待日志排空，最后清理登记资源。业务池及 WebAPI 日志写池、健康检查池和只读池已接入；日志排空前不会关闭其写池。具体数据库实现仍在 repository_core 和 presentation，公共 WebAPI 不依赖数据库驱动。
- 普通装配错误或装配 panic 会等待已登记资源清理；WebAPI 监听绑定失败走同一清理流程。启动取消、运行失败和正常退出都使用一次三十秒期限，后续阶段只使用剩余时间。
- 一个回调失败、panic 或超时不会跳过其余回调；错误按资源名与失败类别汇总，返回入口时同时保留原始失败。期限耗尽后的回调仍会尝试，但未完成 Future 会被取消，不承诺已成功关闭。回调必须协作让出执行权，不启动脱离宿主管理的清理任务。
- 重复 shutdown 不重跑已取出的回调，也不延长期限。无异步清理需求的资源继续普通 Drop；Resources 的类型查找和依赖边界不变。

成功构建的宿主应执行 `run().await`，决定不启动时使用 `shutdown().await`。直接丢弃宿主、外部强行取消 build/run Future、进程强杀或 panic=abort 无法通过 Rust Drop 完成异步排空；本机制不创建后台补偿任务来掩盖这一边界。数据库连接创建 Future 自身报错时，由连接库负责其内部未返回状态；已返回并登记的池由宿主处理。

## 常驻后台服务

公共契约为 [common::background_service](../infrastructure/common/src/background_service.rs)。实现 `BackgroundService`，或提供 `FnOnce(CancellationToken) -> Future<Output = Result<(), String>>` 即可注册。

- 两类宿主构建器提供 `with_background_service(name, service)?`，不要求为独立服务创建路由或任务。
- 需要宿主资源时，在 `WebApiFeature::register_background_services` 或 `TaskServiceFeature::register_background_services` 中注册。WebAPI 接收 `Arc<AppState>`，任务宿主接收现有 FeatureContext；只克隆实际需要的资源。
- 名称必须非空且在同一宿主中唯一。所有服务先登记，装配失败时不会启动这些任务；WebAPI 监听器成功绑定后才启动服务。
- 每个服务启动一次，拥有独立子取消令牌。正常结束不停止宿主；返回错误或 panic 会使宿主进入原有停机流程，并将失败返回给入口。错误文本应避免包含凭据或业务正文。
- 停机先发出取消，各服务并发进行自己的清理，再与请求、队列、事件和日志共用原有的三十秒绝对期限。后台管理器不会逐个重置超时；期限耗尽后 abort 剩余任务并返回失败。
- `run` 必须是协作式异步操作，收到取消后在返回前等待自己的资源清理；禁止在其中产生不受管理的后台任务。阻塞线程或不让出执行权的循环无法由异步 abort 强制终止。
- 管理器释放时取消子令牌并 abort 剩余任务。异步 shutdown 是正常退出路径，不能以 Drop 代替排空。

生产接入为 WebAPI 的 `webapi.health`：仍在启动十秒后首次检查，之后每六十秒检查，单次超时三十秒。检查项、健康响应以及失败日志策略未改变。任务调度、执行租约、配置刷新、事件消费和日志排空仍使用已有流程。

## 自定义调用行为

公共契约为 [InvocationBehavior](../source_generator/source_generator_runtime/src/pipeline.rs)，属性入口为 `source_generator_runtime::behavior`。

```rust,ignore
#[behavior(
    behaviors(first_behavior, second_behavior),
    key_scope = "application.example.operation",
    key_include_self = false
)]
pub async fn operation(&self, value: &str) -> Result<String, AppError> {
    self.execute(value).await
}
```

`behavior = 表达式` 声明单个行为；`behaviors(表达式, ...)` 声明多层行为。表达式可以是实现 `InvocationBehavior<N>` 的类型、泛型函数或闭包，配置通过构造函数或捕获提供。没有默认自动挂载。稳定方法键继续复用 `key_scope`、`key_types(...)`、`key_consts(...)` 和 `key_include_self`，不使用函数地址或反射结果生成键。

| 方法形式 | 传给行为的 next | 行为返回值 |
| --- | --- | --- |
| 同步方法 | `FnOnce() -> T` | `T` |
| async fn | `Future<Output = T>` | `Future<Output = T>` |
| `stream = true` | `Stream<Item = T>` | `Stream<Item = T>` |

`InvocationContext.method` 提供稳定方法键。上下文不自动复制参数、错误或凭据；行为可使用显式捕获或下述共享元数据。输出保留静态类型，`Result`、错误类型和取消分类不会被框架擦除或重新包装。行为可以明确短路或转换结果，也应负责保留调用约定；同步 continuation 只允许消费一次，重试继续使用既有 `#[retry]`。

声明 `behaviors(A, B)` 时先构造 A、再构造 B；进入顺序是 A → B → 方法体，正常退出是方法体 → B → A。多个属性按外层包裹内层处理。异步 next 不会被框架 spawn，丢弃外层 future 会释放它拥有的内层 future；取消后不会继续执行 await 后面的退出代码，需要在提前释放时执行的同步清理应放入行为自己的 RAII 守卫。panic 沿调用栈传播，不转换为业务错误。

行为保留 Rust 的借用及自动 Send 约束，不强加 `'static`，不把引用参数变成克隆副本。行为和方法体同时持有可变借用时仍由编译器拒绝；需要跨 await 共享可变资源应由业务明确设计。泛型函数或命名行为类型适合 next 类型较复杂的情形，闭包可显式标注 `InvocationContext` 以帮助推断。

## 流式生命周期

流方法需要显式 `stream = true`。默认直接返回流；`Result<Stream, E>` 使用 `result = true`。`BoxStream`、`LocalBoxStream`、`Pin<Box<_>>` 自动恢复装箱，自定义别名用 `stream_boxed = true` 指定。流准备阶段的错误直接返回，行为不观察尚未成功创建的流。

通用流管道在首次 poll 时调用行为，未枚举就释放不会进入行为。行为必须把源流保存在返回流中，随调用者的枚举和释放共同结束，不得全量收集或后台持续消费。异步流方法仍先等待创建流，不能把创建方法完成当成枚举完成。

可使用 `ObservedStream` 和 `StreamObserver<T>` 实现同步生命周期钩子：`on_start`、`on_item`、`on_finish` 与 `on_panic`。自然结束、观察器要求停止、提前释放分别为 `Exhausted`、`Stopped`、`Dropped`；停止时最后一个元素仍返回给调用方。钩子不得阻塞或 panic。自然结束和提前释放均先结束内层、再结束外层；panic 通知后继续展开。Drop 不能等待异步清理，必须等待的清理应放入流自己的正常结束路径。

已有 `#[cacheable]` 的运行时执行经过同一异步管道，缓存键、双重读取、锁续租、TTL 和错误降级不变。已有流式 `#[logging]` 使用 `StreamLoggingBehavior` 与共用观察器，仍最多捕获一百项，错误元素直接返回，提前结束记录非自然完成。普通日志与重试也使用 InvocationContext，但是否共享一次调用由下面的显式边界决定。并发限制保留原有独立属性语义。

## 一次调用的共享上下文（AR-13）

在需要统一元数据的方法上用 `#[invocation]` 包裹显式的 `logging`、`retry`、`cacheable`、`behavior` 声明。稳定方法键只在外层声明，内部声明不必单独导入属性宏。原有独立属性仍可使用；不使用边界时，每个独立属性拥有自己的上下文，不按方法键猜测调用身份。

```rust,ignore
#[source_generator_runtime::invocation(
    key_scope = "application.example.load",
    key_include_self = false,
    cancellation = cancellation
)]
#[logging]
#[retry(max_retries = 3, delay_ms = 100)]
#[behavior(behavior = self.observer())]
pub async fn load(&self, id: i64, cancellation: &CancellationToken) -> Result<Value, AppError> {
    self.load_once(id, cancellation).await
}
```

示例中的日志包裹完整重试，观察行为会逐次进入；它们得到同一追踪标识和同一元数据存储。声明顺序决定语义，例如将日志放在重试内部会记录每次尝试。一次边界只允许一个重试属性和一个缓存属性，避免重试次数或参数键含义冲突。其他属性宏应放到 `invocation` 外层；不支持用别名隐藏内部行为属性。

| 元数据 | 所有权及使用规则 |
| --- | --- |
| `method`、`trace_id()` | 方法键保持既有算法；追踪标识在新边界创建，克隆上下文共享同一次调用，不加入缓存键 |
| `arguments()` | 仅显式日志行为按既有快照及级别规则填充；普通引用参数调用后可刷新。返回独立值，不保留参数借用、不默认序列化全部方法 |
| `arguments_key()` | 缓存行为沿用既有稳定参数键算法，成功后保存；尚未执行或生成失败为 None。调用内容可能敏感，不能直接输出日志 |
| `cancellation()` | 边界显式克隆调用方令牌；不自动取消业务 Future。重试仍先检查次数耗尽，再观察取消及等待，错误分类和原始错误类型不变 |
| `retry_count()` | 首次执行为 0，重试等待成功后在进入下一次尝试前更新；尝试之间保留特性和追踪标识 |
| `set_feature(Arc<T>)`、`feature<T>()` | 显式按类型共享的调用数据，要求 Send + Sync + 'static；没有自动服务查找，锁不跨用户代码、await 或被替换值的 Drop |

嵌套调用和递归每次进入 `invocation` 都创建新上下文，没有线程/任务局部继承，也不会仅因方法键相同混合并发调用。边界内的 clone 只共享这一调用；显式传给外部行为的句柄由调用者管理，不应长期保留。

每层行为在自身作用域内克隆上层上下文句柄，内层 `async move` 只移动局部克隆。被重试或缓存包装的 continuation 每次调用都会重新建立该句柄，不消耗闭包捕获的外层上下文；追踪标识、参数键及特性仍共享同一份元数据。

流行为和流日志继续显式声明 `stream = true`：准备阶段创建上下文，成功创建的流拥有句柄，首次枚举才执行行为及开始日志；未枚举直接丢弃不会记录执行日志。枚举的正常完成、错误、panic 与提前释放沿用同一观察器；重试仅覆盖创建流的 Future，不自动重放已经消费的元素。准备阶段失败仍直接返回。取消和 Drop 不会启动后台消费或异步清理。

客户端既有缓存演示及引用参数日志已使用该边界，返回值、120 秒缓存和日志挂载范围保持不变。此实现选择共享元数据及现有行为，不复刻 .NET 服务定位器、完整代理反射或默认参数采集。
