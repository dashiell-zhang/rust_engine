# NetEngine 对齐：阶段性差异清单与下一批范围

审查日期：2026-11-08。本文件是当前待办入口；`netengine-alignment.md` 保留历史过程，不能把其中早期的“尚未实现”直接当作当前缺陷。

当前方向（2026-11-08）：用户要求优先比较框架架构，暂不推进下一批业务接口验收。已确认的四项架构差异、代码依据和建议有限目标见[框架架构差异清单](netengine-architecture-review.md)；用户随后授权目标模式连续完成 AR-01～AR-04，实施结果与验证以架构清单为准。下面保留既有功能批次及验证证据，不自动进入新的业务验收批次。

提交后更新：AR-01～AR-04 已随 `17d276ce03d48d6a7be69a2d65be470827754889` 提交。复核新增 AR-05～AR-07（基础设施依赖方向、任务宿主资源装配、业务能力按需组合），依据、完成标准及连续实施边界见架构清单第 8～9 节；用户随后授权连续实施，现已完成；本轮生产实现、独立依赖检查、工作区 check/build/Clippy/fmt 结果与源码接入变化见架构清单第 10～11 节。未自动进入新的业务验收批次。

初次盘点完成了基线固定、覆盖盘点、重点代码核实和三个批次的建议，当时没有修改生产代码或执行运行验收。后续 B1～B8 的实际改动、验证和限制以各批记录为准；“批次完成”不代表整个复刻目标完成。

## 1. 固定基线

初次盘点基线保留如下。用户随后将 B1～B6 的改动提交为 `acd147dccbe26dbb766b1479897c5e5cb1806e97`；B7 从该提交及干净工作区开始，NetEngine 基线未变。

| 项目 | 本次范围 |
| --- | --- |
| Rust | `D:/ZWork/rust_engine`，提交 `0ce43aed8b76cd130ed713819e8336706f0335a8`；开始时工作区干净，已与 origin/master 同步 |
| NetEngine | `D:/ZWork/NetEngine`，提交 `23a7d0002938d0308c975dc48c04ac66aae0dda8`，加下表五个已授权的未提交文件 |
| 纳入 | 源项目 35 个 csproj 中除 Admin.App 外的 34 个；包括应用、宿主、基础设施、公共工具、源码生成、仓储工具和部署 |
| 排除 | `Presentation/Admin.App` Blazor 项目；不自动追踪远端新提交，不把 .NET/Rust 语言机制相同作为验收条件 |

NetEngine 未提交文件的 SHA-256（按本地文件字节记录，换行变化也会改变摘要）：

| 文件（相对 NetEngine） | SHA-256 | 已授权内容 |
| --- | --- | --- |
| Application/Application.Service.LLM/LlmConversationManageService.cs | `8d86c1b7c21c467fd116cd8eab731c7473eb6b47a7e808eea5e4c4c387a34861` | 日期查询转换 UTC |
| Infrastructure/Logger.Database/Query/LogQueryService.cs | `3de4423697ddc10d6cfde955b4adc7b12352d4ccf407ac0551a4c06864d32120` | 日期查询转换 UTC |
| Infrastructure/QueueTask.Database/QueueTaskManagement.cs | `7999f13af841ccc3d492c2022925ac85b965702bdfecbd2ebf87c9581655611d` | 日期查询转换 UTC |
| Infrastructure/FileStorage.TencentCloud/TencentCloudStorage.cs | `d5f60df043290603d927313c7996945f7322e0fa35dc60e88374ef187becf582` | 上传对象名使用目录加本地文件名 |
| Infrastructure/FileStorage/IFileStorage.cs | `be3257d370157804fbfe10d38450f9f7102906233f4c5d75231b11789e138715` | 上传契约说明 |

B8 经用户明确授权新增第六处源项目修改：`Application/Application.Service/AuthorizeService.cs` 的小程序绑定查询改用微信返回的 openId，文件 SHA-256 为 `54baf94c9fd04755b039da4311e5cdb51af73958da2306d6d8383c5f2df7a112`。上述初始五项保留，B8 结果见对应报告。

后续批次如果基线变化，应记录新提交和受影响条目，不重新把整个仓库从零核查一遍。

## 2. 状态与证据规则

- **已实现且有验证依据**：必须写明验证的具体范围。静态路由对应、编译通过、模型快照一致，都不能升级为整个模块运行等价。
- **已实现但缺少运行验证**：代码已存在；某些场景可能有历史运行记录，但本次未复验，剩余场景仍需验收。
- **确认存在差异**：双方代码能指向具体行为差别。没有实际复现的，明确标注为源码判断。
- **尚未核实**：本轮没有足够依据。它不是已确认缺陷，也不是允许无限扩展调查的任务。

证据分为：**本轮静态核实**、**提交前检查**、**历史运行记录**。历史记录没有逐项绑定当前完整提交，不能作为最新运行镜像或全部功能已验证的证明。不提供“完成百分比”，因为接口数量、代码存在和行为覆盖不能混算。

### 可以沿用的检查

提交前同一生产代码已经通过容器内 `cargo check --workspace --all-features --locked`、`cargo build --workspace --locked`、`cargo clippy --workspace --lib --bins --locked -- -D warnings`、格式检查，以及 business/log/event-bus 三目标 `migration-pending`。三宿主 Release 编译另有此前记录。本轮只改变文档，不重复全量编译，不执行测试。

## 3. 架构与模块覆盖

以下是范围映射和核实入口，不是声称逐个公共方法均已验收。链接指向本次本地工作区。

| 源项目/能力 | Rust 对应 | 当前结论与剩余边界 |
| --- | --- | --- |
| Application.Interface、Application.Model | [接口](D:/ZWork/rust_engine/application/application_interface)、[模型](D:/ZWork/rust_engine/application/application_model) | 已实现且有静态/编译依据：接口依赖模型；处理器经服务契约调用。各 DTO 的全部边界未验收 |
| Application.Service、Application.Service.LLM、Application.Service.SMS | [应用层](D:/ZWork/rust_engine/application) | 已实现但缺少完整运行验证；不能仅由接口齐全推断业务流程一致 |
| WebAPI.Core、Admin.WebAPI、Client.WebAPI | [共享 WebAPI](D:/ZWork/rust_engine/project_core/webapi_core)、[宿主](D:/ZWork/rust_engine/presentation) | 本轮重读 Feature 装配和依赖：共享宿主经应用契约委托，不直接装配具体业务仓储；71/42 路由静态对应 |
| TaskService.Core、TaskService | [共享任务宿主](D:/ZWork/rust_engine/project_core/task_service_core)、[业务任务](D:/ZWork/rust_engine/presentation/task_service) | 工厂创建执行对象、业务处理器由 presentation 装配；运行验收缺口见 V01 |
| Repository、Repository.Core、Repository.Tool | [业务实体](D:/ZWork/rust_engine/repository)、[数据库机制](D:/ZWork/rust_engine/infrastructure/repository_core)、[工具](D:/ZWork/rust_engine/repository_tool) | 已实现；三目标快照与模型一致。历史生产库升级、完整 JSON 映射边界尚未核实，见 V03 |
| QueueTask.Database | [任务存储](D:/ZWork/rust_engine/infrastructure/queue_task_database) | 已实现；本轮重读完成事务、父链行锁、租约和手动重试；未据此宣称所有竞争场景已验证 |
| EventBus、EventBus.Redis、EventBus.Database | [契约](D:/ZWork/rust_engine/infrastructure/event_bus)、[Redis](D:/ZWork/rust_engine/infrastructure/event_bus_redis)、[数据库](D:/ZWork/rust_engine/infrastructure/event_bus_database) | 广播及独立后端已实现；数据库 Position 在事务内推进，取消入口已有实现。业务监听和发布取消缺运行依据，见 V04 |
| Logger.Database、Logger.LocalFile | [数据库日志](D:/ZWork/rust_engine/infrastructure/logger_database)、[文件日志](D:/ZWork/rust_engine/infrastructure/logger_local_file) | 有界缓冲、恢复、排空和独立查询已实现；历史故障运行记录存在，完整启动失败/强制停机仍待验收，见 V05 |
| IdentifierGenerator | [ID 生成](D:/ZWork/rust_engine/infrastructure/identifier_generator) | Redis 租约实现存在；多进程断线与耗尽未充分验证，见 V06 |
| DistributedLock、InMemory、Redis | [锁契约](D:/ZWork/rust_engine/infrastructure/distributed_lock)、[内存实现](D:/ZWork/rust_engine/infrastructure/distributed_lock_memory)、[Redis 实现](D:/ZWork/rust_engine/infrastructure/distributed_lock_redis) | 有实现；与缓存、横切属性共同参与业务。全部 TTL、取消和竞争语义尚未逐项验收 |
| FileStorage、AliCloud、TencentCloud | [存储契约](D:/ZWork/rust_engine/infrastructure/file_storage)、[阿里云](D:/ZWork/rust_engine/infrastructure/file_storage_ali_cloud)、[腾讯云](D:/ZWork/rust_engine/infrastructure/file_storage_tencent_cloud) | 已包含目录加文件名修正；真实账号的签名、分片和恢复仍缺运行验证，见 V07 |
| SMS、AliCloud、TencentCloud | [短信契约](D:/ZWork/rust_engine/infrastructure/sms)、[阿里云](D:/ZWork/rust_engine/infrastructure/sms_ali_cloud)、[腾讯云](D:/ZWork/rust_engine/infrastructure/sms_tencent_cloud) | 实现存在；未把历史任务成功当作实际短信投递成功，见 V07 |
| LLM | [三协议适配器](D:/ZWork/rust_engine/infrastructure/llm) | Chat、Responses、Anthropic 的流式/非流式均存在；D01 已在 B3 修复并通过容器编译，提供商响应仍缺运行验证 |
| Common（含邮件、缓存、图像、数据、JSON/XML 等） | [common](D:/ZWork/rust_engine/infrastructure/common)、[邮件](D:/ZWork/rust_engine/infrastructure/email)、[SMTP](D:/ZWork/rust_engine/infrastructure/email_smtp)、[混合缓存](D:/ZWork/rust_engine/infrastructure/hybrid_cache) | 多数已有实现或独立 crate；本轮不作完整公共方法覆盖保证。确认 D02/D03，其余见 V08 |
| SourceGenerator.Core、SourceGenerator.Runtime | [过程宏](D:/ZWork/rust_engine/source_generator/source_generator_core)、[运行时](D:/ZWork/rust_engine/source_generator/source_generator_runtime) | 服务/任务注册、实体声明与横切属性已存在；全部源生成器支持形态未核实。文档限制存在过期项，见 M01 |
| Deployment | [生成器与模板](D:/ZWork/rust_engine/deployment)、[发布说明](D:/ZWork/rust_engine/docs/deployment.md) | 已有生成和容器打包路径；运行镜像证据落后于当前提交，见 V09 |

### 业务路由覆盖：本轮重新核实

读取两端 16 个 Controller 的 HTTP 声明与路由模板，再对照 Rust 默认 Feature 的 `.route` 注册。比较 HTTP 方法和路径形状，将占位符名称归一化；保留 Demo 的 `/api` 前缀。未发现方法/路径缺项或额外项。

| 宿主 | 源控制器（方法/路径数） | Rust 注册入口 | 状态 |
| --- | --- | --- | --- |
| Admin | Article 11、Authorize 5、Base 5、File 5、LlmModel 5、Operations 13、Role 8、Site 8、Ueditor 2、User 9；合计 **71** | [管理端 Feature](D:/ZWork/rust_engine/presentation/admin_webapi/src/features) | 已实现且有本轮静态清单依据 |
| Client | Authorize 10、Base 6、Demo 9、File 6、Pay 9、User 2；合计 **42** | [客户端 Feature](D:/ZWork/rust_engine/presentation/client_webapi/src/features) | 已实现且有本轮静态清单依据 |

源入口：[管理端 Controller](D:/ZWork/NetEngine/Presentation/Admin.WebAPI/Controllers)、[客户端 Controller](D:/ZWork/NetEngine/Presentation/Client.WebAPI/Controllers)。上述数量不涵盖 Swagger、静态文件、健康检查等非 Controller 入口；不能证明参数名、所有 DTO、守卫、授权数据、返回值和事务效果一致。

## 4. 已确认的行为差异

### D01 — LLM 提供商 JSON 读取未施加相同的 64 层限制

- 2026-11-08 更新：B3 已完成共享读取器修复及容器编译检查，详见 [B3 结果与限制](D:/ZWork/rust_engine/docs/llm-b3-review.md)。以下保留发现时的依据；实际提供商响应与 64/65 层边界尚无本批运行证据。
- 类型：Rust 行为差异，源码依据，未做本轮网络复现；优先级中。
- 源位置：[Chat JsonOptions 与读取](D:/ZWork/NetEngine/Infrastructure/LLM/Compatible/OpenAiCompatibleLlmClient.cs)、[Responses](D:/ZWork/NetEngine/Infrastructure/LLM/Responses/OpenAiResponsesLlmClient.cs)、[Anthropic](D:/ZWork/NetEngine/Infrastructure/LLM/Anthropic/AnthropicMessagesLlmClient.cs)。三者未覆盖默认最大深度；System.Text.Json 默认有效上限为 64，参见[官方 MaxDepth 文档](https://learn.microsoft.com/en-us/dotnet/api/system.text.json.jsonserializeroptions.maxdepth?view=net-10.0)。
- Rust 位置：[wire.rs](D:/ZWork/rust_engine/infrastructure/llm/src/wire.rs)，`ValueVisitor` 不携带深度，`from_slice` 直接使用 serde_json。本轮只读核对容器缓存中锁定的 serde_json 1.0.154 源码，`src/de.rs:63` 初始化 `remaining_depth: 128`，没有同等的 64 层约束。它独立于已修复的 [common 读取器](D:/ZWork/rust_engine/infrastructure/common/src/json_helper/input.rs)。
- 影响：深层提供商响应或 SSE 单个事件的拒绝边界可能不同；普通浅层响应不因此缺少功能。不能把 ExtraBody 保存入口的修复视为本项已修复。
- 验收边界：三协议共用的读取入口统一层级计算，非流式与 SSE 一致；保留重复属性、动态错误字段及已有可空集合行为。只解决深度边界，不顺带追求全部异常文字一致。纳入批次 B3。

### D02 — 对象克隆不具备源项目的通用共享引用/循环引用还原能力

- 类型：框架能力缺口，源码依据；对现有业务的影响尚未证明，优先级低。
- 源位置：[JsonHelper.cloneOpts](D:/ZWork/NetEngine/Infrastructure/Common/JsonHelper.cs) 使用 `ReferenceHandler.Preserve`。
- Rust 位置：[json_helper.rs](D:/ZWork/rust_engine/infrastructure/common/src/json_helper.rs)、[input.rs](D:/ZWork/rust_engine/infrastructure/common/src/json_helper/input.rs)、[output.rs](D:/ZWork/rust_engine/infrastructure/common/src/json_helper/output.rs)。输入登记 `$id`，但对象中的 `$ref` 仍进入元数据拒绝分支；输出没有通用对象身份表。
- 影响：不能将任意共享引用对象图按源约定克隆或作为任务参数还原。[任务参数入口](D:/ZWork/rust_engine/project_core/task_service_core/src/task_handler.rs) 使用该能力；当前消息/演示参数的已支持形态不能证明任意图都支持。
- 后续：先列出现有实际参数模型是否需要共享身份。若没有使用，保留为框架扩展待办；不为追求形式一致重塑全部 Rust 模型，不自动纳入前三批。

### D03 — 一般畸形 JSON 的错误文字仍不同

- 类型：接口诊断差异，源码依据；优先级低。
- 源位置：[WebAPI JSON 装配](D:/ZWork/NetEngine/ProjectCore/WebAPI.Core/Extensions/WebApplicationBuilderExtension.cs) 保留 ASP.NET Core/System.Text.Json 输入处理；历史详细对照见 [请求绑定说明](D:/ZWork/rust_engine/docs/request-binding.md)。
- Rust 位置：[json_body.rs](D:/ZWork/rust_engine/project_core/webapi_core/src/json_body.rs)，剩余 `JsonReadError::Syntax` 分支返回 `Failed to parse the request body as JSON: ...`。
- 影响：部分非法字符、数字或字面量错误的文案/定位可能与源不同；不是已确认的合法业务请求失败。已处理的空白正文、尾随内容和部分截断分支不重复列为缺陷。
- 后续：仅在客户端依赖具体错误契约或有明确失败请求时，按有限样例处理。不把复刻整个 .NET JSON 诊断体系作为下一批完成条件。

### S01 — 源项目微信小程序绑定查询错误（B8 已修复）

- 类型：已确认的源项目缺陷；2026-11-08 用户明确授权 B8 修正，代码及容器构建已完成，真实微信登录仍未联调。
- 源位置：[AuthorizeService.cs](D:/ZWork/NetEngine/Application/Application.Service/AuthorizeService.cs:87)，发现时获取 `(openId, sessionKey)` 后，已有绑定查询使用 `t.OpenId == login.AppId`，而新增绑定保存的是 `OpenId = openId`。B8 已将查询条件改为 `t.OpenId == openId`，锁外首次查询和锁内复查复用同一查询。
- Rust 位置：[authorize/service.rs](D:/ZWork/rust_engine/application/application_service/src/authorize/service.rs)，查找绑定时传入微信返回的 `open_id`。
- 影响：修复前当用户 openId 不等于应用 appId 时，源端可能找不到此前绑定，进入错误的首登/注册路径；修复后查询与保存使用同一身份维度。
- 结果：[B8 记录](D:/ZWork/rust_engine/docs/authorization-b8-review.md)。Rust 无需修改；没有自动合并历史重复用户或改动绑定数据，不将源码核对与编译通过视为完整微信链路运行证明。

## 5. 验证缺口与未核实项

这些条目不能写成“已发现 bug”。允许在有限验收后给出“通过、确认缺陷、环境阻塞、证据不足”，不要求一次消灭所有不确定性。

| ID | 当前状态 | 具体剩余范围与影响 | 双方代码/证据入口 |
| --- | --- | --- | --- |
| V01 | 已实现但缺少完整运行验证 | 队列父子完成/回调原子性、手动重试与有效租约竞争、长执行续租、停机期限耗尽后的恢复。决定任务是否重复或丢失，建议 B1 | [源存储](D:/ZWork/NetEngine/Infrastructure/QueueTask.Database/QueueTaskStore.cs)、[源管理](D:/ZWork/NetEngine/Infrastructure/QueueTask.Database/QueueTaskManagement.cs)、[Rust 存储](D:/ZWork/rust_engine/infrastructure/queue_task_database/src/store.rs)、[管理](D:/ZWork/rust_engine/infrastructure/queue_task_database/src/management.rs)、[执行器](D:/ZWork/rust_engine/project_core/task_service_core/src/runner.rs) |
| V02 | 已实现但缺少当前批次运行验证 | Release 的认证→续签→权限→签名→缓存顺序；权限路由同步的失败回滚、空清单和软删除重建。建议 B2 | [源过滤器](D:/ZWork/NetEngine/ProjectCore/WebAPI.Core/Filters)、[源授权服务](D:/ZWork/NetEngine/Application/Application.Service/AuthorizeService.cs)、[Rust auth](D:/ZWork/rust_engine/project_core/webapi_core/src/auth.rs)、[签名](D:/ZWork/rust_engine/project_core/webapi_core/src/sign_verify.rs)、[授权应用服务](D:/ZWork/rust_engine/application/application_service/src/authorize) |
| V03 | B4 已取得限定迁移链的运行证据，其余待核实 | 当前 Rust 迁移链的带数据升级/回退、DDL 失败回滚、三模型隔离和幂等脚本见 B4；任意旧库接管、xmin 业务版本冲突、分区维护和 JSON 索引复杂映射仍未验收 | [B4 记录](D:/ZWork/rust_engine/docs/repository-b4-review.md)、[源工具](D:/ZWork/NetEngine/Repository.Tool)、[Rust 迁移](D:/ZWork/rust_engine/repository_tool/src/migrator)、[xmin](D:/ZWork/rust_engine/infrastructure/repository_core/src/versioned.rs)、[分区](D:/ZWork/rust_engine/infrastructure/repository_core/src/partition) |
| V04 | B7 已完成有限源码对照与组件编译，业务运行证据仍缺失 | 双方当前均无具体业务事件发布/订阅调用；工厂、重试、释放、关闭及数据库取消路径已核对，未发现本范围确认缺陷。发布竞争、处理器失败/释放和控制连接失败仍未实际验收 | [B7 记录](D:/ZWork/rust_engine/docs/event-b7-review.md)、[源数据库传输](D:/ZWork/NetEngine/Infrastructure/EventBus.Database/DatabaseEventTransport.cs)、[Rust 传输](D:/ZWork/rust_engine/infrastructure/event_bus_database/src/lib.rs)、[事件说明](D:/ZWork/rust_engine/docs/event-bus.md) |
| V05 | 已实现；部分故障有历史运行记录 | 日志磁盘/数据库失败恢复已有记录；三个宿主全部启动失败分支、强制停机下的排空及指标采集仍缺完整依据 | [源数据库日志](D:/ZWork/NetEngine/Infrastructure/Logger.Database)、[源文件日志](D:/ZWork/NetEngine/Infrastructure/Logger.LocalFile)、[Rust 数据库日志](D:/ZWork/rust_engine/infrastructure/logger_database)、[文件日志](D:/ZWork/rust_engine/infrastructure/logger_local_file) |
| V06 | B5 已取得限定 ID 租约场景的运行证据 | 双实例节点隔离/续期、所有权丢失、Redis 暂停超出租期、节点耗尽及退出自然到期见 B5；其他网络交错、时钟和序列边界、通用缓存/锁仍未完整验收 | [B5 记录](D:/ZWork/rust_engine/docs/identifier-b5-review.md)、[源节点租约](D:/ZWork/NetEngine/Infrastructure/IdentifierGenerator/RedisNodeLease.cs)、[Rust ID](D:/ZWork/rust_engine/infrastructure/identifier_generator)、[ID 说明](D:/ZWork/rust_engine/docs/identifier-generator.md) |
| V07 | 已实现但缺少真实提供商运行验证 | 支付下单/退款/回调及证书轮换、云存储、短信、邮件、LLM 的真实服务链路。当前不使用真实凭据、不发真实消息、不触发支付 | [源应用服务](D:/ZWork/NetEngine/Application/Application.Service)、[源 LLM](D:/ZWork/NetEngine/Infrastructure/LLM)、[Rust 支付](D:/ZWork/rust_engine/application/application_service/src/pay)、[Rust LLM](D:/ZWork/rust_engine/infrastructure/llm)、上方提供商映射 |
| V08 | 尚未核实；部分已知边界记录 | XML 完整命名空间/模型映射、历史时区与夏令时边界、Excel 所有类型、源生成器所有支持形态、地区初始数据导入方式。单个工具的文档“未完成”不应触发无限细节开发 | [源 Common](D:/ZWork/NetEngine/Infrastructure/Common)、[Rust Common](D:/ZWork/rust_engine/infrastructure/common)、[源生成器](D:/ZWork/NetEngine/SourceGenerator)、[Rust 宏](D:/ZWork/rust_engine/source_generator) |
| V09 | B6 已验证当前三个发布包，镜像与真实部署仍待核实 | 当前代码的三个 Linux 发布包在已有 Ubuntu 运行镜像中完成启动/停止，部署生成结果一致；未重建三套 Dockerfile 镜像，systemd、云效及其他 Linux 环境仍未验收 | [B6 记录](D:/ZWork/rust_engine/docs/deployment-b6-review.md)、[源部署](D:/ZWork/NetEngine/Deployment)、[Rust 部署](D:/ZWork/rust_engine/deployment)、[部署说明](D:/ZWork/rust_engine/docs/deployment.md) |

### M01 — 指令文档中的一条能力描述已过期

[AGENTS.md](D:/ZWork/rust_engine/AGENTS.md) 仍写四个横切属性均仅支持 `async fn`；[logging.rs](D:/ZWork/rust_engine/source_generator/source_generator_core/src/attributes/logging.rs) 实际已有同步/异步执行分支。`retry`、`cacheable`、`concurrency_limit` 仍有显式 async 检查。本轮只记录，不修改约定；以后维护文档时应区分“代码能力”与“项目允许的使用方式”，不能据旧文字重新实现 logging 同步支持。

## 6. 不列为缺陷或已被后续修复的事项

- Rust 使用 trait、过程宏、SeaORM 和独立适配器 crate，而源使用 DI、Roslyn、EF Core；这是实现方式差异，验收业务契约与分层职责，不追求项目数相等。
- Admin 的 Operations/Site 等控制器在 Rust 中拆到 task/log/link/site 等 Feature；按有效路由和职责对应，不按文件名一一复刻。
- 请求签名 Debug 放行、客户端没有同样的管理端签名包装、公开的 UpdateRoute 等源行为，不仅凭直觉标记为 Rust 遗漏；若要改变它们，应作为单独产品/安全变更。
- 事件数据库服务器端取消已有 `PostgresOperation` 实现和后续历史运行记录；早期“取消未对齐”已过期。保留的是发布取消、控制连接失败等 V04，不能重做已完成修复。
- 客户端 `/Authorize/UpdateRoute` 曾遗漏，当前本轮路由核实已存在；不重新开补接口任务。
- ExtraBody 配置读取的 64 层限制已进入 common 读取器；仅 LLM 独立 wire 读取器是 D01。
- 腾讯云上传对象名和三处日期 UTC 修正已经包含在固定源基线中，不重新询问或重复修复。

## 7. 建议下一阶段：最多三个有限批次

优先顺序按业务影响安排。B1/B2 是“先完成指定验收，确认缺陷才局部修复”，不预设核心代码有错；B3 处理已确认的协议差异。每次只启动一个批次，完成后报告并结束，由用户决定是否提交和进入下一批。

### B1：队列任务事务与恢复闭环（建议首先进行）

2026-11-08 已完成本批有限执行，详见 [B1 结果与限制](D:/ZWork/rust_engine/docs/queue-b1-review.md)。修复短租约续期间隔及实际复现的父链锁升级死锁；长执行、精确竞争交错与强制停机仍缺运行证据。以下保留原验收范围，不据此自动重开 B1，也不标记 V01 全部通过。

**范围**：V01，仅 queue_task_database 与 task_service_core 的必要改动，业务装配只在确有调用问题时调整。使用已有任务和正式管理入口，不创建测试模块、测试依赖或专用验证服务。

**有限验收清单**：

1. 并发子任务完成：父子完成时间与回调在同一事务形成，回调不重复；失败事务不留下部分成功状态。
2. 手动重试与 Worker 竞争：有效租约内拒绝重试；过期或失败后的重试能恢复执行；已成功任务不能重新执行。
3. 长执行与所有权：续租成功时不被另一实例领取；失去所有权的旧 Worker 不能提交新的完成结果。
4. 停机恢复：先停止领取，在预算内排空；超时后留下可按租约恢复的状态，不提前标记成功。

**不包含**：Cron 所有历法边界、对象图克隆 D02、整个事件/日志系统、无关业务任务、性能优化。已有缺参/普通重试/双实例领取的历史案例不整套重跑，仅按受影响路径选择必要检查。

**结束条件**：四项分别给出结果和证据；确认的本范围缺陷修复并完成容器编译检查。若现有正式入口无法触达某情形，标记证据不足并报告所需条件，不为凑“通过”增加测试脚手架或开启新功能。任何此前被自动审批拒绝的运行方式不换形式重试。

### B2：Release 鉴权链与权限同步闭环

2026-11-08 已完成本批有限执行，详见 [B2 结果与限制](D:/ZWork/rust_engine/docs/webapi-b2-review.md)。Release 鉴权/续签/签名、两接口缓存与权限先后顺序、同步回滚及软删除重建取得运行证据，未发现需修改生产代码的问题。空清单与指定并发交错仍只有源码依据；B3 未自动开始。

**范围**：V02，共享 WebAPI 鉴权/签名/缓存及授权应用服务；使用隔离数据，避免写真实权限库。

**有限验收清单**：

1. 未认证与无权限分别得到源约定的 401/403；合法续签在权限前发生，NewToken 保留规则正确。
2. 管理端签名失败、业务内部错误和成功响应的状态/续签头分别符合已确认契约；必须用 Release。
3. 客户端两个 60 秒缓存接口先完成认证与权限检查，完整 Authorization 隔离缓存。
4. UpdateRoute 同一批事务成功或回滚；空清单清理、软删除路由重建不恢复旧授权绑定。

**不包含**：S01 微信登录源修复、全部 113 个接口的业务验收、完整 JSON 文案 D03、HTTP/2 与所有代理行为、通用安全策略重设计。

**结束条件**：四项明确结果，局部缺陷修复并完成容器检查；其余发现只登记。缺少必要运行条件时报告缺口，不以 Debug、纯编译或静态路由数量代替验收。

### B3：LLM 读取深度修复与有限协议核验

2026-11-08 已完成本批代码与编译验收，详见 [B3 结果与限制](D:/ZWork/rust_engine/docs/llm-b3-review.md)。三协议非流式与 SSE 的共同入口已施加 64 层限制；本批没有可用的隔离提供商与模型配置，普通响应、流式完成/错误及边界输入的运行验证保留为缺口，不宣称 LLM 完全等价。

**范围**：D01，必要时涉及 llm 内部共享读取器；沿用已有三协议请求/响应契约。

**有限验收清单**：

1. 共享入口在读取时执行 64 层限制，三种协议的非流式与 SSE 均使用该规则；标量、根容器与子容器的层级定义一致。
2. 不丢失重复属性/数字词法，不把动态错误对象与 DTO 字段转换规则混为一谈。
3. 沿用既有正式试调用入口核验普通响应和流式完成/错误路径；没有可用服务或无法构造边界输入时，区分源码证明与运行证据，不搭建专用模拟服务。

**不包含**：所有提供商 SDK 细节、真实付费模型采购、所有网络分片、共享引用 D02、异常文案逐字复制。不能为了小范围修复重复升级 Cargo 依赖。

**结束条件**：D01 的代码与编译验收完成，已有入口可覆盖的运行结果记录清楚；缺失运行证据仍标记为缺失，不宣称 LLM 完全等价。

## 8. 后续单批目标

### B4：已登记迁移链的数据保留与事务回滚

2026-11-08 已完成本批有限执行，详见 [B4 结果与限制](D:/ZWork/rust_engine/docs/repository-b4-review.md)。指定旧版本的带记录升降级、升级/回退失败回滚、三模型隔离和离线幂等脚本取得运行证据；没有发现需要修改生产代码的问题。其余 V03 范围继续保留，不自动扩大本批。

**范围**：V03 中的迁移执行和离线脚本。以临时 PostgreSQL 的隔离数据检查 Rust 自身已登记的历史版本，源项目仅对照模型隔离、历史表和设计时配置职责。

**有限验收清单**：

1. 从旧业务版本带记录升级到当前版本，再回退到选定版本；核对记录、表名和历史，特别是 File/StoredFile 改名。
2. 唯一约束冲突或外部依赖导致迁移失败时，结构和历史记录一起回滚；移除隔离故障后可继续迁移。
3. 同一物理库分别迁移业务、日志、事件；只回退选定模型，其他模型的表、数据及历史保留。
4. 连接不可用时仍能离线导出；在隔离库执行幂等 Up/Down 脚本，重复执行不重复迁移，区间边界符合说明。

**不包含**：接管任意 .NET 旧库、恢复已删除数据、xmin 应用写入冲突、分区维护、复杂 JSON 索引映射、迁移生成器全部形态。使用正式工具与 SQL 准备/查询隔离状态，不添加测试或验证程序。

**结束条件**：四项给出实际结果或明确缺口；仅修复确认的本范围缺陷并完成适用容器编译检查，清理本批临时资源后报告并结束，不自动进入下一批。

### B5：ID 节点租约与失效保护

2026-11-08 已完成本批有限执行，详见 [B5 结果与限制](D:/ZWork/rust_engine/docs/identifier-b5-review.md)。四项在所列隔离场景取得运行证据，未发现需修改生产代码的问题；不是对全部网络交错或全部 V06 能力的证明。

**范围**：V06 的自动 ID 节点部分，对照 NetEngine 租约所有者与有效期规则，使用隔离 Redis/PostgreSQL、现有客户端宿主及正式路由同步入口触发发号。

**有限验收清单**：

1. 两个宿主共享节点范围时取得不同节点，持续运行超过一个租期仍能发号。
2. 替换某实例节点所有者后，该实例停止发号且不覆盖新所有者，另一个实例仍可工作。
3. Redis 不可用超过本地有效期后拒绝发号；恢复后按已确认所有权处理，不猜测原节点仍属于自己。
4. 节点全部占用时初始化明确失败；正常退出停止续期，保留键直到自然到期。

**不包含**：通用缓存和锁、所有网络交错、时钟跳变、序列压力、随机分配统计。不增加测试、验证程序或专用运行入口，不把本批结果推广为全部 V06 已通过。

**结束条件**：四项分别记录运行结果或缺失条件；确认的范围内缺陷才局部修复，完成适用容器编译检查，清理临时资源后结束，不自动进入下一批。

### B6：当前 Linux 发布包与部署生成结果

2026-11-08 已完成本批有限执行，详见 [B6 结果与限制](D:/ZWork/rust_engine/docs/deployment-b6-review.md)。三个当前发布包完成容器编译、运行库及文件核对、实际启动和正常退出；生成器输出与仓库一致。没有发现需修改生产代码或模板的问题，未扩大到真实部署。

**范围**：V09 的发布包和文件生成部分。沿用正式 `deployment/build-release.sh` 构建三个当前宿主，核对默认配置、初始化数据、许可证和动态库；运行正式部署生成器，核对模板与生成结果。

**有限验收清单**：

1. 三个发布包从当前代码生成，包含各自必要文件及运行库记录。
2. 在已有本机运行镜像中使用发布包实际二进制、默认配置路径和临时环境覆盖，连接隔离数据库/Redis启动；WebAPI 检查健康响应，任务宿主检查正常进入运行状态。
3. 三个宿主正常停止，清理临时资源；不把开发镜像具备构建工具当作发布运行依赖已满足。
4. 生成器只生成文件，当前模板与生成结果一致，沿用源部署的服务/代理/流水线职责。

**不包含**：重建三套 Dockerfile 镜像、真实服务器部署、systemd 实际安装与云效发布、全部 Linux 发行版兼容性、业务功能回归。不创建测试或验证程序，不在本机安装组件。

**结束条件**：四项明确结果或缺口，确认的本范围缺陷才局部修复；发布包保留为本批交付物，临时运行资源清理，完成报告后结束。

### B7：事件总线调用链与失败处理

2026-11-08 已完成本批有限执行，详见 [B7 结果与限制](D:/ZWork/rust_engine/docs/event-b7-review.md)。未发现需要修复的确认缺陷，本批只更新文档。双方均没有实际业务事件发布/订阅入口，因此监听失败、释放与发布竞争没有本批运行证据，不标记为运行通过。

**范围**：V04 的业务调用存在性、三个宿主装配、监听工厂与重试/释放、正常关闭，以及数据库发布取消路径。

**有限验收清单**：

1. 查找双方实际事件声明、发布调用和监听注册，区分框架能力与业务接入。
2. 对照失败监听重试、每次调用的处理器生命周期、参数解析失败与释放失败规则。
3. 核对启动游标、宿主取消与关闭预算、数据库发布事务及控制连接取消路径。
4. 对现有正式入口可触达的场景核验，缺少调用入口时记录限制；完成事件组件容器编译检查。

**不包含**：人为新增业务事件、测试或验证程序、模拟服务、全部网络故障、发布/提交所有交错、强制停机及通用 JSON 兼容性扩展。不重复 B6 的空监听宿主启动来替代业务事件验收。

**结束条件**：明确源码结论和运行证据边界，仅修复确认的范围内缺陷，完成必要检查及记录后结束，不自动扩大范围。

### B8：微信小程序已有绑定识别

2026-11-08 已完成本批有限执行，详见 [B8 结果与限制](D:/ZWork/rust_engine/docs/authorization-b8-review.md)。按用户授权修复 NetEngine 的 S01，Rust 查询条件原本正确；源项目 Release 构建和 Rust 应用组件检查均在容器内通过，真实微信登录未联调。

**范围**：S01 查询字段修复，及双方已有用户、锁内复查、首次注册事务、软删除绑定/用户和后续令牌签发的源码核对。

**不包含**：真实微信调用、模拟微信服务、测试或验证程序、历史重复账号清理、其他登录渠道重构与完整认证链回归。

**结束条件**：修复明确缺陷，完成受影响项目的容器编译检查，区分源码结论与缺少微信凭据的运行边界，保留原有两仓库改动后结束，不自动提交。

## 9. 执行与维护约定

- 新发现先登记 ID、双方位置、影响和证据，不自动扩入当前批次。只因它阻止当前验收时，才说明依赖关系并处理必要部分。
- 不机械复制源项目疑似缺陷。S01 已按 B8 单独授权修复；其他源项目修正仍需判断其任务授权，不能由此前修复无限扩展。
- Rust 工具仍仅在 Docker dev 内运行；stable 与 latest 约定不变，不安装本机组件，不添加或运行测试。
- 批次中按改动做必要局部检查，批次末执行一次适用的格式、check、Clippy、build。只有新改动、失败或明确未解决问题才重跑；只有需要 Release 行为时才构建对应宿主。
- 运行受阻或条件不足时，把具体缺失证据交付出来。不得绕过自动审批拒绝、无限等待外部凭据，或用不断补小细节延长目标。
- 每项保留最近验证对应的提交、命令/操作摘要和结果。生产代码发生相关变化后才重新评估证据，文档改动不触发全工作区重编译。
- 本清单是有限盘点，不承诺枚举了全部潜在差异。下一阶段的完成标准是其明确清单闭合或限制被如实记录，不是“直到没有任何差异”。
