# 宿主扩展

共享宿主提供端点注册、后台服务管理和异步资源清理。业务模块只需声明自己的功能与依赖，即可复用鉴权、文档和停机流程。

## 声明 HTTP 端点

端点通过一个属性同时定义路由、权限和 OpenAPI。例如在宿主中新增 `handlers::example` 模块：

```rust
#[webapi_core::webapi_endpoint(
    get,
    path = "/Example/Ping",
    policy = anonymous,
    permission = none,
    responses((status = 200, description = "服务可用", body = String)),
    tag = "示例"
)]
pub async fn ping() -> &'static str {
    "pong"
}
```

在 Feature 中列举端点：

```rust
use webapi_core::host::features::WebApiFeature;

pub struct ExampleFeature;

impl WebApiFeature for ExampleFeature {
    fn endpoints(&self) -> Vec<webapi_core::endpoint::Endpoint> {
        vec![webapi_core::endpoint!(crate::handlers::example::ping)]
    }

    fn openapi(&self) -> Option<utoipa::openapi::OpenApi> {
        None
    }
}
```

将 Feature 加入宿主档案的功能集合后，端点可访问且出现在开发环境文档中。实际装配参考[管理端入口](../presentation/admin_webapi/src/lib.rs)。

| policy | 行为 |
| --- | --- |
| `anonymous` | 不执行认证、权限和签名 |
| `signed` | 只检查请求签名 |
| `authorized` | JWT 认证、续签和权限检查 |
| `authorized_signed` | 认证、续签、权限检查，再执行签名 |

`permission = "说明"` 将受保护端点加入权限同步清单；`none` 不登记。匿名和仅签名端点不能登记权限。同步通过 `UpdateRoute` 显式执行，见[认证与权限](authorization-initialization.md)。

JSON 正文用 `json_body = "参数名"` 声明绑定名称；`document = false` 隐藏接口文档。一个处理器可声明多个 HTTP 方法。GET 不自动支持 HEAD，需要 HEAD 时显式声明。

`params`、`responses`、`request_body` 和 `tag` 使用 utoipa 语法。Feature 的 `openapi()` 只补充 schema 和标签，路径由端点集合生成。

## 为端点提供服务

在资源工厂中注册应用服务能力；Feature 通过 `validate_resources` 声明依赖；处理器从 `app_state.services()` 取得服务契约。完整步骤见[业务模块与服务装配](database-modules.md)。

需要缓存或请求排队时，使用端点的 `configure_action`；上传正文限制等外层配置使用 `configure`。参考[客户端授权 Feature](../presentation/client_webapi/src/features/authorize.rs)和[管理端文件 Feature](../presentation/admin_webapi/src/features/file.rs)。

## 注册后台服务

独立的常驻异步工作可以通过构建器注册。以下 `builder` 表示 WebAPI 或 TaskService 宿主构建器：

```rust
let builder = builder.with_background_service(
    "report.worker",
    |cancellation: common::cancellation::CancellationToken| async move {
        // 在实际循环中同时等待工作和取消信号。
        cancellation.cancelled().await;
        // 在返回前完成该服务拥有的资源清理。
        Ok(())
    },
)?;
```

需要宿主资源时，在 Feature 的 `register_background_services` 钩子内注册。也可实现 `common::background_service::BackgroundService`。

服务名称在宿主内唯一；登记时不启动，装配成功后统一启动。返回错误或发生 panic 会触发宿主停机。处理器应响应取消并等待自己的清理完成，不应创建无人管理的后台任务。

## 注册异步资源清理

WebAPI 资源工厂接收设置、日志检查选项、共享 HTTP 工厂及 `&mut ResourceCleanup`。TaskService 的资源及数据库工厂也接收宿主持有的清理登记表。

新资源优先使用 `acquire` 创建并登记。例如，`connect_report_client` 和 `close` 为应用自行实现的异步方法，分别返回 `Result<Client, String>` 与 `Result<(), String>`，Client 是可克隆的共享句柄：

```rust
let client = cleanup.acquire(
    "report.client",
    connect_report_client(),
    |client, _deadline| async move { client.close().await },
).await?;
```

| 方法 | 场景 |
| --- | --- |
| `acquire(name, create, close)` | 创建新资源，成功后立即登记 |
| `manage(name, value, close)` | 移交已有可克隆资源 |
| `register(name, close)` | 只登记清理回调 |

先登记依赖、后登记消费者，退出时逆序清理。名称必须非空且唯一。数据库模块使用 `ModuleDatabase::connect_managed`，无需重复登记池清理；日志、事件、后台服务已有的关闭流程也不重复登记。

关闭 common 默认特性时，若只需要此功能可使用 `features = ["resource-cleanup"]`。完整 runtime 已包含它。

## 停机约定

宿主先停止并排空请求、任务、后台服务和事件，再排空日志，最后清理登记资源。整个流程共享三十秒绝对期限，回调收到的 deadline 可用于控制自己的操作。

清理失败会记录对应资源并继续尝试其余回调，超时未完成的工作会被取消。成功构建的宿主应执行 `run().await`；决定不运行时显式调用 `shutdown().await`。直接丢弃宿主或强制结束进程无法等待异步清理。

方法级日志、缓存、重试和自定义调用行为见[调用行为宏](invocation-macros.md)。
