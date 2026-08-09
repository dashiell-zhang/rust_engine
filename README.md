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

`key_scope` 是普通方法的全局唯一基础身份，普通参数类型不再通过源码 Token 加入方法键，因此导入别名、完整路径和其他等价写法不会改变方法身份，也不要求普通参数类型实现 `StableKeyType`。方法自身声明的类型泛型、const 泛型以及参数位置直接或嵌套在引用、容器等类型中的 `impl Trait` 仍会按实际单态化类型自动加入运行时维度。过程宏无法看到外围 `impl` 的泛型；这类维度应通过 `key_include_self = true` 或 `key_types(...)`、`key_consts(...)` 显式加入

普通参数类型不参与方法身份；`key_scope` 应直接表达稳定且全局唯一的业务语义。需要把新旧语义视为不同方法时，应使用新的 `key_scope`

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

`#[retry]` 同样支持 `key_types(...)`、`key_consts(...)` 和 receiver 方法的 `key_include_self`。这些参数只调整 scheduled/exhausted 日志里的 `method` 字段，不改变重试次数、等待时间或错误返回。方法自身声明的泛型会自动加入；普通参数类型不参与方法键，外围 `impl` 泛型仍需显式声明

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

返回 `Result<(), E>` 或显式配置 `cache_value = false` 时，过程宏会在展开阶段直接保留原方法，不生成参数键、缓存访问或序列化代码，因此不会要求参数实现 `StableInvocationKey`，也不会给成功类型引入缓存相关 Trait 约束。`cache`、`key_scope` 等属性语法仍需完整提供，但其中的表达式不会进入生成代码或参与类型检查

`#[concurrency_limit]` 的 `is_use_parameter`、`is_block` 只接受布尔字面量，`semaphore`、`expiry_seconds` 只接受无符号整数字面量，避免同一个锁键的并发策略随单次调用参数变化。未启用 `is_use_parameter` 时不会生成方法参数键，因此参数无需实现 `StableInvocationKey`

自由函数或静态关联函数必须通过 `key_scope` 提供全局唯一的稳定基础作用域。普通参数类型不参与方法键；方法自身声明的类型泛型、const 泛型和参数位置直接或嵌套的 `impl Trait` 会自动在作用域后加入运行时实例维度。只有这些需要取得实际单态化身份的类型泛型和 `impl Trait` 声明约束必须包含 `StableKeyType`，普通参数类型不增加该约束

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

方法键使用无版本前缀 `source-generator-method`，由 `key_scope`、可选的 `Self`、泛型实际类型和 const 泛型等稳定维度组成。纯路径写法或导入别名变化不会改变方法键

参数键使用独立的 `StableInvocationKey` 契约，不再把任意 `Serialize` 输出直接视为完整业务键。整数、字符串、UUID、Duration、Option、有序序列、元组、`BTreeSet`、`BTreeMap`、`serde_json::Number` 和 `serde_json::Value` 等基础类型已经提供实现；用户结构体和枚举应使用派生宏，并声明跨构建稳定的类型作用域

```rust
use source_generator_runtime::StableInvocationKey;

#[derive(serde::Serialize, StableInvocationKey)]
#[invocation_key(scope = "application.article-query")]
struct ArticleQuery {
    id: i64,
    #[serde(skip)]
    tenant_id: i64,
}
```

派生实现与 Serde 属性完全独立，所有字段以及枚举 variant 默认进入稳定键。派生宏还会根据同一个 `scope` 为类型生成 `StableKeyType`；泛型类型会把类型参数的稳定标签和 const 泛型值一并编码，自动约束落在非递归字段的完整声明类型上，纯 `PhantomData<T>` 只要求 `T: StableKeyType`，`Box<T>`、`Rc<T>`、`Arc<T>` 和 `PhantomData<T>` 的稳定类型标签支持 `T: ?Sized`，递归泛型仍能正常派生。当前不支持通过 `invocation_key` 跳过字段；不需要参与业务键的复杂参数应在属性上配置显式 `key = (...)`，只选择真正决定结果的稳定字段

自动递归识别只把 `Node<T>` 和 `self::Node<T>` 这类能够仅凭语法确认的路径视为当前派生类型，不会把 `std::result::Result` 等外部同名类型误判为递归字段。通过完全限定路径、类型别名或自定义包装器间接递归时，应使用 `bound` 明确替换自动字段约束

关联类型、自定义递归包装器或其他无法可靠自动推导的场景，可以通过 `bound` 字符串替换自动字段约束。显式约束中的 Trait 路径按调用方作用域解析；下面示例需要将 `StableInvocationKey` Trait 引入当前作用域

```rust
#[derive(StableInvocationKey)]
#[invocation_key(
    scope = "application.associated-query",
    bound = "T::Item: StableInvocationKey"
)]
struct AssociatedQuery<T: QueryItem> {
    item: T::Item,
}
```

`HashSet` 和 `HashMap` 不直接实现 `StableInvocationKey`，因为它们的迭代顺序不稳定。需要按无序内容参与键生成时，必须使用 `key = source_generator_runtime::unordered_key(&values)`；遗漏包装会得到编译错误，不会静默生成跨进程不稳定的缓存键或锁键

只有在调用方能够保证某个 Serde 表示完整、无损且跨构建稳定时，才可以显式使用 `trusted_serde_key(&value)`。该包装是兼容特殊类型的逃生口，不应作为普通 DTO 的默认实现方式

稳定参数键采用类型标签、长度边界、深度、节点数和总字节数限制的编码

`StableInvocationKey` 不为 `&mut T` 提供实现，因此直接参数、派生 DTO 字段或容器元素中嵌套的可变引用都不能进入缓存键，避免缓存命中跳过原方法的可变副作用。`cacheable` 对直接可见或由类型别名隐藏的可变参数还会执行额外拒绝；`concurrency_limit` 如果只需要读取可变参数生成锁键，应通过显式 `key = &*value` 创建不可变重借用

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
}?;
```

如果需要兼容旧任务名，还可以继续声明 `queue_alias` 和 `schedule_alias`。宏始终先注册全部正式任务，再登记全部别名，因此 alias 可以写在目标任务之前。宏返回 `Result<(), String>`，重复任务名、重复 alias 或同名覆盖会直接返回注册错误

所有 TaskService Feature 完成注册后，宿主会统一解析 alias 链并扁平化到最终正式任务名。alias 可以跨 Feature 指向稍后注册的任务；目标不存在或出现循环引用时，`TaskServiceHostBuilder::build` 会返回包含 alias 和目标名称的启动错误，不会静默忽略

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
