# NetEngine 与 rust_engine：框架架构差异清单

审查日期：2026-11-08。本文保存 AR-01～AR-04 的实施结果，以及提交后复核发现的 AR-05～AR-07。七项均已完成；AR-05～AR-07 的实际改动与独立验证记录见第 10～11 节，未复用前四项编译记录代替本轮验证。

## 1. 当前方向与基线

- 用户要求优先检查两边的框架架构设计差异，暂不推进下一批业务接口验收；排除 `Presentation/Admin.App` Blazor 项目。
- 最初为只读审查；用户随后明确授权目标模式连续完成四项架构工作，不逐阶段等待确认。实际实现和容器验证见下文，未提交、推送或修改 NetEngine。
- Rust：`D:/ZWork/rust_engine`，HEAD 为 `acd147dccbe26dbb766b1479897c5e5cb1806e97`。审查时已有 B7/B8 文档改动，不应覆盖或当作本轮实现。
- NetEngine：`D:/ZWork/NetEngine`，HEAD 为 `23a7d0002938d0308c975dc48c04ac66aae0dda8`，加六个已授权的未提交文件。具体文件与来源见[阶段记录](netengine-next-stage.md)的固定基线及 B8 补充。
- 后续以当前本地源码为比较对象；基线变化时只重新核实受影响条目，不自动拉取远端或从零重做整个项目审查。
- 提交后复核基线：Rust 为 `17d276ce03d48d6a7be69a2d65be470827754889`（用户已提交并推送 AR-01～AR-04），复核开始时工作区干净；NetEngine 仍为上述提交及六处未提交修改。AR-05～AR-07 依据本次复核，不追溯改写前四项的完成记录。

审查时判断：主要分层已基本对应，差异重点为可组合宿主、契约依赖和按业务模块扩展的方式。下列差异描述保留审查时的依据；AR-01～AR-04 结果见第 5～7 节，当前 AR-05～AR-07 结果及接入变化见第 10～11 节。

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
| AR-05 | 部分基础设施组件反向依赖应用契约与运行时 | 依赖方向；下一阶段优先 | 已完成；本轮容器验证通过 |
| AR-06 | TaskService 公共核心固定装配业务资源 | 宿主可组合性；随后处理 | 已完成；本轮容器验证通过 |
| AR-07 | WebAPI 服务工厂要求整套业务能力一起实现 | 按需组合；随后处理 | 已完成；本轮容器验证通过 |

### AR-01：应用契约层混入运行时实现

**双方依据**

- NetEngine：[Application.Interface.csproj](D:/ZWork/NetEngine/Application/Application.Interface/Application.Interface.csproj) 的显式业务项目依赖主要指向 Application.Model。注意 [Directory.Build.props](D:/ZWork/NetEngine/Directory.Build.props) 还隐式引入源码生成器及其运行时，不能宣称源接口项目完全没有间接基础设施依赖。
- Rust：[application_interface/Cargo.toml](../application/application_interface/Cargo.toml) 直接依赖 SeaORM、Reqwest、jsonwebtoken 等；原 `application_interface/src/context.rs`（现为 [runtime/context.rs](../application/application_runtime/src/context.rs)） 包含数据库及宿主资源上下文；原 HTTP 工厂（现为 [http_client/lib.rs](../infrastructure/http_client/src/lib.rs)） 和 原 JWT 实现（现为 [runtime/auth.rs](../application/application_runtime/src/auth.rs)） 分别包含 HTTP 工厂、JWT 编解码实现。
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

恢复上下文时先读总表、第 8～9 节及当前 git diff。AR-01～AR-04 的问题描述为历史依据；AR-05～AR-07 的完成记录见第 10～11 节，不要重新执行已完成架构项或 B1～B8。

## 8. 提交后架构复核：AR-05～AR-07

以下保留实施前复核依据。当时仅核对源码、项目引用和装配路径，没有发现足以据此认定现有业务运行故障的证据。以下是组件复用与框架扩展方式的差异，不是推翻前四项实现，也不要求重做全部架构。

### AR-05：基础设施组件仍反向依赖应用层

**依据**：NetEngine 的 [FileStorage.AliCloud.csproj](D:/ZWork/NetEngine/Infrastructure/FileStorage.AliCloud/FileStorage.AliCloud.csproj) 引用基础设施中的 FileStorage 契约，使用通用 IHttpClientFactory。Rust 的 [阿里云存储依赖](../infrastructure/file_storage_ali_cloud/Cargo.toml)、[腾讯云存储依赖](../infrastructure/file_storage_tencent_cloud/Cargo.toml) 同时引用 application_interface 和 application_runtime；[短信组件](../infrastructure/sms/src/lib.rs) 和 [邮件组件](../infrastructure/email/src/lib.rs) 从应用层回导提供商契约。

**影响**：单独使用文件、短信或邮件组件仍会引入业务契约/模型；仅为 HTTP 工厂引用 application_runtime，还会带入数据库执行上下文和 JWT 实现。AR-01 已解决接口层混入实现的问题，本项继续处理基础设施的依赖方向。

**实施范围**：将通用文件、邮件、短信契约及其请求类型归属对应基础设施组件，将共享 HTTP 工厂放到合适的独立公共组件。应用层引用这些契约或保留必要的业务适配；更新提供商、宿主、上下文和文档。不要将业务 DTO 搬入基础设施，也不要把所有 application_runtime 内容机械下移。

**完成标准**：

- [x] 受影响基础设施组件不再直接或间接依赖 application_interface、application_model、application_runtime 等应用 crate。
- [x] HTTP 工厂可独立引用，不携带业务上下文、SeaORM 或 JWT 实现；依赖图和独立 crate 检查提供证据。
- [x] 保持宿主共享客户端、连接池轮换、命名客户端、签名处理器和取消行为，不在提供商内自行创建长期客户端。
- [x] 现有文件、邮件、短信调用均已接入，协议、错误及公开行为不变；兼容回导不得重新形成反向依赖。

### AR-06：TaskService 公共核心仍固定装配业务资源

**依据**：NetEngine 的 [TaskService.Core.csproj](D:/ZWork/NetEngine/ProjectCore/TaskService.Core/TaskService.Core.csproj) 主要引用队列组件；日志等资源由 [TaskService/Program.cs](D:/ZWork/NetEngine/Presentation/TaskService/Program.cs) 选择注册。Rust 的 [公共构建器](../project_core/task_service_core/src/builder.rs) 要求短信、邮件、文件工厂，并直接创建日志数据库连接；[特性上下文](../project_core/task_service_core/src/features.rs) 固定包含这些资源。

**影响**：不使用消息或文件能力的任务宿主仍需处理这些资源的构造入口；通用调度核心对模板资源集合有固定假设。AR-02 的已完成范围是 WebAPI，本项将相同职责原则落实到任务宿主。

**实施范围**：由 presentation 显式选择任务业务资源、日志提供商及其连接，公共核心负责调度、取消、运行生命周期和停机管理。沿用或适度统一已有资源装配方式，避免再构建一套通用 DI 框架。

**完成标准**：

- [x] 公共调度核心不要求所有宿主提供短信、邮件、文件资源，也不固定创建具体日志数据库连接。
- [x] 当前 task_service 的默认资源、消息任务、示例任务和事件订阅全部接通，未使用的能力可不装配。
- [x] 保持每次执行独立创建处理器、资源 close、租约续期和共享停机预算；日志正常退出仍异步排空。
- [x] 保持显式模块选择和同一连接绑定，不修改队列状态机、调度算法或事务规则。

### AR-07：业务工厂的按需组合粒度仍较粗

**依据**：NetEngine 按业务接口注册实现，[管理端入口](D:/ZWork/NetEngine/Presentation/Admin.WebAPI/Program.cs) 显式注册 ILogManageService，[客户端入口](D:/ZWork/NetEngine/Presentation/Client.WebAPI/Program.cs) 不注册该管理服务。Rust 的 [ServiceFactory](../application/application_interface/src/services/mod.rs) 集中授权、文章、支付、日志和 LLM 等全部方法，[webapi_support](../presentation/webapi_support/Cargo.toml) 直接依赖 LLM 实现；共享资源工厂同时装配具体日志查询。

**影响**：实现可被替换，但仅使用少量业务能力的宿主仍需实现或委托完整工厂，修改单项服务会牵涉不相关契约。AR-03 的调用链已贯穿业务契约，本项进一步处理按需组合。

**实施范围**：按稳定业务能力拆分工厂或注册入口，由宿主显式组合所需能力。保留必要的共享装配与现有过程宏，不按每个 HTTP 路由创建接口或任意拆分 crate。

**完成标准**：

- [x] 只接入某项能力时，不需要实现无关工厂方法；增加或替换一项能力不强制修改其他能力契约。
- [x] 管理端和客户端明确选择各自业务能力，日志管理、LLM 等实现按实际需要装配；不能用空实现或运行时 panic 掩盖缺失注册。
- [x] 请求处理器依赖契约，具体实现只在宿主装配选择；鉴权、续签、权限同步及 LLM 不绕过所选实现。
- [x] 缺少已启用能力所需的资源时在启动装配阶段报告；健康检查要求不能因减少查询服务装配而丢失。
- [x] 两个现有 WebAPI 的路由、业务行为及生命周期保持兼容，完整编译并复核受影响调用链。

## 9. 下一目标的范围与停止条件

本节保留用户授权的有限目标；现已完成，结果见第 10～11 节。用户启用目标模式，授权连续实施 AR-05～AR-07。按 **AR-05 → AR-06 → AR-07** 连续推进，阶段之间不等待确认；允许调整必要依赖顺序，全部完成后停止。

- 保留 AR-01～AR-04、B1～B8 的有效成果，只复查受本次改动影响的部分。
- 一个 Worker 选择一个队列数据库与 NetEngine 的 [AddTaskWorker](D:/ZWork/NetEngine/ProjectCore/TaskService.Core/TaskWorkerServiceCollectionExtensions.cs) 一致；不扩展为单 Worker 跨库调度。
- 生产实现、三个宿主的实际接入、依赖审查和文档更新均完成后，才能关闭条目；不能只添加未接入的抽象。
- 全部 Rust 操作仅在 Docker dev 内执行，不安装本机组件，不固定 stable/latest，不添加或运行测试。
- 最终完成 `cargo fmt --all -- --check`、`cargo check --workspace --locked`、`cargo build --workspace --locked`、`cargo clippy --workspace --lib --bins --locked -- -D warnings`；对拆分后组件补充必要的独立编译和依赖图检查，避免 workspace 特性合并掩盖依赖遗漏。
- 持续记录状态、关键决策、验证和剩余限制。新发现的无关问题另记，不自动纳入本目标，不循环重审整个仓库。
- 不提交、推送、部署，不修改 NetEngine，不执行真实数据迁移。只有确实无法继续的外部依赖、破坏性操作或超出授权范围的决定才需要用户介入，其他工作继续推进。

## 10. AR-05～AR-07 完成结果

本轮在 Rust `17d276ce03d48d6a7be69a2d65be470827754889` 上实施，保留两份已有清单文档改动。NetEngine 基线及原六处改动未变，未提交、推送或部署。

- **AR-05 已完成**：file_storage/email/sms 拥有提供商契约及请求类型，应用接口原模块仅兼容导出；提供商直接引用各自基础设施组件。HTTP 工厂迁至 [http_client](../infrastructure/http_client/src/lib.rs)，三个实现文件与迁移前逐字一致。file_storage 的 local 实现为默认可选特性，纯契约调用方关闭它。事件发布资源契约归属 event_bus。基础设施不再直接或间接依赖应用层。
- **AR-06 已完成**：[TaskService 档案](../project_core/task_service_core/src/settings.rs) 使用可选资源工厂，公共核心复用 [common::resources](../infrastructure/common/src/resources.rs)，不再要求消息或文件资源；具体资源与日志连接由 [入口装配](../presentation/task_service/src/host_support.rs) 创建。内置任务注册前验证消息资源与业务模块，服务依然每次执行创建；示例与事件注册继续使用原上下文能力。队列事务、状态机、取消、租约、close 和 LoggerRuntime 停机管线未修改。
- **AR-07 已完成**：[services](../presentation/webapi_support/src/services.rs) 按 ServiceKey 独立注册构造与校验函数，移除全量 ServiceFactory；默认构造继续使用现有过程宏，不增加完整 DI 容器。[管理端](../presentation/admin_webapi/src/business_resources.rs) 与 [客户端](../presentation/client_webapi/src/business_resources.rs) 显式选择各自能力。每个启用 Feature 启动校验依赖，缺失注册在访问层也返回内部错误，不以空实现或 panic 替代。鉴权、续签、权限同步、普通业务与 LLM 全部沿选定契约调用。
- **资源与依赖**：日志查询单独注册；管理端与日志读健康检查共用一个只读连接，客户端不创建日志查询及读连接。业务读写、日志写、缓存与事件健康检查保持原要求。webapi_support 的 llm 实现可选，两个现有宿主确有 LLM 入口，显式开启此特性。
- **文档**：同步 README、AGENTS.md、[业务模块接入](database-modules.md)、[HTTP 工厂](http-client.md) 与 [任务宿主](task-service.md)。第 5～7 节为前次完成记录，其中 HTTP 归属与全量工厂接入已由本轮替代，不应作为当前接入方法。

## 11. 本轮验证与接入边界

全部 Rust 操作在现有 Docker Compose dev 内执行，使用 stable 工具链、latest 镜像约定与 `/cargo-target`，没有安装本机组件。下列结果对应本轮最终生产代码：

| 检查 | 结果 |
| --- | --- |
| `cargo check --workspace --locked` | 通过 |
| `cargo build --workspace --locked` | 通过，包括三个宿主和仓储工具 |
| `cargo clippy --workspace --lib --bins --locked -- -D warnings` | 通过，无警告 |
| `cargo fmt --all -- --check` | 通过 |
| 单独 `cargo check -p <组件> --locked` | http_client、email、sms、file_storage、两个云存储、email_smtp、两个短信提供商、task_service_core、application_interface 均通过 |
| 关闭可选实现的独立检查 | file_storage --no-default-features 与 webapi_support --no-default-features 均通过 |
| 各组件单独 `cargo tree` | 上述提供商/契约和 HTTP 组件无 application_* 依赖；HTTP 无 SeaORM/JWT；任务核心无应用、短信、邮件、文件、HTTP 工厂依赖 |
| 轻量特性依赖图 | application_interface 无 application_runtime/SeaORM/Reqwest/JWT；webapi_support 关闭默认特性时无 application_service_llm |
| 外部依赖与 HTTP 迁移比较 | Cargo.lock 外部包版本与 checksum 集合完全一致；HTTP 工厂 lib.rs/named.rs/redirect.rs 与原实现逐字一致 |
| 文档与差异检查 | 本轮更新文档的本地链接和 `git diff --check` 通过 |

源码接入变化：HTTP 导入改为 `http_client::HttpClientFactory`；TaskService 档案用可选 build_resources 替代三个固定提供商工厂；WebAPI 按能力注册，调用方使用 `services().能力()?`。自定义能力可扩展 ServiceKey，替换某一实现只更换其构造函数和资源校验，不需要实现其他业务方法。

本轮没有变更 HTTP 路由、业务规则、数据库 schema 或默认提供商协议；没有运行真实数据库迁移、多宿主联调或云提供商调用。编译与源码复核不能代替这些运行验证。未添加或运行测试，未新增模拟程序。当前三项完成标准均已闭合，本目标到此结束，不自动进入下一轮功能对齐。
