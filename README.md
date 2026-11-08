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

常驻服务注册与自定义横切行为见 [宿主扩展](docs/host-extensions.md)。

按需启用的请求字段解密见 [RSA 请求字段解密](docs/webapi-rsa.md)。

模型属性读取、显示名称和 Excel 导出见 [公共数据工具](docs/common-data.md)。
URL 参数、文本过滤和地址分类见 [公共字符串工具](docs/common-string.md)。

脚本执行入口位于 [system_helper](infrastructure/common/src/system_helper.rs)：`linux_shell`、`windows_shell` 同步执行并默认等待 60 秒；`*_blocking_with_options` 接受自定义超时与取消令牌，异步调用使用 `*_with_options`。取消时调用令牌的 `cancel()` 并继续等待方法返回，才能等待进程退出及输出排空；直接丢弃执行 future 仅做尽力清理。取消返回 `Interrupted`，内部错误为 `OperationCancelled`；超时返回 `TimedOut`，命令非零退出保留完整错误输出。

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
│   ├── application_runtime
│   ├── application_model
│   ├── application_service
│   ├── application_service_llm
│   └── application_service_sms
├── infrastructure/
│   ├── cancellation
│   ├── common
│   ├── distributed_cache*
│   ├── distributed_lock*
│   ├── email*
│   ├── event_bus*
│   ├── file_storage*
│   ├── hybrid_cache
│   ├── identifier_generator
│   ├── llm*
│   ├── logger*
│   ├── queue_task_database
│   ├── repository_core
│   └── sms*
├── presentation/
│   ├── admin_webapi
│   ├── client_webapi
│   ├── task_service
│   └── webapi_support
├── project_core/
│   ├── task_service_core
│   └── webapi_core
├── repository/
├── repository_tool/
├── deployment/
└── source_generator/
    ├── source_generator_core
    └── source_generator_runtime
```

带 `*` 的目录表示同类能力下包含抽象 crate 与多个实现 crate，例如 Redis / InMemory、Aliyun / Tencent Cloud 等。

## 分层职责

- application：应用服务、DTO、上下文契约与宿主特化应用逻辑
- repository：基于 SeaORM 的业务实体、关系与持久化逻辑
- infrastructure/repository_core：不依赖业务模型的软删除查询扩展、只读连接、迁移、分区维护与连接健康检查
- infrastructure/queue_task_database：通用任务实体和生产者；绑定宿主选定的业务库，用户标识由业务模块解释
- infrastructure/logger_database：独立日志读写连接、查询服务、落盘缓冲和日志 schema
- infrastructure/event_bus*：跨进程广播契约、Redis Streams 和独立数据库实现
- infrastructure：缓存、分布式锁、文件、邮件、短信、日志、LLM、ID 生成等抽象与实现
- project_core：WebAPI 与 TaskService 的共享宿主装配、中间件与公共运行时能力
- presentation：各运行时宿主入口；webapi_support 复用管理端与客户端的业务装配
- source_generator：过程宏与运行时辅助，减少重复注册和横切代码
- deployment：生成 Nginx、systemd、环境配置示例及云效流水线，构建三个宿主的 Linux 发布包

## 依赖边界

整体依赖关系保持在以下主线上：

- presentation 调用 application
- application 依赖 repository 与 infrastructure
- task_service_core 依赖 application_interface、queue_task_database、repository_core 与日志等基础设施；业务任务服务由 presentation/task_service 装配
- webapi_core 仅通过 WebApiApplication 契约调用续签、权限和路由同步策略，不依赖具体应用服务或业务仓库；presentation/webapi_support 装配业务服务，由两个 WebAPI 入口显式选择
- repository_tool 直接依赖 repository，用于数据库迁移

这套边界用于约束依赖扩散：

- WebAPI 业务调用通过 application_interface::services 的应用契约；具体实现只在运行入口的适配器中选择。后台消息任务同样通过契约工厂调用，具体类型仅在宿主装配和应用内部使用
- application_interface 依赖 application_model，模型层不反向依赖接口层。服务参数和结果使用模型或通用类型，不暴露数据库连接、实体或 HTTP 上下文
- application_runtime 承接实现所需上下文；JWT 编解码归属 infrastructure/jwt_auth，HTTP 工厂归属 infrastructure/http_client，文件、邮件、短信契约归属各自基础设施组件；接口/模型使用 common 的轻量模式。WebAPI 核心不引用应用执行上下文或数据库驱动。宿主资源、LLM 独立依赖、模块数据库和事务绑定详见 [业务模块说明](docs/database-modules.md)
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
| Repository | repository |
| Repository.Tool | repository_tool |
| Infrastructure/Repository.Core | infrastructure/repository_core |
| Infrastructure/QueueTask.Database | infrastructure/queue_task_database |
| Infrastructure/Logger.DataBase | infrastructure/logger_database |
| Infrastructure/EventBus、EventBus.Redis、EventBus.Database | infrastructure/event_bus、event_bus_redis、event_bus_database |
| Infrastructure/* | infrastructure/* |
| ProjectCore/WebAPI.Core | project_core/webapi_core |
| ProjectCore/TaskService.Core | project_core/task_service_core |
| Presentation/Admin.WebAPI | presentation/admin_webapi |
| Presentation/Client.WebAPI | presentation/client_webapi |
| Presentation/TaskService | presentation/task_service |
| SourceGenerator.Core | source_generator/source_generator_core |
| SourceGenerator.Runtime | source_generator/source_generator_runtime |

对应关系强调职责与运行方式，不代表目录命名或语言特性的逐项复制。

完整对齐仍在进行中，已完成项和待核验差异见 [NetEngine 对齐进度](docs/netengine-alignment.md)。

宿主 HTTP 工厂、普通请求、文件流上传和下载的用法见 [对外 HTTP 调用](docs/http-client.md)。

队列状态、父子回调、并行执行与停机语义参见 [任务服务](docs/task-service.md)。

日志缓冲、失败恢复、查询与诊断参见 [数据库日志](docs/database-logger.md) 和 [本地文件日志](docs/local-file-logger.md)。

两个 WebAPI 的地区、字典、雪花 ID、二维码及客户端验证码用法见 [基础接口](docs/base-api.md)。地区与字典查询通过基础服务使用宿主只读连接，不走结果缓存。

JSON 与查询参数的字段大小写、重复字段和可空标量规则见 [WebAPI 请求绑定](docs/request-binding.md)。两个宿主共用解析入口，输入规则不修改响应序列化。

日志表的按月分区声明、迁移和自动维护参见 [PostgreSQL 时间分区](docs/postgresql-partitions.md)。

实体写入与冲突处理参见 [PostgreSQL 行版本与并发保存](docs/optimistic-concurrency.md)。

共享节点配置、租约和固定编号模式参见 [ID 生成器](docs/identifier-generator.md)。

### 任务持久化边界

任务生产者提供 `create(&DatabaseTransaction, ...)` 和 `create_single(...)` 两个入口。前者随调用方业务事务提交或回滚，后者使用绑定的业务连接池独立保存。消息任务、示例任务、回调任务和任务中心复用同一入队实现；邮件参数与附件目录要求见[任务服务](docs/task-service.md#邮件任务)。

迁移 `202611070001` 为有效队列任务定义和定时任务模板添加唯一约束，带参数的定时任务实例允许重复。它还移除任务配置审计用户字段对固定 `User` 表的外键；这些字段本身和现有记录保持不变。如果现有数据有重复的有效定义，唯一索引创建会报错，需要先处理冲突记录。

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
- 所有实例独立消费的事件广播，支持 Redis 和数据库，详见 [事件总线](docs/event-bus.md)
- 过程宏驱动的横切能力与宿主装配辅助

## 快速开始

### 1. 环境准备

- 最新 Rust 稳定版工具链（edition 2024），通过下文的 Docker 开发环境使用，无需在本机安装
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

先配置 [repository_tool/appsettings.toml](repository_tool/appsettings.toml)。构建时复制为可执行文件旁的 `repository_tool.appsettings.toml`，默认按工具位置读取，不依赖当前目录或 WebAPI 配置。工具只读取这份配置，不使用命令行连接覆盖、环境变量或宿主环境配置。

工具无参数运行只显示帮助。`json-index --target business|log|event-bus` 可只读比较模型拥有的 JSON 索引并输出 SQL，不执行 DDL；具体用法见 [数据库工具文档](docs/repository-tool.md)。

```bash
docker compose exec -T dev cargo run --locked -p repository_tool -- migrate --target business
docker compose exec -T dev cargo run --locked -p repository_tool -- migrate --target log
# 仅使用数据库事件后端时需要
docker compose exec -T dev cargo run --locked -p repository_tool -- migrate --target event-bus
```

需要先审查 SQL 时，可使用 `migration-script --target business --output /workspace/dist/business-migrations.sql` 离线导出，再单独执行；区间与幂等导出见 [数据库工具](docs/repository-tool.md)。

模型变化后，在容器内使用 `cargo run --locked -p repository_tool -- migration-add <名称> --target business` 生成增量 Up/Down 和目标模型快照，审查并重新编译后再执行迁移。`migration-pending` 可离线检查未生成的模型变化；生成目录、组件目标及当前支持范围见 [数据库工具](docs/repository-tool.md)。

即使各个模型使用同一个物理数据库，也需分别迁移。业务历史表使用 `__EFMigrationsHistory_Business`，日志使用 `__EFMigrationsHistory_Log`，事件使用 `__EFMigrationsHistory_EventBus`。每个目标只管理自身模型，缺少选定目标连接时直接报错。旧库已有的日志表和数据不会因拆分而被删除或搬迁。

宿主的 `[logger]` 支持 `connection`、`read_connection` 和 `database_identity`。写连接为空时显式选用 `database.connection` 地址，但建立独立连接池；日志读连接为空时选用日志写连接地址，不使用业务只读库。日志 SQL 诊断关闭，避免递归写日志。使用 `local_file` 时不创建日志写池；WebAPI 的日志查询仍使用独立只读池。

数据库日志积压目录按存储标识、项目和数据库目标计算，切换目标不会消费旧目标文件。旧版直接放在 `Logs` 下的积压文件不会被自动导入新目录，迁移前需确认其原目标再处理。损坏的批次在有效记录提交后保留为 `.rejected` 文件；日志清理只处理当前项目。

### 4. 启动宿主

```bash
docker compose exec -T dev cargo run --locked -p admin_webapi
docker compose exec -T dev cargo run --locked -p client_webapi
docker compose exec -T dev cargo run --locked -p task_service
```

按本地开发配置运行时，也可以使用 [.vscode/tasks.json](.vscode/tasks.json) 中已有的 VS Code 任务。

首次准备管理端账号时，在 Development 环境调用 `POST /Authorize/InitData`，按 [权限初始化说明](docs/authorization-initialization.md) 配置随程序发布的初始化文件。初始化和路由同步均为显式操作，宿主启动不改写权限数据。

### 5. 访问入口

Development 环境开放以下文档入口，其他环境不注册 Swagger：

- admin_webapi：`http://127.0.0.1:3000/swagger`
- client_webapi：`http://127.0.0.1:3001/swagger`
- OpenAPI：`/swagger/v1/swagger.json`

两端在所有环境提供 `/healthz`，健康时返回纯文本 `Healthy`，失败返回 503 与 `Unhealthy`。检查范围与后台周期见 [WebAPI 健康检查](docs/webapi-health.md)。

管理端公开工作目录下的 `wwwroot`，客户端不开放静态目录。文件根目录与公开范围见 [WebAPI 静态文件](docs/webapi-static-files.md)，接口用法见 [文件上传与下载](docs/webapi-files.md)，Release 请求签名见 [请求签名](docs/webapi-signing.md)。

`task_service` 为 Worker 宿主，不提供 HTTP Swagger 或健康检查入口。

非 Development 环境的 WebAPI 自动按客户端 `Accept-Encoding` 使用 Brotli 或 gzip 压缩文本、JSON、XML、JavaScript 与 WebAssembly 响应，同质量时优先 Brotli。范围响应和已有编码的响应保持原样，图片、二进制下载与 SSE 不在默认压缩类型中；开发环境不启用响应压缩。

反向代理、307 HTTPS 重定向、HSTS 和请求地址的使用方式见 [WebAPI 代理与 HTTPS](docs/webapi-network.md)。支付通知地址使用当前请求的协议与 Host；`host.https_port` 配置外部 HTTPS 端口，`host.tls` 可启用直接 HTTPS 监听并加载 PEM 或 PFX 证书。

微信预支付使用宿主缓存：JSAPI/App 参数缓存 1.8 小时，H5 链接缓存 4.5 分钟，PC 链接缓存 15 分钟；两种链接按原始字符串保存，JSAPI/App 的 `timeStamp` 按长整数约定返回字符串。平台证书缓存一小时，回源时持有默认 60 秒锁，下载响应验签成功后写入并重新读取缓存。

四个 `CreateWeiXinPay*` 创建接口与源模板一致允许匿名访问，按原始订单号查找未删除记录；订单或创建所需配置不存在时返回空体 204。创建参数取 `WeiXinPay` 模块各键的首值，出站请求的证书、私钥和 V3 密钥再按商户号所在的配置组选择；无组配置使用空 GroupId。JSAPI/App 还需要模块内的调起签名私钥，H5/PC 不增加这一前置条件。

三个 `CreateAliPay*` 接口允许匿名访问。PC/H5 从 `AliPayWeb` 读取配置并为“待支付”订单生成签名链接；小程序从 `AliPayMiniApp` 读取配置，买家取订单创建人的外部账号绑定。两种模块均需要 `AppId`、`AppPrivateKey`、`AliPayPublicKey`，按模块内各键的首值读取；配置、订单或小程序绑定缺失时返回空体 204。PC/H5 在本地生成链接，不在生成阶段向支付宝下单；服务端调用使用配置的支付宝公钥验证响应。

`/Pay/AliPayNotify` 接收普通表单或 multipart 表单，重复字段按出现顺序用逗号连接，文件不作为验签字段。公钥从匹配应用的配置组读取；验签通过后保存通知中的订单支付信息，“业务逻辑”类型返回 `success`，其他类型保存后返回空体 204。空表单、验签不匹配或没有有效订单也返回 204；签名格式、配置和数据转换错误经过共享异常响应处理。

`/Pay/WeiXinPayNotify/{mch_id}` 使用微信原始 JSON 字段名，例如 `event_type`、`resource.original_type`、`resource.associated_data`，并对原始正文验签。交易通知保存未支付订单，已支付订单直接确认；退款通知完成解密及解析后确认接收。成功返回 HTTP 200 与 `{"code":"SUCCESS","message":null}`，处理失败返回 HTTP 501 与 `{"code":"FAIL","message":"失败"}`。通知解密使用 `WeiXinPay` 模块首个 `MchApiV3Key`；平台证书按路径商户号读取缓存，未命中时才加载该商户的传输配置。

支付服务中的微信和支付宝退款、退款查询目前是无参模板方法，没有对应 HTTP 路由。交易号、退款单号与金额仍为模板占位值；接入实际退款业务时需要填充这些参数，并实现结果处理。

## 使用本机 Docker 开发（无需安装 Rust）

已运行 Linux 容器模式的 Docker Desktop 即可使用此方式，无需在 Windows 安装 Rust、编译器或其他组件。

三个宿主的 Dockerfile 使用 `rust:latest` 基础镜像，[rust-toolchain.toml](rust-toolchain.toml) 使用 `stable` 通道，不固定 Rust 版本。开发镜像为该通道安装 rustfmt 和 Clippy。已安装的工具链不会随每次 Cargo 命令自动更新；需要刷新工具链和基础镜像时执行 `docker compose build --pull --no-cache dev`，随后执行 `docker compose up -d dev`。

在项目根目录执行：

```powershell
docker compose up -d --build dev
docker compose exec -T dev cargo check --workspace --locked
docker compose exec -T dev cargo build --workspace --locked
```

[compose.yaml](compose.yaml) 使用管理端 Dockerfile 的 `dev` 阶段，在已有 `dev-base` 上补充 rustfmt 和 Clippy，三个宿主和全部 workspace crate 共用同一开发容器。源码直接挂载到 `/workspace`，本地编辑立即生效；Cargo 下载缓存和 `/cargo-target` 编译产物保存在 Docker 命名卷中，重建容器后仍可复用。首次启动需要下载镜像和依赖，默认同时执行 4 个编译任务，可在启动前通过 `CARGO_BUILD_JOBS` 环境变量调整。

常用开发命令：

```powershell
docker compose exec -T dev cargo fmt --all
docker compose exec -T dev cargo clippy --workspace --lib --bins --locked -- -D warnings
docker compose exec dev bash
```

VS Code 的 [.vscode/tasks.json](.vscode/tasks.json) 已提供 `container` 前缀的启动、检查、构建、格式化、Clippy 和运行任务，会先自动启动开发容器；默认构建任务也已切换为容器内的工作区构建。原有不带此前缀的 Cargo 任务仍使用本机工具链。容器检查和构建命令不运行测试。

容器默认设置 `APP_ENVIRONMENT=Development`，并将 API 的 3000、3001 端口仅映射到本机。编译不需要数据库；运行宿主前需按实际环境配置 PostgreSQL，以及所选实现需要的 Redis 等服务。容器内的 `localhost` 指容器自身，访问 Windows 上的服务请使用 `host.docker.internal`；可以通过 `exec -e` 传入项目已有的配置覆盖变量。例如，连接已准备好的本机开发数据库：

```powershell
docker compose exec -e APP__DATABASE__CONNECTION=postgres://postgres:postgres@host.docker.internal:5432/rust_engine dev cargo run --locked -p admin_webapi
```

启动客户端或任务服务时，将包名替换为 `client_webapi` 或 `task_service`；需要迁移时，在配置好连接后使用同样方式运行 `cargo run --locked -p repository_tool -- migrate --target business`。此开发配置不会自动创建数据库或执行迁移。

停止并移除开发容器（保留缓存）：

```powershell
docker compose down
```

容器生成的是 Linux 可执行文件，可继续在容器内运行。

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

未提供 CORS 配置时，框架默认白名单为 `*.xxx.com`。域名匹配不区分 ASCII 大小写，通配子域不包含根域；显式协议和端口必须匹配。有效预检返回空体 204，并按请求返回允许的方法和请求头，缓存时长为 7200 秒。未允许的来源不返回跨域许可头；没有预检请求头的普通 OPTIONS 仍交给路由处理。

框架不全局公开响应头，也不额外声明私有网络访问许可。续签实际产生 `NewToken` 时，由认证能力公开该头。公开静态资源经过同一跨域策略；Development 的 Swagger GET 按文档服务路径直接响应。

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

部署配置生成与 Linux 发布包用法见 [部署文档](docs/deployment.md)。生成配置可执行 `docker compose exec -T dev cargo run --locked -p deployment`；发布打包同样通过容器完成。

## Source Generator 运行时

`source_generator_runtime` 当前提供的主要横切能力包括：

- `#[logging]`：记录执行开始、成功、异常三类结构化日志
- `#[retry]`：为 async 方法注入自动重试逻辑
- `#[cacheable]`：为查询型方法注入分布式缓存读写和回源锁
- `#[concurrency_limit]`：为方法注入分布式锁并发限制逻辑

同时还提供：

- `generate_service_factory!` / `generate_direct_service_factory!`：生成应用服务工厂方法
- `register_task_registry!`：批量注册 QueueTask / ScheduleTask / alias

其中 `#[logging]` 不再默认批量挂载到所有应用服务。需要时按方法显式引入并标注即可，示例可参考 [客户端演示服务](presentation/client_webapi/src/services.rs)。

## 示例用法

下面的示例优先选自当前仓库已有能力和语法，用于说明常见扩展点的使用方式。

### 1. 给应用服务方法挂日志

日志宏支持同步和异步方法，默认处理 `Result<T, E>`（包括类型别名）；返回普通值或 `()` 时使用 `result = false`。引用参数的修改会保留，panic 展开或异步调用提前释放会记录异常阶段。使用时需要提供稳定且全局唯一的 `key_scope`。带 receiver 的方法还必须显式声明是否将 `Self` 加入方法键。当前仓库中的真实例子可参考 [Demo2Service::add_ref](presentation/client_webapi/src/services.rs)。

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

这个宏自动记录 `executing`、`executed`、`exception`，载荷包含 `method`、`traceId`、命名参数 `args`、`durationMs` 及成功返回的 `result`。单元返回值不输出 result。引用参数在调用后刷新，借用返回值仍占用可变参数时可配置 `refresh_arguments = false` 保留调用前快照。参数解构应放在方法体内。

快照优先保留 Serde 的 JSON 类型，递归路径重复值写为 null，序列化失败时回退为 Display 文本或类型名称。泛型值需要记录字段时，在声明处提供 `Serialize` 约束；未声明该约束的泛型按可用能力回退。取消令牌使用固定占位符。业务 panic 在记录后恢复原始 payload；不能从 Rust Error 自动恢复抛出现场的原始堆栈，`stackTrace` 捕获的是日志边界，`innerMessage` 来自 Error::source，其余无法取得的信息保持 null。错误格式化只在 ERROR 启用时执行。

返回异步序列时使用 `stream = true`，日志从首次消费开始，准备阶段错误直接传播。默认流元素为 Result，普通元素设置 `stream_item_result = false`。完成或提前释放时输出 `EnumeratedCount`、`CompletedNaturally`、`Truncated` 和 `CapturedItems`，仅在 Information 启用时保存前 100 项的独立快照；未消费的流不记录调用日志。每次创建流都有独立状态。常见 `BoxStream`、`LocalBoxStream` 和 `Pin<Box<_>>` 返回类型自动装箱，自定义流别名可显式配置 `stream_boxed = true`。

`#[retry]` 的 scheduled 和 exhausted 错误摘要分别受 WARN 与 ERROR 级别门控

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

如果方法返回 `Result<T, E>`，并且参数适合跨重试复用，可以按需标注 `#[retry]`。以下展示接入方式；该属性不会默认添加到业务调用。

```rust
use source_generator_runtime::{retry, CancellationToken};

impl ExternalService {
    #[retry(
        max_retries = 3,
        delay_ms = 200,
        cancellation = cancellation,
        key_scope = "application.external.fetch",
        key_include_self = false
    )]
    async fn fetch(&self, id: i64, cancellation: &CancellationToken) -> Result<String, AppError> {
        // 方法体按需将 cancellation 传给实际调用。
        self.client.fetch(id, cancellation).await
    }
}
```

当前 Rust 侧支持的常用参数是：

- `max_retries`
- `delay_ms`
- `delay_seconds`
- `cancellation`：调用方取消令牌表达式，支持令牌、引用及 `Arc` 的借用
- `is_cancelled`：自定义错误取消判定，形如 `|error: &MyError| error.is_cancelled()`

错误类型默认实现 `CancellationError`；已有 `AppError`、`LlmError` 和 `OperationCancelled` 提供实现。泛型错误须在声明中保留该约束；外部错误类型可用 `is_cancelled` 指定判定，不能按错误文本或任意内部错误推断取消。提供 `cancellation` 时还需 `From<OperationCancelled>`，用于返回等待期间的取消错误。

`max_retries` 表示首次执行以外的重试次数，零值直接执行一次。方法返回取消错误时立即原样返回，不记录 scheduled/exhausted；普通失败先判断次数是否耗尽，再检查令牌，最后记录重试并等待。正数间隔可被令牌中断；已执行的方法体自行协作取消，宏不强制中断。令牌通过属性显式选择，不按参数名或类型名称猜测。

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
        lock = self.lock,
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

`cache` 接受宿主的 `DistributedCache` 实现、引用或 `Arc`，`lock` 指定回源保护锁。`ttl_seconds` 必须大于零，从写入缓存开始计算，不隐式启用本地一级缓存。成功值按声明的类型序列化与反序列化，不要求 `PartialEq` 或额外执行往返比较。默认回源锁租约为 60 秒，持锁期间自动续期；锁覆盖二次读取、原方法执行和缓存写入，正常结束等待释放，调用被取消时释放句柄。缓存读写或获取锁失败会记录降级信息并继续执行业务。

返回 `Result<(), E>` 或显式配置 `cache_value = false` 时，过程宏会在展开阶段直接保留原方法，不生成参数键、缓存访问或序列化代码，因此不会要求参数实现 `StableInvocationKey`，也不会给成功类型引入缓存相关 Trait 约束。`cache`、`key_scope` 等属性语法仍需完整提供，但其中的表达式不会进入生成代码或参与类型检查

`#[concurrency_limit]` 的 `is_use_parameter`、`is_block` 只接受布尔字面量，`semaphore`、`expiry_seconds` 只接受固定整数字面量；非正值分别使用并发数 1 和 60 秒租约，避免同一个锁键的并发策略随单次调用参数变化。持锁期间自动续期，正常结束等待释放，取消释放句柄。阻断和参数键错误属于内部调用异常，不转换成请求队列过滤器的业务参数错误。未启用 `is_use_parameter` 时不会生成方法参数键，因此参数无需实现 `StableInvocationKey`

自由函数或静态关联函数必须通过 `key_scope` 提供全局唯一的稳定基础作用域。普通参数类型不参与方法键；方法自身声明的类型泛型、const 泛型和参数位置直接或嵌套的 `impl Trait` 会自动在作用域后加入运行时实例维度。只有这些需要取得实际单态化身份的类型泛型和 `impl Trait` 声明约束必须包含 `StableKeyType`，普通参数类型不增加该约束

过程宏看不到外围 `impl<T>`。receiver 方法必须显式配置 `key_include_self = true` 或 `false`；泛型宿主应优先使用 `true` 并为完整 `Self` 实现 `StableKeyType`，实现时可以通过 `compose_stable_key_type` 无歧义地组合固定类型标签和泛型参数标签。无 receiver 的静态关联函数如果使用了外围类型泛型或 const 泛型，必须分别通过 `key_types(...)` 和 `key_consts(...)` 显式加入这些维度

```rust
impl<T, const N: usize> GenericLoader<T, N> {
    #[cacheable(
        cache = shared_cache(),
        lock = shared_lock(),
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

TaskService 侧通过注册表统一声明任务定义和每次执行的处理器工厂。真实例子可参考 [presentation/task_service/src/tasks.rs](presentation/task_service/src/tasks.rs)。

```rust
register_task_registry! {
    registry = registry;

    queue {
        definition: QueueTaskDefinition::new("MessageTask.SendSMS")
            .with_semaphore(1),
        factory: handlers.scoped(BuiltinTaskHandlerFactory::send_sms_queue_task),
    }

    schedule {
        definition: ScheduleTaskDefinition::new("ShowTime")
            .with_cron("0/1 * * * * ?"),
        factory: handlers.scoped(BuiltinTaskHandlerFactory::demo_show_time_schedule_task),
    }
}?;
```

默认注册清单不包含旧任务名。自定义宿主需要名称映射时，可以显式声明 `queue_alias` 和 `schedule_alias`。宏始终先注册全部正式任务，再登记全部别名，因此 alias 可以写在目标任务之前。宏返回 `Result<(), String>`，重复任务名、重复 alias 或同名覆盖会直接返回注册错误

所有 TaskService Feature 完成注册后，宿主会统一解析 alias 链并扁平化到最终正式任务名。alias 可以跨 Feature 指向稍后注册的任务；目标不存在或出现循环引用时，`TaskServiceHostBuilder::build` 会返回包含 alias 和目标名称的启动错误，不会静默忽略

### 5. 通过服务工厂收口宿主调用入口

WebAPI 运行入口通过源码生成的工厂方法统一创建应用服务，避免在 handler 中重复拼装上下文。共享的业务装配位于 [presentation/webapi_support/src/lib.rs](presentation/webapi_support/src/lib.rs)，公共宿主不创建具体业务服务。契约位于 [application/application_interface/src/services](application/application_interface/src/services)，通用和 LLM 实现分别在所属 crate 的 `contracts` 模块绑定；请求处理器只获得契约对象。

```rust
generate_direct_service_factory! {
    impl_generics = <'a>;
    factory = WebApiApplicationAdapter<'a>;

    (self.primary_app_context()) {
        task_center: TaskCenterService<'a>
            => dyn application_interface::services::task_center::TaskCenterService + 'a
    }
}
```

这样宿主调用时就可以统一写成：

```rust
use webapi_support::WebApiApplicationExt;
let service = app_state.services().task_center()?;
let result = service.get_task_setting_list(params).await?;
```

对应的真实 handler 可参考 [presentation/admin_webapi/src/handlers/task/mod.rs#L21](presentation/admin_webapi/src/handlers/task/mod.rs#L21)。

`=> dyn 契约 + 生命周期` 声明让工厂返回 `Box<dyn 契约>`；省略该部分则返回具体类型，供装配和应用内部使用。两个服务工厂宏均支持这一写法。直接工厂还可声明 `implements = 契约类型;` 生成 trait 实现；WebAPI 通过 `webapi_support::services` 的独立能力键注册构造函数，后台消息任务仍使用独立 `TaskServiceFactory`。任务配置管理和队列查询重试可分别依赖 `TaskSettingService`、`QueueTaskManageService`，WebAPI 工厂用 `TaskCenterService` 组合两者，不暴露事务入队方法。

服务借用共享状态的生命周期，不借用临时适配器。宿主档案的 `application` 字段显式选择 `webapi_support::application`；`build_resources` 字段选择资源与逐项业务能力注册，每个启用 Feature 在启动时验证所需能力，公共宿主只负责调用资源工厂、运行健康检查管线、中间件和接口调用。令牌签发、续签缓存与令牌记录维护属于 `AuthorizeService`，权限路由同步的具体实现属于业务适配器。LLM 宿主适配器同样返回接口，流式契约保留 `Send` 流及借用生命周期。

操作者身份模型位于 `application_model::shared::current_user`，由入口从认证信息转换后显式传入用例；LLM 请求、响应和分片位于 `application_model::llm::invocation`。模型层可以被独立使用，不需要依赖具体服务。

LLM 模型与应用配置管理使用业务写库，对话列表及其关联应用、创建人信息使用 `database.read_connection` 对应的只读连接。创建模型和应用返回 JSON 字符串 ID；编辑与删除接口通过查询参数 `id` 指定目标。编辑 DTO 省略 `isEnable` 时默认为 `true`，模型省略 `protocolType` 时默认为 `0`；编辑时接口密钥为空则保留原值。

LLM 调用通过 `LlmInvokeService`，按应用 Code 渲染模板后选择配置的协议。应用层响应与分片保留 `id`、`model`、全部 `choices`（序号、消息或增量、结束原因）以及可空 Token 用量；角色包括 System、User、Assistant、Tool。温度、输出上限等参数放在 `extra_body`，与协议基础字段重名会报错。管理端试用调用向支持的协议传递操作者 `user`，不保存会话；正式调用使用传入的操作者记录会话。

正式调用接收 `LlmParameters`：默认或从 HashMap 转换时使用精确键，也可通过 `LlmParameterComparison::OrdinalIgnoreCase` 选择 Unicode 序号大小写无关比较。管理端试用采用后者，转换已有字典时拒绝比较后重名的参数。参数值允许 null 和空字符串，必填占位符按首次出现的顺序报告缺失项。模板原始空白会保留。流式调用在消费时才读取应用配置，只聚合第一个候选；完整消费且收到非空结束原因后才保存会话，结束标记本身不代表成功。流式请求的 120 秒期限仅覆盖响应头，正文随流的消费与释放结束。


业务调用需要主动取消或控制总时限时，使用应用契约的 `chat_with_cancellation`、`chat_content_with_cancellation` 或 `chat_stream_with_cancellation`，传入 `common::cancellation::CancellationToken`。令牌包含配置读取、模型调用及会话保存的等待过程；流式取消返回独立取消错误，不作为正常流结束。只需要默认行为时继续使用不带令牌的方法。

HTTP 入口可以提取 `webapi_core::request_cancellation::RequestCancellation` 并显式传给服务。共享中间件在请求 future 中断、响应体出错或未完成即释放时发出取消，正常响应结束不触发取消；令牌按请求独立创建，不能拿一个连接的令牌共享给多个 HTTP/2 请求。当前管理端 LLM 试用、客户端流式 LLM 及健康检查入口已接入。两个共享 WebAPI 宿主跟踪请求处理任务，断连先发送令牌并允许处理完成异步清理；宿主排空 HTTP 服务后，在同一停机截止时间内等待残留处理，超时或异常退出才强制取消。仅单独构建 Router、没有共享宿主管理时，仍在原请求 future 内执行。非 HTTP 调用可持有令牌副本并调用 `cancel()`，或者通过定时任务取消副本；请求和响应类型不携带 HTTP 上下文。

### 客户端演示

客户端演示接口始终注册在 `/api/Demo/*`。`Add` 与 `Add2` 通过服务方法缓存 120 秒，后者绑定完整 `ArticleDto` 并将文章字段纳入参数键。`ARef` 展示同步日志与引用参数；`StreamNumbers` 与 `StreamNumbers2` 返回逐项序列化的 JSON 数组。其他接口也可使用 `webapi_core::json_stream::JsonStream` 返回异步序列，响应不会进入对象缓存或 ETag。
