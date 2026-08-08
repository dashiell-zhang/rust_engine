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

使用时需要为 `async fn` 提供稳定且全局唯一的 `key_scope`。带 receiver 的方法还必须显式声明是否将 `Self` 加入方法键。当前仓库中的真实例子可以参考 [application/application_service/src/user/service.rs#L159](application/application_service/src/user/service.rs#L159)。

```rust
use source_generator_runtime::logging;

impl<'a> UserService<'a> {
    #[logging(
        key_scope = "application.user.get-user",
        key_include_self = false
    )]
    pub async fn get_user(&self, target_id: i64) -> Result<Option<UserDto>, AppError> {
        // 方法体
    }
}
```

这个宏会自动记录 `executing`、`executed`、`exception` 三类结构化日志，适合挂在查询、外部调用和关键写操作入口。异常错误摘要只会在 ERROR 日志实际启用时格式化；`#[retry]` 的 scheduled 和 exhausted 错误摘要也分别受 WARN 与 ERROR 级别门控

普通参数的声明类型会按声明顺序编码为编译期稳定的 Token 签名并自动加入方法键，因此 `u32`、`u64`、`Option<T>`、`Vec<T>` 等不同签名不会共享方法键，也不要求普通参数类型实现 `StableKeyType`。方法自身声明的类型泛型、const 泛型以及参数位置直接或嵌套在引用、容器等类型中的 `impl Trait` 仍会按实际单态化类型自动加入运行时维度。过程宏无法看到外围 `impl` 的泛型；这类维度应通过 `key_include_self = true` 或 `key_types(...)`、`key_consts(...)` 显式加入

类型别名目标、关联类型解析结果或其他无法从方法签名 Token 判断的语义发生不兼容变化时，应显式递增 `key_revision = 1`。`key_revision` 默认为 `0`，并由 `logging`、`retry`、`cacheable`、`concurrency_limit` 四个属性以相同方式加入方法键

```rust
impl StaticWorker {
    #[logging(key_scope = "StaticWorker::execute")]
    async fn execute() -> Result<(), AppError> {
        // 方法体
    }
}
```

### 2. 为外部调用加自动重试

如果方法返回 `Result<T, E>`，并且参数适合跨重试复用，可以标注 `#[retry]`。当前仓库中的真实例子可参考 [application/application_service/src/authorize/service.rs#L621](application/application_service/src/authorize/service.rs#L621) 和 [application/application_service/src/pay/service.rs#L1586](application/application_service/src/pay/service.rs#L1586)。

```rust
use source_generator_runtime::retry;

impl<'a> AuthorizeService<'a> {
    #[retry(
        max_retries = 3,
        delay_ms = 200,
        key_scope = "application.authorize.get-wx-mini-app-open-id",
        key_include_self = false
    )]
    async fn get_wx_mini_app_open_id(&self, app_id: &str, code: &str) -> Result<String, AppError> {
        // 外部 HTTP 调用
    }
}
```

当前 Rust 侧支持的常用参数是：

- `max_retries`
- `delay_ms`
- `delay_seconds`

`#[retry]` 的 receiver 必须是不可变引用，其他参数必须实现 `Copy`，并且不能使用签名级 `mut`、`ref mut`、可变引用或可变裸指针，避免某次失败尝试对参数的修改泄漏到下一次重试。需要在单次尝试内修改参数时，应从不可变原参数创建局部副本；该局部值会随方法体在每次尝试时重新初始化

```rust
#[retry(max_retries = 3, key_scope = "worker.execute")]
async fn execute(value: u32) -> Result<u32, AppError> {
    let mut attempt_value = value;
    attempt_value += 1;
    // 使用仅属于当前尝试的 attempt_value
    Ok(attempt_value)
}
```

`String`、`Vec<T>` 等非 `Copy` 参数应改为不可变引用，需要拥有值时再在方法体内通过引用为当前尝试显式克隆。直接使用非 `Copy` 参数，或者在签名中写成 `mut value: T`，都会得到编译期诊断

`#[retry]` 同样支持 `key_types(...)`、`key_consts(...)`、`key_revision` 和 receiver 方法的 `key_include_self`。这些参数只调整 scheduled/exhausted 日志里的 `method` 字段，不改变重试次数、等待时间或错误返回。普通参数声明类型与方法自身声明的泛型会自动加入，外围 `impl` 泛型仍需显式声明

```rust
impl<T, const N: usize> GenericWorker<T, N> {
    #[retry(
        max_retries = 3,
        key_scope = "generic-worker-retry",
        key_types(T),
        key_consts(N)
    )]
    async fn execute() -> Result<(), AppError> {
        // 外部调用
    }
}
```

### 3. 给查询方法加缓存或并发限制

`#[cacheable]` 和 `#[concurrency_limit]` 适合放在有明确读写边界的应用服务方法上。当前仓库已经实现了这两个宏，下面是符合当前实现的最小示例语法：

```rust
use source_generator_runtime::{cacheable, concurrency_limit};

impl<'a> DemoService<'a> {
    #[cacheable(
        cache = self.cache,
        ttl_seconds = 120,
        key_scope = "demo.get-detail",
        key_include_self = false
    )]
    pub async fn get_detail(&self, id: i64) -> Result<DemoDto, AppError> {
        // 查询逻辑
    }

    #[concurrency_limit(
        lock = self.lock,
        is_use_parameter = true,
        semaphore = 1,
        expiry_seconds = 30,
        key_scope = "demo.sync-detail",
        key_include_self = false
    )]
    pub async fn sync_detail(&self, id: i64) -> Result<bool, AppError> {
        // 写操作或同步逻辑
    }
}
```

`#[cacheable]` 的 receiver 必须是不可变引用，其他参数使用不带子模式的标识符绑定。宏只生成一份原方法体，并由缓存旁路或正常回源路径调用同一个异步闭包，因此局部 `static`、`OnceLock` 和局部函数项不会因运行时分支而分裂

`ttl_seconds` 是 L1 与 L2 共同遵守的端到端缓存有效期。HybridCache 的全局 L1 有效期只作为额外上限；从 L2 回填 L1 时会沿用条目的剩余有效期，不会因本地回填重新延长绝对截止时间

返回 `Result<(), E>` 或显式配置 `cache_value = false` 时，过程宏会在展开阶段直接保留原方法，不生成参数键、缓存访问或序列化代码，因此不会要求参数实现 `Serialize`，也不会给成功类型引入缓存相关 Trait 约束。`cache`、`key_scope` 等属性语法仍需完整提供，但其中的表达式不会进入生成代码或参与类型检查

`#[concurrency_limit]` 的 `is_use_parameter`、`is_block` 只接受布尔字面量，`semaphore`、`expiry_seconds` 只接受无符号整数字面量，避免同一个锁键的并发策略随单次调用参数变化。未启用 `is_use_parameter` 时不会序列化方法参数，因此参数无需实现 `Serialize`

自由函数或静态关联函数必须通过 `key_scope` 提供稳定基础作用域。所有普通参数的声明类型会自动加入静态签名维度；方法自身声明的类型泛型、const 泛型和参数位置直接或嵌套的 `impl Trait` 会自动在作用域后加入运行时实例维度。只有这些需要取得实际单态化身份的类型泛型和 `impl Trait` 声明约束必须包含 `StableKeyType`，普通参数类型不增加该约束

过程宏看不到外围 `impl<T>`。receiver 方法必须显式配置 `key_include_self = true` 或 `false`；泛型宿主应优先使用 `true` 并为完整 `Self` 实现 `StableKeyType`，实现时可以通过 `compose_stable_key_type` 无歧义地组合固定类型标签和泛型参数标签。无 receiver 的静态关联函数如果使用了外围类型泛型或 const 泛型，必须分别通过 `key_types(...)` 和 `key_consts(...)` 显式加入这些维度

```rust
impl<T, const N: usize> GenericLoader<T, N> {
    #[cacheable(
        cache = shared_cache(),
        key_scope = "generic-loader",
        key_types(T),
        key_consts(N)
    )]
    async fn load() -> Result<String, AppError> {
        // 查询逻辑
    }
}
```

`key_types(...)` 接受一个或多个外围类型，`key_consts(...)` 接受一个或多个外围 const 泛型或关联常量路径。方法自身声明的泛型已经自动加入，重复手工配置会产生编译诊断。确实需要让不同 receiver 类型共享同一个显式 scope 时，可以明确配置 `key_include_self = false`，并应在调用点审查共享缓存结果或锁范围是否安全

普通参数类型签名采用带版本的 Token Tree 编码，每组 Token、标识符和字面量均包含数量或字节长度边界，不依赖编译器运行时类型名称或源码空白。此修改会让四个属性的方法键发生一次有意迁移；旧缓存会自然失效，滚动发布期间新旧版本的缓存键与分布式锁键也会隔离

参数键会无损保留完整 `i128` 与 `u128` 数值。直接整数、`serde_json::Number` 和 `serde_json::Value::Number` 使用相同的最终 JSON 节点预算，不会把 serde_json 的私有透明结构重复计入项目数或深度。升级前已经使用超出 `i64/u64` 范围的 128 位整数作为缓存或并发限制参数时，旧键曾因浮点转换丢失精度；升级后这些调用会发生一次有意键迁移，以消除相邻大整数共享键的风险

参数键中的无序集合必须通过 `unordered_key` 显式声明集合语义。缓存返回值通过 `PartialEq` 校验 JSON 往返语义，因此 `HashSet` 按集合语义比较，不会因随机迭代顺序被误判为变化

默认参数键或显式键值本身不能直接或嵌套包含 `dyn Trait`。Rust 无法从任意 trait object 通用取得具体实现类型，继续生成键可能让不同实现共享缓存或锁；默认参数中直接可见的 trait object 会得到编译期诊断，类型别名、标准容器或通过 Serializer 接口暴露的派生字段会由运行时门控拒绝。`cacheable` 会绕过缓存并正常执行业务方法，启用参数键的 `concurrency_limit` 会在获取锁前返回明确错误，未启用参数键时不受影响。确需动态多态时，应通过显式 `key = (...)` 排除 trait object，并把稳定实现标签与必要业务维度纳入显式键；普通泛型参数和 `impl Trait` 仍会按实际单态化类型隔离。命名包装类型如果通过完全自定义的 `Serialize` 在调用 Serializer 接口前隐藏内部 trait object，则无法由该运行时门控通用反射，必须由包装类型或显式键保证实现身份稳定

参数直接包含，或通过派生结构体字段、标准容器元素嵌套包含 `std::sync::Mutex`、`std::sync::RwLock` 或启用 Serde 的 `lock_api`/`parking_lot` 同步锁时，会在对应锁的 Serde 实现尝试加锁前被拒绝，避免参数键生成阻塞异步工作线程或发生自锁死。自定义 `Serialize` 如果在调用 Serializer 接口前自行执行阻塞逻辑，则不在该运行时门控的保护范围内

`cacheable` 不支持可变引用或可变裸指针参数：直接可见的类型会得到编译期诊断，类型别名隐藏的可变类型会在访问缓存前由运行时门控拒绝，即使显式 `key = (...)` 没有选择该参数也不能绕过。被选入缓存键的派生结构体字段或容器元素同样会递归检查。安全拒绝会记录缓存降级日志并直接执行业务方法，保留原方法对参数的可观察修改；公共参数键和并发限制参数键不应用这项缓存专用约束

参数规范化会在构建 `serde_json::Value` 前拒绝同一对象内的重复属性，并按 JSON 反转义后的属性名比较，因此 `"x"` 与 `"\u0078"` 也视为重复。该规则适用于 RawValue 和自定义 SerializeMap，可避免不同原始输入在属性覆盖后生成相同键

这两个宏的当前参数约束可以直接参考 [source_generator/source_generator_core/src/lib.rs](source_generator/source_generator_core/src/lib.rs)。

`generate_service_factory!` 默认继续兼容 `ApplicationServiceFactory<'a, State>` 形状。其他生命周期名称、无生命周期、额外类型或 const 泛型以及自定义 where 子句，可以在 `factory` 前声明 `impl_generics = <...> where ...;`

过程宏会按调用方 `Cargo.toml` 解析 `source_generator_runtime` 的实际依赖名称，因此可以通过 Cargo 的 `package` 配置重命名后使用，无需额外声明 `extern crate`

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
