# NetEngine 与 rust_engine：框架架构差异清单

更新日期：2026-11-08。AR-01～AR-16 已完成并提交，用户确认已推送；AR-15、AR-16 的原实施及验证见第 26～27 节。提交后发现的 repository_core 独立编译问题归为 AR-16 补正，处理与验证见第 28 节，本次补正尚未提交。更新时间规则统一仅为可选观察，用户明确暂不调整；前轮目标与停止记录作为历史保留，不代表当前待办状态。

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
| AR-08 | WebAPI 公共核心仍绑定部分业务资源与应用运行时 | 公共宿主边界；优先处理 | 已完成；本轮容器验证通过 |
| AR-09 | LLM 客户端工厂及模型配置解析不能独立替换 | 组件替换能力；优先处理 | 已完成；本轮容器验证通过 |
| AR-10 | 缺少统一的通用后台服务扩展入口 | 生命周期扩展 | 已完成，验证见第 17 节 |
| AR-11 | 横切能力缺少通用自定义行为扩展机制 | 调用管道扩展 | 已完成，验证见第 17 节 |
| AR-12 | 具体组件配置仍集中在公共宿主并存在重复定义 | 配置归属与扩展边界 | 已完成；容器验证见第 21 节 |
| AR-13 | 横切行为共享的调用上下文及内置行为统一程度仍有限 | 管道进一步统一；已按限定范围完成 | 已完成并提交；容器验证见第 23 节 |
| AR-14 | 软删除规则依靠显式查询入口，缺少同等默认保障 | 持久化默认约束；已按限定范围完成 | 已完成并提交；容器验证见第 23 节 |
| AR-15 | 运行路由、权限清单与 OpenAPI 元数据分开维护 | 声明一致性 | 已完成并提交；容器验证见第 27 节 |
| AR-16 | 自定义宿主资源缺少配套的异步释放注册入口 | 生命周期扩展；按限定范围完成 | 原实现已提交；独立编译补正已完成，验证见第 28 节 |

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

恢复上下文时先读总表、第 12～15 节及当前 git diff。AR-01～AR-04 的问题描述为历史依据；AR-05～AR-07 的完成记录见第 10～11 节，不要重新执行已完成架构项或 B1～B8。

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

## 12. 再次提交后的架构复核：AR-08～AR-11

复核基线：Rust `a176933225bda107e3ab962a6d066ebe062f68e9`，用户已提交并推送 AR-05～AR-07，复核开始时工作区干净。NetEngine 仍为 `23a7d0002938d0308c975dc48c04ac66aae0dda8` 及原六处已授权修改；未拉取远端，以当前本地源码比较，排除 Admin.App。本次仅审查源码和项目引用，未修改生产代码或重新编译。

以下是剩余的宿主边界与扩展能力差异，不代表已确认现有业务故障。以上保留复核时判断；AR-08、AR-09 后续已获授权并完成，结果见第 14～15 节。AR-10、AR-11 仅登记，不自动扩大目标。保留 Rust 的显式注册、借用生命周期、trait 和 SeaORM，不要求复制 .NET 的完整 DI、自动扫描或 EF 变更跟踪。

### AR-08：WebAPI 公共核心仍绑定部分业务资源与应用运行时

**双方依据**：NetEngine 的 [WebAPI.Core 项目](D:/ZWork/NetEngine/ProjectCore/WebAPI.Core/WebAPI.Core.csproj) 引用 Application.Model、Common 和 DistributedLock，不引用文件、邮件提供商或数据库实现，具体资源由入口选择。Rust 的 [宿主档案](../project_core/webapi_core/src/host/mod.rs) 仍要求邮件和文件构造入口，[AppState](../project_core/webapi_core/src/host/runtime/state.rs) 固定保存两类资源；[依赖声明](../project_core/webapi_core/Cargo.toml) 仍引用 application_runtime、repository_core 和 SeaORM，[startup](../project_core/webapi_core/src/startup.rs) 包含 PostgreSQL 连接选项构造。JWT 编解码和 CurrentUser 的引用经过 application_runtime，连带引入应用数据库上下文。

**影响**：复用公共 WebAPI 宿主仍需处理与所选 API 无关的业务资源和数据库依赖。现有两端都创建邮件发送器，但源码未发现两端业务调用使用该状态字段；文件能力仍有真实调用，不能直接删除。AR-02 已完成数据库连接的创建职责外移，AR-06 只处理 TaskService，本项收尾 WebAPI 的剩余固定资源和依赖。

**实施范围**：沿用 WebApiResources 和按能力注册机制，由 presentation 选择文件、邮件等业务资源；通用 HTTP 工厂继续宿主共享。将数据库连接选项等机制归回合适的仓储组件，梳理共享 JWT 实现和 CurrentUser 的导入归属，使公共 WebAPI 核心不通过应用运行时引入数据库上下文。保留已有配置兼容入口，不为迁移配置而重建通用配置框架。

**完成标准**：

- [x] WebAPI 核心不再强制邮件、文件业务资源构造；需要文件能力的现有入口显式注册，并在启动时校验。
- [x] webapi_core 不再依赖 application_runtime、SeaORM 或 repository_core；JWT 共享实现与通用用户模型不通过应用数据库上下文引用，不以重导出掩盖间接依赖。
- [x] 数据库连接选项在所属公共组件复用，两个 WebAPI、TaskService 和工具的受影响调用保持原连接参数与目标，不重复复制实现。
- [x] 现有 API、鉴权续签、文件行为、健康检查、共享 HTTP 工厂及停机预算保持兼容；未使用的邮件能力可不装配。
- [x] 组件独立编译、依赖图检查及调用链审查提供证据，更新接入文档。

### AR-09：LLM 客户端工厂及模型配置解析不能独立替换

**双方依据**：NetEngine 定义 [ILlmClientFactory](D:/ZWork/NetEngine/Infrastructure/LLM/ILlmClientFactory.cs) 和 [ILlmModelConfigResolver](D:/ZWork/NetEngine/Infrastructure/LLM/ILlmModelConfigResolver.cs)，[LlmInvokeService](D:/ZWork/NetEngine/Application/Application.Service.LLM/LlmInvokeService.cs) 与 [LlmAppService](D:/ZWork/NetEngine/Application/Application.Service.LLM/LlmAppService.cs) 注入两者；默认配置解析实现在应用层。Rust 的 [client_adapter](../application/application_service_llm/src/llm/client_adapter.rs) 直接调用 llm::create_client，[调用服务](../application/application_service_llm/src/llm/invoke_service.rs) 和 [应用管理服务](../application/application_service_llm/src/llm/app_service.rs) 直接构造该适配器，配置解析绑定到 load_enabled_model_config 函数。

**影响**：AR-07 已支持替换整项 LLM 业务服务，但只替换协议客户端创建方式或模型配置来源时，仍需修改应用实现或替换整项服务；底层依赖的替换粒度不足。

**实施范围**：补充独立客户端工厂和模型配置解析契约，通用 LLM 契约归属基础设施，数据库查询实现留在应用层。宿主选择默认实现并注入 LLM 上下文；不把业务数据库查询移入基础设施，不新增协议或更改模型表。

**完成标准**：

- [x] 客户端工厂和配置解析器可分别替换，接入自定义实现不要求替换整个 LLM 业务服务。
- [x] 管理端试用、客户端普通调用和流式调用均使用所选依赖，不绕过注入重新创建默认实现。
- [x] 默认实现保留三个现有协议、模型启用/软删除条件、HTTP 复用及期限、错误转换、取消、流生命周期和会话保存行为。
- [x] 具体实现由 presentation 选择；启动检查所需资源，关闭 webapi_support 的 llm 特性仍可独立编译，不新增基础设施到应用层的反向依赖。
- [x] 完成必要独立编译、依赖审查和接入文档更新，区分源码验证与未进行的真实模型调用。

### AR-10：缺少统一的通用后台服务扩展入口

**双方依据**：NetEngine 的 [BackgroundServiceGenerator](D:/ZWork/NetEngine/SourceGenerator/SourceGenerator.Core/DependencyInjection/BackgroundServiceGenerator.cs) 生成 AddHostedService 注册并聚合到宿主入口。Rust 的 [WebApiFeature](../project_core/webapi_core/src/host/features.rs) 提供路由、校验和事件注册，[TaskServiceFeature](../project_core/task_service_core/src/features.rs) 提供任务与事件注册；[WebAPI 运行器](../project_core/webapi_core/src/host/runtime/builder.rs) 和 [任务运行器](../project_core/task_service_core/src/builder.rs) 管理各自固定的运行流程，未提供任意常驻服务的统一注册入口。

**影响与边界**：已有任务、事件、日志等生命周期能力不因此缺失；增加新的常驻监听或刷新服务时，需要另行接入启动、失败传播、取消和停机。它是扩展机制差异，不能把现有组件内部的后台任务视为尚未实现。

**后续可选完成标准**：提供轻量常驻服务契约和显式注册，统一失败处理、取消与剩余停机预算；至少让一个合适的现有生产后台流程使用该机制，不用空演示代替接入。不要求自动扫描或完整 Generic Host。当前不实施、不改现有任务调度。

### AR-11：横切能力缺少通用自定义行为扩展机制

**双方依据**：NetEngine 的 [ProxyBehaviorAttribute](D:/ZWork/NetEngine/SourceGenerator/SourceGenerator.Runtime/Attributes/ProxyBehaviorAttribute.cs)、[IInvocationAsyncBehavior](D:/ZWork/NetEngine/SourceGenerator/SourceGenerator.Runtime/Pipeline/IInvocationAsyncBehavior.cs) 和 [IInvocationBehavior](D:/ZWork/NetEngine/SourceGenerator/SourceGenerator.Runtime/Pipeline/IInvocationBehavior.cs) 支持外部定义行为并接入统一调用管道。Rust 的 [过程宏入口](../source_generator/source_generator_core/src/lib.rs) 和 [运行时辅助](../source_generator/source_generator_runtime/src/lib.rs) 提供既有日志、重试、缓存和并发限制，但没有同等通用的行为注册与调用管道契约。

**影响与边界**：已有四类横切能力不能据此认定有缺陷；新增一类可复用行为时，目前要新增专用宏或手工包装。宏顺序、同步/异步及流式支持须按实际现有实现判断，不套用旧文档限制。

**后续可选完成标准**：允许外部自定义行为接入，明确进入/退出顺序、错误与取消传播、借用及流式生命周期，并复用既有方法键和运行时辅助；接入至少一类已有生产行为，避免额外维护两套相互独立的机制。不复制运行时反射或全套 .NET 代理系统。当前不实施。

## 13. 本轮目标范围：仅连续完成 AR-08、AR-09

用户已启用并完成本目标，本节保留授权范围与停止条件。按 AR-08 → AR-09 连续推进，不逐阶段等待确认，允许为共享依赖调整内部顺序。

- 以第 12 节双方源码依据和完成标准为范围，保留 AR-01～AR-07、B1～B8 的成果；先核对两仓库基线与已有改动，只复查受影响部分。
- 完成公共 WebAPI 核心剩余资源和依赖解耦，以及 LLM 工厂、配置解析的独立注入；必须落实到现有宿主实际调用，不以空实现、未接入抽象或文档代替完成。
- 自主处理常规实现选择和编译问题。只有确实无法继续的外部条件、破坏性操作或超范围决定需要用户介入；继续推进不受影响的部分。
- 保持 API、业务规则、数据库结构、默认配置兼容、健康检查、取消和停机预算。沿用显式注册、trait、工厂和过程宏，不构建完整 DI 或新配置框架。
- AR-10、AR-11 保留待办，不自动实施；新发现的无关问题只登记，不扩大目标或循环重审全仓库。
- 全部 Rust 操作仅在 Docker Compose dev 内执行；不在本机安装组件，保持 stable/latest 与 /cargo-target，不添加或运行测试、测试依赖或模拟验证程序。
- 最后在容器内通过 `cargo fmt --all -- --check`、`cargo check --workspace --locked`、`cargo build --workspace --locked`、`cargo clippy --workspace --lib --bins --locked -- -D warnings`，补充拆分组件和可选特性的必要独立编译、依赖图检查。只因新改动、失败或未解疑点重复验证。
- 持续更新本清单的状态、关键决定、验证依据及接入变化；更新相关 README、AGENTS.md 和接入文档，明确真实数据库/云服务联调等未覆盖边界。
- 不提交、推送、部署，不修改 NetEngine，不执行真实数据库迁移。两项完成标准均达到、验证通过、文档更新后结束，交付可统一审查的工作区，不自动开启下一目标。

## 14. AR-08、AR-09 完成结果

本轮从 Rust `a176933225bda107e3ab962a6d066ebe062f68e9` 实施，保留起始两份清单文档改动；NetEngine 提交和原六处修改未变。AR-10、AR-11 没有实施，不自动开始下一轮。

- **AR-08 已完成**：[jwt_auth](../infrastructure/jwt_auth/src/lib.rs) 独立承接原 JWT 编解码实现，application_runtime::auth 仅兼容导出，公共宿主直接引用 jwt_auth 与 application_model 的 CurrentUser。[数据库纯配置](../infrastructure/common/src/database_settings.rs) 不引用驱动；[连接选项构造](../infrastructure/repository_core/src/connection.rs) 被 WebAPI、TaskService 与仓储工具共同使用，保留各入口参数及驱动默认值。webapi_core 的直接和传递依赖均不含 application_runtime、repository_core 或 SeaORM。
- **业务资源按需装配**：WebAPI 资源回调接收已经配置的同一个 HTTP 工厂；两端入口注册 FileStorageResource，本地模式显式传 None。File 及使用默认文件上下文的 User/Article/Link/Site 在启动时检查资源。核心 AppState 和档案移除邮件及文件固定构造要求，两端未使用的邮件实例取消，任务邮件资源保持原样。
- **AR-09 已完成**：[LLM 工厂与解析契约](../infrastructure/llm/src/factory.rs) 归属基础设施；[默认数据库解析器](../application/application_service_llm/src/llm/model_config_resolver.rs) 归属应用层，持有原业务写池的 Arc 克隆。工厂和解析器分别以 [LLM 资源](../presentation/webapi_support/src/llm_resources.rs) 注册，替换其中一项不要求替换业务服务。
- **实际接入**：[管理端资源入口](../presentation/admin_webapi/src/business_resources.rs) 和 [客户端资源入口](../presentation/client_webapi/src/business_resources.rs) 显式选择两项默认依赖。管理端试用、普通对话、流式对话经注入契约调用，LlmApp/LlmInvoke 启动校验两个资源。模型 CRUD 改用 LlmModelContext，会话查询保持只读连接，两者不强制依赖运行工厂及解析器。
- **兼容性复核**：配置字段和默认值未改，WebAPI 最小池 5、连接期限 10 秒；TaskService 最小池 30、连接期限 50 秒；工具最小池 1、连接期限 50 秒。任务及工具未设置的获取期限、最大存活通过 None 保留驱动默认值。JWT 算法与密钥解析、API 路由、健康检查、日志排空、任务租约与停机流程未改；文件仍共享宿主 HTTP 工厂。
- **LLM 行为复核**：启用/软删除/ID 查询条件和写池身份保持，未增加缓存；三个协议的创建函数与协议实现未改。适配器只改为调用所选工厂，错误转换保持原样。应用契约的取消包装继续覆盖配置读取和提供商调用，流仍持有其所需资源并按原规则保存会话，没有新增后台查询或脱离宿主的任务。

源码接入变化详见 [业务模块与宿主装配](database-modules.md)：资源工厂新增 Arc<HttpClientFactory> 参数；公共 AppState 的 file_storage/email_sender 字段与档案对应工厂字段移除；WebApiStartup 的连接选项方法改为 repository_core::connection 的函数；LlmContext 的 HTTP 工厂字段替换为两个契约引用并需启用 application_runtime 的 llm 特性，模型管理使用 LlmModelContext。旧 JWT 类型和数据库纯配置类型保留单向兼容导出。README、AGENTS.md、HTTP 与任务接入文档已同步。

## 15. AR-08、AR-09 验证与停止记录

全部 Rust 操作在现有 Docker Compose dev 内执行，产物仍在 /cargo-target，stable/latest 未改，未在本机安装组件。

| 检查 | 本轮结果 |
| --- | --- |
| `cargo fmt --all -- --check` | 通过 |
| `cargo check --workspace --locked` | 通过 |
| `cargo clippy --workspace --lib --bins --locked -- -D warnings` | 通过，无警告 |
| `cargo build --workspace --locked` | 通过，含两个 WebAPI、任务宿主及工具 |
| 单独 `cargo check -p <组件> --locked` | jwt_auth、webapi_core、llm、repository_core、task_service_core、repository_tool 均通过 |
| 关闭可选特性独立编译 | application_runtime、webapi_support 的 --no-default-features 均通过 |
| 分包依赖图 | webapi_core/jwt_auth 无 application_runtime、repository_core、SeaORM；llm 无应用或仓库依赖；application_runtime/webapi_support 关闭默认特性时无 llm、application_service_llm |
| 迁移与版本比较 | JWT 实现与原文件逐字一致；全部 WebAPI 连接池默认函数一致；外部 Cargo 包版本及 checksum 集合未变 |
| 调用链与生命周期复核 | 两项 LLM 运行依赖独立选择，三个调用入口接通；原应用取消契约、协议创建/流实现、HTTP 工厂和任务运行器文件未变 |
| 文档与差异检查 | 本轮更新文档的本地链接及 `git diff --check` 通过 |

本轮没有新增或运行测试，也没有进行真实数据库迁移、云模型调用或服务器部署。源码及编译证据不能替代这些运行联调。两项完成标准已达到，保留当前工作区供统一审查；未提交、推送或修改 NetEngine。本目标完成后停止，AR-10、AR-11 仍为待选扩展。

## 16. AR-10、AR-11 连续实施结果

用户在提交并推送 AR-08、AR-09 后授权连续完成这两项，不逐阶段确认。本轮 Rust 基线为 `2a0fb3776b703405ba2acaee15e77eaa772e4d10`，开始时工作区干净；NetEngine 仍为 `23a7d0002938d0308c975dc48c04ac66aae0dda8` 及既有六处未提交修改，本轮未改源项目。

| 条目 | 完成内容 | 代码依据 |
| --- | --- | --- |
| AR-10 | 通用常驻服务契约、拒绝重名的显式注册表、运行失败和 panic 传播、取消与剩余停机期限管理 | [background_service](../infrastructure/common/src/background_service.rs) |
| AR-10 | WebAPI 与 TaskService 均提供构建器注册入口及 Feature 注册钩子；装配完成后才运行，服务失败走宿主正常停机，超时 abort | [WebAPI 构建器](../project_core/webapi_core/src/host/runtime/builder.rs)、[任务构建器](../project_core/task_service_core/src/builder.rs)、[WebAPI Feature](../project_core/webapi_core/src/host/features.rs)、[任务 Feature](../project_core/task_service_core/src/features.rs) |
| AR-10 | 现有 WebAPI 健康检查以 `webapi.health` 注册，维持启动十秒、六十秒间隔、三十秒单次超时及原有检查项目 | [健康检查](../project_core/webapi_core/src/health.rs)及 WebAPI 构建器 |
| AR-11 | `#[behavior]` 支持单个或有序多个外部行为，同步、异步、流式分别接收 continuation、Future、Stream；复用既有稳定方法键规则 | [属性宏](../source_generator/source_generator_core/src/attributes/behavior.rs)、[运行时管道](../source_generator/source_generator_runtime/src/pipeline.rs) |
| AR-11 | 行为使用静态类型和显式捕获，不要求全局注册、反射、'static 或统一 Send；保持具体结果、错误及借用类型 | 同上；框架不会主动吞掉错误或取消，自定义行为负责自己的转换策略 |
| AR-11 | 流从首次 poll 执行行为，公共观察器区分自然完成、错误项后停止、提前释放和 panic，提前释放也按内层到外层结束 | [pipeline](../source_generator/source_generator_runtime/src/pipeline.rs) |
| AR-11 | 现有缓存运行时接入异步行为管道，既有流日志改为公共行为及观察器，避免两套 poll/Drop 生命周期 | [cacheable](../source_generator/source_generator_runtime/src/cacheable.rs)、[logging_stream](../source_generator/source_generator_runtime/src/logging_stream.rs) |

接入规则及使用边界见[宿主扩展](host-extensions.md)。任务调度、执行租约、事件消费、业务接口和数据迁移未改；缓存策略、普通日志、重试和并发限制默认值未改。流日志继续最多捕获一百项，准备阶段失败不启动流日志，提前结束仍记录非自然完成。

## 17. AR-10、AR-11 验证与停止记录

以下均使用本机 Docker `dev` 服务，沿用 stable 工具链与容器命名卷：

- `cargo check --workspace --locked`：通过。
- `cargo build --workspace --locked`：通过，包含三个运行时宿主、工具及宏组件。
- 最终 `cargo clippy --workspace --lib --bins --locked -- -D warnings`：通过。
- `cargo fmt --all -- --check` 与 `git diff --check`：通过。
- 静态复核：后台服务在完成注册后才启动，两类宿主均监听失败并使用同一停机期限；健康检查实现未改；缓存原有读写和锁逻辑未改；流日志保留快照格式、错误项返回及捕获上限。

本轮没有新增或运行测试，没有在本机安装工具，没有连接真实业务数据库、执行迁移、部署或修改 NetEngine。容器编译与静态复核不等同于真实宿主的故障注入、取消时序验证或所有外部自定义行为组合的运行验收。扩展代码应遵守协作式取消；异步 Drop 不会等待清理，阻塞工作不能靠 abort 强制停止。

AR-10、AR-11 的登记范围已完成。该阶段结束时 AR-01～AR-11 均无未实施条目，保留工作区供阶段性审查；用户随后已提交并推送。新一轮复核与有限目标见第 18～19 节。

## 18. 再次提交后的架构复核：AR-12～AR-14

Rust 基线为 `adf1b03f5a5bd3774cf144a3a7d307bbd95f8600`，用户已提交并推送 AR-10、AR-11，复核开始时工作区干净。NetEngine 仍为 `23a7d0002938d0308c975dc48c04ac66aae0dda8` 及此前六处未提交修改。本轮只做源码结构复核，排除 Admin.App，未修改生产代码、未重新编译、未拉取远端。

主要分层与宿主扩展入口已基本对应。以下是剩余的扩展和默认约束差异，不是已经复现的业务故障，也不推翻 AR-01～AR-11 按各自范围完成的记录。显式工厂与借用上下文、SeaORM 显式事务继续保留，不为形式一致实现完整 DI 容器或 EF 变更跟踪。

### AR-12：具体组件配置仍集中在公共宿主并存在重复定义

**双方依据**：NetEngine 的文件提供商在组件内定义 [FileStorageSetting](D:/ZWork/NetEngine/Infrastructure/FileStorage.AliCloud/Models/FileStorageSetting.cs)，经组件的 [注册扩展](D:/ZWork/NetEngine/Infrastructure/FileStorage.AliCloud/IServiceCollectionExtensions.cs) 接收配置，由 [Admin.WebAPI](D:/ZWork/NetEngine/Presentation/Admin.WebAPI/Program.cs) 选择装配。短信同样在 [SMS.AliCloud](D:/ZWork/NetEngine/Infrastructure/SMS.AliCloud/IServiceCollectionExtensions.cs) 中声明配置与注册。

Rust 的 [WebAPI Settings](../project_core/webapi_core/src/host/runtime/settings.rs) 与 [TaskService Settings](../project_core/task_service_core/src/settings.rs) 仍声明文件提供商枚举、云存储字段、SMTP 配置，任务核心另声明短信提供商及具体字段。与此同时，[阿里云文件](../infrastructure/file_storage_ali_cloud/src/lib.rs)、[腾讯云文件](../infrastructure/file_storage_tencent_cloud/src/lib.rs)、[阿里云短信](../infrastructure/sms_ali_cloud/src/lib.rs)、[腾讯云短信](../infrastructure/sms_tencent_cloud/src/lib.rs)、[SMTP](../infrastructure/email_smtp/src/lib.rs) 已各有运行配置类型。

**影响与边界**：AR-06、AR-08 已完成运行时资源装配解耦，这里关注配置模型归属。当前新增提供商若沿用标准配置入口，仍可能需要修改公共核心；重复的字段和默认值也容易漂移。既有宿主并不因此不能运行。公共监听、鉴权、健康检查和静态文件确实使用的配置仍应留在公共边界，不应把所有 file 字段机械移出。

**建议**：具体提供商参数由所属组件持有；提供商选择、组合配置与兼容转换由 presentation 或相应装配层负责。公共核心提供读取共同配置源及取得宿主扩展配置的必要入口，不引用具体云存储、短信或 SMTP 实现，不把耦合简单转移为另一个全量配置汇总 crate。

**完成标准**：

- 两类公共宿主不再枚举或定义具体文件、短信、SMTP 提供商参数；新增此类提供商可以只改所属组件及入口装配。
- 三个现有宿主实际接入新的配置路径；组件配置复用已有类型或有明确职责的兼容映射，不保留多份独立维护的同义字段和默认值。
- 保持现有 TOML 的节名、字段名、component 值、默认值、路径和凭据传递语义；环境文件、环境变量、旧配置兼容及覆盖顺序不变。核心配置和业务扩展配置复用同一来源与覆盖规则，不重复建立互相不一致的加载链。
- 保持本地文件模式、静态资源是否公开、任务内容根目录、共享 HTTP 工厂、日志过滤重载及取消/停机行为；不新增提供商，不自动热切换资源。
- 不恢复公共宿主到业务运行时、仓储或具体提供商的反向依赖；完成容器检查、编译和依赖复核。

### AR-13：横切行为共享的调用上下文及内置行为统一程度仍有限

**双方依据**：NetEngine 的 [InvocationContext](D:/ZWork/NetEngine/SourceGenerator/SourceGenerator.Runtime/Pipeline/InvocationContext.cs) 包含参数快照、参数键、取消令牌、TraceId 和特性存储，[InvocationPipeline](D:/ZWork/NetEngine/SourceGenerator/SourceGenerator.Runtime/Pipeline/InvocationPipeline.cs) 将同一上下文传给各行为。Rust 的 [pipeline](../source_generator/source_generator_runtime/src/pipeline.rs) 当前只共享稳定方法键；[日志运行时](../source_generator/source_generator_runtime/src/logging.rs) 与 [重试宏](../source_generator/source_generator_core/src/attributes/retry.rs) 仍分别建立部分调用信息，其他值由行为显式捕获。

**影响与边界**：AR-11 已提供可用的自定义扩展入口，缓存和流日志也已复用。复杂行为组合仍需额外传递参数键、取消和关联信息；一次调用的多个内置行为没有统一上下文保障。这是进一步统一的机会，不是已有缓存、日志或重试缺失。不能据此默认采集所有参数或引入服务定位器。

**后续完成标准**：先限定实际需要共享的调用元数据及其所有权，选择现有内置行为接入；明确嵌套调用、重试次数、取消和流枚举的上下文生命周期。复用现有稳定键与快照规则，保留敏感信息边界和具体错误类型，不要求复制 .NET 的全部反射与代理设施。本目标仅登记，不实施。

### AR-14：软删除规则依靠显式查询入口，缺少同等默认保障

**双方依据**：NetEngine 的 [SoftDeleteFilterGenerator](D:/ZWork/NetEngine/SourceGenerator/SourceGenerator.Core/EntityFrameworkCore/SoftDeleteFilterGenerator.cs) 按上下文生成 `HasQueryFilter`，由 [DatabaseContext](D:/ZWork/NetEngine/Repository/DatabaseContext.cs) 注册。Rust 在 [repository::extensions](../repository/src/extensions.rs) 为实体声明软删除扩展，由 [repository_core::extensions](../infrastructure/repository_core/src/extensions.rs) 提供 `find_active`、`find_by_id_active` 和显式过滤；直接调用 SeaORM 的普通查询入口不会自动获得这些过滤。

**影响与边界**：现有查询可以与源项目一致，但后续新增查询容易漏过滤，关联查询也需明确各实体的过滤。此次没有逐条证明业务查询存在遗漏，不能把这个结构判断写成已发现数据泄露。历史记录、管理查询和恢复流程可能需要包含已删除记录，不能全局机械加过滤。

**后续完成标准**：限定实体声明和常见查询入口的统一方式，明确查询活跃数据与显式包含删除记录的区别，并覆盖关联查询的使用约定；接入受影响生产路径，保留合法的历史查询。保持 SeaORM 显式查询及既有 xmin 保护，不借此实现 EF 变更跟踪或全量业务重写。本目标仅登记，不实施。

## 19. 下一目标：仅连续完成 AR-12

用户随后启用本目标，AR-12 已连续实施完成，结果见第 20～21 节。本节保留原范围及停止条件；AR-13、AR-14 未纳入实施范围。

以本清单第 18 节 AR-12 的依据和完成标准为范围，完成 rust_engine 的具体组件配置归属调整：把文件存储、短信、SMTP 的具体提供商参数归还所属组件，把提供商选择、组合配置和兼容映射放到 presentation 或相应业务装配层，使 WebAPI 与 TaskService 公共核心不再因新增这些提供商而修改。三个现有宿主必须实际接入，不以未使用的抽象或仅移动类型后继续由核心汇总全部提供商代替完成。

实施开始先核对两仓库当前基线，保留本次清单文档及其他已有改动。先整理现有字段、默认值、覆盖顺序与消费者，再进行最小必要调整。保持现有 TOML、环境覆盖、旧配置兼容、目录与文件公开策略、共享 HTTP 工厂、日志过滤重载和停机预算；具体资源继续由入口选择，不引入完整配置框架、DI 容器、自动热切换或新提供商。公共宿主实际使用的通用配置仍留在公共层。

连续推进至完成，不逐阶段等待确认；常规方案选择、兼容映射和编译问题自主处理。只有确实阻塞执行的外部条件、破坏性操作或超出范围的决定需要用户介入。AR-13、AR-14 仅保留待办；无关发现只登记，不扩大目标，不从零循环审查全仓库。

全部 Rust 工具链操作在 Docker Compose `dev` 中执行，不在本机安装 Rust、Cargo、SDK、构建依赖或其他组件；保持 stable 工具链、latest 镜像和 `/cargo-target` 命名卷。遵守 AGENTS.md，不新增或运行测试，不添加测试依赖或模拟验证程序。

完成后在容器内通过：

- `cargo fmt --all -- --check`
- `cargo check --workspace --locked`
- `cargo build --workspace --locked`
- `cargo clippy --workspace --lib --bins --locked -- -D warnings`

同时对受影响的核心及组件做必要的独立编译和依赖检查，静态核对三个宿主的配置兼容映射；不连接真实业务服务，不输出凭据。检查通过后只因新的改动、失败或未解疑点重复验证。

更新本清单的 AR-12 状态、实施落点、验证依据及剩余边界，按需同步 README、AGENTS.md 和配置接入文档。完成标准达到、容器验证通过、文档更新后结束并交付可审查的工作区；不提交、推送、部署，不修改 NetEngine，不执行数据库迁移，不自动开始 AR-13 或 AR-14。

## 20. AR-12 完成结果

本轮以 `adf1b03f5a5bd3774cf144a3a7d307bbd95f8600` 为基线，保留开始时两份清单文档改动，未修改 NetEngine。生产改动仅涉及组件配置归属、三个入口装配及公共配置扩展入口。

| 完成项 | 当前实现与依据 |
| --- | --- |
| 提供商参数归属组件 | 云文件组件的运行配置增加 Deserialize/Default，承接原有空值及 false 默认；短信与 SMTP 直接复用组件已有配置类型，不再由宿主复制字段 |
| 表现层选择文件提供商 | 新增 [file_storage_support](../presentation/file_storage_support/src/lib.rs)，三个入口共用 local/aliyun_oss/tencent_cos 选择与构造；它只装配文件组件，不汇总其他业务配置，不依赖公共宿主 |
| 公共 WebAPI 配置收敛 | [Settings](../project_core/webapi_core/src/host/runtime/settings.rs) 移除具体提供商类型和未使用的邮件配置；保留静态资源及业务上下文需要的文件路径、公开地址 |
| 公共任务配置收敛 | [任务 Settings](../project_core/task_service_core/src/settings.rs) 移除文件、短信、SMTP 类型及文件根目录档案字段；[任务入口配置](../presentation/task_service/src/configuration.rs) 负责组合及旧根目录回退 |
| 同源配置解析与早期校验 | 两个 Settings 保留同一次加载的合并快照，通过 `extension<T>()` 交给入口解析；三个档案的 `validate_configuration` 在资源连接创建前校验业务配置，不另读文件或环境变量 |
| 实际装配接入 | [管理端](../presentation/admin_webapi/src/llm_support.rs)、[客户端](../presentation/client_webapi/src/host_support.rs)、[任务入口](../presentation/task_service/src/host_support.rs) 均使用新配置路径，文件和短信继续复用原 HTTP 工厂 |

配置文件、部署模板、业务接口、提供商请求与发送逻辑均未修改。兼容映射详见[组件配置](component-configuration.md)，包括原 component 值、SMTP 587、云字段默认值、WebAPI/任务根目录回退、公开地址传入 url、旧 endpoint 继续不参与 API 地址构造。邮件附件继续以工作目录为内容根目录，不使用任务文件存储根目录。

配置入口变化：运行态 Settings 不再直接 Deserialize，必须经 `Settings::load` 建立含快照的完整配置；公共启动日志不再枚举具体文件提供商。WebAPI 无邮件发送用例，旧 `[message.smtp]` 可继续保留，但未使用节的字段类型不再由公共核心校验；任务入口仍校验 SMTP。没有新增提供商热切换，日志过滤重载继续沿用原来源与监听。

## 21. AR-12 验证与停止记录

所有 Rust 操作在现有 Docker Compose `dev` 内执行，保持 stable/latest、`/cargo-target`，未在本机安装任何组件。

| 验证 | 结果 |
| --- | --- |
| `cargo check --workspace --locked` | 通过 |
| `cargo build --workspace --locked` | 通过，包含三个宿主及工作区工具 |
| `cargo clippy --workspace --lib --bins --locked -- -D warnings` | 通过；校验回调用具名类型别名，未关闭诊断 |
| `cargo fmt --all -- --check`、`git diff --check` | 通过 |
| 独立 `cargo check --locked -p <crate>` | file_storage_support、webapi_core、task_service_core 均通过 |
| 独立依赖图 | 两个公共核心均无 file_storage_support、具体云文件/短信/SMTP、application_runtime/application_service 或业务 repository 依赖；WebAPI 经契约层保留文件/短信/邮件契约的间接依赖，未声称完全不引用这些契约 |
| Cargo.lock 对照 | 仅本地工作区成员及依赖边变化；外部包版本、来源与 checksum 集合未变，锁文件在容器离线更新后使用 locked 完成验证 |
| 源码兼容性复核 | 逐项核对默认值、URL/endpoint 映射、根目录回退、同源加载、早期验证、HTTP 工厂和内容目录；公共设置已无具体提供商类型残留 |

本轮没有新增或运行测试、模拟验证程序，没有连接真实数据库、云存储、短信或 SMTP 服务，没有执行迁移或部署。配置兼容性证据是源码对照与编译，不代表已完成所有环境覆盖组合或外部服务的运行联调。

提交前复核修正：TaskService 原先通过 `serde(flatten)` 合并文件提供商配置，导致环境变量字符串无法沿用 config 的布尔值转换。现改为路径/消息配置与 `FileStorageConfiguration` 分别从同一启动快照直接解析，再组合为运行态配置；保留 `[file]` 字段、默认值、根目录回退及覆盖顺序。修复后已在容器重新通过工作区 check、build、Clippy（`-D warnings`）与格式检查；未新增测试或运行真实宿主联调。

AR-12 完成标准已达到，更新 README、AGENTS.md、任务文档和组件配置文档后停止，保留未提交改动供统一审查。AR-13、AR-14 仍为待办，未自动实施；不启动新的全仓库循环对齐。

## 22. AR-13、AR-14 连续实施结果

用户提交并推送 AR-12 后明确授权连续完成两项，无需逐阶段确认。本轮 Rust 基线为 `714a47ca49a0d197626758d552437e05a3bf550f`，开始时工作区干净；NetEngine 基线仍为 `23a7d0002938d0308c975dc48c04ac66aae0dda8` 及原有六处修改，未修改源项目、未拉取远端。仅推进上述两项，不扩展下一轮全仓库对齐。

| 项目 | 实施结果 |
| --- | --- |
| AR-13 调用元数据 | [InvocationContext](../source_generator/source_generator_runtime/src/pipeline.rs) 拥有一次调用的追踪标识、可选取消令牌、重试次数、按需参数快照与完整参数键，以及显式类型化特性；clone 共享元数据，返回值及错误保持静态类型 |
| AR-13 组合入口 | 新增 [invocation 属性](../source_generator/source_generator_core/src/attributes/invocation.rs)，统一展开显式 logging/cacheable/retry/behavior；按声明顺序进入，方法键只在边界声明，禁止多个重试或缓存键；保留独立属性兼容 |
| AR-13 内置接入 | 普通/流日志使用同一 InvocationContext；缓存参数键写入上下文且实际执行沿用公共管道；重试日志与其他行为使用同一 trace_id，等待成功后更新次数；[客户端既有演示](../presentation/client_webapi/src/services.rs) 已接入，未额外挂载日志或重试 |
| AR-13 生命周期 | 嵌套调用/递归创建新边界，无任务局部继承；流成功准备后持有上下文，首次枚举才开始行为，未消费不记录日志；重试覆盖 Future，不重放流元素；取消分类、先耗尽后检查令牌及停机边界不变 |
| AR-14 实体声明 | 新增 [SoftDelete 派生](../source_generator/source_generator_core/src/derive/soft_delete.rs)，25 个业务实体及 3 个组件实体在 Model 声明处接入；移除散落的手写 trait 实现与中央注册名单 |
| AR-14 默认/历史入口 | 固有 find/find_by_id 默认使用 DeleteTime IS NULL；find_with_deleted/find_by_id_with_deleted 显式读取历史，泛型契约仍可用 find_active/find_by_id_active；没有改动 xmin 或写入语义 |
| AR-14 关联接入 | [SoftDeleteSelectExt](../infrastructure/repository_core/src/extensions.rs) 在 JOIN ON 中过滤关联目标并保留既有关系条件；授权功能列表、路由绑定列表接入。路由仍返回，已删除关联功能变为 None；LLM 应用名称映射随默认入口排除已删除模型 |
| AR-14 历史保留 | 权限初始化六处恢复查询、数据库日志积压 ID 去重显式包含删除记录；xmin 泛型回读、数据库事件原生 SQL 状态游标保持原语义 |

设计及使用边界见[宿主扩展](host-extensions.md)和[软删除查询](soft-delete.md)，README 与 AGENTS.md 已同步。共享上下文不提供服务定位器、不默认采集参数；快照及键仍复用既有算法，不输出缓存参数键。软删除固有方法保护具体实体的常见入口，不宣称拦截全部 SeaORM 查询；原始泛型入口、普通关联方法、桥接表与原生 SQL 仍需明确过滤。未改变数据库 schema、迁移、队列状态转换、事件游标或业务事务。

## 23. AR-13、AR-14 验证与停止记录

Rust 操作全部在 Docker Compose dev 中执行，保持 stable/latest 和 `/cargo-target`，未在本机安装任何组件。

| 验证 | 结果 |
| --- | --- |
| `cargo check --workspace --locked` | 通过，无警告 |
| `cargo build --workspace --locked` | 通过，包含三个宿主与工作区工具 |
| `cargo clippy --workspace --lib --bins --locked -- -D warnings` | 通过，未关闭诊断；清理因实体固有查询入口新增而不再使用的 EntityTrait 导入 |
| `cargo fmt --all -- --check`、`git diff --check` | 通过 |
| `cargo check --locked -p source_generator_core -p source_generator_runtime -p repository_core` | 通过，过程宏、运行时与仓储核心协同检查 |
| 实体与查询复核 | 28 个 Model 声明覆盖既有软删除实体；核对普通查询、两处关联查询、六处初始化恢复、日志去重及 xmin 泛型回读；事件状态 SQL 未改 |
| 配置/依赖边界 | 未新增 Cargo 依赖或修改锁文件、工具链和容器配置；未向公共宿主引入业务/提供商依赖 |

没有新增或运行测试、模拟验证程序，没有连接真实数据库或云服务，没有执行迁移、提交、推送或部署；NetEngine 的原六处未提交修改保持原状。编译和源码复核不能代替全部自定义行为组合、取消时序、流枚举或数据库 JOIN 的运行验收；复杂关联仍按文档显式处理。

AR-13、AR-14 按本轮限定的调用上下文和常见查询边界完成，清单、接入文档与 AGENTS.md 已同步。当前工作区留待用户审查和阶段性提交，不自动启动新一轮架构或业务对齐。

提交前复核修正：组合属性原先让各层直接引用边界的同一上下文变量，`retry → behavior → logging` 和 `cacheable → behavior → logging` 中的 `async move` 会消耗可重复调用闭包捕获的句柄。现在统一在每层展开的作用域内克隆上层句柄，内层只移动局部克隆；每次重试重新建立句柄，元数据仍共享同一 Arc。修复后容器工作区 check、build、Clippy（`-D warnings`）、格式检查及 `git diff --check` 均通过。组合所有权路径已按生成代码复核；遵守工作区约定，未新增测试或模拟编译程序，未声称现有生产调用点覆盖全部组合。

## 24. 再次提交后的架构复核：AR-15、AR-16

用户确认 AR-13、AR-14 已提交并推送。本轮本地 Rust 基线为 `5e5a307f1c9f7d53397d371923c7ceee31a02c44`，审查开始时工作区干净；NetEngine 基线仍为 `23a7d0002938d0308c975dc48c04ac66aae0dda8` 及原有六处未提交修改。依据为两边本地当前代码，未拉取远端；排除 Admin.App。主要分层没有发现需要重新调整的问题，不将已经接受的显式工厂、静态宏管道和 SeaORM 查询方式重复列为待办。

### AR-15：统一运行路由、权限清单与 OpenAPI 的声明来源

**双方依据**：NetEngine 的 [AuthorizeController.UpdateRoute](D:/ZWork/NetEngine/Presentation/Admin.WebAPI/Controllers/AuthorizeController.cs) 从 `IActionDescriptorCollectionProvider` 读取控制器路径及授权元数据，[公共 WebAPI 注册](D:/ZWork/NetEngine/ProjectCore/WebAPI.Core/Extensions/WebApplicationBuilderExtension.cs) 通过 Swagger 使用控制器描述信息。Rust 的 [WebApiFeature](../project_core/webapi_core/src/host/features.rs) 分别提供 `register`、`permission_routes` 和 `openapi`；以文章模块为例，[路由与权限列表](../presentation/admin_webapi/src/features/article.rs)、[处理器文档注解](../presentation/admin_webapi/src/handlers/article/mod.rs)及 [OpenAPI 处理器清单](../presentation/admin_webapi/src/openapi/article.rs)需要分别维护。

**影响与边界**：新增或修改接口容易遗漏其中一处；本轮确认的是声明重复，并非已证明现有路由、权限或文档存在失配。DTO schema、示例和说明可以保留独立声明，不要求仿制 MVC 反射系统。

**完成标准**：建立共享端点声明或宏，让运行路径、HTTP 方法、授权/匿名元数据、权限同步信息及 OpenAPI 操作关联由同一来源派生；迁移管理端和客户端现有特性，不只提供未接入的抽象。处理特殊端点和多方法端点，明确不参与权限同步或文档的声明方式；文档所需类型信息可继续由 utoipa 提供，但不再独立手写可能漂移的路径、方法和处理器枚举。保留原有 URL、方法、权限备注、显式 UpdateRoute、Development 文档入口，以及认证、续签、权限、绑定、签名、缓存与请求队列的执行顺序；不因合并声明给匿名接口加鉴权或改变 Ueditor 策略。静态核对迁移前后端点集合、权限清单和文档操作的对应关系，记录合法例外。

### AR-16：自定义宿主资源的异步释放注册入口

**双方依据**：NetEngine 的 [RedisLock 注册](D:/ZWork/NetEngine/Infrastructure/DistributedLock.Redis/IServiceCollectionExtensions.cs)与 [IDisposable/IAsyncDisposable 实现](D:/ZWork/NetEngine/Infrastructure/DistributedLock.Redis/RedisLock.cs)体现了容器管理资源释放的接入方式。Rust 的 [Resources](../infrastructure/common/src/resources.rs) 仅保存和查找类型化资源，[WebApiResources](../project_core/webapi_core/src/host/resources.rs)另提供健康检查；[WebAPI](../project_core/webapi_core/src/host/runtime/builder.rs)与 [TaskService](../project_core/task_service_core/src/builder.rs)对已有日志、事件和后台服务分别收尾，没有与自定义资源注册配套的异步清理入口。

**影响与边界**：新增需要异步排空或关闭的资源时，还需另接生命周期。[通用后台服务](../infrastructure/common/src/background_service.rs)已经允许在取消后异步清理，任务处理器也已有 close，不能将本项解释成现有资源泄漏或 AR-10 未完成。本项原为按需改进，用户本轮明确要求与 AR-15 一起完成；只补轻量资源清理机制，不引入完整 DI、自动依赖图或全新后台服务框架。

**完成标准**：提供显式、可选的异步清理注册契约，并实际接入 WebAPI、TaskService 的资源装配和退出路径。明确资源所有权、清理顺序、重复注册与重复释放规则；消费者排空后再关闭其依赖，复用宿主剩余停机期限，不逐资源重置预算。覆盖构建中途失败、正常退出和运行失败的清理路径，清理失败或超时可诊断，单个失败不得跳过其余可清理资源。对适合使用新入口的现有生产资源完成接入，保留其他资源普通 Drop；现有日志、事件、ID 租约与任务收尾各自只执行一次，不为使用新接口强行迁移全部组件。静态复核超时取消、部分装配与已有后台清理的交互，不增加脱离宿主管理的清理任务。

## 25. 下一目标：连续完成 AR-15、AR-16

本节保留用户要求整理并随后启用的目标文本；两项已连续实施，结果及容器验证见第 26～27 节。

以本清单第 24 节为固定范围，连续完成 rust_engine 的 AR-15、AR-16：统一管理端与客户端运行路由、权限同步和 OpenAPI 的端点声明来源，并为 WebAPI 与 TaskService 自定义资源提供配套的异步清理注册入口。两项都必须接入现有生产装配，不能以新增未使用的抽象作为完成。

开始时核对两仓库基线，保留本清单及其他已有改动；先记录现有端点、权限与文档对应关系，以及资源所有权和停机顺序，再实施最小必要调整。保持第 24 节的兼容性边界和既有分层，不重开 AR-01～AR-14，不修改 NetEngine，不扩展 Admin.App、业务接口验收、数据库迁移或新提供商。无关发现只登记，不从零循环审查全仓库。

无需中途或逐阶段等待确认，常规方案选择、接入和编译问题自主解决。只有确实无法继续的外部阻塞或超出授权范围的操作才报告具体原因；不因可自行解决的实现选择停下来询问。

遵守 AGENTS.md：全部 Rust 工具链操作仅在本机 Docker Compose `dev` 服务执行，不在 Windows 安装 Rust、Cargo、SDK、构建依赖或其他组件；保持 stable 工具链、latest 基础镜像及 `/cargo-target` 命名卷。不新增或运行测试，不添加测试依赖、测试辅助代码或模拟验证程序。

完成后从项目根目录执行并通过：

- `docker compose exec -T dev cargo fmt --all -- --check`
- `docker compose exec -T dev cargo check --workspace --locked`
- `docker compose exec -T dev cargo build --workspace --locked`
- `docker compose exec -T dev cargo clippy --workspace --lib --bins --locked -- -D warnings`
- `git diff --check`（仅文本检查，可在本机执行）

同时静态复核端点声明迁移、权限和文档对应关系、资源清理顺序及失败路径；如改宏，核对过程宏与运行时协同。检查通过后，仅因新改动、失败或未解疑点重复验证，不连接真实业务库或云服务执行破坏性验证。

更新本清单的 AR-15、AR-16 状态、实施落点、验证依据与剩余边界，按需同步 README、AGENTS.md 和接入文档。完成标准满足、容器检查与编译通过、文档更新后停止，交付可审查的改动和简短结果；不自动提交、推送、部署，不自动开始下一轮架构对齐。编译通过不表述为全部运行时场景已验证。

## 26. AR-15、AR-16 连续实施结果

用户启用第 25 节目标后连续完成两项，保留目标开始前的清单改动。Rust 基线仍为 `5e5a307f1c9f7d53397d371923c7ceee31a02c44`；NetEngine 提交和原六处未提交修改保持不变，未拉取或修改源项目。

| 项目 | 实施结果及证据 |
| --- | --- |
| AR-15 统一声明 | [webapi_endpoint 属性](../source_generator/source_generator_core/src/attributes/webapi_endpoint.rs) 同时生成运行方法/路径、策略、权限说明与 utoipa 文档；[Endpoint](../project_core/webapi_core/src/endpoint.rs) 按策略装配签名和鉴权，不重复维护路径与方法 |
| AR-15 特性装配 | [WebApiFeature](../project_core/webapi_core/src/host/features.rs) 使用 endpoints 单一集合；两个宿主的 18 个 Feature 全部迁移，独立 register/permission_routes 和 OpenApi paths 枚举均移除。补充文档只保留 schema/标签，宿主拒绝补充 paths，并检查运行端点重复 |
| AR-15 兼容性 | 112 个端点、113 个显式方法映射、68 条权限路径、111 个有文档端点及 33 个 JSON 绑定名称保持一致；路径、权限备注、授权/匿名/签名策略、文档 security 与处理器正文按迁移前源码核对。Ueditor 双方法、无签名、无文档；上传、缓存、请求队列与支付回调策略保留 |
| AR-16 清理契约 | [ResourceCleanup](../infrastructure/common/src/resource_cleanup.rs) 提供 acquire/manage/register；名称唯一、宿主独占、逆序 FnOnce 回调，共享绝对期限。创建前拒绝重复，资源创建成功后立即登记；一次失败、panic 或超时继续尝试后续清理，错误按名称与类别汇总 |
| AR-16 公共宿主 | [WebAPI builder](../project_core/webapi_core/src/host/runtime/builder.rs) 和 [TaskService builder](../project_core/task_service_core/src/builder.rs) 将登记簿传给资源工厂；装配返回错误或 panic、启动取消、WebAPI 绑定失败、正常及运行失败退出均等待清理。未运行的已构建宿主提供显式 shutdown |
| AR-16 生产资源 | [ModuleDatabase::connect_managed](../infrastructure/repository_core/src/module.rs) 分别登记读写池；三个宿主实际使用。WebAPI 的日志写池、惰性健康检查池、日志只读池在 [业务资源装配](../presentation/webapi_support/src/resources.rs) 登记，日志组件结束后才关闭；任务消息资源不强加异步收尾 |
| 文档与规则 | 同步 [宿主扩展](host-extensions.md)、[模块接入](database-modules.md)、README 与 AGENTS.md；明确扩展接口变更、资源所有权及异步取消边界 |

公共 WebAPI 未新增数据库驱动或业务实现依赖；没有新增 Cargo 依赖，也没有调整锁文件、工具链、容器镜像、数据库 schema、迁移、业务事务或队列状态机。TaskService 日志组件自身拥有的连接和事件/ID 资源保持既有所有者，不另登记第二次释放。

## 27. AR-15、AR-16 验证与停止记录

全部 Rust 操作在 Docker Compose dev 执行，保持 stable/latest 和 `/cargo-target`；未在 Windows 安装组件。

| 验证 | 结果 |
| --- | --- |
| `docker compose exec -T dev cargo check --workspace --locked` | 通过，无警告 |
| `docker compose exec -T dev cargo build --workspace --locked` | 通过，包含三个宿主及工作区工具 |
| `docker compose exec -T dev cargo clippy --workspace --lib --bins --locked -- -D warnings` | 通过，未关闭诊断 |
| `docker compose exec -T dev cargo fmt --all -- --check` | 通过 |
| `git diff --check` | 通过 |
| AR-15 源码核对 | 逐项比较迁移前端点与当前声明，方法、路径、权限、策略、绑定名及处理器正文无变化；检查补充 schema 和标签、自动收集操作 schema、特殊路由与中间件装配顺序 |
| AR-16 源码核对 | 检查单池成功/后续失败的登记、工厂错误与 panic 回收、构建取消期限、未启动宿主、绑定失败、正常退出及后台失败；检查消费者/日志先结束、资源后关闭，清理错误继续与超时后取消，不使用后台补偿任务 |

没有新增或运行测试、测试依赖、测试辅助代码或模拟编译程序；未连接真实业务数据库、云服务进行联调，未执行迁移、提交、推送或部署。上述源码和编译证据不代表生产取消竞争、驱动故障或全部扩展回调已经运行验收。

剩余边界：端点扩展接口已从分别维护 register/permission_routes 改为 endpoints，下游自定义 Feature 需迁移；DTO 文档仍可独立声明。资源清理只接管显式登记的资源，额外模块应通过同一登记簿 connect_managed 后传给装配辅助函数，不能重复登记。宿主成功构建后应 run 或显式 shutdown，直接丢弃或从外部取消 build/run Future、进程强杀、panic=abort 无法保证异步收尾。超时不会延长宿主预算，未完成回调被取消并返回失败；同步阻塞回调必须由接入方避免。

AR-15、AR-16 达到本轮完成标准，文档已更新，工作区保留供阶段性审查。到此停止，不自动开始新的全仓库对齐或重新评估已完成条目。

提交前复核修正：端点宏允许显式 HEAD，但共享路由收尾原先无条件追加 HEAD 拒绝处理器，导致重复注册并在宿主构建时 panic。现按同一路径合并后的声明方法集合判断，仅在未声明 HEAD 时追加拒绝处理；显式 HEAD、同一声明中的 GET/HEAD、同路径分别注册的 GET 与 HEAD 均保留已注册处理器，未声明 HEAD 时仍返回原有 405，Allow 继续来源于显式方法集合。修复后容器工作区格式检查、check、build、Clippy（`-D warnings`）及 `git diff --check` 均通过。该分支按源码与 Axum 注册逻辑复核，遵守工作区约定未新增或运行测试、模拟程序，未声称执行了 HEAD 端点运行联调。

## 28. AR-16 提交后补正：通用仓储独立编译

本轮基线为 Rust `90763accd6985be18caa41433b37ecc6131ba3d2`，用户确认 AR-15、AR-16 已提交并推送，开始时工作区干净。仅修复独立组件的特性依赖，不启动新的架构改造。

**问题与原因**：容器单独执行 `cargo check --locked -p repository_core` 报 E0433：`common` 中找不到 `resource_cleanup`。AR-16 的 `ModuleDatabase::connect_managed` 引用了该模块，但模块受 `common::runtime` 控制，repository_core 又显式关闭 common 默认特性。此前工作区检查因其他成员启用了 runtime 而通过，不能证明该组件的依赖声明完整。

**修正**：[common/Cargo.toml](../infrastructure/common/Cargo.toml) 新增 `resource-cleanup` 特性，只额外启用 Tokio 与 time；[模块导出](../infrastructure/common/src/lib.rs)改由该特性控制。[repository_core/Cargo.toml](../infrastructure/repository_core/Cargo.toml)保留 `default-features = false`，显式启用 `resource-cleanup`。common 默认 runtime 包含新特性并保持 Tokio full，原宿主能力不变；未为仓储开启图片、Excel 等整个 runtime 工具集合，未新增依赖版本或改动 Cargo.lock。

**范围**：不改变清理回调、停机预算、连接行为、实体更新时间、数据库迁移或业务代码。更新时间规则逐实体实现的观察暂不编号为实施任务，不在本次处理。接入说明同步到[宿主扩展](host-extensions.md)。

**容器验证**：以下 Cargo 命令均分别通过 `docker compose exec -T dev` 执行。独立组件与工作区使用不同 Cargo 调用，避免特性合并掩盖问题。

| 验证 | 结果 |
| --- | --- |
| `cargo check --locked -p common --no-default-features --features resource-cleanup` | 通过，最小资源清理特性可单独检查 |
| `cargo check --locked -p repository_core` | 通过，原 E0433 消失 |
| `cargo build --locked -p repository_core` | 通过，独立构建成功 |
| `cargo check --workspace --locked` | 通过 |
| `cargo build --workspace --locked` | 通过，包含三个宿主及工作区工具 |
| `cargo clippy --workspace --lib --bins --locked -- -D warnings` | 通过 |
| `cargo fmt --all -- --check`、`git diff --check` | 通过 |
| `cargo tree --locked -p repository_core -e features -i common` | 仅启用 common 的 resource-cleanup 及 Tokio 依赖，未启用 default/runtime |

未新增或运行测试，未安装本机组件；stable/latest、Cargo.lock 和 `/cargo-target` 保持不变。未修改 NetEngine，未提交、推送或部署。本次补正完成后停止，不推进更新时间规则或其他架构调整。
