# 日志、重试、缓存与调用行为

`source_generator_runtime` 的属性宏可为服务方法添加日志、重试、缓存或并发限制，让业务方法集中处理用例本身。宏按方法显式启用，不会默认拦截全部服务。

下列示例是应用服务中的方法片段，`AppError` 使用 `common::app_error::AppError`；业务类型、字段和单次调用方法由应用提供。

## 方法日志

```rust
#[source_generator_runtime::logging(
    key_scope = "example.load",
    key_include_self = false
)]
async fn load(&self, id: i64) -> Result<String, AppError> {
    self.load_once(id).await
}
```

支持同步和异步方法。普通返回值或单元值使用 `result = false`；返回异步序列时显式使用 `stream = true`，日志随枚举生命周期记录，最多捕获前一百项。

## 重试

```rust
#[source_generator_runtime::retry(
    max_retries = 3,
    delay_ms = 100,
    key_scope = "example.fetch",
    key_include_self = false
)]
async fn fetch(&self, id: i64) -> Result<String, AppError> {
    self.fetch_once(id).await
}
```

`max_retries` 不包含首次调用；默认三次重试、等待零毫秒。仅支持 async 方法，receiver 使用不可变引用，其他参数必须为 Copy；String、Vec 等使用不可变引用，并在单次尝试内按需克隆。

错误类型实现 `CancellationError`，或通过 `is_cancelled` 指定取消判定。取消错误立即返回。配置 `cancellation = token` 可中断重试等待，但正在执行的方法体仍需自行响应令牌；错误类型还需支持 `From<OperationCancelled>`。

## 缓存与并发限制

以下服务持有共享的 `cache` 和 `lock`：

```rust
#[source_generator_runtime::cacheable(
    cache = self.cache,
    lock = self.lock,
    ttl_seconds = 120,
    key_scope = "example.detail",
    key_include_self = false
)]
async fn detail(&self, id: i64) -> Result<DetailDto, AppError> {
    self.load_detail(id).await
}

#[source_generator_runtime::concurrency_limit(
    lock = self.lock,
    semaphore = 1,
    is_use_parameter = true,
    expiry_seconds = 30,
    key_scope = "example.sync",
    key_include_self = false
)]
async fn sync(&self, id: i64) -> Result<(), AppError> {
    self.sync_once(id).await
}
```

缓存仅保存成功值，TTL 从写入时计算，默认六十秒。成功类型需支持序列化与反序列化。回源时通过锁避免重复加载；缓存或锁故障会降级为业务调用。`Result<(), E>` 或 `cache_value = false` 不生成缓存逻辑。

并发限制默认为一个名额、非阻塞；`is_use_parameter = true` 将参数纳入锁键。`is_block`、`is_use_parameter` 使用布尔字面量，`semaphore` 和 `expiry_seconds` 使用整数字面量。这两个宏会管理持锁期间的续期和正常释放。

## 稳定方法键与参数键

`key_scope` 使用全局唯一、跨构建稳定的名称。receiver 方法明确设置 `key_include_self`，泛型宿主通常应包含 Self，避免不同实例类型共用缓存或锁。外围 impl 的泛型可通过 `key_types(...)`、`key_consts(...)` 补充。

自定义参数模型派生稳定键：

```rust
use source_generator_runtime::StableInvocationKey;

#[derive(StableInvocationKey)]
#[invocation_key(scope = "example.article-query")]
struct ArticleQuery {
    id: i64,
    tenant_id: i64,
}
```

全部字段默认进入参数键，Serde 的跳过或重命名不改变此规则。只需部分字段时在调用宏中使用显式 `key = (...)`。无序 HashMap/HashSet 使用 `unordered_key(&value)`，不能直接依赖迭代顺序。

泛型与关联类型需要相应的 `StableKeyType` / `StableInvocationKey` 约束；复杂情况通过 `#[invocation_key(bound = "...")]` 显式声明。

## 组合行为

使用 `invocation` 为多层行为共享一次调用的追踪标识、参数元数据和重试次数：

```rust
#[source_generator_runtime::invocation(
    key_scope = "example.load-with-retry",
    key_include_self = false
)]
#[logging]
#[retry(max_retries = 3, delay_ms = 100)]
async fn load_with_retry(&self, id: i64) -> Result<String, AppError> {
    self.load_once(id).await
}
```

上例日志包裹完整重试；将日志放在 retry 内层则每次尝试记录。方法键只在外层声明，内部行为无需另行导入属性宏。其他属性宏放在 invocation 外层。

每次进入边界创建独立上下文，嵌套或并发调用互不混合。`InvocationContext` 提供 `trace_id()`、`retry_count()`、`cancellation()`、`set_feature` 和 `feature` 等入口；参数快照仅在需要的行为中采集。

## 自定义行为与流

实现 `InvocationBehavior<N>`，通过 `#[behavior(behavior = ...)]` 或 `behaviors(A, B)` 注册。进入顺序为 A、B、方法体，退出顺序相反。

| 方法形式 | next 类型 |
| --- | --- |
| 同步方法 | `FnOnce() -> T` |
| async 方法 | `Future<Output = T>` |
| `stream = true` | `Stream<Item = T>` |

流行为在首次枚举时进入，随流的释放结束，不应提前收集整个流或后台消费。可使用 `ObservedStream` 和 `StreamObserver` 观察开始、元素与结束。重试只覆盖流的创建，不重放已消费元素。

运行时 API 见[调用管道](../source_generator/source_generator_runtime/src/pipeline.rs)，端点与资源生命周期见[宿主扩展](host-extensions.md)。
