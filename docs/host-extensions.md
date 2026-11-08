# 宿主后台服务与自定义调用行为

本页对应架构清单 AR-10、AR-11。使用显式注册和 Rust 类型约束，不依赖反射、程序集扫描或全局服务容器。

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

`InvocationContext.method` 提供稳定方法键。上下文不自动复制参数、错误或凭据；行为需要的值通过显式捕获传递。输出保留静态类型，`Result`、错误类型和取消分类不会被框架擦除或重新包装。行为可以明确短路或转换结果，也应负责保留调用约定；同步 continuation 只允许消费一次，重试继续使用既有 `#[retry]`。

声明 `behaviors(A, B)` 时先构造 A、再构造 B；进入顺序是 A → B → 方法体，正常退出是方法体 → B → A。多个属性按外层包裹内层处理。异步 next 不会被框架 spawn，丢弃外层 future 会释放它拥有的内层 future；取消后不会继续执行 await 后面的退出代码，需要在提前释放时执行的同步清理应放入行为自己的 RAII 守卫。panic 沿调用栈传播，不转换为业务错误。

行为保留 Rust 的借用及自动 Send 约束，不强加 `'static`，不把引用参数变成克隆副本。行为和方法体同时持有可变借用时仍由编译器拒绝；需要跨 await 共享可变资源应由业务明确设计。泛型函数或命名行为类型适合 next 类型较复杂的情形，闭包可显式标注 `InvocationContext` 以帮助推断。

## 流式生命周期

流方法需要显式 `stream = true`。默认直接返回流；`Result<Stream, E>` 使用 `result = true`。`BoxStream`、`LocalBoxStream`、`Pin<Box<_>>` 自动恢复装箱，自定义别名用 `stream_boxed = true` 指定。流准备阶段的错误直接返回，行为不观察尚未成功创建的流。

通用流管道在首次 poll 时调用行为，未枚举就释放不会进入行为。行为必须把源流保存在返回流中，随调用者的枚举和释放共同结束，不得全量收集或后台持续消费。异步流方法仍先等待创建流，不能把创建方法完成当成枚举完成。

可使用 `ObservedStream` 和 `StreamObserver<T>` 实现同步生命周期钩子：`on_start`、`on_item`、`on_finish` 与 `on_panic`。自然结束、观察器要求停止、提前释放分别为 `Exhausted`、`Stopped`、`Dropped`；停止时最后一个元素仍返回给调用方。钩子不得阻塞或 panic。自然结束和提前释放均先结束内层、再结束外层；panic 通知后继续展开。Drop 不能等待异步清理，必须等待的清理应放入流自己的正常结束路径。

已有 `#[cacheable]` 的运行时执行经过同一异步管道，缓存键、双重读取、锁续租、TTL 和错误降级不变。已有流式 `#[logging]` 使用 `StreamLoggingBehavior` 与共用观察器，仍最多捕获一百项，错误元素直接返回，提前结束记录非自然完成。普通同步/异步日志、重试和并发限制维持原有专用语义，不重复实现第二套默认行为。
