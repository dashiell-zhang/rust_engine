# AGENTS.md

## 工作区规则

- 不要编写测试代码。
- 不要添加 `#[cfg(test)]` 模块、`tests/` 目录或测试辅助代码。
- 不要添加仅用于测试的依赖或 `dev-dependencies`。
- 如果某项改动通常需要包含测试，除非用户明确要求，否则请跳过测试相关内容。

## 快速链接

- 架构与分层职责：[README.md](README.md)
- 工作区 crate 列表：[Cargo.toml](Cargo.toml)
- 可复用的构建与运行任务：[.vscode/tasks.json](.vscode/tasks.json)
- 数据库工具入口：[repository_tool/src/main.rs](repository_tool/src/main.rs)
- WebAPI 共享宿主入口：[project_core/webapi_core/src/lib.rs](project_core/webapi_core/src/lib.rs)
- TaskService 共享宿主入口：[project_core/task_service_core/src/lib.rs](project_core/task_service_core/src/lib.rs)
- 软删除查询扩展：[repository/src/extensions.rs](repository/src/extensions.rs)
- 过程宏实现：[source_generator/source_generator_core/src/lib.rs](source_generator/source_generator_core/src/lib.rs)

## 工作区结构

- 这是一个 Cargo workspace，包含三个运行时宿主：`admin_webapi`、`client_webapi` 和 `task_service`。
- 共享的宿主装配代码位于 `project_core/webapi_core` 和 `project_core/task_service_core`。
- 应用逻辑归属 `application/*`；持久化归属 `repository`；可插拔适配器归属 `infrastructure/*`。
- `repository_tool` 是数据库迁移 CLI；不要把 schema 逻辑移动到 presentation 层的 crate 中。
- `source_generator/source_generator_core` 和 `source_generator/source_generator_runtime` 提供跨服务使用的过程宏与运行时辅助工具。

## 变更策略

- 仅关注生产代码、构建配置和运行时行为。
- 优先删除或避免测试相关代码，而不是引入新的测试脚手架。
- 变更应保持在现有分层边界内；优先修复所属 crate，而不是在宿主 crate 周围打补丁。
- 保留现有中文注释和命名风格，除非周边文件明显采用了不同约定。
- 优先采用最小化、局部化变更。除非任务明确要求，否则不要重塑模块或 crate 边界。

## 构建与验证

- 构建或检查工作区时，优先使用 [.vscode/tasks.json](.vscode/tasks.json) 中定义的 VS Code 任务。
- 常用检查命令：
	- `cargo check --workspace`
	- `cargo build -p admin_webapi`
	- `cargo build -p client_webapi`
	- `cargo build -p task_service`
	- `cargo clippy --workspace --all-targets -- -D warnings`
	- `cargo fmt --all`
- 即使工作区中定义了 `cargo test --workspace` 任务，除非用户明确要求，否则不要运行测试，也不要添加测试。

## 运行时入口

- 管理端 API 宿主：[presentation/admin_webapi/src/main.rs](presentation/admin_webapi/src/main.rs)
- 客户端 API 宿主：[presentation/client_webapi/src/main.rs](presentation/client_webapi/src/main.rs)
- 任务服务宿主：[presentation/task_service/src/main.rs](presentation/task_service/src/main.rs)
- 默认配置文件：
	- `presentation/admin_webapi/appsettings.toml`
	- `presentation/client_webapi/appsettings.toml`
	- `presentation/task_service/appsettings.toml`
- `APP_CONFIG_PATH` 可覆盖默认配置路径，传入值应为不带 `.toml` 后缀的路径。

## 仓储工具

- `repository_tool` 是数据库迁移的标准入口。
- 常用命令：
	- `cargo run -p repository_tool -- migrate`

## 分层边界

- HTTP 相关关注点、路由注册和宿主装配应放在 `presentation/*` 与 `project_core/*`。
- 对外 HTTP 调用应复用 `application_interface::http_client::HttpClientFactory` 这类宿主级工厂，由 `project_core/*` 或运行时宿主统一创建并通过上下文注入；不要在应用服务或基础设施调用点长期持有自行创建的 `reqwest::Client`。
- 业务流程与编排应放在 `application_service*` 系列 crate 中。
- 跨宿主契约应放在 `application_interface`，DTO 和配置模型应放在 `application_model`。
- 数据库实体、关系与查询辅助代码应放在 `repository`。
- 与具体提供商相关的集成应放在对应的 `infrastructure/*` crate 中，不要在应用服务内部通过分支处理。

## 改动落点

- 新增或调整管理端/客户端 HTTP 接口时，优先改 `presentation/admin_webapi/src/features` 或 `presentation/client_webapi/src/features`，并检查对应宿主 `src/lib.rs` 的 profile/feature 注册。
- 共享的 WebAPI 中间件、鉴权、签名校验、限流、OpenAPI 装配，优先改 `project_core/webapi_core/src`，不要在单个宿主重复实现。
- 后台任务注册、轮询和宿主级配置装配，优先改 `project_core/task_service_core/src` 与 `presentation/task_service/src/lib.rs`。
- 业务规则、应用编排和用例流程放在 `application/application_service*`；如果只是换基础设施实现，不要把分支逻辑塞回应用服务。
- SeaORM 实体、关系、查询扩展和软删除行为放在 `repository/src`；数据库迁移定义放在 `repository_tool/src/migrator`。
- 横切能力默认行为或宏展开逻辑，优先改 `source_generator/source_generator_core`，必要时同步修改 `source_generator/source_generator_runtime`。

## 配置与宿主约定

- 三个运行时宿主的默认配置路径定义在各自 `presentation/*/src/lib.rs`，环境变量 `APP_CONFIG_PATH` 会覆盖默认值，传入路径不带 `.toml` 后缀。
- 运行或排查配置问题时，优先从宿主 `appsettings.toml` 和 `project_core/*/settings` 的加载路径开始，不要先怀疑业务层。
- `appsettings.toml` 中的 `component` 字段用于选择基础设施实现；优先通过配置和宿主注册切换实现，而不是在应用层写条件分支。
- `repository_tool` 支持 `APP_CONFIG_PATH` 和 `--config-path`，数据库迁移入口固定为 `cargo run -p repository_tool -- migrate`。

## 持久化与迁移

- 对带 `DeleteTime` 的实体，查询活跃数据时优先使用 `SoftDeleteEntityExt::find_active()` 或 `find_by_id_active()`，不要直接从 `Entity::find()` 开始再忘记补过滤。
- 查询可空父节点或顶层树结构时，显式使用 `is_null()`，不要依赖 `Column.eq(None)` 去表达 `NULL` 语义。
- 新增迁移时，除了补充 `repository_tool/src/schema` 里的语句构建，还要把迁移注册到 `repository_tool/src/migrator/mod.rs` 的 `all_migrations()`。
- 当前迁移历史表固定为 `__MigrationHistory`；只改 schema 语句而不登记版本，会导致环境之间行为不一致。
- `llm_app` 当前 schema 与 .NET 对齐时不包含 `ApiKey`、`BaseUrl`，备注列名是 `Remark`；改实体字段前先核对库表。

## 鉴权与请求签名

- JWT 与请求签名相关共享能力位于 `project_core/webapi_core`；涉及 Token 校验、续签或签名验证时，优先在共享宿主层定位实现。
- 请求签名中间件在 `debug_assertions` 下会直接放行；不要用 Debug 运行结果判断生产环境签名流程是否生效。
- 生产签名校验依赖 `Token`、`Time`、`Authorization: Bearer <jwt>` 请求头，并组合请求路径与请求体参与摘要计算。
- `jsonwebtoken` 10.x 在本仓库需要启用 `rust_crypto` 特性；如果调整 JWT 依赖或相关 crate，先确认不会引入运行时 panic。

## 代码生成与宏

- `#[logging]` 不是默认全量挂载；只在需要的方法上显式 `use source_generator_runtime::logging;` 后再添加属性。
- 现有 `#[logging]`、`#[retry]`、`#[cacheable]`、`#[concurrency_limit]` 仅支持 `async fn`；改方法签名或扩宏时先检查这一前提。
- `#[retry]` 默认最多重试 3 次、每次等待 0 ms；`#[cacheable]` 默认 `ttl_seconds = 60`；`#[concurrency_limit]` 默认使用 `self.lock`、并发数 `1`、非阻塞。
- 横切属性必须配置稳定且全局唯一的 `key_scope`；receiver 方法还必须显式配置 `key_include_self = true` 或 `false`。方法自身泛型会自动加入方法键，外围 `impl` 泛型需要通过 `Self` 或显式键维度覆盖。
- 需要修改横切默认值时，先确认是业务想调单个调用点参数，还是确实要改宏默认行为；优先改属性参数，避免扩大影响面。

## 已知兼容性坑

- 角色/权限功能树递归组装时，向下传递子模块名称不能固定传 `None`，否则深层模块名会保留父级前缀。
- JWT 私钥解析当前同时兼容 PKCS8 DER 和旧的 SEC1 DER，配置值仍是 Base64 文本；处理密钥导入问题时不要先改配置格式。
- 如果某个接口或查询行为需要和 NetEngine 对齐，优先检查空值语义、字段命名和共享宿主层中间件，而不是直接改表面返回值。

## 代码生成与宏

- 在手写重复装配代码之前，先确认 `source_generator_runtime` 中是否已有宏覆盖该模式。
- 现有横切属性包括 `#[logging]`、`#[retry]`、`#[cacheable]` 和 `#[concurrency_limit]`。
- 服务和任务注册可能已通过宏生成；优先扩展宏输入，而不是手工复制展开后的代码。
- 修改宏行为时，同时验证过程宏 crate 和运行时辅助 crate，因为它们是协同演进的。

## 实务建议

- 当某项功能依赖基础设施选型时，优先切换或注册正确的实现 crate，而不是添加宿主特定条件分支。
- 当涉及持久化初始化时，优先检查迁移代码与运行时配置是否匹配。
- WebAPI、TaskService 等宿主如果需要对外 HTTP 能力，应各自在宿主状态中装配共享 HTTP 客户端工厂，并把工厂引用传入需要的应用上下文；这样既能复用连接池，又能按生命周期轮换底层 client，避免 DNS 和长连接长期不刷新的问题。
- 需要更广泛的项目背景时，请回链到 [README.md](README.md)，不要在 agent 指令中重复架构说明。
