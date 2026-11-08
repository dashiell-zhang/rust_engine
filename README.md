# rust_engine

rust_engine 是一个 Rust 后端框架，提供管理端 API、客户端 API、后台任务和数据库迁移工具。它将业务服务与数据库、缓存、文件、消息等组件分开，适合在统一基础设施上扩展业务模块。架构设计参考 [NetEngine](https://github.com/dashiell-zhang/NetEngine)，本仓库不包含前端应用。

## 能解决什么问题

| 能力 | 使用场景 |
| --- | --- |
| WebAPI | 注册接口、生成 OpenAPI、登录鉴权、权限控制、签名和参数验证 |
| 数据持久化 | PostgreSQL 读写分离、软删除、并发冲突保护和数据库迁移 |
| 后台任务 | 事务入队、失败重试、父子任务、定时执行 |
| 基础设施 | 替换缓存、锁、云存储、邮件、短信、日志和 LLM 提供商 |
| 部署工具 | 生成 Nginx、systemd 和云效配置，构建 Linux 发布包 |

## 快速开始

所有 Rust 命令在本机 Docker 的 `dev` 容器内执行，无需在电脑上安装 Rust、Cargo 或其他构建组件。工具链使用 `stable`，Docker 外部基础镜像使用 `latest`。

### 1. 启动开发容器

在项目根目录执行：

```powershell
docker compose up -d --build dev
docker compose exec -T dev cargo check --workspace --locked
```

源码挂载到容器的 `/workspace`，下载缓存与编译产物保存在 Docker 命名卷，编译目录为 `/cargo-target`。Compose 仅启动开发容器，PostgreSQL 和需要使用的 Redis 应另行准备。

### 2. 配置连接与组件

编辑需要运行的宿主配置：

- [管理端配置](presentation/admin_webapi/appsettings.toml)
- [客户端配置](presentation/client_webapi/appsettings.toml)
- [任务服务配置](presentation/task_service/appsettings.toml)

先确认数据库、缓存、锁、ID 生成器、事件后端和 JWT 配置，再按需设置文件、短信和邮件组件。容器访问电脑上的服务时，Docker Desktop 可使用 `host.docker.internal`；容器内的 `127.0.0.1` 指向容器自身。

自动分配 ID 节点需要 Redis，即使业务缓存使用内存实现也一样。单实例可显式选择[固定节点编号](docs/identifier-generator.md#固定编号)。多个实例共同处理任务时，应共享缓存和锁的连接及前缀。

配置覆盖方式和示例见[组件配置](docs/component-configuration.md)。真实凭据放在运行环境或外部配置中。

### 3. 初始化数据库

编辑 [数据库工具配置](repository_tool/appsettings.toml)，分别填写业务库、日志库和需要使用的事件库连接，再执行：

```powershell
docker compose exec -T dev cargo run --locked -p repository_tool -- migrate --target business
docker compose exec -T dev cargo run --locked -p repository_tool -- migrate --target log
# 仅使用数据库事件后端时需要
docker compose exec -T dev cargo run --locked -p repository_tool -- migrate --target event-bus
```

即使连接同一物理数据库，也要分别迁移所需模型。宿主启动不自动迁移。SQL 预览、增量迁移及回退操作见[数据库工具](docs/repository-tool.md)。

### 4. 运行服务

在不同终端按需启动：

```powershell
docker compose exec -T dev cargo run --locked -p admin_webapi
docker compose exec -T dev cargo run --locked -p client_webapi
docker compose exec dev cargo run --locked -p task_service
```

任务服务的 Debug 模式提供交互菜单，在当前进程选择要启用的任务；Release 模式从数据库读取启停配置。详细说明见[任务服务](docs/task-service.md)。

开发容器默认使用 Development 环境：

| 服务 | 入口 |
| --- | --- |
| 管理端文档 | `http://127.0.0.1:3000/swagger` |
| 客户端文档 | `http://127.0.0.1:3001/swagger` |
| OpenAPI | 各 API 的 `/swagger/v1/swagger.json` |
| 健康检查 | 各 API 的 `/healthz` |

Swagger 仅在 Development 开放。任务服务不提供 HTTP 入口。首次配置管理员请阅读[认证与权限初始化](docs/authorization-initialization.md)。

### 5. 编译与更新工具链

```powershell
docker compose exec -T dev cargo build --workspace --locked
docker compose exec -T dev cargo clippy --workspace --lib --bins --locked -- -D warnings
docker compose exec -T dev cargo fmt --all
```

更新开发镜像和稳定版工具链：

```powershell
docker compose build --pull --no-cache dev
docker compose up -d dev
docker compose exec -T dev cargo check --workspace --locked
docker compose exec -T dev cargo build --workspace --locked
```

也可使用 [VS Code 任务](.vscode/tasks.json)中以 `container` 开头的任务。

## 目录与扩展入口

| 目录 | 放什么 |
| --- | --- |
| `presentation/*` | API 与任务入口、处理器、功能注册、具体组件选择 |
| `project_core/*` | 共享 WebAPI 和任务宿主 |
| `application/application_model` | DTO 和业务模型 |
| `application/application_interface` | 跨宿主业务服务契约 |
| `application/application_runtime` | 应用服务运行上下文 |
| `application/application_service*` | 业务用例与编排 |
| `repository` | 业务实体、关系与查询 |
| `infrastructure/*` | 可独立复用的组件及提供商实现 |
| `repository_tool` | 数据库模型与迁移 CLI |
| `source_generator/*` | 过程宏与运行时辅助 |
| `deployment` | 部署模板、生成器与发布脚本 |

新增业务通常先定义 DTO 和服务契约，再实现应用服务，在宿主中注册能力，最后添加 HTTP 端点或任务处理器。接入步骤见[业务模块与服务装配](docs/database-modules.md)和[宿主扩展](docs/host-extensions.md)。

## 使用文档

| 主题 | 文档 |
| --- | --- |
| 配置与部署 | [组件配置](docs/component-configuration.md)、[部署与发布](docs/deployment.md) |
| 接口开发 | [宿主扩展](docs/host-extensions.md)、[请求绑定](docs/request-binding.md)、[调用行为宏](docs/invocation-macros.md) |
| 登录与网络 | [认证和权限](docs/authorization-initialization.md)、[请求签名](docs/webapi-signing.md)、[RSA 解密](docs/webapi-rsa.md)、[代理与 HTTPS](docs/webapi-network.md) |
| 基础与文件接口 | [基础接口](docs/base-api.md)、[文件上传下载](docs/webapi-files.md)、[静态文件](docs/webapi-static-files.md)、[健康检查](docs/webapi-health.md) |
| 数据库 | [模块装配](docs/database-modules.md)、[迁移工具](docs/repository-tool.md)、[软删除](docs/soft-delete.md)、[并发保存](docs/optimistic-concurrency.md)、[时间分区](docs/postgresql-partitions.md) |
| 后台与集成 | [任务服务](docs/task-service.md)、[事件广播](docs/event-bus.md)、[HTTP 客户端](docs/http-client.md)、[LLM 调用](docs/llm.md)、[支付接口](docs/payments.md) |
| 日志与协调 | [数据库日志](docs/database-logger.md)、[文件日志](docs/local-file-logger.md)、[ID 生成器](docs/identifier-generator.md)、[分布式锁](infrastructure/distributed_lock/README.md) |
| 公共工具 | [数据与 Excel](docs/common-data.md)、[字符串工具](docs/common-string.md) |
