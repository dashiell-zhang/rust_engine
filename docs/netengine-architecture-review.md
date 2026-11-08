# NetEngine 与 rust_engine：框架架构差异清单

审查日期：2026-11-08。本文保存架构审查基线及 AR-01～AR-04 的实施结果，供上下文恢复使用。四项架构工作已完成，不代表全部业务功能均经过运行等价验证。

## 1. 当前方向与基线

- 用户要求优先检查两边的框架架构设计差异，暂不推进下一批业务接口验收；排除 `Presentation/Admin.App` Blazor 项目。
- 最初为只读审查；用户随后明确授权目标模式连续完成四项架构工作，不逐阶段等待确认。实际实现和容器验证见下文，未提交、推送或修改 NetEngine。
- Rust：`D:/ZWork/rust_engine`，HEAD 为 `acd147dccbe26dbb766b1479897c5e5cb1806e97`。审查时已有 B7/B8 文档改动，不应覆盖或当作本轮实现。
- NetEngine：`D:/ZWork/NetEngine`，HEAD 为 `23a7d0002938d0308c975dc48c04ac66aae0dda8`，加六个已授权的未提交文件。具体文件与来源见[阶段记录](netengine-next-stage.md)的固定基线及 B8 补充。
- 后续以当前本地源码为比较对象；基线变化时只重新核实受影响条目，不自动拉取远端或从零重做整个项目审查。

审查时判断：主要分层已基本对应，差异重点为可组合宿主、契约依赖和按业务模块扩展的方式。下列差异描述保留审查时的依据；当前实现以第 5～7 节为准。

## 2. 已基本对应，继续保留

| 维度 | 当前结论 |
| --- | --- |
| 分层职责 | 两边均有 Presentation、Application、Repository、Infrastructure、ProjectCore，以及仓储工具、部署和源码生成组件 |
| 公共宿主边界 | Rust 公共宿主已不直接依赖具体业务应用服务或业务仓库；业务适配器位于 presentation |
| 持久化组件归属 | 通用仓储机制、队列、日志和事件分别归属独立基础设施组件 |
| 提供商替换 | 缓存、锁、文件等组件已有契约和实现分离，由宿主选择提供商 |
| WebAPI 业务调用 | 主要请求处理器通过服务工厂获得业务契约，方法参数与结果基本保持模型或通用类型 |

以上为本轮源码结构判断，不代表逐个接口、提供商或生命周期行为均已运行验证。

## 3. 差异总表

条目编号固定保留，后续直接更新状态和证据，不因拆分实施批次而重新编号。

| 编号 | 差异 | 性质与优先级 | 当前状态 |
| --- | --- | --- | --- |
| AR-01 | 应用接口层同时包含业务契约与运行时实现 | 依赖边界；优先处理 | 已完成；容器验证通过 |
| AR-02 | 公共 WebAPI 宿主固定装配具体数据库与日志查询 | 宿主可组合性；后续处理 | 已完成；容器验证通过 |
| AR-03 | 业务服务替换与契约调用未贯穿所有入口 | 服务装配；后续处理 | 已完成；容器验证通过 |
| AR-04 | 多业务模块缺少同等明确的数据库绑定机制 | 框架扩展能力；最后评估 | 已完成；容器验证通过 |

### AR-01：应用契约层混入运行时实现

**双方依据**

- NetEngine：[Application.Interface.csproj](D:/ZWork/NetEngine/Application/Application.Interface/Application.Interface.csproj) 的显式业务项目依赖主要指向 Application.Model。注意 [Directory.Build.props](D:/ZWork/NetEngine/Directory.Build.props) 还隐式引入源码生成器及其运行时，不能宣称源接口项目完全没有间接基础设施依赖。
- Rust：[application_interface/Cargo.toml](../application/application_interface/Cargo.toml) 直接依赖 SeaORM、Reqwest、jsonwebtoken 等；原 `application_interface/src/context.rs`（现为 [runtime/context.rs](../application/application_runtime/src/context.rs)） 包含数据库及宿主资源上下文；原 HTTP 工厂（现为 [runtime/http_client.rs](../application/application_runtime/src/http_client.rs)） 和 原 JWT 实现（现为 [runtime/auth.rs](../application/application_runtime/src/auth.rs)） 分别包含 HTTP 工厂、JWT 编解码实现。
- 关联观察：[application_model/Cargo.toml](../application/application_model/Cargo.toml) 依赖较宽的 common。应核实模型实际使用的能力，而不是直接拆分整个 common。

**影响**：业务方法签名基本干净，但引用业务契约仍引入较重的运行时依赖。这里指出的是 crate 职责边界，不是声称所有服务方法暴露了数据库。

**建议**：区分业务契约与模型、运行时上下文、具体实现，按职责迁移依赖与装配。优先复用合适的现有 crate；确需新 crate 时先说明依赖方向，不能为目录形式一致机械拆分。

**完成标准**：业务契约层不再承载具体 HTTP/JWT 实现及数据库执行上下文；调用方只引用契约时不被迫直接依赖这些实现。接口行为、宿主共享客户端与 ID 生成器等生命周期约定保持不变，容器检查通过。

### AR-02：公共 WebAPI 宿主固定装配数据库与日志查询

**双方依据**

- NetEngine：[Admin.WebAPI/Program.cs](D:/ZWork/NetEngine/Presentation/Admin.WebAPI/Program.cs) 和 [Client.WebAPI/Program.cs](D:/ZWork/NetEngine/Presentation/Client.WebAPI/Program.cs) 注册具体数据库健康检查；[数据库模块说明](D:/ZWork/NetEngine/Docs/DatabaseModules.md) 明确公共 WebAPI.Core 提供管线，不默认注册业务数据库。
- Rust：[AppState](../project_core/webapi_core/src/host/runtime/state.rs) 固定包含业务读写连接、日志连接及具体 `LogQueryService`；[builder.rs](../project_core/webapi_core/src/host/runtime/builder.rs) 负责创建这些资源。

**影响**：现有宿主可用，但新增不使用这些数据库的 API 宿主、替换数据模块时，需要处理公共状态中的固定假设。

**建议**：具体数据库与查询服务由 presentation 选择装配；公共宿主负责资源生命周期、健康检查管线、请求处理和停机管理。具体状态扩展机制待实施时确定，不预设完整 DI 容器或任意类型注册表。

**完成标准**：公共宿主不强制绑定具体业务/日志模型；现有管理端和客户端仍显式注册所需健康检查，日志读写隔离及停机预算不退化。不能把“可选装配”误改成关闭现有健康检查要求。

### AR-03：服务契约和实现选择未贯穿所有入口

**双方依据**

- NetEngine：[MessageTask.cs](D:/ZWork/NetEngine/Presentation/TaskService/Tasks/MessageTask.cs) 通过 `IMessageService`、`ISmsService` 接收业务能力。
- Rust：[tasks.rs](../presentation/task_service/src/tasks.rs) 中的任务处理器直接创建 `MessageService`、`SendSmsService`。
- Rust WebAPI 的 [webapi_support](../presentation/webapi_support/src/lib.rs) 已通过宏返回业务契约，但 `services()` 返回固定适配器，具体服务绑定集中在该适配器中；[WebApiApplication](../project_core/webapi_core/src/host/application.rs) 当前主要委托续签、权限和路由同步，并非所有业务服务的替换入口。

**影响**：已有接口抽象，但按宿主替换单项业务实现还不如源项目灵活；任务处理器仍知道具体应用服务的构造方式。不是声称 Rust 无法替换实现。

**建议**：处理器依赖业务契约或工厂，具体实现由宿主装配决定；继续使用显式工厂和现有过程宏，不照搬 .NET DI。

**完成标准**：任务处理器不再自行选择具体应用服务；明确 WebAPI 每个宿主选择业务实现的边界，保持每次任务创建独立处理器、资源关闭和停机语义。

**约定提醒**：当前 README 允许任务和应用内部使用返回具体类型的工厂。这是相对 NetEngine 的架构选择差异，不是已有代码违反本地规则；若调整，应同步相关说明。

### AR-04：多业务模块的数据库绑定方式尚未同等完善

**双方依据**

- NetEngine：[PostgreSqlServiceCollectionExtensions.cs](D:/ZWork/NetEngine/Infrastructure/Repository.Core/Extensions/PostgreSqlServiceCollectionExtensions.cs) 提供 `AddPostgreSqlContext<TContext>`；[QueueTaskServiceCollectionExtensions.cs](D:/ZWork/NetEngine/Infrastructure/QueueTask.Database/QueueTaskServiceCollectionExtensions.cs) 提供 `AddQueueTaskDatabase<TContext>`，按具体上下文类型绑定目标。
- 源项目[模块约定](D:/ZWork/NetEngine/Docs/DatabaseModules.md) 规定模块拥有独立读写上下文，同作用域业务与入队服务共享上下文；不保证跨库原子提交。
- Rust 标准 [AppState](../project_core/webapi_core/src/host/runtime/state.rs) 围绕一套业务读写库组织，[AppContext（已迁入 runtime）](../application/application_runtime/src/context.rs) 使用通用数据库连接引用；[QueueTaskProducer](../infrastructure/queue_task_database/src/producer.rs) 支持接收连接及事务，但标准装配尚无同等明确的多模块绑定路径。

**影响**：组件可以使用多个连接，但第二个业务模块需要更多自定义装配。不能把这一点写成“不支持多数据库”或现有单库功能缺陷。

**建议**：前三项边界稳定后，再设计模块、连接、队列生产者与事务的绑定方式；先定义实际模块接入场景，避免直接引入庞大通用框架。

**完成标准**：不同模块目标明确且不受注册顺序影响；同模块业务写入与事务入队可以共用事务；模块间不泄漏实体、连接或事务，不暗示跨库原子性。实施范围需另行限定，不能自动扩展为迁移所有现有业务模块。

## 4. 合理差异与非目标

- Cargo crate 不必逐个机械对应 csproj；`webapi_support` 作为共享业务适配层是合理的 Rust 组织方式。
- 显式工厂、trait 与过程宏可以替代 .NET DI、反射和 Roslyn，不以语言机制相同为完成条件。
- SeaORM 显式事务不必模拟 EF 变更跟踪；需要明确事务所有权，而不是假定多个独立服务调用天然处于同一事务。
- 两边应用层都可以使用仓储；本轮不建议额外引入整套 Domain 层、逐实体 IRepository 或其他全面改造。
- 不把本清单升级为全项目无差异证明，也不重做已完成的 B1～B8。其他功能差异继续由原阶段清单管理。

## 5. 已完成的实施清单

- [x] AR-01：新增 [application_runtime](../application/application_runtime/src/lib.rs)，迁移上下文、HTTP 工厂和 JWT；更新所有调用方。接口层没有上述实现的兼容回导，避免依赖重新绕回。common 增加默认开启的 runtime 特性，DTO/接口/事件契约关闭默认特性；数据枚举、属性映射与 Excel 操作分离，保留同一套 JSON 绑定规则。
- [x] AR-03：新增 `ServiceFactory<'a>` 与 `TaskServiceFactory` 契约；扩展现有服务工厂宏的 `implements` 声明。WebAPI、LLM、鉴权及权限路由同步经过宿主注册的工厂；后台消息任务只取得业务契约，具体实现位于 [service_factory.rs](../presentation/task_service/src/service_factory.rs)。
- [x] AR-02：公共 WebAPI 移除 logger_database/llm 直接依赖以及固定数据库字段，改用 [WebApiResources](../project_core/webapi_core/src/host/resources.rs)。业务连接、日志查询与具体检查由 [presentation 资源工厂](../presentation/webapi_support/src/resources.rs) 创建；两个 WebAPI 档案已接入，业务策略在启动时校验必要资源。缓存、事件和健康检查取消仍由公共宿主管理。
- [x] AR-04：新增 [模块数据库与事务](../infrastructure/repository_core/src/module.rs)；现有 WebAPI、TaskService 使用 repository::BusinessModule。类型/名称重复注册拒绝覆盖，额外模块有明确注册入口。TaskService 按模块选择队列目标，任务服务及生产者复用相同连接。事务入队要求 ModuleTransaction，校验所有者后才写入，完成/父子链/回调仍使用存储原有单一事务。
- [x] 配置与工具：运行时支持 `database.modules.<名称>`；工具 `--module` 从固定文件选择命名连接并校验 `model` 与 `--target`。未改历史表、实体 schema、迁移集合或真实数据；配置示例均为注释占位，不新增真实凭据。
- [x] 文档：更新 README、AGENTS.md、任务/事件/仓储工具说明；新增 [业务模块接入说明](database-modules.md)。

四项已达到本清单的架构完成标准，本目标在此停止，不自动进入下一批业务功能验收。

## 6. 验证与复核依据

全部 Rust 操作在现有 Docker Compose `dev` 容器完成，产物仍在 `/cargo-target`，未安装本机工具。最终生产代码通过：

| 检查 | 结果 |
| --- | --- |
| `cargo check --workspace --locked` | 通过 |
| `cargo build --workspace --locked` | 通过，包含三个运行时宿主及仓储工具 |
| `cargo clippy --workspace --lib --bins --locked -- -D warnings` | 通过 |
| `cargo fmt --all -- --check` | 通过 |
| `cargo clippy -p application_interface --lib --locked -- -D warnings` | 通过，独立验证轻量特性组合，不依赖整个 workspace 的特性合并 |
| 接口 crate 的 `cargo tree` | 不包含 application_runtime、SeaORM、Reqwest、jsonwebtoken、Skia、image、calamine、rust_xlsxwriter |
| Cargo.lock 比较 | 外部包版本和校验值均未改变，仅调整本地 crate 依赖 |
| HTTP/JWT 迁移内容比较 | auth.rs、http_client.rs、named.rs、redirect.rs 与迁移前内容一致 |
| 仓储工具 `--help` | 已显示 `--module`，未连接数据库 |
| 文档与差异检查 | 本轮文档本地链接及 `git diff --check` 通过 |

除编译外，已按源代码复核：

- NetEngine 的按上下文注册数据库/队列方式，对应 Rust 的类型模块注册与同连接事务入队；不再依赖一个可被覆盖的默认注册。
- NetEngine 任务通过 IMessageService/ISmsService 获取能力，对应 Rust 任务契约工厂；没有在任务注册表共享执行对象。
- WebAPI 业务实现选择覆盖权限路由同步，避免自定义服务工厂被默认适配器绕过。
- 模块克隆共享连接；生产者不会提交调用方事务。内部队列完成事务仍包含父链、回调及 xmin 回读，未修改行锁/租约/调度算法。
- 业务只读连接仍在 PostgreSQL 会话设置只读。日志查询保持独立连接，文件日志模式的日志写库健康探测保持惰性连接；只有管理端探测日志读库。
- 资源健康检查并行执行并传递原取消令牌，宿主停机预算和日志排空逻辑未改变。

## 7. 兼容性与验证边界

- HTTP API、既有业务规则和默认连接选择保持原样；Rust 源码接入发生变化：上下文/HTTP/JWT 导入转向 application_runtime，WebAPI 档案新增资源工厂，TaskService 档案新增数据库模块工厂，公共事务入队参数改为 ModuleTransaction。
- 额外模块需在 presentation 显式注册类型和连接。配置条目不自动生成业务模型。仓储工具命名连接只选择已登记模型的另一个实例；新增实体模块仍需登记自己的 schema、迁移集合及历史表，不能借用错误模型迁移。
- 一个 TaskService Worker 选择一个模块的队列；多库 Worker 分别装配宿主实例。没有增加跨库事务、自动模块发现或完整 .NET DI 容器。
- 保留 Rust 的显式工厂、trait、过程宏和事务方式；不复制 EF 变更跟踪，也不新增 Domain 层。
- 本轮未进行真实数据库迁移、多数据库运行联调、外部提供商调用或全量业务回归；结构与调用链审查、编译和静态检查不等于这些运行验证。
- 没有添加或运行测试，没有固定工具链/镜像版本，没有提交、推送、部署或修改 NetEngine；此前授权的源项目六处改动保留。

恢复上下文时先读本节和完成清单，再看当前 git diff。不要把第 3 节保留的历史问题描述当作仍未实施的待办，也不要重新执行 B1～B8。
