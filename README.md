# rust_engine

基于 Rust 的分层后端工程，参考 [NetEngine](https://github.com/dashiell-zhang/NetEngine.git) 的职责划分与运行语义进行重构。

项目使用 Cargo workspace 组织，当前聚焦后端运行时能力，包含两个 WebAPI 宿主、一个 TaskService 宿主、数据库迁移工具，以及应用层、仓储层、基础设施层和源码生成能力。

## 项目定位

- 基于 Rust async 生态实现接近 NetEngine 的后端分层方案
- 保持宿主装配、业务编排、持久化、基础设施切换之间的边界清晰
- 对齐 NetEngine 的核心职责与运行语义，但不机械复制 .NET 的项目形态
- 让各类基础设施实现、数据库迁移和宿主注册可以独立演进

当前仓库不包含 NetEngine 中的前端宿主，重点是 Rust 后端部分的落地与演进。

## 当前范围

当前仓库提供的运行入口包括：

- admin_webapi：管理端 WebAPI
- client_webapi：客户端 WebAPI
- task_service：后台任务宿主
- repository_tool：数据库迁移工具

## 工作区结构

```text
rust_engine/
├── application/
│   ├── application_interface
│   ├── application_model
│   ├── application_service
│   ├── application_service_llm
│   ├── application_service_sms
│   └── application_service_task_service
├── infrastructure/
│   ├── common
│   ├── distributed_cache*
│   ├── distributed_lock*
│   ├── email*
│   ├── file_storage*
│   ├── hybrid_cache
│   ├── identifier_generator
│   ├── llm*
│   ├── logger*
│   └── sms*
├── presentation/
│   ├── admin_webapi
│   ├── client_webapi
│   └── task_service
├── project_core/
│   ├── task_service_core
│   └── webapi_core
├── repository/
├── repository_tool/
└── source_generator/
    ├── source_generator_core
    └── source_generator_runtime
```

带 `*` 的目录表示同类能力下包含抽象 crate 与多个实现 crate，例如 Redis / InMemory、Aliyun / Tencent Cloud 等。

## 分层职责

- application：应用服务、DTO、上下文契约与宿主特化应用逻辑
- repository：基于 SeaORM 的实体、关系、查询扩展与持久化逻辑
- infrastructure：缓存、分布式锁、文件、邮件、短信、日志、LLM、ID 生成等抽象与实现
- project_core：WebAPI 与 TaskService 的共享宿主装配、中间件与公共运行时能力
- presentation：各运行时宿主入口
- source_generator：过程宏与运行时辅助，减少重复注册和横切代码

## 依赖边界

整体依赖关系保持在以下主线上：

- presentation 调用 application
- application 依赖 repository 与 infrastructure
- project_core 依赖 application_interface、repository 与部分基础设施抽象
- repository_tool 直接依赖 repository，用于数据库迁移

这套边界用于约束依赖扩散：

- application_interface 承载跨宿主共享抽象，不要求所有服务都接口化
- 宿主是否启用某类能力，由 crate 引用、配置和宿主装配共同决定
- 宿主特化服务拆成独立 crate 后，可以收敛依赖面与注册范围

## 与 NetEngine 的对应关系

| NetEngine | rust_engine |
| --- | --- |
| Application.Interface | application/application_interface |
| Application.Model | application/application_model |
| Application.Service | application/application_service |
| Application.Service.LLM | application/application_service_llm |
| Application.Service.SMS | application/application_service_sms |
| Application.Service.TaskService | application/application_service_task_service |
| Repository | repository |
| Repository.Tool | repository_tool |
| Infrastructure/* | infrastructure/* |
| ProjectCore/WebAPI.Core | project_core/webapi_core |
| ProjectCore/TaskService.Core | project_core/task_service_core |
| Presentation/Admin.WebAPI | presentation/admin_webapi |
| Presentation/Client.WebAPI | presentation/client_webapi |
| Presentation/TaskService | presentation/task_service |
| SourceGenerator.Core | source_generator/source_generator_core |
| SourceGenerator.Runtime | source_generator/source_generator_runtime |

对应关系强调职责与运行方式，不代表目录命名或语言特性的逐项复制。

## 核心能力

- PostgreSQL 持久化，基于 SeaORM
- WebAPI 统一宿主装配、OpenAPI 与 Swagger UI
- JWT 鉴权、权限路由校验、Token 续签
- 请求签名校验与 RSA 字段自动解密
- 分布式缓存、Redis / InMemory 实现与 HybridCache 封装
- 分布式锁抽象及 Redis / InMemory 实现
- 文件存储抽象与本地、阿里云、腾讯云实现
- 邮件、短信、日志、LLM 等基础设施能力的抽象与多实现切换
- 队列任务与定时任务宿主能力
- 过程宏驱动的横切能力与宿主装配辅助

## 快速开始

### 1. 环境准备

- 支持 edition 2024 的 Rust 稳定版工具链
- PostgreSQL
- Redis，仅在缓存或锁切到 Redis 实现时需要

### 2. 检查配置

运行时宿主默认配置文件：

- [presentation/admin_webapi/appsettings.toml](presentation/admin_webapi/appsettings.toml)
- [presentation/client_webapi/appsettings.toml](presentation/client_webapi/appsettings.toml)
- [presentation/task_service/appsettings.toml](presentation/task_service/appsettings.toml)

首次运行前建议确认这些配置项：

- database.connection
- cors.allowed_origin_list
- jwt 相关密钥
- file 存储配置
- cache / lock 的 component 与 connection
- 邮件、短信 provider 配置，以及 LLM 模型/协议配置

仓库中的默认值面向本地开发。部署或公开仓库前，应替换示例连接串、密钥与第三方服务配置。

### 3. 初始化数据库

```bash
cargo run -p repository_tool -- migrate
```

### 4. 启动宿主

```bash
cargo run -p admin_webapi
cargo run -p client_webapi
cargo run -p task_service
```

按本地开发配置运行时，也可以使用 [.vscode/tasks.json](.vscode/tasks.json) 中已有的 VS Code 任务。

### 5. 访问入口

默认开发配置下：

- admin_webapi：`http://127.0.0.1:3000/swagger-ui`
- client_webapi：`http://127.0.0.1:3001/swagger-ui`
- OpenAPI：`/api-docs/openapi.json`
- 健康检查：`/healthz`

`task_service` 为 Worker 宿主，不提供 HTTP Swagger 入口。

## 配置约定

三个运行时宿主都遵循相同的配置加载规则：

1. 先加载 `appsettings.toml`
2. 如果设置了 `APP_ENVIRONMENT`，再加载 `appsettings.{环境名}.toml`
3. 最后应用 `APP__节名__键名` 形式的环境变量覆盖

例如：

```powershell
$env:APP_ENVIRONMENT = "Development"
$env:APP__DATABASE__CONNECTION = "postgres://postgres:postgres@127.0.0.1:5432/rust_engine"
```

如果需要切换基础配置路径，可以设置 `APP_CONFIG_PATH`，值为不带 `.toml` 后缀的路径：

```powershell
$env:APP_CONFIG_PATH = "presentation/client_webapi/appsettings"
```

环境变量命名示例：

- `APP__DATABASE__CONNECTION` 对应 `database.connection`
- `APP__HOST__LISTEN_ADDRESS` 对应 `host.listen_address`
- `APP__CORS__ALLOWED_ORIGIN_LIST__0` 对应 `cors.allowed_origin_list[0]`

## CORS 说明

`admin_webapi` 与 `client_webapi` 通过 `project_core/webapi_core` 统一复用跨域校验逻辑，白名单来自各自宿主配置中的 `cors.allowed_origin_list`。

支持的常见写法包括：

- `*`
- `*.example.com`
- `example.com`
- `localhost`
- `localhost:5173`
- `https://admin.example.com`

仓库默认配置偏向本地开发，生产环境应改成明确白名单，避免继续保留全量放通。

## 常用命令

```bash
cargo check --workspace
cargo build -p admin_webapi
cargo build -p client_webapi
cargo build -p task_service
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
```

三个宿主都提供独立的 Dockerfile：

- [presentation/admin_webapi/Dockerfile](presentation/admin_webapi/Dockerfile)
- [presentation/client_webapi/Dockerfile](presentation/client_webapi/Dockerfile)
- [presentation/task_service/Dockerfile](presentation/task_service/Dockerfile)

## Source Generator 运行时

`source_generator_runtime` 当前提供的主要横切能力包括：

- `#[logging]`：记录执行开始、成功、异常三类结构化日志
- `#[retry]`：为 async 方法注入自动重试逻辑
- `#[cacheable]`：为查询型方法注入 HybridCache 读写逻辑
- `#[concurrency_limit]`：为方法注入分布式锁并发限制逻辑

同时还提供：

- `generate_service_factory!` / `generate_direct_service_factory!`：生成应用服务工厂方法
- `register_task_registry!`：批量注册 QueueTask / ScheduleTask / alias

其中 `#[logging]` 不再默认批量挂载到所有应用服务。需要时按方法显式引入并标注即可，示例可参考 [application/application_service/src/user/service.rs](application/application_service/src/user/service.rs)。

## 示例用法

下面的示例优先选自当前仓库已有能力和语法，用于说明常见扩展点的使用方式。

### 1. 给应用服务方法挂日志

最简单的用法是在 `async fn` 上直接标注 `#[logging]`。当前仓库中的真实例子可以参考 [application/application_service/src/user/service.rs#L159](application/application_service/src/user/service.rs#L159)。

```rust
use source_generator_runtime::logging;

impl<'a> UserService<'a> {
    #[logging]
    pub async fn get_user(&self, target_id: i64) -> Result<Option<UserDto>, AppError> {
        // 方法体
    }
}
```

这个宏会自动记录 `executing`、`executed`、`exception` 三类结构化日志，适合挂在查询、外部调用和关键写操作入口。

### 2. 为外部调用加自动重试

如果方法返回 `Result<T, E>`，并且参数适合跨重试复用，可以标注 `#[retry]`。当前仓库中的真实例子可参考 [application/application_service/src/authorize/service.rs#L621](application/application_service/src/authorize/service.rs#L621) 和 [application/application_service/src/pay/service.rs#L1586](application/application_service/src/pay/service.rs#L1586)。

```rust
use source_generator_runtime::retry;

impl<'a> AuthorizeService<'a> {
    #[retry(max_attempts = 3, delay_ms = 200)]
    async fn get_wx_mini_app_open_id(&self, app_id: &str, code: &str) -> Result<String, AppError> {
        // 外部 HTTP 调用
    }
}
```

当前 Rust 侧支持的常用参数是：

- `max_attempts` 或 `max_retries`
- `delay_ms`
- `delay_seconds`

### 3. 给查询方法加缓存或并发限制

`#[cacheable]` 和 `#[concurrency_limit]` 适合放在有明确读写边界的应用服务方法上。当前仓库已经实现了这两个宏，下面是符合当前实现的最小示例语法：

```rust
use source_generator_runtime::{cacheable, concurrency_limit};

impl<'a> DemoService<'a> {
    #[cacheable(cache = self.cache, ttl_seconds = 120)]
    pub async fn get_detail(&self, id: i64) -> Result<DemoDto, AppError> {
        // 查询逻辑
    }

    #[concurrency_limit(lock = self.lock, is_use_parameter = true, semaphore = 1, expiry_seconds = 30)]
    pub async fn sync_detail(&self, id: i64) -> Result<bool, AppError> {
        // 写操作或同步逻辑
    }
}
```

这两个宏的当前参数约束可以直接参考 [source_generator/source_generator_core/src/lib.rs#L718](source_generator/source_generator_core/src/lib.rs#L718) 和 [source_generator/source_generator_core/src/lib.rs#L764](source_generator/source_generator_core/src/lib.rs#L764)。

### 4. 批量注册队列任务和定时任务

TaskService 侧通过注册表统一声明任务定义和处理器。真实例子可参考 [presentation/task_service/src/tasks.rs#L170](presentation/task_service/src/tasks.rs#L170)。

```rust
register_task_registry! {
    registry = registry;

    queue {
        definition: QueueTaskDefinition::new("System.Queue.SendSms")
            .with_semaphore(4)
            .with_remark("发送短信消息"),
        handler: handlers.send_sms_queue_task(),
    }

    schedule {
        definition: ScheduleTaskDefinition::new("System.Schedule.CleanUserToken")
            .with_cron("0 0 0 * * *")
            .with_remark("按默认保留策略定时清理用户 Token"),
        handler: handlers.clean_user_token_schedule_task(),
    }
}
```

如果需要兼容旧任务名，还可以继续声明 `queue_alias` 和 `schedule_alias`。

### 5. 通过服务工厂收口宿主调用入口

WebAPI 宿主通过源码生成的工厂方法统一创建应用服务，避免在 handler 中重复拼装上下文。真实例子可参考 [project_core/webapi_core/src/host/runtime/app_services.rs#L88](project_core/webapi_core/src/host/runtime/app_services.rs#L88)。

```rust
generate_direct_service_factory! {
    impl_generics = <>;
    factory = AppState;

    (self.primary_app_context()) {
        user: UserService<'_>,
        task_center: TaskCenterService<'_>,
        site: SiteService<'_>
    }
}
```

这样宿主调用时就可以统一写成：

```rust
let service = app_state.task_center();
let result = service.get_task_setting_list(params).await?;
```

对应的真实 handler 可参考 [presentation/admin_webapi/src/handlers/task/mod.rs#L21](presentation/admin_webapi/src/handlers/task/mod.rs#L21)。
