# NetEngine 对齐进度

> 当前待办与批次边界见 [阶段性差异清单](D:/ZWork/rust_engine/docs/netengine-next-stage.md)。本文件保留历史过程，早期“尚未实现/未验证”可能已被后续记录覆盖；不要直接据此重开开发任务。2026-11-08 盘点已固定双方代码基线，后续按有限批次推进。

## 对照基准

- 本地项目：`D:\ZWork\NetEngine`；首次核对包含当时工作区未提交的源码调整。
- 首次核对时间：2026-11-07。
- 当时 HEAD：`5044b546d9245d2fc1fdd40e55dc558610a6a10b`。
- 当前复核 HEAD：`23a7d0002938d0308c975dc48c04ac66aae0dda8`。NetEngine 工作区另外包含用户授权的腾讯云上传命名及三个日期查询 UTC 修正；该提交将应用服务接口、DTO 和表现层调用调整纳入版本，此前对照过的内容仍需按当前源码逐项确认。
- 范围：除 `Presentation/Admin.App` Blazor 项目之外的后端、基础设施、工具和部署能力。
- 以下记录是持续开发清单，不代表已完成全部功能等价验证。后续核验应重新读取 NetEngine 当前源码。

## 已落地的公共数据库和任务边界

- 新增 `repository_core`，承载软删除查询契约、只读连接和与业务表无关的健康检查。
- 业务实体保留在 `repository`，通用软删除契约的实体实现分别由实体所属 crate 提供。
- `QueueTask`、`TaskSetting` 移入 `queue_task_database`，审计用户标识不再导航到固定业务用户实体。
- `Log` 实体移入 `logger_database`，该组件不再依赖业务仓库。
- TaskService 业务适配器移入运行入口；`task_service_core` 移除对业务仓库、业务模型和具体业务服务的直接依赖。
- WebAPI 具体服务装配移到 `presentation/webapi_support`，两个入口显式选择业务策略；`webapi_core` 通过 `WebApiApplication` 委托续签、权限与路由同步，移除具体应用服务和业务仓库依赖。JWT 验证留在公共宿主，令牌签发和续签归授权应用服务；普通依赖树确认不再传递依赖业务服务或仓库。
- WebAPI 使用的授权、用户、角色、文章、链接、站点、地区、配置、文件、支付、日志、任务及 LLM 操作已声明应用接口并绑定实现；两个入口的业务和 LLM 工厂返回动态契约，处理器不再引用具体服务。任务配置与队列管理分别定义接口，事务入队保留在内部。服务工厂宏支持显式契约返回，保留实际宿主资源借用生命周期。
- 依赖方向调整为 `application_interface -> application_model`；操作者身份、权限同步项和 LLM 调用数据归属模型层，模型层不再反向依赖接口层。接口操作不暴露持久化实体、连接、事务或 HTTP 上下文；当前方法签名覆盖已有 WebAPI 调用，仍须结合逐项业务核验补齐 NetEngine 的行为及 DTO 差异。
- 统一任务生产者，复用显式事务入队与独立入队，统一名称、回调参数和计划时间校验。
- 注册迁移 `202611070001`，对齐有效任务配置唯一约束和审计用户外键边界。已在后续数据工具阶段应用到独立临时空库，尚未对已有业务数据库执行。
- 日志使用独立写连接池及只读查询服务，日志 SQL 自身的诊断关闭；WebAPI 健康检查覆盖业务读写库与日志写库，只有管理端检查日志读库。
- 任务结果经过配置的日志组件写入，日志查询与任务表操作分离；文件日志模式不创建数据库日志写连接。
- 日志落盘目录按数据库目标和项目隔离；同批 ID 去重并事务提交，损坏文件保留为 `.rejected`，清理只作用于当前项目且处理极大保留天数。
- 迁移工具使用专用配置，支持 business/log 目标及独立迁移历史；通用迁移运行器使用事务和数据库锁，避免部分迁移或并发执行。未连接或修改实际业务数据库。
- 新增 EventBus 公共契约、Redis Streams 后端和独立 PostgreSQL 后端；各进程独立游标、启动跳过历史、断线续读、按顺序派发、失败监听单独重试及到期取消已实现。
- 三个宿主由运行入口选择事件后端，Feature 在后台消费启动前注册监听；应用层通过发布器契约使用广播。新增稳定事件协议名派生宏、事件目标健康检查和 `event-bus` 独立迁移目标。
- 事件后台循环异常会通知宿主停止；正常停止支持 Ctrl+C 与容器 SIGTERM。尚未执行实际跨进程运行验证。
- 雪花时间位布局已集中，支持带符号的时间边界和溢出检查；分区声明派生宏校验单列预分配 i64 主键，日志模型声明按月分区。
- 日志迁移生成分区父表及当前一个初始子分区；三个宿主的数据库日志启动时维护当前和三个未来分区，并每十分钟重试。维护带锁续期、32 个创建上限、实际目录与空洞校验、长名称截短及同名对象检查，不自动转换或删除已有分区。
- 29 个业务、任务和日志实体已映射 xmin，现有单行写入改用公共版本保存入口：更新/删除校验原始版本，新建在同一事务内批量回读真实版本，失败回滚事务或保存点；建表语句排除系统列。
- Rust 新建操作统一采用事务回读，因此不需要 EF 特定的分区快照命令切换。普通表和已有分区表使用相同版本路径；后台维护仍只处理显式声明的表。后续数据工具阶段已在独立临时库完成日志初始分区 DDL，维护及并发写入仍需运行核验。
- 日志采用批量保存与版本回读；任务完成时在续租结束后读取当前所有权/租约/版本，避免使用领取时已过期的 xmin。更新时间钩子保留调用方显式设置。
- 对照当前 NetEngine 的 RedisNodeLease，ID 节点已从业务缓存占位改为独立 Redis 原子租约：同协议键、所有者确认、立即续期、保守有效期、失效停发和退出保留占用。固定编号参数成对校验，取消序列等待不会重用已发序列；全部组件共享宿主生成器。

- 数据库日志新增双队列与字节预算、重要日志预留、后台稳定 ID、退避重试和限时排空；失败临时文件保留并独占恢复，严格校验批次命名，重名不覆盖。入库支持 BOM/CR/LF/CRLF 与无效 UTF-8 替换，有积压时连续消费，活跃项目日志保留清理和六阶段故障诊断已落地。
- 日志只读查询改用独立结果契约，包含检索转义 SQL 通配符；积压目标使用驱动实际解析的连接身份，密码与证书轮换不改变目录。新增拉取式内存/磁盘诊断快照；尚未接入标准指标采集器。
- WebAPI 正常停机、事件关闭和数据库日志排空共享 30 秒预算，端口绑定失败及任务菜单退出/失败也执行日志排空；事件关闭被取消时保留任务句柄以便释放时终止。任务轮询中断已在后续阶段处理；全部宿主启动失败路径仍待后续生命周期审查。

- 本地文件提供者已改为独立双队列与字节预算、重要日志预留、限时排空、按连续小时段确认和退避重试。失败段按原偏移重写，启动及追加时保留未完成尾部，清理按最后写入时间并跳过待重试文件；不再使用同步线程退出等待。
- 新增 Critical 宏与独立级别过滤，控制台和两个文件提供者均可区分 Critical/Error，None 禁用事件但保留 span 上下文。数据库文件对齐驼峰字段、字符串 ID、秒级偏移时间、删除时间和行版本读取；字段名不区分大小写。本地文件对齐 category/level/content 格式，日志根目录统一为 Logs。

- 队列持久化集中到 QueueTaskStore 契约及数据库实现，领取使用原始 xmin，开始时增加执行次数；候选按次数、最近执行和创建时间排序，遵循一秒创建间隔和 5×次数分钟退避。最后一次执行中断的过期租约收敛为 Failed。
- 队列完成通过数据库行锁和同一事务提交成功、子树完成及回调；父任务自身未成功时不产生回调，重复完成不重复入队。失败不改计划时间，返回值可作为未显式指定的回调参数。
- 队列与定时执行改为独立调度与受宿主管理的并行任务集合，队列按已启用配置和剩余并发容量领取，工作者使用随机 UUID 标识。并发锁丢失后继续独立数据库续租；停机先停止领取，在共享 30 秒预算中等待执行、事件和日志收尾。
- 定时触发移除跨实例锁，补齐本进程 skip_if_running、配置实例名称、本地时区、初次五秒等待与 256 毫秒迟到窗口；新定义默认禁用，缺少实例 Cron 时回退声明默认值。
- 公共 Cron 组件支持秒字段、日/星期互斥、L/W/#、月末偏移、名称、跨周期范围、星期名称间隔及年份；注册、配置保存和调度共用解析器，移除原 cron 依赖。创建带参实例时保留空白 Cron，避免复制模板值覆盖运行时声明默认值；特殊时区和边界日期仍需实际运行核验。
- 任务管理集中到 `QueueTaskManagement`，应用仅保留 DTO 与审计装配。重试在事务行锁内拒绝成功记录和有效租约；配置修改保护定义、模板、实例身份，带参参数统一 JSON 校验，模板名称去重排序，查询按原文处理通配符并限制分页。配置与业务审计共用事务；未对实际数据库执行并发验证。
- 任务注册和生成宏改用每次执行的工厂，处理器支持异步 close；队列作用域覆盖执行、事务完成和释放，构造/执行/释放 panic 均进入错误处理。queue_action 适配可序列化返回值，完成事务确认所有权及回调配置后才序列化；示例已使用 JSON 参数和返回值回调，短信为子任务、电话独立入队，ShowTime 每轮投递 17 条。
- 任务配置通过存储契约读取，Release 每 60 秒同步内存快照并补齐定义；利用有效定义唯一约束及失败后重新查询处理并发初始化。Debug 不保存配置，菜单与事件订阅和已选任务并行，选择仅在当前进程有效；带参实例支持动态启用、禁用和删除。

- 新增独立 `deployment` 工作区工具，读取统一配置及模板，生成两个 API 的 Nginx 配置、三个 systemd 服务、启停命令、共享/业务连接环境变量示例及三份云效流水线。排除 Admin.App；生成前校验参数和占位符，持有目录锁，全部写好后再替换旧结果，替换失败恢复备份。
- 发布脚本在容器构建 Linux x64 Release 包，保留宿主默认配置路径和可选 Production 配置，并记录构建环境与动态库要求。Docker artifact 阶段同步包含配置，日志目录统一为 Logs。服务使用现有 APP 环境配置入口；流水线在 rust:latest 中构建，更新发布文件并保留日志、上传数据和外部环境文件，不执行数据库迁移。

- 数据工具新增 `json-index`，从三个目标的显式模型元数据读取所属表，在只读事务中按 search_path 解析 schema，比较工具保留索引并输出 SQL。支持复合字段、方向、唯一性、JSON 名称映射、标量类型转换与分区键校验；当前 NetEngine 业务模型未声明 JSON 索引，Rust 亦保持空声明，不添加业务索引来替代验证。
- 工具无参数只显示帮助；默认配置随构建复制到可执行文件旁，保留已有显式路径与环境覆盖。在独立临时 PostgreSQL 容器上完成业务四项迁移、日志分区初始迁移、事件迁移，确认三个独立历史表和当前月份日志分区；从工作区外运行三个目标的索引差异命令均成功且输出为空。未操作已有业务数据库。

权限初始化阶段已补齐管理端 `POST /Authorize/InitData`、应用服务契约及单一事务写入。初始化文件与当前 NetEngine 原文件一致，构建和管理端发布包复制到可执行文件旁；恢复管理员、角色、功能、路由与授权，重复执行不重复创建记录。权限默认模块名及无前导斜杠路由键按 .NET 宿主对齐，移除启动时自动同步权限表的行为，改为显式调用路由同步入口。

在独立临时 PostgreSQL 上实际验证首次/重复初始化、软删除恢复、管理员登录与功能列表、已绑定路由拒绝和恢复授权，以及错误父功能引用后的六表事务回滚。Development 与 Production 实例均正常启动，Production 初始化返回拒绝；错误初始化前后的六表摘要相同。未操作用户已有数据库。这些结果不覆盖全部业务 API、并发初始化或 Release 签名流程。

基础接口阶段新增与 NetEngine `IBaseService` 对应的基础契约和应用实现，两个宿主通过工厂调用，地区和字典读取独立只读连接，移除这两类接口原先使用的地区结果缓存和写库路径。图形验证码从授权接口移回基础服务；地区 ID、叶子 null、空父节点数组与雪花 ID 的 JSON 字符串输出已对齐，字典重复键改为失败，省略分组参数按 0 处理。二维码使用 L 纠错和固定 500×500 输出。

在独立临时 PostgreSQL 的主库和读库分离配置下实际启动两个宿主：主库无地区数据，两个宿主均返回读库数据；已验证省/市/区筛选、城市优先、空结果、完整树、软删除过滤、读取修改后的数据、字典重复键失败及恢复、JSON 内容类型、二维码尺寸。客户端验证码已确认 PNG、缺省/有效/无效 GUID 分支，以及独立临时 Redis 中四位内容和刷新后 300 秒 TTL。未运行已有业务库，不添加或运行测试代码。通用请求绑定的大小写、空值与所有模型的长整数 JSON 规则仍需统一复核。

请求绑定阶段新增共享输入适配：按强类型 DTO 字段进行 ASCII 大小写匹配，JSON 保留原始字段顺序并以最后赋值为准，查询标量重复参数取首值；支持数字字符串及目标范围检查，查询布尔文本忽略大小写，JSON 布尔保持类型检查。字典键和动态 JSON 内容不改名，输入错误统一返回 400，保留 Axum 媒体类型与请求大小检查。46 个实际入站 DTO 可空标量字段已分别接入 JSON/Query 转换器，空白按 null 处理；必填字符串空白拒绝，数组和对象不使用标量转换器。

独立临时 PostgreSQL 上实际验证混合大小写登录、相同/异大小写重复 JSON 字段的最后值语义、查询重复参数首值、数字越界、带引号数字、分页参数、查询布尔、错误类型、空白可空筛选参数、日期参数与必填密码、空白备注落库为 NULL、数组不能绑定备注，以及 415 媒体类型拒绝。没有调用外部 LLM 服务。泛型集合参数、嵌套 DTO 和所有接口模型的完整输入组合仍需继续逐项核验，不能以这些请求验证替代全接口审计。

## 用户角色返回与分页语义

对照当前 `UserService`、`RoleService`、相关 DTO 及 `LongConverter`，已调整用户详情与列表的角色表示：详情使用逗号分隔且 `roleIds` 为 null，列表使用顿号并提供角色 ID 数组，无有效角色时 `roles` 为空字符串。列表按有效关联记录映射角色，保留重复关联；获取不存在的用户返回 null。创建用户和角色返回 application/json 的字符串 ID。

用户编辑要求至少一个角色，省略与空数组均返回 400；密码空白也不能通过服务校验。用户关键词按大小写敏感的字面子串匹配，SQL LIKE 的百分号、下划线及反斜杠经过转义。用户名锁不再裁剪空白，与保存的用户名保持一致。创建和更新角色关联时，ID 生成失败必须退出事务，不能跳过记录后提交；更新按请求中角色出现次数逐项保留、删除与新增。

共享分页模型允许可空列表。已核对的用户、角色、分类、管理端文章、链接及 LLM 模型/应用/会话查询在总数为零时返回 null，总数非零但本页没有数据时返回空数组。任务配置、队列与日志服务始终映射组件结果，保持空数组；其余未逐项核对的分页入口保留既有行为。

新增业务迁移 `202611070002`，移除 Rust 额外施加的用户角色组合唯一性，保留普通查询索引；NetEngine 当前 `UserRole` 模型没有该唯一约束。重复运行迁移时实际发现历史列 `Version` 与查询结果字段映射不一致，已通过明确查询别名修复公共迁移读取。

隔离 PostgreSQL 与实际管理端已验证：详情/列表角色分隔符和 null、无角色时空字符串/空数组、缺失及空角色请求、通配符字面搜索与大小写、重复角色创建及增减、无效角色导致用户创建/更新事务回滚，以及两个创建接口的 JSON 字符串 ID。用户、分类、文章、链接及 LLM 空列表和任务空数组均实际调用确认；角色非空总数的越界页为空数组。迁移验证包括已有四版本的库追加第五版本、全新库执行五版本及重复执行，业务/日志/事件三目标再次执行均成功。ID 分配故障未单独注入，角色与权限操作的全部语义仍需继续审计。

## 权限树、分配与即时授权读取

用户/角色权限树 DTO 已区分模块数组与功能叶节点的 null 集合；用户角色、功能树查询移除源服务不存在的目标用户预检。用户角色及功能取消操作在无关联时返回成功，写入仍由数据库外键保证关联目标存在。用户功能分配与 NetEngine 相同，先查询直接或继承授权中的首条记录，再决定新增、单条软删除或拒绝移除继承权限；不再优先删除全部直接授权。用户权限树在遍历前读取一次角色集合，递归使用同一份成员关系。

对照当前 `AuthorizeService.CheckFunctionAuthorizeAsync` 与 `GetFunctionListAsync`，删除应用服务内部的路由授权/功能列表结果缓存、版本快照、失效通知及仅为失效收集用户的额外查询；接口声明的响应缓存单独按 Controller 处理。路由权限直接检查当前有效授权记录，不额外增加源代码没有的 Function 软删除预检；功能列表的关联查询仍过滤已删除功能。功能标记不再裁剪首尾空白。用户名和角色编码占用、角色仍关联人员的业务异常改为 HTTP 400，并更新接口说明；角色编码锁使用组件默认过期时间。

隔离数据库的实际管理端调用已验证三层模块递归、叶节点 null、无目标用户的可选项、取消不存在关系、直接授权增删、继承授权拒绝单独取消，以及角色删除前的有效关联检查。对同一用户令牌先访问绑定路由，再从数据库直接撤销授权，下一请求立即 401 且功能列表立即为空；恢复授权立即 200。功能软删除时，路由按仍有效的授权记录判定，功能列表过滤该功能，与当前源服务的两种查询保持一致；无令牌仍 401。带空白的非空 sign 不匹配原标记。上述业务冲突返回 400 已实际确认。

用户删除与令牌生命周期、功能名称中重复父名称的 Replace 语义、其他授权接口及并发分配仍需继续核对，不能将本阶段视为全部鉴权完成。

## JWT、续签与令牌记录生命周期

对照两端 `PermissionService`、共享 JwtBearer 配置与 `AuthorizeService`，JWT 声明改为 `userId`、`tokenId`；移除 Rust 额外添加的逐请求 UserToken 状态检查、状态缓存及其宿主/应用契约。签名、issuer、audience、exp 验证保留，nbf 验证显式启用，并使用 IdentityModel 默认 300 秒时钟容差和有效时间顺序检查。用户名登录按原值匹配，错误凭据返回源服务的 400 与提示。

续签移至权限检查及业务处理之前；使用源服务的锁与缓存键、默认锁过期时间、十分钟结果缓存。新令牌通过 `NewToken` 返回并公开给跨域客户端，权限拒绝改为 403 且保留已生成的续签头。续签、清理或缓存写入失败不再吞掉。首次续签时按源模型物理清理七天前的有效 Token 记录，不添加每小时节流或额外续签审计步骤。

旧密码改密和用户删除移除附加的令牌撤销。短信改密的密码更新、有效令牌记录物理删除和审计在同一事务完成；验证码不额外消费。源项目未将令牌历史记录作为 JWT 有效性门槛，因此删除历史记录后，仍在有效范围内的 JWT 继续经过正常权限和业务检查。

独立 PostgreSQL、Redis 以及实际两端宿主验证：登录声明名称、两端复用续签结果、NewToken/CORS 响应头、未来生效及过期的容差内外请求、反向有效区间、错误签名/issuer/audience。24 个并发跨宿主请求只生成一条新续签记录，未取得锁的请求不返回新令牌；绑定路由的无权限请求为 403，且取得续签结果时仍带 NewToken。实际检查了缓存约 600 秒 TTL、LastId、七天清理保留新记录及已有软删除记录。

旧密码改密后令牌记录数量不变；短信改密清理有效记录且同一验证码可重复使用；原 JWT 仍能通过认证。通过隔离库中临时外键依赖使令牌清理失败，确认密码变更及令牌删除一起回滚。用户删除后令牌记录数量不变、用户查询返回 null。验证短信操作只入队，没有启动短信消费者或发送外部短信。所有临时数据均属于本轮隔离服务。

标准认证挑战头/错误体、所有受保护接口的 OpenAPI 403 声明、源码中的其他认证入口与完整签名中间件顺序仍需继续逐项核对；不能以这些结果认定所有认证能力已完成。前一阶段记录的权限拒绝 401 已由本阶段校正为源宿主的 403。

## 请求签名与接口文档复核

对照当前 `SignVerifyFilter`、`HttpContextExtension.GetRequestBody` 和公共中间件注册，管理端受保护路由改为先认证、续签和权限检查，再进入签名中间件。时间只判断原时间加三分钟是否大于当前时间，保留签名原文中的 Time 文本；补上 DateTimeOffset 有效范围检查并避免极端整数溢出。表单媒体类型按完整类型、不区分 ASCII 大小写识别；重复 ASCII 字段按首次名称归并，值以逗号连接。普通正文补齐 gzip 解压及 UTF-8、UTF-16、UTF-32 BOM 解码，后续处理器仍收到原始正文。

使用独立 PostgreSQL、Redis 和实际 Release 管理端验证了普通文本、未来时间、过期时间、五种 BOM、gzip 及 gzip+BOM、重复 URL 表单字段、重复 multipart 字段及多文件摘要；有效请求返回 200，过期请求返回 401。实际创建角色验证了签名层读取后 JSON 正文仍能进入业务处理。已绑定且没有权限的路由在缺失或错误签名时均先返回空体 403；需要续签且签名错误的请求返回 401 和 NewToken，跨域响应公开该头，正确签名后返回 200。

共享 OpenAPI 为明确要求 Bearer 的操作补齐 403 与 NewToken 响应头说明。实际运行暴露出原有整份文档 JSON 回读失败会丢弃增强结果的问题，现改为直接遍历 OpenAPI 模型，保留引用及未声明正文的响应，并保留枚举增强能力。管理端实际输出的 60 个、客户端 16 个受保护操作均包含 403 和续签头，匿名登录未增加权限拒绝声明。当前管理端文档没有 enum schema，因此运行结果不能证明枚举名称、成员说明和 Flags 已与 NetEngine 对齐。最终 Release 二进制再次确认 UTF-16/32 正文及不完整尾部的替换解码返回 200，篡改正文返回 401。

仍需继续核对：标准认证挑战头与错误体、签名解析异常在 Development/Production 的响应、Unicode 表单键归并和区域性排序、编码路径语义、模型绑定与各 Action 过滤器的完整先后顺序、请求大小限制，以及其他认证入口。本阶段不代表认证与 WebAPI 全部能力完成。

## 认证挑战、异常响应与运行环境复核

对照当前 NetEngine 的 JwtBearer 注册、PermissionService、ClaimsPrincipalExtension、GlobalError 和公共中间件，以及 [ASP.NET Core 10 JwtBearerHandler](https://github.com/dotnet/aspnetcore/blob/release/10.0/src/Security/Authentication/JwtBearer/src/JwtBearerHandler.cs)，JWT 失败改为区分没有凭据与校验失败的空体 401，输出 Bearer 挑战及适用的 invalid_token 描述。签名、颁发者、受众和有效时间验证保留；接收模型不再强制等同于签发 DTO，支持无 iat、数组 aud 和数字业务 ID。业务身份缺失与 JWT 校验失败分开处理，issuer 数组不接受。

内部异常统一经过共享响应，Development 保留 500 诊断信息，其他环境返回 400 与通用 errMsg。按照 [ExceptionHandlerMiddleware](https://github.com/dotnet/aspnetcore/blob/release/10.0/src/Middleware/Diagnostics/src/ExceptionHandler/ExceptionHandlerMiddlewareImpl.cs) 的清理行为，生产异常响应设置 no-cache/no-store、Pragma 和 Expires，不带此前生成的续签头；不复制源日志中的完整认证头与请求正文。请求 future 的 panic 添加共享捕获边界，缓存响应读取失败和文件响应构建失败也复用统一错误处理。

签名过滤器合并重复 Token/Time/Authorization 请求头；缺失或非法时间、无法解析的 JWT、文本解压和表单解析失败改为内部异常，缺少签名摘要则进入正常摘要比较。匿名签名接口只解析 JWT、不附加认证；正常签名 401 不添加 WWW-Authenticate。

环境名在启动时固定，依次采用 APP_ENVIRONMENT、DOTNET_ENVIRONMENT、ASPNETCORE_ENVIRONMENT，默认 Production。配置覆盖、Development 初始化入口和 Swagger 使用同一判断。Swagger 改为源项目的 /swagger 与 /swagger/v1/swagger.json，非 Development 不注册。环境变量优先级参照 [WebApplication 环境说明](https://learn.microsoft.com/en-us/aspnet/core/fundamentals/environments?view=aspnetcore-10.0)，保留 Rust 的 APP_ENVIRONMENT 显式入口。

实际运行隔离库、缓存和三个宿主：开发与生产管理端使用同一 Release 程序，生产客户端使用 Debug 程序，确认环境行为独立于编译配置。验证了缺失/空凭据、非 Bearer、损坏 JWT、错误签名、issuer/audience、缺失标准声明、过期/未生效/反向时间区间及三种可接受的声明形态。生产环境缺失业务声明返回通用 400。签名接口验证了非法时间、极端整数、重复头、非法 JWT、gzip 失败，以及匿名接口的原始 JWT 和无签名 JWT 解析行为。

隔离库临时重命名 User 表后，两端生产登录返回通用 400、开发登录返回 500 诊断信息；随后恢复原表。实际续签记录存在时，内部异常不返回 NewToken，而签名摘要错误、业务校验错误和空体 403 仍带续签头。管理端 60 个、客户端 16 个受保护操作的实际文档包含认证挑战头说明。实际验证 APP_ENVIRONMENT 优先、DOTNET_ENVIRONMENT 覆盖 ASPNETCORE_ENVIRONMENT、对应 Development TOML 加载，以及无环境变量时自动加载 Production TOML并关闭 Swagger。

仍需逐项核对：多项 JWT 声明同时错误时的优先级、NumericDate 和受众尾斜杠等边界、加密 JWT、Development 异常页面与内容协商、已开始输出的流式响应故障，以及完整 MVC 模型绑定/Action 过滤器顺序。panic 捕获仅完成编译检查，没有添加故意触发 panic 的接口或测试代码；不能将本轮结果视为所有异常路径已验证。

## 响应缓存与 ETag 复核

对照当前 `CacheDataFilter`、`ETagFilter`、`HttpContextExtension.GetParameters` 和两端 Controller 声明，客户端 `User/GetUser` 与 `Authorize/GetFunctionList` 使用 60 秒、包含完整 Authorization 的响应缓存；管理端用户查询移除源接口未声明的 ETag。认证、续签和权限守卫位于缓存外层，功能列表应用服务仍直接读取数据库。

缓存键包含独立处理器身份、HTTP 方法、PathBase、路径、路由参数和请求参数。处理器身份由注册处声明，不能依赖可配置的权限模块名。查询参数保留首次出现顺序，重复 ASCII 名称按首次名称归并并用逗号连接；非空 Content-Length 的 GET 正文也参与键计算。签名与缓存共用 gzip/BOM 文本解码，读取后保留原始正文；参数解析失败或 Query/Body 重名时跳过缓存并继续处理请求。

共享 JSON 响应保留一次序列化后的对象快照，缓存保存值与 HTTP 状态，不再依赖响应阶段是否已有 Content-Length，也不限制为 200；不保存 null、文件、流、异常结果及响应头。缓存损坏和读取/加锁故障回源，写入故障不覆盖业务响应。ETag 使用 GET 非空对象的 JSON 摘要，精确匹配单个 If-None-Match 时返回空体 304；当前源项目两端没有声明 ETag 的业务路由，保留显式挂载能力。

实际运行独立 PostgreSQL、Redis 和两端最新 Debug 程序，完成初始化与登录。两个客户端接口生成不同缓存项，TTL 为 60 秒；数据库修改后相同请求读取缓存，变化的参数或 Authorization 使用独立项。删除后返回 null、恢复后同请求返回对象，确认 null 不缓存。直接损坏独立缓存项后请求正常回源并修复；将已有缓存对象的状态改为 400 后，实际接口恢复 400。UTF-8 与 gzip+UTF-16 BOM 的等价 GET 正文命中相同参数键。管理端用户查询没有 ETag。最终程序重新预热缓存后，撤销绑定路由授权立即返回空体 403；功能列表同请求仍返回缓存内容，换参数则立即读取撤销后的空字典。

权限运行核验使用 `Client.WebAPI` 与无前导斜杠的实际路由身份；初始化文件含有旧模块名与带斜杠路由，已与当前 NetEngine 文件比较，完整 JSON 内容一致，没有擅自转换源数据。

本阶段仍不能证明全部 MVC 过滤器语义一致：处理器身份采用 Rust 模块身份，缓存键不是与 .NET 逐字节通用；Unicode 参数归并与区域性排序、编码路径、无 Content-Length 正文、字符串对象内容协商、ProblemDetails、取消/断线及中间件组合顺序还需继续核验。缓存 400 的运行检查验证了恢复路径，未证明现有业务端点会写入 400 对象；ETag 泛用能力完成编译和源码复核，未添加专用接口进行运行验证。

## 请求队列与锁释放复核

对照当前 `QueueLimitFilter` 和六处 Controller 声明，管理端 SetUserRole、SetUserFunction 保持等待模式；管理端 CreateRole、UpdateRole 与客户端两种修改密码接口保持按 Authorization 分桶的立即阻断模式。阻断返回 400 与 `请勿频繁操作`。锁键改为独立处理器身份、可选完整 Authorization、可选原始请求参数 JSON 的 MD5，不再以 HTTP 方法/路径和排序后的查询串生成 SHA-256。缓存与队列共用请求参数解析，保留正文供后续处理器读取。

正数 `expiry_seconds` 使用从获取锁时开始的固定租约，并留给后端自然到期；0 使用 60 秒租约，随请求完成释放，不自动续期。删除请求完成后再启动持有计时任务及 ManuallyDrop。LockGuard 提供等待释放完成与保留至自然到期两种操作；Redis 的主动释放和 Drop 共用一次启动的所有权校验任务，正常等待完成后再返回，取消等待不会取消已派发的解锁。Redis 解锁错误记录日志并保留业务结果。缓存正常完成也等待释放其防击穿锁。

实际运行独立 PostgreSQL、Redis 与两端 Debug 宿主，逐个占用六个实际接口对应的 Redis 槽位：四个阻断接口均立即返回 400 和指定消息；两个等待接口在预置的 1.2 秒租约到期后继续执行并返回 200。移除槽位后，四个阻断接口正常进入业务校验，不再返回重复请求消息；正常请求返回时槽位均已释放。共享参数提取调整后，客户端用户查询仍实际写入对象响应缓存。对独立 Redis 暂时拒绝 SET、EVAL/EVALSHA，分别确认加锁失败跳过限制、解锁失败保留业务响应，两个故障均产生对应日志；随后恢复权限。

仍需核验：当前业务声明没有正数 Expiry 或 IsUseParameter，相关通用分支只完成实现与编译检查，未添加专用业务端点或测试代码；请求取消、panic、长操作超过租约、多进程竞争，以及 MVC 模型绑定先于 Action 过滤器的执行顺序还没有完整运行证据。Rust 的处理器身份不与 .NET DisplayName 字符串逐字节相同。Unicode 参数与区域性排序沿用上一阶段未完成项，不能将本轮结果视为全部过滤器已经一致。

## RSA 字段解密能力复核

当前 NetEngine 提供 `RSADecryptFilter`、`RSAEncryptedAttribute` 和 RSA 配置模型，但全仓库没有 Controller 使用该过滤器，也没有请求字段实际声明该属性。Rust 补齐对应的按需能力，不将解密强制挂载到已有登录或其他业务接口。

`source_generator_runtime::RsaDecrypt` 契约与同名 derive 遍历请求模型，`#[rsa_encrypted]` 限定 String 或 Option<String> 字段；嵌套模型、可选对象、Box、列表、数组及字典值递归处理，未标记字符串和字典键保持原样。配置与密钥不进入模型层，公共宿主通过可选 `[rsa]` 配置提供私钥；`RsaJson`、`RsaQuery` 沿用原请求绑定，再执行解密。其他绑定方式可显式调用共享 `decrypt_request`。

解密使用 Base64 与 OAEP-SHA256。私钥缺失转为共享内部异常；Debug 构建中的非法密文只记录固定消息并保留原值，Release 构建停止处理并返回内部异常，由运行环境决定外部错误响应。Common RSA 私钥导入补上无换行 PEM 和 Base64 空白处理，RSA 解密后的 UTF-8 非法序列改用替换字符，与源 CryptoHelper 的 Encoding.UTF8.GetString 一致。没有输出或新增任何真实密钥。

用法见 [RSA 请求字段解密](webapi-rsa.md)。验证限于源码复核、工作区构建与静态检查；源项目没有实际使用入口，本轮没有新增专用接口、测试代码或临时 Rust 校验程序。新 derive 尚未由实际业务 DTO 实例化，不能以过程宏 crate 编译通过证明所有生成结果正确；嵌套模型的端到端解密、错误响应及新密钥解析分支仍缺实际运行证据。MVC Action 过滤器与 Rust 参数提取器在缓存/排队组合中的执行顺序也需继续核对。

## CORS 与公共管线复核

对照当前 `WebApplicationBuilderExtension` 的白名单配置，以及 [ASP.NET Core 10 CorsService](https://github.com/dotnet/aspnetcore/blob/release/10.0/src/Middleware/CORS/src/Infrastructure/CorsService.cs) 和 [CorsMiddleware](https://github.com/dotnet/aspnetcore/blob/release/10.0/src/Middleware/CORS/src/Infrastructure/CorsMiddleware.cs)，公共宿主改为只有 Origin 与预检方法头同时存在的 OPTIONS 才提前返回空体 204。允许来源回传请求方法、请求头、凭据与 7200 秒缓存时间；拒绝来源不添加 CORS 响应头，普通 OPTIONS 继续路由。

修正 ASCII 通配域名大小写匹配，保留通配子域不含根域、可选协议与端口限制；避免按字节截取非 ASCII 白名单导致 panic。缺少配置时默认 `*.xxx.com`，宿主已有显式开发配置不变。不再全局公开 Content-Disposition、Content-Length、ETag、NewToken 或声明私有网络许可；认证仍在实际续签时公开 NewToken。静态资源纳入公共 CORS，Swagger GET 按源管线顺序提前完成，Swagger 的预检仍进入 CORS。

独立 PostgreSQL 与实际两端 Debug 宿主验证了：通配配置/来源大小写、根域和伪装后缀拒绝、协议/端口允许与拒绝、任意 PROPFIND 预检方法、请求头回传、无 Origin、非法 Origin、普通 OPTIONS、受保护接口及不存在路由的预检。允许预检返回 204、空体及预期响应头，拒绝预检返回 204 且无许可头；普通 OPTIONS 返回路由 405。两端实际静态文件返回正确跨域头，Swagger 文档 GET 无跨域头、文档预检有跨域头。未配置实际续签令牌复核 NewToken，本轮只确认中间件保留功能层已有响应头。

上述验证只覆盖 CORS 范围；生产响应压缩、代理头与 HTTPS 策略在后续节记录。静态目录范围、内容类型、条件请求及请求地址工具的全部边界仍需逐项对照。URL/Uri 在国际化域名、非 HTTP 方案、重复或带引号头等边界上的解析差异尚未全部核验，不能以这些请求结果证明整个 WebAPI 管线已一致。

## 生产响应压缩复核

对照当前 NetEngine 的 `AddResponseCompression`、`UseResponseCompression` 和 [ASP.NET Core 10 压缩提供者](https://github.com/dotnet/aspnetcore/blob/release/10.0/src/Middleware/ResponseCompression/src/ResponseCompressionProvider.cs)，两个 WebAPI 共用非 Development 压缩层。使用默认文本、JSON、XML、JavaScript、WebAssembly 类型；已有 Content-Encoding 或 Content-Range 时跳过，不加最小正文大小限制。协商按质量选择，同质量依次优先 br、gzip、identity；保留首个正质量候选及源实现的通配符处理顺序，全部禁用或格式不支持时保持原状态码与正文，不改为 406。

正文交给 tower-http 流式编码，未增加整段缓冲；Brotli/gzip 使用质量 1，对应源提供者 Fastest。对符合类型且携带非空协商头的响应追加 Vary: Accept-Encoding，保留 CORS 的 Vary: Origin；压缩时移除 Content-Length、Content-MD5，保留 Accept-Ranges。[源响应包装器](https://github.com/dotnet/aspnetcore/blob/release/10.0/src/Middleware/ResponseCompression/src/ResponseCompressionBody.cs) 与压缩库的默认响应头策略不同，因此在共享层显式处理。

独立 PostgreSQL 与三个实际 Debug 二进制宿主（两端 Production、管理端 Development）完成 HTTP 核验：两种编码的五字节文本及 200 KB 文本解压后均与原文一致；重复协商头、不同质量、同质量、identity、零质量、通配符、重复格式及非法质量按预期选择。生产环境实际业务校验返回的 400 JSON 压缩，开发环境不压缩。JSON、WebAssembly 与健康接口响应压缩；图片、二进制、401、404、204 保持原样；范围请求 206 保留原字节与 Content-Range，条件请求 304 空体，HEAD 带编码头且不发送正文。压缩静态响应保留 Accept-Ranges，CORS 与编码 Vary 同时存在。

本阶段没有增加测试模块或伪造业务接口。已有编码、Content-MD5、SSE 类型排除以实现审查为依据，尚无相应实际业务响应的运行证据；长时间流式响应、取消、尾部字段及全部畸形头边界仍需继续核验。本机未安装组件，未重新生成 Release 发布包，不能据此宣称整个 WebAPI 或框架已对齐。

## 代理、HTTPS 与请求地址复核

对照当前 NetEngine 公共服务和管线以及 [ASP.NET Core 10 ForwardedHeadersMiddleware](https://github.com/dotnet/aspnetcore/blob/release/10.0/src/Middleware/HttpOverrides/src/ForwardedHeadersMiddleware.cs)，Rust 接入实际连接地址，统一 RequestContext。沿用源配置的 IPv4 全网段、IPv4 映射地址与 IPv6 回环信任范围，从最右侧消费一跳 For/Proto，保留原值和剩余链；未启用 Host/Prefix 转发。最右侧 IP 非法时本跳协议不生效；空引号值忽略，未闭合引号保留为非法值，不当作缺失值。客户端 IP 按当前 HttpContextExtension 的头优先级与连接回退处理，支付和可选限流共用该结果。

按照 [HttpsRedirectionMiddleware](https://github.com/dotnet/aspnetcore/blob/release/10.0/src/Middleware/HttpsPolicy/src/HttpsRedirectionMiddleware.cs) 和 [HstsMiddleware](https://github.com/dotnet/aspnetcore/blob/release/10.0/src/Middleware/HttpsPolicy/src/HstsMiddleware.cs)，已添加可配置外部 HTTPS 端口、307 跳转及一年期 HSTS。端口未确定继续 HTTP；已转发为 HTTPS 时不跳转。HSTS 排除默认本机主机，不附加 includeSubDomains/preload；全局异常清除该头，普通业务错误保留。Development 文档 GET 在该策略前结束，文档预检仍进入公共策略。压缩、异常、HTTPS、CORS 的共享层按相应顺序装配。

七个支付创建接口改用请求协议与 Host 构造通知地址，微信 H5 使用统一客户端 IP。支付宝小程序路径按当前源声明为 `/api/Pay/AliPayNotify`，其与源控制器直接路径的差异仍需结合真实代理部署核验。请求日志改为通过相同上下文记录完整 URL。请求协议以实际连接为依据，absolute-form 的请求行不能把明文连接改成 HTTPS。直接 TLS 监听在后续阶段补齐，见下一节。

独立数据库和五个实际宿主验证了配置端口 8443/443、ASPNETCORE_HTTPS_PORT 回退 4444、无端口继续 HTTP、IPv6 回环连接、代理协议大小写、最右侧选择、非法 IP/协议、IPv4/IPv6 带端口、完整/未闭合/空引号。实际响应证明 307 保留转义路径/查询串及 IPv6 Host、忽略转发 Host/Prefix；HTTPS 同时返回 HSTS/CORS/gzip，localhost 和 IPv6 回环 Host 排除 HSTS，业务 400 保留而全局内部错误 400 清除 HSTS。Swagger GET 不重定向且无 HSTS，预检执行重定向。可选限流实际验证单 IP、不同 IP、剩余代理链、original-forwarded-for 优先级及不读取 X-Real-IP。

在隔离库通过实际初始化与登录接口准备账号，并使用临时订单与本地 RSA 密钥调用支付宝 PC/H5：返回签名链接中的 notify_url 确认为请求的 HTTPS 域名/IPv6 和端口。只生成本地签名 URL，没有访问支付平台或发起交易。临时破坏隔离配置的签名密钥后，实际内部错误返回通用消息且无 HSTS。

直接 TLS 监听、证书配置与端口发现的后续实现见下一节；非回环 IPv6 来源、非标准 IP 文本、全部畸形 Host/头与协议边界、微信和支付宝小程序实际外部调用仍未验证。不能把本节视为整个网络栈或支付能力已对齐。

## 直接 TLS 与证书复核

对照 NetEngine 两端宿主的 SetKestrelConfig、Kestrel 证书配置与开发 HTTPS 入口，在共享宿主增加可选 `host.tls`：支持 PEM/CRT 证书链、单独或合并 PEM 私钥、加密 PEM 私钥、带密码的 PFX/PKCS#12。证书解析复用 OpenSSL，握手使用 rustls；OpenSSL 通过 vendored 依赖只在容器编译，不增加 Windows 组件或运行时动态 OpenSSL 依赖。无效证书、缺失私钥、错误密码和证书/私钥不匹配均终止启动。

采用 [Axum Listener](https://docs.rs/axum/0.8.9/axum/serve/trait.Listener.html) 将 HTTP/HTTPS 交给同一个服务与停机预算；握手并行处理，十秒超时，不占住其他连接的接受过程。每个连接提供真实地址与 TLS 标记，RequestContext 不从请求行推断协议。支持 TLS 1.2/1.3 和 ALPN HTTP/2、HTTP/1.1；未显式配置重定向端口时，在环境配置后读取已绑定的 HTTPS 实际端口。HTTP 地址为空可以只开启 HTTPS，部分监听绑定失败会释放已打开的监听器。

使用独立 PostgreSQL、临时 CA 与四个实际宿主，分别验证分离 PEM、PFX、加密私钥且仅 HTTPS、合并 PEM 四种配置。所有请求启用证书和主机校验；四个宿主的 TLS 1.2、TLS 1.3、HTTP/2 健康接口均返回 200，静态正文压缩后正确还原，HSTS/CORS 同时生效。HTTP 307 指向实际 HTTPS 端口，系统分配端口也正确发现。未发送 ClientHello 的连接不阻塞后续 TLS 请求，并在十秒后关闭；向 TLS 端口发送明文后，后续正常 TLS 请求仍成功。

实际启动错误密码 PFX、无密码加密 PEM、证书与私钥不匹配、缺失文件及 HTTPS 端口被占用的宿主，均非零退出并释放已绑定 HTTP 端口，输出不包含提供的密码。临时 CA 最初缺少客户端严格校验要求的 KeyUsage，已补全临时证书扩展后重新运行；没有关闭证书校验。

这仍不等同于完整 Kestrel 配置能力：多 HTTPS 端点、SNI 多证书、证书热加载、HTTP/3、客户端证书策略及开发证书自动发现尚未逐项实现或核验。当前证书仅在启动时加载，生产部署与证书轮换流程仍需进一步对照。

## 尚需继续实现或逐项核验

| 领域 | 当前已确认的差异 / 后续核验内容 |
| --- | --- |
| 公共数据库 | 统一连接装配、独立组件数据库选项、读写路由与健康检查覆盖 |
| 日志 | 读写、查询、目标隔离、迁移和分区维护已拆分；当前 NetEngine 要求共享宿主 ID，已遵循。缓冲、预留、重试、恢复、排空和查询过滤已实现，存储级别文本已对齐。Critical、文件格式与 LocalFile 写入恢复已补齐；标准拉取指标已接入；仍需实际采集端导出验证、启动失败生命周期和实际文件/数据库运行核验 |
| 任务 | 已实际核对缺参/显式 null、三次重试、租约状态控制、双 Worker 竞争领取，以及定时任务双实例独立触发、在线 Cron 修改、禁用与默认 Cron 恢复。仍需 Cron 特殊时区/边界日期、长执行续租与停机取消、任务管理并发及其余处理器生命周期核验 |
| WebAPI | 公共宿主业务依赖、应用契约调用及模型依赖方向已调整；Query 与 JSON 跨参数错误已统一汇总，认证、媒体类型与 Action 过滤器顺序已有实际对照。仍需一般畸形 JSON/转换器完整诊断、内容协商、扩展层级追踪和宿主生命周期等逐项核验 |
| 事件总线 | 公共运行时、Redis/数据库后端及宿主接入已落地；实际双宿主已核对 Redis 启动跳过历史、独立游标、无效记录与重连，以及数据库保留清理和软删除状态行。三宿主启动等待、后端恢复和取消已有运行证据，但锁等待中的数据库查询在宿主退出后仍保留到解除锁，服务器端查询取消尚未对齐；跨语言事件载荷、监听工厂/重试/取消及发布事务仍需实际调用核验 |
| PostgreSQL 分区 | UTC+8 时间边界、声明、初始迁移、日志定期维护和事务内 xmin 回读已实现；仍需逐项复核约束、失败/取消语义和数据库运行行为 |
| ID 生成 | 已对齐共享 Redis 节点租约、所有权确认、立即续期、失效保护和固定模式；仍需实际多进程、断线和节点耗尽场景核验 |
| 业务功能 | 管理端、客户端全部 API、DTO、授权、支付、站点、用户、文件、消息、LLM 与任务中心逐项比对 |
| 基础设施 | 缓存、锁、对象存储、短信、LLM、公共工具及源码生成器的全部功能核对 |
| 数据工具 | 三目标迁移、JSON 索引目录差异和随工具定位配置已实现并完成空库实际运行；权限初始化入口及文件已补齐并通过实际运行。仍需 JSON 复杂类型映射自动生成、非空索引声明与历史库差异核验；NetEngine 地区 XLSX 文件当前没有发现代码导入入口，需继续核对地区数据准备方式 |
| 部署 | 三个发布包已实际构建，管理端运行镜像、生成的 Nginx HTTPS 代理、客户端图形输出及三个宿主正常退出已有容器运行证据；仍需另外两个独立运行镜像、其他目标 Linux 运行库兼容、systemd 及云效平台上传下载和部署链路核验 |

## 验证约定

所有 Rust 格式化、检查和编译均在 Docker `dev` 服务中进行；不在 Windows 安装工具链或组件，不添加或运行测试。最终需要重新核对 NetEngine 工作区，再进行完整工作区编译检查。

2026-11-07 本阶段已在容器通过：`cargo check --workspace --all-features`、`cargo build --workspace --all-features --locked`、`cargo clippy --workspace --all-features --lib --bins --locked -- -D warnings` 和 `cargo fmt --all -- --check`。这些检查验证编译和静态诊断，不代表已验证数据库迁移或全部运行时行为。

部署阶段已实际运行生成器，从工作区根目录和 deployment 目录均能生成并替换 11 份文件；三个宿主均通过容器 Release 构建和发布脚本，随包默认配置与源码内容一致。VS Code 中保留的旧 Cargo 任务标签也已统一调用容器。尚未执行目标服务器服务启动或云效流水线，生成成功不等于已完成部署运行验证。

权限初始化阶段再次通过容器全工作区 check、build、Clippy（all-features、lib/bins、warnings 为错误）及格式检查。管理端 Release 发布包已重新生成，二进制与构建产物、初始化文件与源码均通过摘要核对；运行验证使用的宿主进程和临时 PostgreSQL 已停止并移除临时数据库容器。未重新构建 Docker runtime/artifact 镜像。

基础接口阶段通过容器全工作区 `check --all-features --locked`、`build --all-features --locked`、`clippy --all-features --lib --bins --locked -- -D warnings` 以及格式检查。两端临时宿主及本轮独立 PostgreSQL、Redis 容器已停止；未重新生成上一阶段的 Release 发布包。

请求绑定阶段通过相同范围的容器全工作区 check、build、Clippy 及格式检查。临时管理端进程和本轮 PostgreSQL 容器已停止，未修改已有业务数据库。Release 发布包仍为先前阶段产物。

用户角色与分页阶段通过容器全工作区 `check --all-features --locked`、`build --all-features --locked`、`clippy --all-features --lib --bins --locked -- -D warnings`、格式检查及差异空白检查。实际调用使用的临时管理端已停止，独立 PostgreSQL 容器与其中两个隔离数据库已移除；未重新生成 Release 发布包。

权限树与即时授权读取阶段再次通过相同范围的全工作区编译、检查、Clippy、格式及差异空白检查。临时宿主已停止，独立 PostgreSQL 容器已移除；Windows 本机未安装组件，未添加或运行测试代码，Release 发布包尚未重新生成。

JWT 与令牌生命周期阶段通过容器全工作区 `check --all-features --locked`、`build --all-features --locked`、`clippy --all-features --lib --bins --locked -- -D warnings`、格式及差异空白检查。两端临时宿主、独立 PostgreSQL 与 Redis 已停止并移除服务容器，临时密钥副本及令牌文件已删除；未修改既有业务数据库，也未重新生成 Release 发布包。

请求签名与接口文档阶段通过相同范围的容器全工作区 check、build、Clippy、格式和差异空白检查，并完成管理端 Release 编译及上述实际 HTTP 核验。临时两端宿主已停止，本阶段独立 PostgreSQL、Redis 容器已移除，令牌与临时密钥文件已删除。未安装本机组件、未添加或运行测试代码，未重新生成部署发布包。

认证挑战与环境阶段通过容器全工作区 `check --all-features --locked`、`build --all-features --locked`、`clippy --all-features --lib --bins --locked -- -D warnings`、格式和差异空白检查，并完成最新管理端 Release 编译。上述实际请求使用的三个临时宿主均已停止；独立 PostgreSQL、Redis 容器已移除，临时令牌、密钥副本和配置链接已删除。未操作已有业务数据库，未安装本机组件，未添加或运行测试代码，未重新生成部署发布包。

响应缓存与 ETag 阶段通过容器全工作区 `check --all-features --locked`、`build --all-features --locked`、`clippy --all-features --lib --bins --locked -- -D warnings`、格式及差异空白检查。两端临时宿主已停止，独立 PostgreSQL、Redis 容器已移除，临时令牌与响应快照已删除。未安装本机组件、未添加或运行测试代码；本阶段未重新生成 Release 发布包。

请求队列与锁释放阶段通过容器全工作区 `check --all-features --locked`、`build --all-features --locked`、`clippy --all-features --lib --bins --locked -- -D warnings`、格式及差异空白检查。锁组件新增复用已有 tracing 依赖，Cargo.lock 已同步，未安装本机组件。两端临时宿主与独立 PostgreSQL、Redis 均已停止，临时令牌已删除；未添加或运行测试代码，未重新生成 Release 发布包。

RSA 字段解密阶段通过容器全工作区 Debug check/build、Release check、Clippy（all-features、lib/bins、locked、warnings 为错误）、格式和差异空白检查。未启动临时业务服务，未安装本机组件，未添加或运行测试代码；未生成新的 Release 发布包。宏运行时复用已有 chrono 依赖，Cargo.lock 已同步。

CORS 与公共管线阶段通过容器全工作区 `check --all-features --locked`、`build --all-features --locked`、`clippy --all-features --lib --bins --locked -- -D warnings`、格式和差异空白检查。两端临时宿主已停止，独立 PostgreSQL 已移除，临时配置链接与静态样本已删除；未安装本机组件，未添加或运行测试代码，未重新生成 Release 发布包。

生产响应压缩阶段通过容器全工作区 `check --all-features --locked`、`build --all-features --locked`、`clippy --all-features --lib --bins --locked -- -D warnings`、格式及差异空白检查。三个临时宿主已停止，独立 PostgreSQL 容器已移除，临时静态文件已清理；未安装本机组件，未添加或运行测试代码，未重新生成 Release 发布包。

代理、HTTPS 与请求地址阶段通过容器全工作区 `check --all-features --locked`、`build --all-features --locked`、`clippy --all-features --lib --bins --locked -- -D warnings`、格式及差异空白检查。五个临时宿主已停止，独立 PostgreSQL 容器已移除，临时令牌、RSA 密钥、SQL 与静态文件已清理。未安装本机组件，未添加或运行测试代码，未重新生成 Release 发布包。

直接 TLS 阶段通过容器全工作区 `check --all-features --locked`、`build --all-features --locked`、`clippy --all-features --lib --bins --locked -- -D warnings`、格式及差异空白检查。默认纯 HTTP 宿主实际返回 200；持有未完成 TLS 握手和已完成请求的 TLS 长连接时，宿主在约 0.25 秒内退出。五个常驻临时宿主及五次启动失败实例均已停止，独立 PostgreSQL 已移除，临时 CA、证书、私钥和静态文件已清理。未安装本机组件，未添加或运行测试代码，未重新生成 Release 发布包。

## 健康检查与静态 Web 根目录

已重新核对两端 Program.cs、CacheHealthCheck、DatabaseHealthCheck 与发布器：健康端点改为全方法纯文本 Healthy/Unhealthy、200/503 和禁止缓存头；组件并行执行，缓存只写固定十秒空值，移除读取/删除和额外锁探测。两端均检查 Log.Write，文件日志另建惰性探测连接；Log.Read 仅管理端注册。后台延迟十秒、周期六十秒，一轮三十秒，停机取消并共享资源关闭预算。

隔离 PostgreSQL/Redis 与三个真实宿主验证：两端七种 HTTP 方法、HEAD 空体、缓存头、压缩/CORS/HSTS；禁止 Redis GET/DEL/EVAL/EVALSHA 仍健康，禁止 SET 返回 503。无 HTTP 请求时，启动约十秒和七十秒出现缓存探测键。暂停独立日志读库后，管理端及以该库作为 Log.Write 的客户端约三十秒返回 503，其他客户端立即返回 200；恢复后均恢复健康。数据库探测三十秒限制覆盖池获取和查询，尚未复现 Npgsql 自定义 CommandTimeout 与独立连接超时的全部组合。

静态文件改由管理端显式开放文件根目录，默认工作目录 wwwroot；客户端默认文件根目录为工作目录且不开放静态目录。移除按本地组件自动给两端挂载 /uploads 的做法。已注册 API 优先；GET/HEAD 提供已知类型文件，不自动返回目录首页。采用 ASP.NET Core 的 382 项 MIME 映射并保留 MIT 许可证；发行脚本和 Docker 包含对应许可文件。

参照 [StaticFileMiddleware](https://github.com/dotnet/aspnetcore/blob/release/10.0/src/Middleware/StaticFiles/src/StaticFileMiddleware.cs)、[StaticFileContext](https://github.com/dotnet/aspnetcore/blob/release/10.0/src/Middleware/StaticFiles/src/StaticFileContext.cs) 与 RangeHelper 增加秒级 FILETIME/长度 ETag、条件请求、单范围与 If-Range；文件正文继续由 Tower 流式读取。实际请求验证管理端公开/客户端 404、无首页、未知类型与点文件 404、API 优先、200/206/304/412/416、弱 ETag、日期条件、后缀范围、非法及多范围回退、HEAD 忽略范围，以及 gzip/CORS/HSTS 组合。HSTS 最初使用本机 Host 导致断言失败，已按源规则排除本机地址并用普通域名验证。

这不代表文件全功能对齐：畸形条件头解析、特殊路径/符号链接、文件变动期间的响应、云上传/访问权限及发布资产流程仍需继续核查；未生成新的 Release 包或重建生产镜像。公开资产由部署方提供，本阶段未引入 Admin.App。


健康检查与静态目录阶段已通过容器全工作区 `check --workspace --all-features --locked`、`build --workspace --all-features --locked`、`clippy --workspace --all-features --lib --bins --locked -- -D warnings`、格式检查、发布脚本语法和差异空白检查。MIME 表大小写问题已修复，最新二进制的 `.IVF` 请求返回正确类型。三个隔离宿主均正常退出，独立的两个 PostgreSQL 和 Redis 容器已移除，临时静态文件已清理。未添加或执行测试代码，未在 Windows 安装任何组件。

## 文件下载、图片与软删除

重新对照当前两端 FileController 和 Application.Service/Basic/FileService：删除仅更新 DeleteTime/DeleteUserId；移除 Rust 中同步删除物理文件或云对象的行为。GetFile/GetImage 只取本地文件，移除不存在时自动下载云对象到公开目录的分支。文件 URL 按配置原文与路径拼接，示例补齐末尾斜杠；存储对象与记录分别发号，上传保留原始文件名、业务名和 sign，列表按非 null sign 精确过滤。零/负 ID 按未找到记录处理。Development 文件根目录同步为管理端 wwwroot、客户端工作目录。

共享 file_response 提供流式附件下载，复用 MIME 表，普通文件未知类型回退 ZIP 映射 application/x-zip-compressed，图片未知类型进入内部异常；中文文件名提供 ASCII 回退和 UTF-8 filename*。参照 [PhysicalFileResultExecutor](https://github.com/dotnet/aspnetcore/blob/release/10.0/src/Mvc/Mvc.Core/src/Infrastructure/PhysicalFileResultExecutor.cs) 与 [FileResultHelper](https://github.com/dotnet/aspnetcore/blob/release/10.0/src/Shared/ResultsHelpers/FileResultHelper.cs) 提供修改日期和日期条件，不启用范围或 ETag。原图始终附件下载，缩放结果无附件名、固定 PNG；任一目标边超过原图时保留原文件，一边为零按 f32 比例截断，否则使用指定两边和最近邻。图片 CPU 处理放到阻塞任务，补启用 BMP/ICO/GIF/WebP 解码能力；并未证明与 Skia 所有编解码器逐像素等价。

两端上传返回 JSON 字符串 ID，文件字段名匹配忽略大小写。临时文件守卫覆盖表单读取失败、写入失败、业务失败与取消。隔离数据库迁移后，通过真实初始化/登录/上传/下载/删除接口验证，共 48 次请求通过：私有标志下匿名本地读取、中文附件名、304/412、Range/If-Match 不改变原文件响应、未知 MIME、独立存储与记录 ID、零 ID、软删除保留物理文件、重复删除 false，以及 BMP 原图和七组缩放尺寸/PNG 输出。主动断开两端 multipart 上传后临时目录为空。最初的空 sign 输入按必填校验被拒绝；另一次 BMP 请求暴露未启用解码器，已补齐功能后完整重跑。

剩余文件差异仍需推进：普通上传的 50 MiB 限制及 Release 请求签名的缓冲上限、远程下载限制和取消传播、两个云提供商实际访问行为、Ueditor 全部动作、特殊文件名/图片格式与 Skia 像素结果、并发文件变动时的响应。此次没有将这些未核验项标为完成。

文件接口阶段已在容器通过全工作区 check/build（all-features、locked）、严格 Clippy、格式和差异空白检查。最终复核将未知扩展名回退改为直接复用 ZIP 映射，避免误写为 application/zip；两端最新二进制确认返回 application/x-zip-compressed，Swagger 下载参数已无 isInline。两个临时宿主及隔离 PostgreSQL 已停止/移除，令牌、上传数据、运行日志与临时目录已清理。未添加或运行测试代码，未安装 Windows 组件，未重新生成 Release 包。

## 大文件上传与 Release 签名回放

对照当前 SignVerifyFilter、HttpContextExtension.GetRequestBody、EnableBuffering 及上传入口的 DisableRequestSizeLimit：新增共享 ReplayBody，小于等于 30 KiB 时使用内存，超出后使用匿名临时文件。临时目录遵守 ASPNETCORE_TEMP，文件所有权随回放流转移，读取失败、签名失败和取消都会关闭句柄。采用 [tempfile](https://docs.rs/tempfile/3.27.0/tempfile/fn.tempfile.html) 的 OS 回收语义；默认内存阈值参照 [EnableBuffering](https://github.com/dotnet/aspnetcore/blob/release/10.0/src/Http/Http/src/Extensions/HttpRequestRewindExtensions.cs)。multipart 文件使用增量 SHA-256，整体摘要也按段更新；文本/gzip 保留源完整字符串读取方式，不再额外使用签名 10 MiB 限制。

删除普通上传的 50 MiB 限制和对应 file.max_upload_bytes 配置；两端上传入口禁用默认 body 限制。Ueditor 同样禁用路由总大小限制，保留业务规则；确认源 UeditorController 仅 Authorize，不含 SignVerifyFilter，改用共享认证/权限守卫。同步 2,147,483,647 字节的配置业务上限及图片、文件扩展名列表；返回 JSON 文本的 Content-Type、成功 error:null 和可直接拼接配置前缀的相对地址。普通表单及 Ueditor multipart 临时文件由作用域守卫覆盖中断清理。

使用真实 Production 环境的 Release 两端宿主、独立 PostgreSQL 和实际登录账号验证。两端各上传 60 MiB 文件，下载长度及 SHA-256 一致。管理端上传过程中观察到指定目录的匿名文件句柄；本次 RSS 基线 24,872 KiB、峰值 31,840 KiB，增加 6,968 KiB，未按文件长度整包进入内存。额外验证无 Content-Length 的分块 multipart、重复普通字段、多文件摘要、超过旧上限的 12 MiB 文本与 gzip 解码文本、非法签名 401。请求体缓存与业务临时文件在完成后均释放。

真实 Ueditor 在 Release 无 Token/Time 时配置和 60 MiB 文件上传成功；匿名请求仍返回空体 401。配置大小及扩展名、文本 Content-Type、error:null 和 URL 前缀拼接通过检查。暂停发送请求后观察到缓存句柄，断开连接后句柄消失；暂时移走指定缓存目录时，小请求仍使用内存成功，大请求返回生产内部异常且没有回退其他目录。该隔离目录随后恢复。

验证范围仍有限：此次使用 60 MiB 样本，没有实际上传接近 2 GiB 的文件。非 multipart 文本/表单仍会完整解码，不能据此声称常量内存。普通表单的字段/数量/头限制、非上传路由的服务器默认上限、缓存/排队参数快照、Unicode 字段排序和畸形表单语义仍需核对。Ueditor Base64、远程抓图、错误响应完整字段与额外别名仍待继续对齐；云提供商和全部部署链路尚未完成核验。

大文件与签名阶段已通过容器全工作区 check、Debug build、Release build（均 all-features/locked）、严格 Clippy、格式及差异空白检查。Release 管理端动态库清单没有额外动态 OpenSSL 依赖。两个 Release 宿主、初始化用 Debug 宿主和隔离 PostgreSQL 均已停止/移除，临时令牌、上传文件、缓存目录与运行日志已清理。没有添加或运行测试代码，没有在 Windows 安装组件；未重新生成发布包或部署生产服务。

## UEditor 动作、表单与上传错误

重新核对当前 UeditorController、UploadHandler 与 CrawlerHandler：action 按原始查询集合的逗号连接结果精确分发，移除推断、裁剪和 crawlimage 别名。uploadKey 先按可空字符串绑定，再检查正整数；config 不受非法 uploadKey 影响。首次真实调用发现空 uploadKey 被共享必填字符串校验提前拒绝，已改用既有可空 Query 转换器并复核。

普通文件、涂鸦、抓图复用 UEditor 表单读取入口，支持 URL 编码与 multipart，字段名忽略 ASCII 大小写并保留重复值顺序。普通上传选择第一个匹配的文件字段，完整读取包括后续字段后才调用文件服务；普通文本字段不再被误当作默认名称文件。URL 编码按字段流式读取并使用默认 2,048 字节键、4 MiB 值和 1,024 项限制，multipart 普通字段不套用 URL 编码值上限，参照 [FormFeature](https://github.com/dotnet/aspnetcore/blob/release/10.0/src/Http/Http/src/Features/FormFeature.cs) 与 [FormPipeReader](https://github.com/dotnet/aspnetcore/blob/release/10.0/src/Http/WebUtilities/src/FormPipeReader.cs)。

涂鸦移除 undefined 字段回退和 data URL 剥离；重复值连接逗号后解码，忽略空格/Tab/CR/LF，并允许 .NET Base64 解码接受的非零未使用尾位。上传失败保留六个协议字段和 null，根据是否进入保存阶段决定 original/title；文件服务失败转换为协议错误，涂鸦临时文件也使用取消清理守卫。抓图表单移除额外 64 KiB/20 项限制，失败项保留 url:null/fileId:null，成功地址经过配置前缀归一化。

独立 PostgreSQL 迁移及真实管理端初始化/登录后，44 项实际 HTTP 核验通过：无效/空/溢出 uploadKey、显式和重复 action、字段大小写、Base64 空白与非法输入、data URL 拒绝、multipart 涂鸦、缺失/空/非法类型文件、首文件选择、残缺尾部拒绝、协议错误完整字段、URL 编码长度与数量限制、匿名 401、12 MiB Base64 multipart 对应 9 MiB 原文下载、25 个抓图来源不截断、真实保存失败保留原始文件名及上传断开后的临时文件清理。验证未使用伪造接口或新增测试文件。

仍未完成 UEditor 全量对齐：远程抓图仍复用普通下载服务，重定向、2 GiB 大小上限、状态文本、初始地址与 DNS 校验顺序、失败隔离和取消传播需要独立对齐。表单编码/Unicode、multipart 边界和头限制、临时缓冲位置及特殊文件名行为也尚未全部核验；未据此宣称功能全量一致。

UEditor 表单阶段已通过容器全工作区 check/build（all-features、locked）、严格 Clippy、格式及差异空白检查。仅为管理端引用工作区已有 futures-util，锁文件同步引用；初次 Cargo 在共享目录写锁文件失败，随后补齐既有包引用并通过 locked 检查。独立管理端和 PostgreSQL 已停止/移除，临时令牌、上传文件和运行日志已删除。未添加或运行测试代码，未安装本机组件；本阶段未重新生成 Release 发布包。

## UEditor 远程抓图与文件扩展名

当前 CrawlerHandler 的网络接入已独立映射到管理端 UEditor 处理器，保存仍通过文件应用契约；不再套用普通远程上传的 10 MiB 限制、状态文本或无重定向行为。初始地址先解析和检查全部公网 IP，再检查初始 URL 文件名；各次连接重新解析并固定已验证地址，禁用代理。响应只接受 200 和有效 image/ 媒体类型，声明长度与逐块接收长度均受 2,147,483,647 字节限制。保存及生成 URL 的异常只影响当前项，错误前缀、可空字段与配置前缀拼接保持协议语义。

共享 HTTP 工厂新增由调用方控制期限的流式创建入口，保留工厂连接池选项；普通客户端选项不变。抓图的最终响应头期限为 100 秒，正文不继承普通工厂的整包超时。重定向支持 300/301/302/303/307/308，最多五次，超限返回最后响应，HTTPS 不降级；参照 [RedirectHandler](https://github.com/dotnet/runtime/blob/release/10.0/src/libraries/System.Net.Http/src/System/Net/Http/SocketsHttpHandler/RedirectHandler.cs) 和 [HttpClient](https://github.com/dotnet/runtime/blob/release/10.0/src/libraries/System.Net.Http/src/System/Net/Http/HttpClient.cs)。同一次抓图保留 Cookie，启用 reqwest HTTP/2 与 Cookie 功能；新增六个传递依赖只由容器解析下载，Cargo.lock 已同步。状态码名称与机器已有 .NET 10 枚举元数据比对，包含 307 的 RedirectKeepVerb 别名。

临时文件创建与清理守卫在同一阻塞任务中完成，等待创建期间被取消也会丢弃并清理任务结果。另核实 Path.GetExtension 对 .png、末尾点和空格的行为，新增 common::io_helper::file_extension，供 UEditor 和文件服务使用；存储对象后缀不再先裁剪原文件名。

独立数据库与真实管理端验证：18 类非法或非公网来源返回 INVALID_Url；公开 PNG 和 GitHub 实际重定向成功，原图下载长度和摘要一致；404 返回 Url returns NotFound，DNS 失败使用抓取错误前缀，200 的 JSON 与 application/octet-stream 被拒绝。公开 Pexels 图片以 PNG 格式抓取 15,891,332 字节，保存下载与独立读取源资源的 SHA-256 一致，证明已越过旧 10 MiB 限制。另一次抓图在临时文件写到 48,671 字节时断开请求，临时文件回收且没有新增正式文件。真实保存目录故障只令当前项失败，下一来源仍处理，临时文件为空。

最新二进制还验证八种 UEditor/普通上传文件名组合：.png、大小写后缀、末尾点、末尾空格及末尾目录分隔符，确认类型校验与存储路径后缀一致。再次通过混合来源、真实重定向和保存失败清理。初次大文件候选中，GitHub 对一个 PNG 返回 application/octet-stream，部分第三方请求返回 402/403；这些按协议拒绝，未放宽 MIME 或伪造来源来通过检查，随后使用可正常提供 image/png 的公开资源完成大文件验证。

未证明所有网络边界等价：五次与六次重定向边界、Cookie 全部规则、DNS 变更与混合结果的实网场景、HTTP/2 入站取消、近 2 GiB 文件、非标准 URI/编码、底层默认请求头和各平台异常文本仍需进一步核验。Ueditor 表单的头/边界限制与暂存位置仍待推进；普通 RemoteUploadFile 的下载策略、全部云提供商与部署链路也未完成对齐。

抓图阶段已在容器通过全工作区 check/build（all-features、locked）、严格 Clippy、格式及差异空白检查。独立管理端和 PostgreSQL 已停止/移除，临时令牌、图片、日志与结果记录已清理。没有新增或执行测试代码，没有安装 Windows 组件；本阶段未重新生成 Release 发布包。

## 普通远程上传与默认 HTTP 工厂

重新读取当前 Client.WebAPI/FileController、FileService.RemoteUploadFileAsync、Common/HttpClientExtension.DownloadFileAsync 及三个 Program 的默认 AddHttpClient 注册：该接口只有客户端提供，使用普通客户端，禁止自动重定向、启用 gzip/deflate/Brotli 解压、不保存 Cookie。源服务没有 UEditor 的公网地址、图片类型、10 MiB 或非空正文规则。Rust 已移除这些额外限制和不再使用的 remote_upload_image_file 契约，由文件服务统一调用 common 下载工具，再调用普通上传保存；网络和下载异常统一为“远程文件下载失败”。RemoteUploadFileDto 的 sign 移除默认空字符串，和其他必填字符串一样由共享绑定层拒绝遗漏/null/空白；确认源 StringConverter 会将空白字符串转换为 null，因此没有依据 ASP.NET 通用默认值放宽输入校验。

默认工厂同步 100 秒请求期限、60 秒连接空闲期限、无独立建连期限及默认不限制每主机空闲连接数，保留 120 秒客户端轮换。参照 [HttpHandlerDefaults](https://github.com/dotnet/runtime/blob/release/10.0/src/libraries/Common/src/System/Net/Http/HttpHandlerDefaults.cs)。普通与流式客户端分别缓存并一起轮换；流式客户端不设置整包超时，由下载工具只约束响应头。锁中毒时取回已有配置，不再回退 Client::new()。启用宿主默认 HTTP/2、解压与系统代理支持，Ueditor 的一次性定制客户端保持独立规则。

共享下载工具先创建目标目录，再检查 2xx；返回明确路径或错误，不把失败表示为 None。成功正文按流写入同目录 GUID.download 文件，完整接收后原子替换目标；创建任务与清理守卫共同持有资源，文件服务也持有发布后暂存路径的清理守卫。文件名顺序改为显式名称、Content-Disposition filename*、filename、初始 URL、随机名称，保留路径末段和空格；支持常用 RFC 5987 与带引号 Base64 MIME 名称。该通用文件名分支的所有编码和畸形头还未实际运行验证，当前远程上传调用方始终提供生成的临时文件名。

独立 PostgreSQL 和真实两端宿主共完成 25 项主要核验：客户端从管理端实际静态目录下载 20,971,520 字节文本、0 字节空文件和中文内容，保存下载长度与 SHA-256 一致；gzip 响应的线长为 246,430 字节，工厂默认协商 Brotli 时也正确保存完整原文；404、无效/非 HTTP 地址与重定向返回固定 400 错误，四个必填字符串的缺失/null/空白均被拒绝，匿名请求 401。正在解压下载的请求于暂存文件 16,384 字节时断开，暂存文件回收且没有新增正式文件。另通过实际目录故障确认下载后保存失败的清理，并复核 Ueditor 仍拒绝本地来源且可以跟随真实 GitHub 重定向。

严格检查首次指出固定分组与余数写法，已使用 stable 支持的切片分组 API 修正，没有关闭 lint。仍需核验：普通与流式客户端分别持有连接池相对于源共享 handler 的行为、代理/命名客户端/证书配置、100 秒边界及长正文、全部压缩组合、通用文件名解析的异常输入、跨平台文件替换和原始异常链。源默认不跟随重定向并非“后续需要开放”；Ueditor 五次重定向是单独协议，不能套用普通客户端规则。全部框架功能对齐与最终发布仍未完成。


## HTTP 公共请求与上传工具

继续逐行对照当前 Common/HttpClientExtension.cs：post_string 仅接受忽略大小写的 json/xml，使用源约定的媒体类型和 UTF-8 字符集；URL 编码表单显式带 charset，并按 URI 非保留字符规则处理 *、~、空格及 UTF-8 字节，参照 [FormUrlEncodedContent](https://github.com/dotnet/runtime/blob/release/10.0/src/libraries/System.Net.Http/src/System/Net/Http/FormUrlEncodedContent.cs)。multipart 在发送前校验所有字段名及文件名，文本部分带 text/plain; charset=utf-8，文件部分不额外设置 Content-Type。移除未使用的任意文件 MIME 参数及出错静默回退，新增持有 Body 的 FileStream 项，避免强制把上传文件读入 Vec 或重复复制。

普通 get/delete/post 工具在正文完整读取后返回，保持状态、版本、URL、响应头和扩展信息；HTTP 非成功状态仍留给调用方处理。参照 [HttpClient](https://github.com/dotnet/runtime/blob/release/10.0/src/libraries/System.Net.Http/src/System/Net/Http/HttpClient.cs) 的 ResponseContentRead 默认值和默认缓冲上限，正文传输错误在工具调用内返回，并同时检查声明长度和实际读取长度。原有下载工具保持流式，不受普通响应缓冲路径影响。新增的 http 直接依赖和 reqwest stream 特性均由容器离线解析，使用工作区已有包。

本阶段未添加测试或临时验证程序；当前生产调用方只有下载工具，因此新增普通请求及文件流分支尚无实际网络调用证据，不能以编译通过替代运行验证。仍需对齐请求 Options 的传递、自定义请求头的容错和 Content-Headers 分派、multipart 特殊字段/文件名编码及默认边界格式，并核验长响应期限、取消及近 2 GiB 缓冲边界。客户端 SkipSsl、HttpSign、CarryCert 注册和签名处理器尚未实现；源 CarryCert 注册包含示例证书路径及处理器生命周期问题，需要按框架用途落实可用装配，不能机械复制已释放处理器或示例密码。所有框架领域的未完成项继续保留。

已检查并停止上阶段两个遗留的独立 Debug 宿主，确认进程路径后释放；隔离 PostgreSQL 容器已移除，专属临时目录及指针、令牌、上传文件和日志均已清理，保留 dev 开发容器。

HTTP 公共工具阶段已在容器通过全工作区 check/build（all-features、locked）、严格 Clippy（lib/bins、-D warnings）、格式和差异空白检查。未执行测试或新增测试代码，未安装 Windows 组件，未重新生成 Release 发布包。


## 命名 HTTP 客户端与出站签名

对照当前 Client.WebAPI/Program.cs 和 Libraries/HttpHandler/HttpSignHandler.cs，在宿主工厂补充命名注册、按名称缓存、惰性创建及连接池/处理器协同轮换。每次请求持有自己的准备状态，令牌、时间戳和 multipart 摘要不存入跨请求共享字段。共享 WebAPI 只提供装配回调和宿主配置容器，客户端注册 SkipSsl、HttpSign、CarryCert；管理端和任务宿主没有自动获得这些专属注册。普通发送的期限覆盖缓存、登录、签名、网络与正文读取；流式发送在响应头后交还取消管理。自定义 Content-Type 覆盖 multipart 时按最终媒体类型决定是否准备文件摘要。

HttpSign 使用注入缓存和普通登录客户端：缓存未命中时提交 name/passWord，成功响应正文作为令牌缓存；请求加入 Authorization/Token/Time，签名组合令牌最后一段、毫秒时间、PathAndQuery 和对应媒体类型的内容。JSON 解码后签名并恢复原始字节，multipart 文本与文件分别排序，文件采用大写 SHA-256；新增可定位 FileHandle，摘要后回到文件起点，非可定位流报错。只在 200 响应保存首个非空 NewToken，保存发生在响应正文缓冲前，不增加业务请求重试。登录地址、账号、密码及缓存键可配置，不把源示例密码写成运行默认值。

CarryCert 惰性读取 PKCS#12，提取证书、私钥和链，检查密钥匹配，再构建 reqwest TLS 身份。未使用时无需提供证书；错误信息不包含密码。源示例 using HttpClientHandler 后返回会提前释放处理器，本实现由连接池持有客户端和身份，不复制该生命周期错误。SkipSsl 只作用于显式获取的同名客户端，默认与携带证书客户端保持服务端验证。

容器内全工作区 check、Debug build（all-features、locked）和严格 Clippy 已通过；首次编译发现 SHA-2 新数组类型不支持直接 UpperHex，已改为逐字节大写编码后复核。实际创建独立 PostgreSQL，完成 business/log/event-bus 三目标迁移；真实客户端和管理端均健康启动，未使用的缺失证书不阻止启动，日志未出现配置密码。签名账号与证书密码的错误配置类型均在客户端监听前被拒绝，诊断没有泄露配置密码。

本轮运行证据只覆盖宿主装配与配置，尚未覆盖命名客户端的实际出站调用。现有业务调用没有使用这些源模板命名客户端，未为验证增加临时 Rust 程序、假接口或测试代码。签名与续签的跨端调用、TLS 客户端证书握手、失效/并发缓存、120 秒轮换、100 秒期限、特殊编码/字段排序及取消后的句柄释放仍需实际核验；当前字段排序是 Rust 字符串顺序，尚未复刻 .NET 按当前文化排序。框架完整目标仍未完成。

进一步核对 [DefaultHttpClientFactory](https://github.com/dotnet/runtime/blob/release/10.0/src/libraries/Microsoft.Extensions.Http/src/DefaultHttpClientFactory.cs) 与 [HttpHandlerDefaults](https://github.com/dotnet/runtime/blob/release/10.0/src/libraries/Common/src/System/Net/Http/HttpHandlerDefaults.cs)：源工厂允许未注册名称并使用独立的默认 handler（Cookie、重定向和解压默认值不同于本项目的显式注册），当前 Rust 对未注册名称返回错误，这一差异尚未对齐。请求 Options、自动处理器日志/指标和全部重定向细节也仍在后续范围内，不能把显式名称已落地当作整个 IHttpClientFactory 已等价。

命名客户端阶段最终格式与差异空白检查通过；独立客户端、管理端和 PostgreSQL 已停止/移除，专属配置、日志、迁移记录和临时目录指针已清理。未在 Windows 安装组件，未新增或执行测试，未重新生成 Release 发布包。


## 未注册名称、请求选项与请求头分派

当前工厂为未注册非空名称创建按名称隔离的惰性默认客户端，移除先前的“未注册名称”错误。其 HTTP/1.1、Cookie 与无自动解压规则独立于项目显式注册。注册表和每个名称的实例分别同步，连接池及 Cookie 状态按既有期限一起轮换；空名称继续使用项目默认客户端。

默认名称的重定向循环参照 [RedirectHandler](https://github.com/dotnet/runtime/blob/release/10.0/src/libraries/System.Net.Http/src/System/Net/Http/SocketsHttpHandler/RedirectHandler.cs)：支持 300/301/302/303/307/308，最多跟随 50 次，超限、无有效 Location 或 HTTPS 降级返回当前响应；继承原片段，按源规则改写 POST/303 请求为 GET，清除内容头及 chunked 标记，并在每次跟随时移除 Authorization/Proxy-Authorization。Cookie 接收与选择独立处理，保留手工 Cookie 并追加容器值，参照 [HttpConnection](https://github.com/dotnet/runtime/blob/release/10.0/src/libraries/System.Net.Http/src/System/Net/Http/SocketsHttpHandler/HttpConnection.cs)。没有把 reqwest 默认重定向次数或超限错误当作源行为。

新增 RequestOptions 类型和命名客户端绑定入口，支持类型值、区分大小写的名称、TryAdd 保留与显式覆盖。每次发送复制容器，贯穿 multipart 准备、请求前和响应后处理；返回响应扩展保存同一请求的最终选项，公共缓冲工具保留这些扩展。值用共享引用保持对象身份，容器增删不会影响下一请求。当前这是 Rust 处理器的选项通道，未映射 .NET 内部特殊传输选项或原生 reqwest 的所有扩展。

公共请求头按当前 Common/HttpClientExtension.SetHeadersAndOptions 分派：无效名称忽略，标准内容头仅在存在正文时替换，普通请求头追加。内容头分类与重定向移除共用一个入口，参照 [HttpContentHeaders](https://github.com/dotnet/runtime/blob/release/10.0/src/libraries/System.Net.Http/src/System/Net/Http/Headers/HttpContentHeaders.cs)。reqwest/http 的头值格式约束继续生效；并未声称其所有畸形值行为都与 TryAddWithoutValidation 相同。

这些分支尚未通过实际出站调用验证；本轮没有增加用于验证的临时 Rust 程序、假网络服务或测试代码。明确仍缺：流式和 multipart 请求的可重放内容模型（当前不可克隆正文需要保留方法重发时返回错误，即使底层 FileHandle 可定位）、CookieContainer 全部细节、跨主机/长链重定向实测、文化排序、内置传输选项、旧池到期回收与默认处理器指标日志。未知名称已不再拒绝不代表整个 HttpClientFactory 已完全等价，签名发送与客户端证书握手的实测也仍未完成。

未注册名称与请求选项阶段已在容器通过全工作区 check/build（all-features、locked）、严格 Clippy（lib/bins、-D warnings）、格式与差异空白检查。没有安装本机组件，没有新增或运行测试，没有创建临时宿主或数据库；本阶段未重新生成 Release 发布包。


## 用户账户写入与短信换绑

对照当前 `Application/Application.Service/User/UserService.cs`，用户创建、编辑、删除、用户角色和直接功能授权使用普通保存，不额外生成 DataUpdateLog。Rust 对应路径已移除附加的变更日志写入和 GetUser 的额外 logging 属性，保留创建者、修改者、删除者及仓储更新时间。角色列表不另加源查询未声明的排序。

用户名查重先于角色 ID 解析，编辑时再查询目标用户；缺失目标使用源错误 `无效的UserId`。角色 ID 保留重复项，允许有符号整数，格式及溢出错误进入共享内部异常处理。数组元素的 null/空白通过 DTO 的局部转换保留至服务解析，不修改公共数组绑定规则；数组自身为 null 或数字/布尔元素仍在绑定阶段拒绝。元素不隐式增加 Required 的依据是 ASP.NET Core [DataAnnotationsMetadataProvider](https://github.com/dotnet/aspnetcore/blob/release/10.0/src/Mvc/Mvc.DataAnnotations/src/DataAnnotationsMetadataProvider.cs) 对类型元数据的处理，以及 NetEngine 的 StringConverter 和 long.Parse 调用。此判断来自源码对照，本轮未运行 .NET 宿主。

短信修改手机号使用原始非空字符串匹配缓存键、验证码和手机号。成功或失败均不主动删除验证码；先校验验证码，再查询当前用户和其他活跃用户的手机号。号码被占用返回 400；该操作不写 UpdateUserId。相同手机号不执行更新；管理端重复提交完全相同字段和修改者时也不重写用户行，单独修改角色不刷新用户版本。

本轮在专属 PostgreSQL 和 Redis 容器执行业务、日志、事件三个模型迁移，启动真实管理端和客户端宿主，调用初始化、登录、用户增改删、短信换绑和授权接口。确认字符串空格、重复角色、用户名/角色解析/目标缺失的错误优先级、密码 default 保留、负数角色关联、空元素及溢出处理、继承权限不可直删、直接授权增删和删除用户不级联角色。查询数据库确认同值提交的 xmin 与 UpdateTime 未改变，手机号修改保留原 UpdateUserId，整个流程 DataUpdateLog 为零；查询 Redis 确认成功、号码占用、验证码错误及账户缺失后缓存仍保留。运行核验使用现有真实二进制与服务，没有新增测试代码或假接口。

当前用户模块仍不能认定完全等价：分页的有符号范围、请求校验完整错误模型、文化相关用户名大小写及整数解析的所有特殊字符、全部查询排序与深层权限树仍需继续核对。额外的 UserInfo 资料方法本轮未调整。检查当前 NetEngine 时还发现业务迁移历史常量已为 `__EFMigrationsHistory_Business`，Rust 的历史命名及所有模型迁移入口需按最新源项目重新审计，不能沿用先前文档即认定一致。命名 HTTP 客户端实测、其他应用用例、基础设施及部署验证继续属于完整目标。

本轮最终修改已在容器通过全工作区 check/build（all-features、locked）及严格 Clippy（lib/bins、-D warnings）。未安装本机组件，未运行测试，未生成新的 Release 发布包。


## 数据库工具固定配置与模型迁移历史

重新核对最新 `Repository.Tool/Configuration/ToolDatabaseOptions.cs`、三个 ContextFactory 以及业务、日志、事件写上下文。工具现固定读取可执行文件旁的 `repository_tool.appsettings.toml`，不加载当前目录配置、APP_CONFIG_PATH、环境专用配置或 APP__DATABASE__ 覆盖；移除 --config-path 与 --database-url。执行 migrate/json-index 必须显式选择 --target，无参数及 --help 在不读取配置、不连接数据库的情况下显示帮助。宿主自身的环境配置约定不受此变更影响。

三个所属模型声明 `__EFMigrationsHistory_Business`、`__EFMigrationsHistory_Log`、`__EFMigrationsHistory_EventBus`，公共迁移器始终使用完整的 public schema 限定名称。历史记录的 MigrationId（varchar(150)，主键）与 ProductVersion（varchar(32)，非空）参照 [EF Core HistoryRepository](https://github.com/dotnet/efcore/blob/release/10.0/src/EFCore.Relational/Migrations/HistoryRepository.cs)；Rust 记录当前原生迁移标识及 rust_engine 工具版本，不伪装 EF 产品版本，也不将其他迁移链视为当前迁移已执行。初始业务、事件建表不再使用 IF NOT EXISTS 跳过已有模型表；发生冲突时事务回滚，不给未知结构补登成功历史。没有自动改名或转换既有历史表，没有操作用户数据库。

本轮真实运行证据来自一个专属 PostgreSQL 容器中的五个隔离数据库及复制到临时目录的现有 repository_tool 二进制。验证了：无配置时帮助可用、未指定目标拒绝执行、旧配置覆盖参数拒绝、缺配置失败；工具目录配置有效时，错误的环境覆盖和当前目录配置均不生效。三个模型在同库首次迁移与重复迁移通过，历史记录数量分别为 5/1/1，字段长度、非空与 public 归属已直接查询目录确认；两进程并发迁移只登记五个业务版本。日志单独迁移没有创建业务或事件表；缺少日志连接明确失败。已有 User 表的空历史数据库迁移失败，原记录保留且历史表创建回滚。search_path 为 tenant,public 时，事件模型表创建在 tenant，历史表留在 public；随后重复迁移及只读 JSON 索引命令均通过。

本轮容器内全工作区 all-features、locked 的 check/build 和严格 Clippy 已通过。没有新增或执行测试，没有安装本机组件。本轮没有实现 EF Core 式模型快照、自动生成增量迁移、迁移 SQL 导出或回退；当前 Rust 迁移链仍是人工登记的 schema 语句，不能因历史格式一致就声称迁移工具完整等价。其他未完成的框架及应用差异仍继续保留在完整目标内。


## 原生迁移 SQL 离线导出

按当前 NetEngine 数据库工具文档中的 `dotnet ef migrations script` 工作流，新增 `repository_tool migration-script --target`。三个目标共用直接 migrate 的定义，公共实现在 repository_core::migration，不引入对业务实体的反向依赖。读取固定工具配置后离线生成 SQL；支持版本号、名称、完整 MigrationId 的起止选择，起点不含、终点包含，0 表示未应用迁移，相同位置返回空输出。参照 [EF Core 迁移脚本文档](https://learn.microsoft.com/en-us/ef/core/managing-schemas/migrations/applying#sql-scripts) 提供普通和按历史判断的幂等脚本；本项目脚本同时沿用原生迁移器的事务与数据库锁。

幂等步骤按当前模型 public 历史表判断，未应用时动态执行其原始 SQL，再写同格式历史，避免跳过步骤仍解析旧表/列。PostgreSQL E 字符串显式处理单引号及反斜线，外层 DO 定界符避开正文内容；现有迁移内的嵌套 DO 块已通过真实执行。--output 使用容器内路径、UTF-8、自动创建父目录并覆盖指定文件，父目录行为对照 [MigrationsScriptCommand](https://github.com/dotnet/efcore/blob/release/10.0/src/ef/Commands/MigrationsScriptCommand.cs)。生成阶段失败不会触碰输出文件；路径写入失败单独报错。相关用法已加入数据库工具文档、README 与 AGENTS。

本轮使用专属 PostgreSQL 容器和七个隔离数据库，运行实际工具生成及执行 SQL，没有加入测试代码。不可解析的数据库 URL 仍可离线导出三个模型，输出未包含连接内容。普通脚本、幂等脚本连续两次、业务分段脚本分别执行成功；用 pg_dump 完整 schema（只忽略随机 restrict 标记）及三张历史表内容逐一比较，三种路径均与直接 migrate 的数据库一致。两进程同时执行业务幂等脚本，五个版本仅登记一次；已有 User 表冲突导致脚本事务回滚，原记录保留，未遗留历史表。tenant search_path 的事件表与 public 历史归属正确。未知迁移、错误模型、反向区间、缺目标、缺连接、空范围、输出父路径错误和显式覆盖均经真实 CLI 核验；错误区间未覆盖原输出文件。

全工作区 all-features、locked 的 check/build、严格 Clippy、格式和差异空白检查通过。本轮未在 Windows 安装组件，未新增或执行测试。反向迁移仍缺 Down 定义；模型快照、自动增量生成、按目标版本回退及其他尚未核验能力继续保留在完整目标内，当前不能认定与 EF Core 迁移工具整体等价，也没有把本轮脚本能力当作整个 NetEngine 翻版已完成。

## 短信认证、密码修改与微信首登

对照当前 `AuthorizeService.cs`、授权 DTO、`MessageTask.cs`、短信服务及缓存扩展，短信发送使用原始手机号、固定五分钟缓存和 `MessageTask.SendSMS`（并发数 1）。验证码范围为 100000 至 999998；签名和模板保持源调用的空字符串。入队与缓存写入并行且都等待结束，缓存读取错误视为未命中，缓存写入失败不覆盖队列结果。删除自定义短信过期、冷却、任务名和模板配置，避免宿主与源规则分叉。队列短信 DTO 保留 PascalCase 和根对象、参数字典的 `$id` 元数据，消费时移除字典引用标识，不传给提供商。

短信登录精确匹配手机号与验证码，成功后不消费验证码；新用户使用 UUID 用户名、随机初始密码及源风格显示名称，不追加变更日志。短信改密先读取用户手机号、验证缓存，再重新读取用户，在同一事务内修改密码并按原始行版本删除有效 Token 记录，保留软删除记录。旧密码改密不清理 Token。密码及更新人都未变化时不发出用户 UPDATE；这两条路径不写 DataUpdateLog，也不将 Token 数据表变成 JWT 黑名单。

真实客户端接口、独立 PostgreSQL 和 Redis 已核验：带空格手机号原样入队；300 秒有效期与重复发送拒绝；同验证码重复登录复用用户并创建独立 Token；随机初始密码不等于旧实现的可预测密码；带空格密码精确匹配；短信改密清理有效 Token 后，既有 JWT 仍能访问用户接口；同值改密保持 xmin 与 UpdateTime；旧密码改密保留有效 Token；删除账户后的旧密码改密返回 400，短信改密按首次用户查询失败返回 Development 500。审计表始终没有这些操作的额外记录。临时禁用 Redis GET/SET 分别确认读取失败继续入队、写入失败仍返回成功，随后恢复权限。最新二进制另通过隔离库 CHECK 约束制造队列写入失败，确认接口报错、队列无新增而验证码缓存仍写入；约束已移除。

微信首登改用同一事务创建用户及外部绑定，UUID 用户名和随机密码、不额外写审计日志、不预填 UpdateTime。绑定查询同时过滤已删除用户。小程序采用源名称的等待锁并在锁内复查，正常和错误结果都等待解锁；公众号保持源无额外锁的行为，昵称为 null 才拒绝授权，保留空昵称和原始空白。AppId/Code 不再由应用服务裁剪；配置查询支持空 GroupId。授权码交换移除额外重试，要求成功 HTTP 状态，小程序同时确认 openid 与 session_key 存在。错误不回显令牌、密钥或完整上游正文。用户信息的 Privilege 改为可空数组。实际新客户端已健康启动，两个微信缺配置入口均按源服务返回内部错误，未向微信发出请求。

本阶段仍有明确边界：没有真实微信授权凭据，不能证明成功交换、注册事务回滚、多实例锁竞争和提供商短信发送已运行验证；没有增加假接口或临时 Rust 验证程序。源小程序查询当前将 OpenId 与 AppId 比较，Rust 保留按返回的 OpenId 查找，未复制疑似源缺陷，此项仍是明确行为差异。自动名称使用固定 UTC 文本格式，尚未复刻 .NET 当前文化；共享请求绑定仍拒绝空必填字符串，不能以移除服务裁剪声称模型绑定完全一致。短信专用编码不是通用 ReferenceHandler.Preserve 对象图实现。用户名索引唯一性、Name 的 citext 类型、迁移快照/回退、命名 HTTP 客户端实测及此前各领域未完成项继续保留。

最终代码已在容器通过全工作区 all-features、locked 的 check/build、严格 Clippy（lib/bins、-D warnings）、格式与差异空白检查。两端临时宿主及独立 PostgreSQL、Redis 均已停止，专属配置、令牌、日志和临时目录已清理。未在 Windows 安装组件，未新增或执行测试，未重新生成 Release 发布包；完整对齐目标仍未完成。

## 用户姓名类型与身份索引

当前源 `User.cs` 将 Name 声明为 citext，对 UserName、Phone、Email 声明普通索引；CD/CUD_User 基类提供时间索引，审计关系具有外键索引。UserBindExternal 未声明外部身份组合唯一约束。Rust 用户实体现以自定义 citext 列及写入转换映射 Name，读回保留原始文本；初始业务建表前启用 citext。新迁移 `202611070003_align_user_identity_schema` 对已有 Name 列转换类型，移除用户名和外部绑定的额外唯一索引及旧的绑定用户部分索引，补齐源模型普通索引。用户名查重仍由用户应用服务执行，没有把姓名规则推广到其他文本字段。

citext 的比较由 PostgreSQL 执行，受数据库区域设置影响，并要求扩展操作符所在 schema 可见；本实现没有用 Rust lowercase 替代。参见 [PostgreSQL citext 文档](https://www.postgresql.org/docs/current/citext.html)。业务迁移默认将扩展安装在 public，已核验业务表处于 tenant、search_path 为 tenant/public 时仍能使用类型，迁移历史仍位于 public。

实际使用独立 PostgreSQL 的四个数据库核验：先用改动前保存的真实工具创建五版本旧结构并插入用户，再用新工具只执行第六项迁移，原始姓名及其他值保留。新工具从空库直接迁移、旧结构升级，以及旧 SQL 加新幂等增量脚本（重复执行两次）的完整 schema 导出一致，比较仅忽略 pg_dump 随机 restrict 标记。第四个数据库用完整幂等脚本初始化 tenant 表，citext 位于 public，六项历史完整登记。

最新管理端实际完成权限初始化、用户创建、改名和读取。大小写不同的姓名关键词均匹配保留原始大小写的姓名，带 `%` 和 `_` 的关键词按字面匹配；仅改变用户名关键词大小写不匹配。直接写入同名用户和重复外部绑定成功；同名用户分别用各自密码登录得到对应用户 ID，管理端创建重复用户名仍返回业务错误。以上验证覆盖真实 SeaORM 读取、保存及查询，不仅是手写 SQL。没有真实微信授权，因此不据此宣称微信成功回调已验证；其他业务表的索引和类型仍须继续核对，历史迁移快照及自动增量生成仍未完成。

本阶段已通过容器全工作区 all-features/locked 的 check、build、严格 Clippy、格式和差异空白检查。临时管理端、独立 PostgreSQL 及四个隔离数据库已停止/移除，旧工具副本、SQL、令牌、配置和日志已清理。未修改既有业务数据库，未新增或运行测试，未在 Windows 安装组件；Release 发布包尚未重新生成，完整目标继续保持未完成。

## 业务索引与角色写入

对照源 Role、Function、FunctionAuthorize、FunctionRoute、LlmApp、Product、Order 实体及 OnModelCreating，移除 Rust 额外施加的角色编码、功能标记、授权组合、模块/路由组合、LLM 应用 Code、SKU 和订单号唯一约束。按源声明补充普通索引及授权/路由外键索引，补充 Product.Name 索引。UserInfo.User 与 User.UserInfo 是显式配对的引用导航，UserId 为必填外键；按 [EF Core 关系及索引约定](https://learn.microsoft.com/en-us/ef/core/modeling/relationships/conventions#indexes)，一对一唯一索引覆盖全表，不能只限制有效资料。Rust 补充双向关系声明并建立全表 UserId 唯一索引，保留任务有效定义唯一约束。新迁移为 `202611070004_align_business_model_indexes`。

隔离 PostgreSQL 四库验证：改动前真实工具建立六版本旧结构，新工具完成第七版本升级；新建空库、旧库升级和旧 SQL 加幂等增量脚本（连续执行两次）导出的完整 schema 一致，比较仅忽略 pg_dump 随机 restrict 标记。成功写入重复角色编码、功能标记、角色/用户授权、路由、产品 SKU、订单号及 LLM 应用 Code，确认数据库约束已改变。另一个旧结构库保留同用户一条有效、一条软删除资料，新迁移在创建一对一索引时失败，整项事务回滚：两条资料仍在、旧索引仍在、历史仍为六项，没有自动删改资料。

角色创建、编辑、删除和功能授权回到源服务的普通保存语义，不额外写 DataUpdateLog。创建和编辑按原始 Code 加锁、查重，正常及错误路径均等待锁释放；编辑先判断编码冲突再判断角色存在，相同字段不 UPDATE。功能授权只处理第一条有效关联，新建只设置创建人，不预填更新时间；重复启用无写入。角色删除先检查有效 UserRole，只软删除角色，保留功能授权；角色选项查询移除源代码没有的额外排序。

真实管理端、PostgreSQL 和 Redis 已核验：角色原始空白保留、重复编码拒绝、缺失目标时的校验先后顺序、同值编辑保持 xmin；重复授权保持原记录，取消一次只软删除一条重复授权；新增授权的 UpdateTime 为 null 且创建人存在。角色有关联人员时删除被拒绝，软删除关联后可删除角色且原授权保留；重复删除及不存在角色均成功。预占实际 roleCode 锁得到源业务错误，外部占用保持原值；解除后创建成功，普通成功和错误请求结束后 Redis 槽位已清空。DataUpdateLog 无额外记录。上述数据库重复写入验证不代表产品、订单或 LLM 全部业务流程已对齐；其他表的字段、关系删除行为和索引仍需继续审查，分页模型与通用绑定差异也仍未完成。

最终双向关系声明补充后，容器全工作区 all-features/locked 的 check、build、严格 Clippy、格式与差异空白检查再次通过；新工具导出的完整业务 SQL 与已验证版本逐字节一致。临时管理端、PostgreSQL 四个隔离库、Redis、旧工具副本、SQL 和令牌配置均已停止或清理。未修改既有业务数据库，未安装 Windows 组件，未添加或执行测试；未重新生成 Release 发布包，完整对齐目标仍未完成。


## 有符号分页、查询文档与字面搜索

对照当前 `PageRequestDto.Skip`、`PageListDto<T>` 及普通业务、队列、日志查询，页码、每页条数和总数统一为 i32。偏移使用共享环绕运算，Debug 与 Release 不因溢出检查产生不同结果；只校验每页 1 至 100 条，不把非正页码统一拒绝或转为第一页。普通列表先计数，零条保留 null；有数据时通过 repository_core 的 SignedPage 保留参数化查询及有符号 OFFSET。队列和日志在查询前拒绝负偏移，空列表保留 []；计数超出 Int32 时明确失败，不静默截断。

实际 PostgreSQL 与管理端核验了空表、已有用户/角色、负页码、Int32 上下界和越界绑定、每页 0/负数/101、正常第二页及带关键词查询。page=1073741825、pageSize=4 的偏移环绕为零；page=-2147483648、pageSize=1 返回末页外的空数组。空表普通查询 page=0 仍返回 total=0/list=null；队列和日志提前报内部错误。Development 对负 OFFSET 返回 500，Production 返回 400 和固定内部异常消息。这里的 Production 使用 Debug 二进制，仅验证环境错误映射，不作为 Release 请求签名的证据；未用超过二十亿条实际记录验证总数溢出。

实际 Swagger 发现查询 DTO 默认被描述为路径参数，且分页类型别名让不同列表覆盖同一个 PageListDto schema。现查询 DTO 显式声明 Query，分页默认值复用共享函数；响应及组件注册显式使用 PageListDto<具体类型>。真实管理端 69 个路径、83 个参数及客户端 31 个路径、27 个参数中，不再存在名称未出现在路径内的路径参数。管理端 11 种分页响应分别对应用户、角色、链接、栏目、文章、LLM 模型/应用/对话、任务配置、队列与日志 DTO；可选参数、分页默认值及范围正确。微信支付回调商户号仍是必填路径参数。

LLM 三个列表原先把关键词直接拼为 LIKE 模式，输入 % 会返回隔离库中的两条记录，与源 Contains 不符。公共仓储扩展新增 contains_pattern，显式转义 %、_ 和反斜线；用户、队列和日志已有的等价转义也统一复用，是否裁剪仍归各用例。隔离库写入两组 LLM 模型、应用及对话后，通过实际接口确认三个列表对这些特殊字符分别只命中字面记录，完整关键词与普通关键词各命中一条，无匹配时为 null；每页一条可依次读取两页，第三页为空数组，负偏移报错。模型与关联名称的读取均成功，没有调用外部 LLM。

本阶段全工作区 all-features/locked 的 check、build 和严格 Clippy 已通过。完整 MVC 校验错误结构、Unicode/文化语义、其他业务字段及关系、迁移快照与回退、HTTP 客户端完整互操作、提供商和部署验证仍未完成，不能据本阶段结果认定整个 NetEngine 翻版已完成。

最终格式检查与差异空白检查通过。两个临时管理端、一个客户端及专属 PostgreSQL 已停止并移除，专属配置、令牌和日志目录已清理，仅保留原开发容器。未修改既有业务数据库，未安装本机组件，未新增或执行测试；未重新生成 Release 发布包。


## LLM 配置写入与对话只读连接

对照当前 LlmModelService、LlmAppService、编辑 DTO 与管理端控制器，模型及应用创建改为 JSON 字符串 ID；应用编辑/删除查询参数改为源接口的 id。编辑 DTO 的 IsEnable 默认 true，模型 ProtocolType 默认 0，应用 LlmModelId 缺失时保留值类型默认 0 并由关联模型查询判断。源业务异常统一返回 400，不再把名称/Code 重复、锁占用、模型被引用或禁用状态映射为 409。

配置管理按源顺序获取名称或 Code 锁，再获取模型 ID 锁，所有普通成功及错误路径均按逆序等待释放。模型编辑先查目标后查重；应用编辑先查目标，再校验 ExtraBodyJson、加锁、查重及检查模型状态；应用创建先查 Code 再查关联模型。备注、SystemPromptTemplate 和 ExtraBodyJson 保留绑定后的原文，只有源明确调用 Trim 的名称、标识、端点、密钥、Code 和 User 模板裁剪。相同字段及修改者不发出 UPDATE；删除只设置删除时间与删除者，保留原修改者，已有更新时间机制继续生效。重复删除和不存在的负 ID 删除返回 true。

真实管理端、独立 PostgreSQL 与 Redis 核验：创建结果为 application/json 的字符串 ID；省略默认字段成功且数据库实际为协议 0/启用 true。重复名称、重复 Code、无效协议、缺密钥、缺失目标加非法 JSON、重复 Code 加无效模型均按预期顺序返回 400。非空文本前后空格在数据库及读取结果中保留；空密钥编辑保留既有密钥。连续同值编辑前后的 xmin 与 UpdateTime 完全相同。逐一预占名称、Code、模型 ID 锁，确认请求返回对应业务错误且外部持有者仍在；第二把锁失败后 Redis 仅余预占锁，移除后无残余槽位。没有启动提供商请求或加入测试代码。

模型存在启用应用时禁止禁用；应用禁用后模型可禁用，随后创建/启用应用被拒绝，创建禁用应用成功。任意有效应用仍阻止删除模型；软删除应用后模型可删除，重复删除成功。数据库确认未经编辑的模型和应用删除后 UpdateUserId 仍为 null，DeleteUserId 与 UpdateTime 有值，对话记录保留，DataUpdateLog 无额外记录。

LlmConversationManageService 改为只接收 ReadDatabaseConnection，不再通过 LlmContext 访问写库。隔离 PostgreSQL 的写、读两个数据库分别迁移并写入不同模型、应用、用户和同 ID 对话；实际接口返回读库对话、读库应用名与创建人，按读库应用 Code 搜索成功，按仅存在于写库的正文搜索为零。配置列表继续读取写库。Swagger 核验了 id 查询参数、默认启用值与模型协议默认值。

全工作区 all-features/locked 的 check、build 与严格 Clippy 已通过。本阶段没有验证外部 LLM 成功调用、流式取消或提供商协议，也没有完成通用模型绑定错误结构、模板参数文化规则及软删除导航联接的全部语义；不能认定整个 LLM 组件或完整翻版目标已完成。

最终格式与差异空白检查通过。临时管理端、专属 PostgreSQL 的两个数据库及 Redis 已停止并移除，配置、令牌和日志已清理；仅保留原开发容器。未安装本机组件，未新增或运行测试，未修改既有业务数据库，未重新生成 Release 发布包。


## 历史迁移固定定义

重新检查发现业务初始、旧结构和 LLM 对齐迁移仍通过当前实体、索引与公共补丁函数生成 SQL；日志初始迁移也直接读取当前 Log 实体。这样调整模型会改变同一 MigrationId 的含义。现将七项业务迁移的固定 SQL 存放于 repository_tool/src/migrator/snapshots，日志和事件的初始迁移产物存放于所属组件 src/migrations；执行及导出仍共用原登记链和公共迁移器。repository_tool/src/schema.rs 只保留当前业务模型的 JSON 索引比较清单，移除历史迁移对当前建表、索引及补丁构造器的依赖。

日志保留当前 NetEngine NpgsqlPartitionMigrationsSqlGenerator 的行为：在固定迁移产物中保存父表、索引和当时的分区元数据，通过公共分区算法按 SQL 生成时刻创建初始子分区；不把迁移日期固定成永久初始分区，也不从新实体重新获取旧迁移的分区声明。当前日志的 Id/月分区元数据与 SQL 同存一个产物，后续模型变更不能修改该定义。

先保存改动前真实工具，离线逐项导出并固化当前定义。改动后九项迁移的 288 条语句逐句完全相同。独立 PostgreSQL 四个数据库分别使用旧工具迁移、新工具迁移、新普通脚本和新幂等脚本；完整 schema 导出逐字节相同（仅忽略 pg_dump 随机 restrict 标识），三个模型的全部 MigrationId/ProductVersion 记录亦相同，数量为 7/1/1。重复直接迁移不执行步骤，三个幂等脚本各重复执行一次均成功。新旧工具针对实际目录的三个 JSON 索引比较结果一致，当前实体索引检查能力未被固化为旧模型。最终将日志元数据也移入固定产物后，再次确认最新二进制三个模型完整幂等 SQL 与改动前工具逐字节一致。

本次固定的是当前工作区实际导出的定义，没有声称恢复此前未保存的历史版本产物，没有改写已有迁移记录。这解决后续实体修改导致历史 SQL 漂移的问题，不等同于 EF Core ModelSnapshot 或自动迁移生成。当前仍缺少 Down、指定版本回退、模型差异推导与 Add-Migration 对应能力；完整框架对齐目标继续保持未完成。维护说明与 AGENTS 已同步，后续结构变化必须新增迁移，不能改写旧产物。

最终代码已在容器通过全工作区 all-features/locked 的 check、build 和严格 Clippy。没有在 Windows 安装组件，没有新增或执行测试，没有使用临时 Rust 验证程序或修改既有业务数据库。

最终格式与差异空白检查通过。专属 PostgreSQL 及四个数据库已移除，旧/新工具副本、连接配置、导出 SQL 与临时目录已清理，仅保留原开发容器；未重新生成 Release 发布包。


## 指定迁移版本与反向 SQL

公共迁移定义增加固定 Down 构造器，三个模型均已登记撤销定义。`migrate --to` 接受版本号、唯一名称或完整 MigrationId，省略时升级到最新，0 表示撤销全部已应用步骤。执行先在同一事务与模型迁移锁内逆序撤销目标之后的已应用步骤，再补齐目标之前缺失的步骤；历史删除与 DDL 一起提交。`migration-script` 复用相同版本解析及 Up/Down 定义，反向区间生成逆序 Down，幂等回退以历史存在为条件。无效版本在连接前报错，导出仍然离线。

业务 Down 依据已固定的前置结构。真实 PostgreSQL 逐步应用七项旧工具导出的 Up，确认除任务定义唯一索引外，后续整理步骤未改变其固定前置版本的结构；这些步骤的 Down 只撤销历史记录，不虚构未保存的旧库结构。初始 Down 按依赖逆序删除所属表，保留历史表与共享 citext；日志删除分区父表及附加分区，事件删除所属两表。没有修改原 Up 产物，没有使用 CASCADE 或推测人工历史数据。此前固定定义前的结构无法据此恢复，这不是任意旧数据库的自动修复或数据还原能力。

在隔离 PostgreSQL 中，七个指定版本的正向结构与旧工具固定脚本逐字一致；六次逐级回退与各自前置结构一致，历史数量正确，保留表中的数据不变。原生、普通 SQL、幂等 SQL 三条路径均完成三个模型升级、回退到零和重新升级，并在回退中间版本后比较完整结构、数据及历史；重复幂等回退成功。额外外部视图阻止最后一张业务表删除时，三条路径均完整回滚，前后完整数据库导出相同。业务回退保留日志和事件数据，日志回退保留事件数据，全部撤销后仍保留外部表、其数据及共享扩展。六份正向 SQL 与改动前输出逐字一致；反向导出在不可连接的配置地址下成功。

容器内全工作区 all-features/locked 的 check、build 与严格 Clippy 已通过。使用说明与 AGENTS 已同步。模型快照、自动增量迁移生成、提供商互操作及其他尚未完成的框架差异继续保留，当前结果不代表完整 NetEngine 翻版目标已经完成。

最终格式与差异空白检查通过。专属 PostgreSQL 及五个隔离数据库已移除，旧工具、副本配置、导出脚本与临时目录已清理；仅保留原开发容器。没有安装本机组件、修改既有业务数据库、新增或执行测试，未重新生成 Release 发布包。


## 当前模型快照与增量迁移生成

增加公共关系模型快照与离线差异生成，覆盖当前模型使用的表、列类型、可空性、默认值、主键、命名外键、索引、注释和分区元数据。当前实体通过 versioned 建表语句树提取结构，排除 xmin；业务和队列显式索引分别归属实体组件，日志及事件自己提供模型。历史迁移继续读取固定产物。首先用隔离 PostgreSQL 对比旧迁移链与当前模型独立建库的完整 schema，三个模型完全一致后保存不可变基线，未凭当前实体直接假定旧库结构。

工具增加 migration-add、migration-pending 和 model-snapshot。生成器读取最后迁移嵌入的模型，输出 Up/Down 操作、目标模型、Rust 入口与登记文件，不连接数据库。生成文件需要审查并重新编译；目录锁阻止同时写入，登记最后替换，旧二进制与磁盘登记不一致时拒绝继续生成。模型与快照存在变化时，默认 migrate 拒绝执行，显式 --to 仍可选择已登记版本。分区初始范围在生成 SQL 时计算，周期变化只更新模型，不能通过普通迁移切换分区或更换分区键。

对照最新 NetEngine 的 Index 属性、CD/CUD 时间索引和关系索引约定，补齐当前模型缺失的普通索引，未增加服务层查重对应的唯一约束。实际使用新生成器登记 20261107015309_align_declared_model_indexes：业务 114 项、日志 2 项、事件 4 项，Up 全为新增索引，Down 全为删除对应索引。编译后在隔离数据库执行三项迁移，完整 schema 与当前模型直接建库结果相同；回退后与旧基线完全相同；各幂等增量脚本执行两次后仍与当前模型一致。三个模型均通过 migration-pending，旧二进制重复生成被拒绝，未覆盖已生成登记。

当前生成能力仍不等同于完整 EF Core：原生枚举、生成列、额外列约束、序列变更和复杂类型迁移尚未完整覆盖；重命名目前需要审查并手工调整生成操作，数据转换与数据恢复不由结构差异自动推断。本次实际验证覆盖当前完整模型建库及索引增量的升级/回退，尚未逐项运行所有列、主键和表变化组合，也没有重新验证所有运行时或外部提供商行为。完整翻版目标继续保持未完成。


最终代码通过容器内全工作区 all-features/locked 的 check、build、严格 Clippy、格式及差异空白检查。最终工具的模型查看、变化检查和迁移 SQL 导出已在不可连接地址下执行成功；复制的生成目录可以生成空迁移供自定义操作使用，未修改默认登记。专属 PostgreSQL 及三个数据库、工具副本、配置、导出 SQL 和临时生成目录均已清理，仅保留原开发容器。没有在 Windows 安装组件、修改既有业务数据库、新增或运行测试，未重新生成 Release 发布包。


## LLM 调用契约、模板与协议流

应用层调用结果现在保留完整候选（序号、角色、内容和结束原因）、响应 ID、模型及有符号可空用量，流式结果不再把不同候选拼接到一个 delta。正式服务与客户端演示只聚合第一个候选，收到非空结束原因且完整消费后才保存会话；流的配置加载延迟到消费开始。服务、接口绑定和所有现有调用点已同步，管理端试用显式接收操作者并传入请求 user。普通 Demo 使用源代码的 jiafa，流式 Demo 保留 sumqw，均传递可选登录身份。

三个协议统一温度、输出上限等 ExtraBody 扩展参数，基础字段冲突报错，Anthropic 仅在未提供时默认 max_tokens=4096。OpenAI 兼容协议保留缺失消息、空候选和只含用量的分片语义，并处理提供商流式 error。Responses 的结束原因取实际 status，Anthropic 的 message_stop 和兼容协议的 [DONE] 只结束流，不再补造完成标记。非成功 HTTP 状态立即返回错误。流式 HTTP 使用宿主无整包期限的客户端，仅等待响应头施加 120 秒期限；按字节积累完整行再解码，支持 UTF-8 BOM、LF/CRLF/CR 与无尾换行，避免中文被网络分片切断。协议 DTO 按声明字段忽略 ASCII 大小写并按输入顺序保留重复字段的最后赋值，动态 JSON 键不改名。

正式调用保留原始 Code 和提示词，依次校验应用、模板参数、ExtraBodyJson、模型。配置损坏使用内部错误，运行时扩展 JSON 过滤空键与 null 值。参数支持 null，管理端字典单独绑定以保留空字符串和原始键，ExtraBody 在请求绑定时限制为对象。必填键按首次出现顺序去重；正式调用使用精确字典键，管理端使用 Unicode 一对一大写映射，并拒绝忽略大小写后重名的参数。实现依据 [.NET ICU 序号映射](https://github.com/dotnet/runtime/blob/main/src/native/libs/System.Globalization.Native/pal_common.c) 与 [ICU4X simple_uppercase](https://docs.rs/icu_casemap/latest/icu_casemap/struct.CaseMapperBorrowed.html#method.simple_uppercase)，保留无点 i 与长 s 的特殊规则，不使用完整字符串大小写展开。

本轮依据当前 NetEngine 源码逐项修正并检查调用关系，尚未用真实提供商执行完整对话、超长流、取消和会话写入场景，也未运行 .NET/Rust 的跨平台 Unicode 对照。本轮当时的正式调用只提供精确 HashMap；后续调用取消与参数比较小节已补充 Ordinal/OrdinalIgnoreCase 选择。HTTP 断连到应用任务取消的整个宿主链、非 UTF-8 BOM 响应、其余 Demo 行为及其他框架模块仍需继续审计；此处不代表完整翻版目标完成。

最终代码已通过容器内全工作区 all-features/locked 的 check、build 和严格 Clippy，以及格式检查。新增依赖仅用于生产代码的 Unicode 参数比较和流式请求期限，下载与编译均在容器内完成；没有新增或运行测试，没有安装 Windows 组件，没有调用真实提供商或修改数据库。本轮未重新生成 Release 发布包。


## LLM 调用取消与参数比较选择

新增公共取消令牌适配，取消错误独立保留为 AppErrorKind::Cancelled 和 LlmError::Cancelled。基础设施 LLM 客户端、应用客户端及应用调用契约提供带取消的方法，不带令牌的方法保留默认不主动取消的用法。应用契约的取消包围整个用例 future，流式消费每次等待均观察令牌；取消返回错误并释放内部流，不把它当作成功耗尽来执行后续会话保存。管理端试用和客户端流式调用显式传入 HTTP 请求令牌。

公共宿主在最外层为每个请求创建独立令牌，处理 future 提前释放时发出取消；响应体承接同一个生命周期守卫，转发帧、trailer、长度提示与结束状态，正常完成时解除守卫，错误或未完成即丢弃时取消。HEAD、204、304 和信息响应按省略正文处理。没有把取消绑到整个连接或用户登录态。实现基于当前 Hyper 的请求 future 所有权和 HTTP/2 reset 分支，并使用 [CancellationToken 的运行与释放语义](https://docs.rs/tokio-util/latest/tokio_util/sync/struct.CancellationToken.html#method.run_until_cancelled)。这不等于证明所有传输异常已实时传播：最后一帧交给传输层之后的写入结果、客户端半关闭、WebSocket 升级以及远端数据库命令的取消仍需实际核验。

模板参数模型改为 LlmParameters，由调用方选择 Ordinal 或 OrdinalIgnoreCase；从原始 HashMap 转换默认精确匹配。管理端按忽略大小写方式构造，构造时拒绝等价键重复，后续 insert 按选择的比较方式替换原值。必填判断和模板渲染复用参数模型的 get，不再由应用服务硬编码正式调用大小写规则。Unicode 一对一映射移入现有公共字符串助手，原有函数完整保留。当前支持仓库实际使用的这两种序号规则，尚未提供任意 .NET 自定义比较器或语言相关比较器。

该轮核查还确认了客户端 Demo 的路由、注册条件、数字流和服务代理差异；后续“客户端 Demo 与缓存回源”小节记录对应实现与验证范围。

本轮最终代码通过 Docker 开发容器内全工作区 all-features/locked 的 check、build、严格 Clippy，以及格式和差异空白检查。未新增或运行测试，未安装本机组件，未修改 NetEngine、既有数据库或外部提供商。本轮验证以源码调用链和构建检查为主，未把它作为真实断连、HTTP/2 reset 或远端数据库取消已验证的证明；整体目标保持未完成。


## 客户端 Demo 与缓存回源

对照当前 NetEngine 的 DemoController、DemoService、Demo2Service、CacheableBehavior 和 DistributedLockLeaseRenewer，客户端演示接口改为始终注册在 `/api/Demo/*`，移除 debug-demo 特性门槛，加入全部九个接口的 OpenAPI 描述。普通 LLM 示例调用前保留获取 ID 的行为。宿主局部服务通过现有服务工厂宏装配，第一个示例通过接口调用，第二个返回具体服务；只借用共享宿主基础设施。

加法缺省参数为零，有符号整数加法使用 wrapping_add 保留源未检查溢出的语义。Add2 使用完整 ArticleDto，包含封面 FileInfoDto，长整数输入规范为字符串 ID，创建时间支持带偏移或本地 ISO 日期。引用参数示例调用同步日志宏，计算结果后将引用参数设为 2019。logging 支持同步或异步方法，默认识别 Result（包括别名），普通返回值显式 result=false；未完成调用释放或 panic 展开记录异常阶段。

cacheable 使用宿主 DistributedCache 与分布式锁，成功值按声明类型直接序列化和反序列化，移除方法缓存附带的 HybridCache 和 PartialEq 往返要求；独立 HybridCache 组件保持可用。两个示例有效期均为 120 秒，Add2 的完整文章序列化表示参与参数键。回源锁覆盖二次读取、业务和写入，默认 60 秒租约，每三分之一租约（最多二十秒）续期；失败在安全期限内按最多一秒间隔重试，丢失租约只记录错误，不重跑业务。正常结束等待释放，取消释放句柄和续期 future。缓存读写、锁获取失败保留降级回源，参数键不完整时旁路；零有效期按无效配置失败。

共享 JsonStream 按消费进度输出 JSON 数组，不收集整个序列、不添加对象缓存或 ETag 标记；错误终止响应流。数字数量为 i32，默认五个，负数为空，不再截断一百个。第一种每项等待 50 毫秒，第二种准备 100 毫秒后每项等待 30 毫秒。

在 Docker 内启动隔离的 PostgreSQL、Redis 和现有客户端二进制，实际 HTTP 调用确认 Echo 默认值、Add 缺省零值、i32 溢出结果、ARef 结果、负数空数组、完整 101 项以及旧路径 404。curl 分块记录显示第一种约 50 毫秒逐项返回，第二种约 100 毫秒后开始、每项约 30 毫秒；响应为 JSON 与 chunked。两个不同文章请求均返回 5，Redis 中生成不同键；重复请求命中时剩余有效期继续递减，没有重置为 120 秒。缺失标题返回 400，ARef 日志存在同 trace_id 的 executing/executed。临时服务与目录已清理，未连接既有数据库或真实 LLM 提供商。

全工作区 all-features/locked 的 check、严格 Clippy 与 build 通过；未新增或运行测试，未安装本机组件。本轮没有证明长时间回源的续期故障、断连后的服务端资源释放、日志参数/结果/堆栈捕获、所有日期和绑定错误文本、内容协商与流起始失败行为完全一致。logging 的流结果捕获和完整代理行为仍需继续对齐，其余框架功能也尚未完成最终逐项审计，整体目标继续保持未完成。


## 方法日志快照与共享租约生命周期

对照当前 LoggingBehavior、AutoProxyGenerator、AsyncStreamResultSnapshot 和 ConcurrencyLimitBehavior，日志属性补充独立的命名参数快照、成功返回值、异常信息和调用后引用参数刷新。载荷使用 event、method、traceId、args、durationMs、result、exception，单元值不记录 result。泛型按声明处可用的 Serialize/Display 能力捕获，其他类型回退类型名称；取消令牌使用固定占位符。复用已有递归 Serde 遍历，日志模式独立于缓存键的类型、项目数量与浮点零规范化规则，当前路径重复引用写 null；格式化或序列化 unwind 不覆盖业务结果。业务 panic 记录后恢复原始 payload，普通异步调用提前释放仍记录未完成。

异常包含具体错误类型、消息和直接 Error::source 消息。Rust 标准 Error 没有统一的原始抛出堆栈接口，目前 stackTrace 是日志边界的捕获，无法取得的 innerSource/innerStackTrace 为 null，不能将此实现视为还原 .NET 异常现场。引用返回值仍占用可变参数时，可显式 refresh_arguments=false；普通所有权参数只保留调用前独立快照。

新增显式 stream=true 模式和 LoggedStream。准备阶段失败直接传播；首次 poll 才创建日志状态，未枚举的流不记录调用。结束或提前释放输出枚举数量、自然完成标记、截断标记和最多前 100 个元素的独立快照；元素捕获受 Information 门控，错误或 panic 进入 exception 而不伪装自然完成。普通元素和 Result 元素分别支持，常见装箱流自动保持装箱形式，自定义别名可显式指定。Rust 流是单次消费对象，每次函数调用创建独立状态；参数快照在返回流前取得，尚未证明所有延迟枚举、内部可变性和 .NET 重复枚举场景等价。

缓存回源与 concurrency_limit 共用租约运行器，操作期间自动续期，正常结束等待释放，取消或 unwind 由句柄释放。非正并发数按 1，非正租约按 60 秒处理。并发阻断与参数键失败通过 LockError 保留内部错误分类，不再转为 AppError 的普通参数错误；源码中的无效操作异常应走共享内部异常响应。

Docker 全工作区 all-features/locked 的 check、严格 Clippy、build 通过。在隔离 PostgreSQL/Redis 中运行现有客户端，ARef(7,11) 返回 18，开始日志为数值 a=7/b=11，完成日志为 a=2019/b=11/result=18，traceId 一致；溢出调用返回并记录 -2147483648。Add/Add2 仍返回 5，无缓存降级日志。实际 Swagger 包含九个 Demo 路径，加法参数默认 0、数字流默认 5 且均为可选。临时服务和目录已清理。

未添加或运行测试，未安装本机工具。流模式的实际运行、日志快照复杂循环对象、序列化失败、panic 恢复和长时间租约故障未在本轮实际验证，编译通过不能替代这些证据。当轮发现的 retry 取消分类缺口在后续小节处理；自定义行为管道、完整代理组合和其他框架模块也需要继续对照，整体翻版目标保持未完成。

## 重试取消与支付证书响应

重新读取当前 NetEngine 的 RetryBehavior 与 AutoProxyGenerator：取消异常直接传播；普通异常先检查次数耗尽，再观察调用方令牌、记录 scheduled 并进入可取消的延迟；零次重试直接调用目标方法，不主动取消已执行的方法体。Rust retry 按这一顺序处理，取消错误不再进入重试或耗尽日志。增加独立的 cancellation 基础契约，供 common、llm 与宏运行时共同依赖，避免基础设施反向依赖应用层或重型公共工具。AppError、LlmError 保留各自现有取消分类，实现 CancellationError 和 From<OperationCancelled>。

宏通过显式 cancellation 表达式选择令牌，正数等待支持取消。自定义错误通过公共契约或 is_cancelled 判定，不以消息文本或任意内部错误链识别取消。当前仍沿用 Copy 参数约束，令牌作为方法参数时借用传入；这与 .NET 自动选择首个 CancellationToken 参数的语法不同。尚未证明所有自定义错误、参数别名和宏组合均等价。

核对实际调用点时确认 NetEngine 的 PayService 未启用重试，Rust 移除证书下载方法额外添加的自动重试。同时补齐证书下载响应验签：保留原始响应正文和签名头，解密本次返回的平台证书，再用序列号选出的证书验证该响应，成功后才允许调用方缓存；验签失败返回内部错误。该流程复用已有 RSA/X.509 验签实现，没有请求真实支付提供商。

最终代码已通过 Docker dev 容器中全工作区 all-features/locked 的 check、严格 Clippy、build，以及格式和差异空白检查。取消分类默认分支曾随现有支付调用点完成编译，移除该额外重试后仓库没有业务重试调用点；带令牌与自定义判定的宏展开尚未通过实际业务调用核验，不能把运行时辅助函数编译通过当作所有展开路径已验证。未添加或运行测试、未安装本机组件、未修改 NetEngine 或既有数据库。

继续核对的具体差异包括支付证书缓存写入失败与回读行为、支付锁的期限与续期、全部支付业务接口和提供商协议，以及前文尚未完成的代理管道和框架全量审计。缓存与锁的后续处理见下一节；整体目标保持未完成。

## 微信支付缓存与证书回源

重新沿 PayService 调用点核对 Common/IDistributedCacheExtension，确认源对象 GetAsync 将读取及反序列化失败视为未命中且不删除缓存，字符串 GetStringAsync 则传播读取错误；SetAsync 扩展吞掉普通写入失败。Rust 支付对象读取改为对应降级路径，取消了删除损坏缓存和额外警告。PayContext 由宿主提供必需的缓存引用。预支付移除额外的分布式锁和二次读取，使用源逻辑键且不对订单号 trim；JSAPI/App 有效期 6480 秒，H5/PC 原始字符串有效期分别为 270/900 秒。

证书逻辑键为商户号加 GetWeiXinPayCertificates，锁键再加 lock。读取未命中时获取默认六十秒锁，保留锁内二次读取、不自动续期，正常成功和失败均等待释放；取消依赖句柄 Drop 释放。证书下载和响应验签完成后尝试写入，再读取缓存；写入失败且回读仍为空时返回内部错误“证书获取失败”，不直接返回内存下载值。缓存保留 data、有效期、加密元数据及解密 certificate，替代原有二元数组。证书时间读取支持提供商 ISO 格式及源缓存的 yyyy/MM/dd HH:mm:ss zzz，写入使用后者；尚未覆盖 DateTimeOffset.TryParse 的全部区域性输入或所有畸形 JSON。

修正四种微信下单的通知地址拼接，保留处理器传入的尾部斜杠后追加商户号。JSAPI/App 的 timeStamp 改为字符串，缓存输入同时接受数值和字符串，与源 LongConverter 的输出约定对应。

Docker 内创建独立 PostgreSQL/Redis，使用现有 repository_tool 完成业务迁移，再运行现有客户端二进制。向隔离缓存放入对象和原始链接后，四种实际支付创建接口均返回 200；两种时间戳输入均返回字符串，H5/PC 返回未加 JSON 引号的原始链接。损坏对象缓存仍保留原字节并进入订单查询；Redis WRONGTYPE 在对象读取时降级到订单查询，在 H5 字符串读取时返回 Development 内部错误 500。真实 Swagger 的两种 timeStamp schema 均为 string。该核验未创建交易或访问支付平台，临时宿主、数据库、Redis、配置及密钥均已清理。

最终代码通过容器内全工作区 all-features/locked 的 check、严格 Clippy、build，以及格式和差异空白检查；未新增或运行测试，未安装本机组件。缓存写入实际有效期、完整证书下载和验签、写入后回读失败及长时间锁超时尚未实际验证。后续需继续处理源创建接口的可空结果、订单和配置选择规则、请求签名与支付协议；当前 Rust 仍有源服务未要求的订单归属/状态预检，不能认定支付模块或整体框架已完成对齐。

## 微信创建接口的空结果与配置选择

对照当前 PayController、PayService、客户端 Program 和公共授权注册，四个 CreateWeiXinPay 创建接口没有 Authorize 标注或全局 fallback 授权。Rust 移除这四个路由额外的登录/权限包装与处理器 UserContext 参数，应用契约改为 Option 返回；支付宝入口与其他服务不随此批修改。订单查询保留软删除过滤，按原始 OrderNo 查询，不增加用户归属、待支付状态或 trim 条件。

订单不存在，或模块内缺少 AppId/MchId 时返回 None；JSAPI/App 缺少模块内首个 MchApiCertKey 时也返回 None。HTTP 入口映射为空体 204，依据源默认 MVC 的 [HttpNoContentOutputFormatter](https://raw.githubusercontent.com/dotnet/aspnetcore/release/10.0/src/Mvc/Mvc.Core/src/Formatters/HttpNoContentOutputFormatter.cs)；不改动共享 JSON 提取器的其他响应行为。非空缓存优先返回，创建空结果不写缓存；H5/PC 原先缓存为空字符串且无可创建结果时保留空字符串，返回 200 空文本。

创建字段按模块内各键的首值读取，不强制位于同一组；HTTP 请求再按商户值定位 GroupId，组内必需的证书序列号、签名私钥和 V3 密钥缺失时返回内部错误。可空 GroupId 显式查询 IS NULL。JSAPI/App 调起签名仍使用创建阶段的模块首个私钥，不用商户组私钥替换。微信 RSA 签名复用公共密码助手，接受 PEM 及 Base64 DER 的 PKCS1/PKCS8 格式。金额乘一百使用可检查乘法，中点取偶数；响应字段通过已有 JSON 助手读取，缺失时返回 code:message，不额外拒绝显式 null 或空字符串。

在独立 PostgreSQL/Redis 中迁移并运行现有客户端：不带令牌访问四种接口，缺失订单或配置均返回 204 且正文为零字节；匿名缓存命中返回 200。已支付、无创建人的订单可进入配置流程；只有 AppId/MchId 时 JSAPI/App 返回 204，而 H5/PC 进入商户配置检查。AppId 位于另一组、商户及三个 HTTP 配置位于 NULL 组时，四种调用均进入本地 RSA 解析阶段，使用故意无效的隔离密钥在发起 HTTP 前结束。软删除订单及去掉空格后的不同订单号返回 204，包含原空格的订单号能查询到记录。原始空链接缓存且无订单返回 200 空文本。Swagger 显示四种接口无 security 要求且包含 204 响应。没有请求真实支付提供商或创建交易，临时进程、容器、配置和密钥已清理。

最终代码通过 Docker 全工作区 all-features/locked 的 check、严格 Clippy、build，以及格式和差异空白检查；未添加或运行测试、未安装本机组件。未验证成功支付响应、两套不同有效签名私钥的完整调用、退款和回调的全部分支。支付宝创建接口仍有授权、可空返回和查询规则差异，其余代理管道、基础设施与业务模块也仍需全量复核，整体目标继续保持未完成。

## 支付宝创建入口与服务端响应验签

三个支付宝创建入口移除源模板没有的登录与权限包装，应用契约以 Option 表达空结果，HTTP 映射为空体 204。先按 AliPayWeb/AliPayMiniApp 模块读取 AppId、AppPrivateKey、AliPayPublicKey 各键首值，不要求同组，不把空字符串等同于缺失；再按原始订单号查询有效订单。PC/H5 只检查 State 为“待支付”，不额外拒绝 PayState=true 或无创建人的订单。小程序查订单创建人的有效外部绑定，不使用当前 HTTP 用户，也不增加订单状态检查；空 OpenId 与缺失绑定保持区别。

网页链接在本地签名生成，H5 的 quit_url 仅位于 biz_content，保留空字符串；协议参数只过滤空字符串、不裁剪空白。RSA2 使用公共密码助手，接受 PEM 和 Base64 DER。签名时间使用进程本地时间，服务端下单字符集按源调用使用 utf-8，退款使用 UTF-8；请求携带 charset 查询参数和对应 Content-Type。alipay_sdk 元数据对应 NetEngine 当前实际引用的 AlipaySDKNet.Standard 4.9.1424，这是协议兼容字段，不表示 Rust 依赖 .NET 运行时。

依照官方 [DefaultAopClient](https://raw.githubusercontent.com/alipay/alipay-sdk-net-all/master/v2/AlipaySDKNet.Standard/DefaultAopClient.cs)、[AopJsonParser](https://raw.githubusercontent.com/alipay/alipay-sdk-net-all/master/v2/AlipaySDKNet.Standard/Parser/AopJsonParser.cs) 和 [AopResponse](https://raw.githubusercontent.com/alipay/alipay-sdk-net-all/master/v2/AlipaySDKNet.Standard/AopResponse.cs)，服务端调用增加响应验签。通过 RawValue 保存对应业务节点或 error_response 的原始 JSON，不重新序列化；成功响应必须有签名，带非空 sub_code 的错误响应仅在附带签名时验签。保留 SDK 的空公钥跳过和转义斜杠重验规则。重复根字段拒绝解析，签名失败返回内部错误；错误诊断不再附带完整原始响应。所有根字段去重比 SDK 只检查重复业务节点更严格，未宣称畸形协议输入逐字等价。

使用独立 PostgreSQL/Redis 和现有客户端二进制实际核验：三个匿名创建入口在配置缺失时返回 204；PC/H5 缺失、已支付状态、软删除以及去掉订单号空格后的不同记录均返回 204。模块配置分散在不同组时，State=待支付 且 PayState=true、没有创建人的订单仍生成 200 链接，保留原始订单号。使用容器内生成的临时 RSA 密钥和 OpenSSL 对返回链接验签通过，金额、商品描述、PC 二维码模式、H5 空退出地址和通知地址符合当前源调用；TZ=Asia/Shanghai 时协议时间对应本地时间。补齐 alipay_sdk 后重新编译并运行，两个链接仍验签通过。Swagger 三个入口均无 security 声明并包含 204。

小程序缺少创建人或绑定返回 204；对已支付订单添加创建人的有效空 OpenId 绑定后进入本地 RSA 签名阶段，使用故意无效的隔离私钥在出站请求前终止。软删除该绑定后恢复 204。没有连接真实支付平台、创建交易或退款，临时宿主、数据库、缓存、密钥与配置目录均已清理。

最终代码通过容器内全工作区 all-features/locked 的 check、严格 Clippy、build，以及格式和差异空白检查；未添加或运行测试、未在本机安装组件。新服务端响应验签已编译并按 SDK 审查，但未用真实平台响应验证，也未搭建模拟提供商。小程序源方法对非空 trade_no 抛错的疑似反向判断已提出确认，当前提取成功交易号的行为暂未改动，不能记为已对齐。

下一步明确差异：AliPayNotify 的返回值、配置组选择、订单更新条件和审计行为仍与源方法不同；源退款方法仍是无参占位调用，Rust 现有退款用例包含额外业务规则。完整支付协议、代理组合及其他框架模块的全量审计仍未完成，整体目标保持未完成。

## 支付宝回调与表单字段

重新核对当前 PayController.AliPayNotify、PayService.AliPayNotifyAsync、RepositoryDbContext 保存前处理及 SaveChangesWithUpdateLogAsync，确认回调调用普通 SaveChangesAsync：有修改时补充 UpdateTime，不生成 DataUpdateLog。Rust 删除回调专属的额外审计写入，保留原始 xmin 的 update_versioned；对所有支付字段完全相同的通知跳过写入，对已支付但内容变化的通知仍执行保存。

回调先用 Module.StartsWith("AliPay")、AppId 键和值查应用所在组，再读取该组首个 AliPayPublicKey，不额外要求私钥或同 Module。没有匹配应用时按源 FirstOrDefault 可空结果选择 NULL 组，显式使用 IS NULL。RSA2 验签复用公共密码助手，排除 sign/sign_type 和空值；签名不匹配返回 None，缺失签名、无法解析签名或公钥等异常走内部错误。规则依据官方 [AlipaySignature.RSACheckV1](https://raw.githubusercontent.com/alipay/alipay-sdk-net-all/master/v2/AlipaySDKNet.Standard/Util/AlipaySignature.cs) 及本地源调用链。

验签后按原始订单号查询未删除记录，取消源方法没有的 trade_status、已支付短路和金额相等预检。保存通知金额、可空流水号、支付时间及已支付状态；仅订单 Type 为“业务逻辑”返回 success，其余类型保存后返回 None。空字典、签名不匹配和缺失订单均映射为 204；无效金额或时间按源转换异常归为内部错误。无偏移的支付时间使用宿主本地时区，不再固定东八区；缺失 gmt_payment 按 DateTime.MinValue 转换并处理最小 UTC 边界。

共享 FormFields 支持普通表单和 multipart。复用已有参数合并函数，保留首次字段名，对 ASCII 大小写相同字段按出现顺序逗号连接；不合并 Query，文件流式消耗且不加入字段字典。支付宝处理器使用该提取器，OpenAPI 增加 204 并去除原有验签 401 声明。当前仍沿用宿主/提取器的大小限制，未认定所有畸形表单、非 ASCII 大小写、ASP.NET FormOptions 上限或解析错误文本完全等价。

在独立 PostgreSQL/Redis 中迁移并运行客户端，使用容器内临时 RSA 密钥对真实 HTTP 回调签名。空表单、缺失/软删除订单、格式正确但不匹配的签名返回 204；业务逻辑订单返回 success，其他类型返回 204，但两者均保存支付字段。配置应用位于 AliPayCustom，公钥位于同组 OtherModule 时成功；未知应用回退 NULL 组也成功，无需配置私钥。普通表单和 multipart 的重复 memo/MEMO 均合并后验签，multipart 同名文件不影响字段签名。非法金额或 Base64 签名在 Development 返回 500，订单保持未支付。

数据库确认通知金额与原订单金额不同且 trade_status 为 WAIT_BUYER_PAY 时仍按源方法保存；TZ=UTC 下无偏移支付时间保存为同一 UTC 时刻。缺失流水号保存 NULL，缺失支付时间保存公历最小 UTC。相同签名通知再次发送时 xmin 和 UpdateTime 均不变；修改金额及流水号并按合并字段重新签名后，已支付订单继续更新。整个过程 DataUpdateLog 行数为零。临时客户端、数据库、缓存、配置及密钥均已清理，没有调用支付提供商。

全工作区 all-features/locked 的 check、严格 Clippy、build，以及格式和差异空白检查通过；未添加或运行测试、未在本机安装组件。本轮没有验证真实支付平台、并发回调冲突或完整区域性日期/小数解析，不能以编译和有限运行分支证明所有输入等价。微信回调、退款占位契约、小程序返回分支以及此前记录的框架全量复核仍需继续，整体目标未完成。

## 微信通知绑定、解密与确认

对照当前 PayController、PayService.WeiXinPayNotifyAsync 及四个通知模型，修正 HTTP DTO 的字段命名：保留 event_type、create_time、resource_type、original_type、associated_data 等下划线名称，补齐源声明的必填字符串和摘要，创建时间缺失时使用最小日期。新增共享 JsonWithBody，复用 WebAPI 的大小写绑定和错误响应，同时保留 UTF-8 原文用于验签，不通过重新序列化获得正文。重复请求头按 StringValues 的逗号连接规则传入服务。交易、付款人、金额及退款明文模型归属 application_model，引用字段可空、金额整数缺失为零，避免给内部 JsonHelper 解析额外施加 MVC 必填约束。

平台证书读取改为仅接受商户号；缓存未命中并取得锁后才加载该商户的传输配置。缓存命中不要求 AppId、MchId 配置记录、API 私钥或 V3 密钥。通知需要解密时单独查询 WeiXinPay 模块首个 MchApiV3Key，不将回调密钥改为商户配置组密钥；证书下载仍使用商户组内密钥。已有缓存回读、默认租约锁及正常结束等待释放保持不变。

服务内统一捕获处理异常并维护成功标记。交易事件必须为 TRANSACTION.SUCCESS、明文状态为 SUCCESS 且存在有效订单才确认；已支付记录直接确认，未支付记录按通知金额除以 100 保存，不额外检查 AppId、商户号或原金额相等。保存继续使用 xmin，并取消额外 DataUpdateLog。缺失交易流水号保存 NULL，缺失成功时间保存最小日期，不以当前时间替代。退款 original_type 为 refund 时必须解密并解析对应 DTO；源模板尚未执行退款业务动作，不依退款状态改变确认结果。未知事件、无订单和处理失败返回 FAIL/失败，HTTP 为 501；成功 message 保留 null，不泄漏内部异常正文。源方法两个独立分支及异常前已取得的成功标记保持原有顺序。

在独立 PostgreSQL/Redis 中运行现有客户端，使用容器已有 OpenSSL 生成临时 X.509 证书、RSA 签名及 AES-GCM 密文，未安装额外组件或启动模拟支付平台。缓存放入该商户的平台证书，数据库只放置另一组的模块 V3 密钥，没有 AppId/MchId/API 私钥配置。真实 POST 通知成功保存订单，证明缓存命中不依赖传输配置且按模块读取解密密钥。明文 AppId/商户号不同、金额不同仍按源保存；含缩进和中文正文可验签，外层 EVENT_TYPE 大写也通过共享绑定，原文未被改写。

缺失/软删除订单、非成功交易状态、未知事件、不匹配签名和损坏密文均得到 501 与固定失败文本。退款 ABNORMAL 状态在正常解密解析后得到 200，坏密文和非法金额模型得到 501。HTTP 缺少 summary 或错误使用 eventType 返回 400，Swagger 展示下划线字段及必填要求。缺失交易流水号/时间分别保存 NULL 和最小 UTC 日期；重复已支付通知返回成功且 xmin/UpdateTime 不变，DataUpdateLog 保持零行。删除隔离证书缓存后，缺少传输配置在出站请求前终止并返回固定失败结果。临时进程、数据库、缓存、证书、密钥和配置均已清理。

全工作区 all-features/locked 的 check、严格 Clippy、build，以及格式和差异空白检查通过；未添加或运行测试、未安装本机工具。未访问真实微信服务，也未证明所有证书回源故障、并发通知、HTTP 大小限制、内部 JsonHelper 大小写及区域性日期解析与 .NET 完全一致；公共 JsonHelper 的反序列化规则仍需跨调用点复核。退款方法的源占位契约、小程序返回分支及框架其他模块全量审计仍未完成，整体目标保持未完成。


## 退款模板与内部 JSON 解析

再次对照当前 PayService 四个退款方法及全仓调用点，确认源实现是无参、无业务返回值的模板流程，没有对应 HTTP 路由。Rust 契约和服务实现改为相同形态，删除源模板没有的请求 DTO、订单归属与退款金额预检、额外结果映射。微信退款保留模块配置存在性判断，传输配置仍按商户分组；退款和查询使用源模板的空标识及占位金额。支付宝按 AliPayWeb 各键首值装配，保留 UTF-8、RSA2 及空业务参数，业务错误结果分支不添加源方法没有的异常或更新动作。微信退款响应模型补齐全部字段，引用字段可空、整数缺失为零。这些方法仍需要业务接入，未调用真实提供商进行退款验证。

内部 JsonHelper 的字段匹配原先仍区分大小写，支付调用点还绕过该助手直接使用 serde_json。将已有 WebAPI 字段遍历实现下沉到 common::json_helper::input，HTTP 层继续负责 URL 解码和请求提取，内部 JSON 单独选择规则：模型声明字段按 OrdinalIgnoreCase 匹配，字典和动态对象键保留原样，同名模型字段使用最后一个输入值；内部空字符串保留，不套用 MVC 必填判断。内部 i64 允许数字字符串，i32 等普通数值仍要求 JSON 数字；WebAPI 原有数字字符串与 Query 首值规则保持原行为。支付对象缓存、通知解密明文、退款响应和证书响应统一接入 json_to_object；克隆助手没有一并套用大小写不敏感规则。

增加内部日期转换器，支持 ISO、yyyy/MM/dd HH:mm:ss zzz、带短横线的空格分隔格式以及无偏移本地日期，校验年份、UTC 范围和十四小时偏移并保留百纳秒精度。支付通知明文、退款响应与证书复用该转换器；HTTP 外层通知仍使用 ISO 输入规则，不对所有字符串做日期猜测。

在独立 PostgreSQL/Redis 中迁移并运行现有客户端二进制：大写预支付缓存字段成功返回，重复 APPID/appId 取最后值，空 package 与空白 nonce 保留，数值时间戳返回字符串。大写证书缓存字段、空加密元数据和斜杠日期可读取。使用临时 OpenSSL 证书及已有 libcrypto 加密、签名后发送实际微信通知，大写交易字段和金额子字段解析成功，2026/11/07 12:34:56 +08:00 保存为 04:34:56 UTC，实付金额为 4.56；字符串 total 被拒绝，关联订单保持未支付。退款明文的无偏移本地时间和最小日期可解析，非法月份与 +15:00 偏移返回固定 FAIL；外层非 ISO 日期返回 400。

实际 Demo 请求同时确认 Query 大小写重复字段取首值，HTTP 模型大写字段与数字字符串可绑定；必填字符串为空白及布尔字符串仍返回 400。全工作区 all-features/locked 的 check、严格 Clippy、build 和格式检查通过。临时宿主、数据库、Redis、配置和密钥已清理；没有请求支付平台，没有添加或运行测试代码，没有安装本机组件。

尚未覆盖 DateTimeOffset.TryParse 的全部区域性输入与夏令时边界、全部 serde 自定义/扁平化模型、先出现非法值后被重复字段覆盖的行为，以及 JSON 最大深度和 Clone 引用保留等规则。公共序列化、其他内部 JSON 调用点、支付宝小程序源分支及前述框架全量审计仍需继续，不能据此认定支付模块或完整翻版已完成。


## JSON 长整数与任务参数复核

对照 Common/JsonHelper、LongConverter、QueueTaskProducer、QueueTaskStore 及任务运行器，ObjectToJson 和 ObjectCloneJson 都注册 long 转换器；JsonCloneObject 使用大小写敏感的 cloneOpts。Rust 公共写入助手增加按 Serde 原始类型递归的 i64 字符串转换，覆盖可空值、集合、字典值、结构和枚举包装，不把 i32/u64 或已形成的 serde_json 文档数字一并改为字符串。字典键交给原 JSON 序列化器，模型名称仍由 Serde 声明决定；Value、Number、RawValue 及其常见透明指针包装保留自身序列化协议。

克隆读取使用单独模式，字段精确匹配、i64 接受数字或数字字符串、其他数字保持严格类型，内部空字符串不受 HTTP 必填验证影响；clone 对象助手复用同一编码和读取流程。共享输入适配增加拥有所有权的 RawValue 子树接收，保持对象键及数值内容，不施加字段匹配；此适配由已解析子树重新编码，不能用于要求保留原始空白或原始正文签名的场景。

GetValueByKey 改用原始 JSON 值，按照 [JsonElement.ToString 与 GetProperty](https://raw.githubusercontent.com/dotnet/runtime/release/10.0/src/libraries/System.Text.Json/src/System/Text/Json/Document/JsonElement.cs) 的语义处理：按键精确匹配且重复键取最后值，null 为空字符串，布尔值为 True/False，字符串解码，数字、数组和对象保留该值的原始 JSON 文本。缺失键、非对象根节点和无效 JSON 返回 None。common 显式声明 serde_json 的 raw_value 功能，不依赖其他 crate 偶然启用该功能。

在独立 PostgreSQL/Redis 中迁移并运行现有 task_service，通过 Debug 菜单仅启用 ShowName、SendSMS、CallPhone、ShowNameSuccess 四个既有示例队列处理器，没有启用真实 MessageTask.SendSMS。两条初始任务分别携带中文、换行和引号，以及空字符串；运行后主任务、子任务、独立电话任务和结果回调共八条全部成功，每条执行次数为一。数据库中的 Parameter、CallbackParameter 均保留 JSON 字符串和值，父子关系及完成标记正确。临时宿主、数据库、缓存、配置与日志已清理，没有调用外部短信或支付平台。

最终全工作区 all-features/locked 的 check、严格 Clippy、build、格式和差异空白检查通过。没有新增或运行测试代码，没有在本机安装组件。实际运行覆盖现有字符串任务链路，i64 泛型转换、动态文档包装、RawValue 和按键读取的新增边界主要依据代码审查及编译，未增加专用测试程序或接口。

普通对象的全局日期/命名策略、克隆引用标识和循环引用、所有自定义 Serde 类型、JSON 深度与非法重复字段处理尚未完成通用对齐。现有短信 DTO 的专用引用元数据适配保持原样，不能把本次长整数修复解释为完整 ReferenceHandler.Preserve 已实现。整体框架目标仍未完成，后续继续复核这些公共能力及其他模块。


## 默认任务清单与演示空参数

重新枚举当前 NetEngine 的 QueueTask/ScheduleTask 声明，源宿主只有 MessageTask.SendSMS、MessageTask.SendEmail、ShowName、SendSMS、CallPhone、ShowNameSuccess 六个队列任务及 ShowTime 一个定时任务。Rust 删除默认注册中的三项 Token 清理任务和三项旧名称别名；清理仅供这些任务使用的处理器、宿主适配器及 application_service_task_service crate，工作区成员、宿主依赖、锁文件和 README 对应关系同步更新。当前 NetEngine 没有 Application.Service.TaskService 项目，不再保留这条错误映射。未删除或迁移用户数据库中的旧任务记录。

邮件任务名称改为 MessageTask.SendEmail，默认并发由四改为一；短信默认并发一，四个演示队列任务并发五，ShowTime Cron 保持 0/1 * * * * ?。源同步方法没有填充默认备注，因此移除 Rust 内置定义的额外备注；通用自定义任务注册能力仍保留。

四个演示处理器读取 Option<String>，ShowName 原样传递可空姓名和返回值；日志显示时才将空姓名显示为空文本。数据库 NULL、JSON null 与空字符串应保持区别，不把 null 改写为空字符串后入队。早期实现曾将缺失参数补为 JSON null，此推断与源宿主不符，已在后文“队列缺参、重试与双实例领取”阶段修正。ShowTime 在某次投递失败时记录 DemoTask.WriteHello 并结束本轮，匹配源方法的异常捕获，不再将这个已处理的错误报告给调度运行器。

当时在独立 PostgreSQL/Redis 中仅运行 Rust Debug task_service，菜单恰好显示七个默认任务，TaskSetting 表保持零行。分别投入缺失参数、JSON null、空字符串和普通字符串四条 ShowName，观察到派生任务共十六条均成功；这一结果未与源宿主配对，不能作为空参数语义已对齐的证据，后续双宿主验证已证实并修正差异。随后仅在隔离库增加拒绝新 ShowName 的约束，运行既有 ShowTime，实际记录到捕获日志，运行器失败日志为零，队列总数保持十六。约束已移除。

容器内额外完成 task_service Release 构建并实际运行。TaskSetting 自动生成七条且全部禁用，名称、两个消息任务并发一、四个演示任务并发五和 ShowTime Cron 与源声明一致；备注和参数全部为 NULL，没有额外清理任务或旧别名。临时 Debug/Release 宿主、数据库、缓存、配置及日志均已清理，没有调用真实邮件或短信服务。

最终全工作区 all-features 的 check、严格 Clippy、locked build、格式及差异空白检查通过。移除工作区成员时仅由容器内 Cargo 同步锁文件，未升级依赖或安装本机组件；未新增或运行测试。此次验证只证明默认注册、上述演示分支及配置初始化，邮件业务参数、附件处理和 SMTP 调用尚未据此完成对齐：当前 Rust 邮件 DTO 仍采用自己的字段/消息编码，需要继续与源 SendEmailDto 及引用元数据对照。此前公共 JSON、框架能力和全量业务复核仍未完成，整体目标继续保持进行中。


## 邮件契约、列表参数与附件目录

对照当前 IMessageService、MessageTask、SendEmailDto 和 MessageService，邮件任务改为经消息服务契约调用单一 send_email 用例。移除旧嵌入式附件消息及两套参数回退，模型使用 PascalCase 字段，引用字段保留可空性。SendEmailDto::to_queue_json 提供对象和列表引用元数据，长整数 ID 由公共克隆编码写为字符串；任务读取接受普通数组和有序的 $id/$values 列表包装。该列表处理只作用于 Clone 模式的集合目标，不全局改写字典或动态 JSON。元数据顺序与值类型依据 [.NET 10 元数据读取实现](https://raw.githubusercontent.com/dotnet/runtime/release/10.0/src/libraries/System.Text.Json/src/System/Text/Json/Serialization/JsonSerializer.Read.HandleMetadata.cs) 核对，尚未实现完整的引用解析器。

邮件附件以宿主启动时内容目录为基准，删除备用根目录配置与搜索。按原始 ID 顺序处理有效文件记录，缺失和软删除记录报告内部错误；重复路径按字符串比较，既拒绝相同路径，也保留路径文本不同但指向同一文件的源行为。启用云存储时总是下载，失败保留已下载文件，只有发送成功后才删除本次下载路径。本地存储不构造下载适配器。SMTP 附件固定 application/octet-stream，保留记录文件名；缺少显示名称时不再用账号补齐，没有附件时使用单一正文。收件人格式错误作为内部错误，不回显地址。

在独立 PostgreSQL/Redis 中迁移并启动现有 Debug task_service，只启用 MessageTask.SendEmail，SMTP 限定到容器本机确认未监听的端口，未联系外部邮件服务。14 条任务各执行一次：带 $id/$values 和字符串文件 ID 的参数、普通数组、null/空附件列表进入 SMTP 并因连接拒绝失败；仅存在于备用目录的文件不能读取；软删除/缺失记录、重复 ID、两条记录的相同路径在发送前失败；包含 ./ 的不同路径文本成功读取后进入 SMTP。溢出 i64、颠倒列表元数据、字符串布尔值在参数解析时失败；错误字段大小写和 null 收件人保持内部失败。全部任务执行次数为一、状态恢复 Pending、租约清空；本地附件及备用目录文件原文保留。

全工作区 all-features/locked 的 check、严格 Clippy、build、格式及差异空白检查通过。临时宿主、数据库、Redis、配置和日志已清理；没有添加或运行测试代码，没有在本机安装组件。文档同步了邮件入队格式、目录和清理要求。

本轮没有真实 SMTP 成功投递，也没有调用云存储，因此云文件下载、覆盖及成功后清理仅有代码对照证据。源 SMTP 是方法内空配置占位，Rust 继续由宿主配置装配；这属于接入方式差异，不能宣称 SMTP 全部行为等价。完整 MIME/地址编码、空参数异常次序、流式附件内存占用、Clone 的重复引用标识、$ref/循环引用与通用序列化仍需继续。SendEmailDto 编码写入路径目前没有业务调用方，只有编译和代码核对证据。其他框架能力全量复核及支付宝小程序待决定分支仍未完成，整体目标保持进行中。


## 短信契约、失败结果与提供商请求

重新对照 ISmsService、SmsService、SendSMSDto 及两个 ISMS 实现，源提供商返回 Task<bool>，捕获发送异常后返回 false；应用服务直接按 Task 等待，不使用布尔值。因此 Rust 增加应用层 SmsService 契约，将提供商返回值恢复为 bool，内置实现把请求错误转换为 false，应用服务正常完成，不再由发送失败额外触发队列重试。参数解析及根参数为空的错误仍在提供商之前发生，保留任务失败处理。短信发送完成日志不宣称短信送达，也不输出手机号或响应正文。

模型统一为 SendSmsDto，属性 PascalCase，引用字段及字典值允许 null。验证码生产者同步使用此模型，保持手机号原文、空签名和空模板。移除短信服务里的专用 JSON 读取；任务统一经公共克隆绑定处理。克隆模式下，类型化对象和字典允许开头的字符串 $id，并拒绝重复、非字符串或出现在普通字段之后的元数据；字典保留键名大小写、顺序及最后一次重复赋值。动态 JSON 的 deserialize_any 仍保留自身键，不全局删除 $id。完整跨对象引用解析和循环引用尚未实现。

腾讯请求字段按 [V20190711 SDK 模型](https://raw.githubusercontent.com/TencentCloud/tencentcloud-sdk-dotnet/master/TencentCloud/Sms/V20190711/Models/SendSmsRequest.cs) 改为 SmsSdkAppid、Sign、TemplateID，保持 API 2019-07-11 与广州区域。手机号直接拼接 +86，不裁剪或归一化；模板值按字典顺序传递，null 字典在组件内失败，null 值保留。日期签名从同一时间戳生成，避免跨 UTC 日期时两个时钟读数不一致。依据 [SDK 请求处理](https://raw.githubusercontent.com/TencentCloud/tencentcloud-sdk-dotnet/master/TencentCloud/Common/AbstractClient.cs)，只将 HTTP/API Error 视为请求异常，不将 SendStatusSet 的号码级状态另行升级为错误。

阿里云请求依据 [SendSmsWithOptionsAsync](https://raw.githubusercontent.com/aliyun/alibabacloud-csharp-sdk/master/dysmsapi-20170525/core/Client.cs) 和 [ACS3 签名实现](https://raw.githubusercontent.com/aliyun/darabonba-openapi-util/master/csharp/core/Client.cs)，改为 POST、查询参数及空正文的 ACS3-HMAC-SHA256 签名，删除旧 GET/HMAC-SHA1、额外 OutId 和 RegionId。手机号、签名和模板原样传递，可空字段省略，模板字典按 JSON 编码。源组件没有读取业务 Code，因此请求正常结束即返回 true。两家发送器均接收宿主创建的 HttpClientFactory，每次发送获取工厂客户端，取消自行长期持有 reqwest::Client；配置和签名值不再被裁剪改写。相关依赖及锁文件由容器 Cargo 同步。

在独立 PostgreSQL/Redis 运行现有 task_service，分别选择阿里云和腾讯组件，凭据全部为空以确保在出站前结束。每家投入 12 条短信任务：六条可解析参数（元数据、空对象、可空字段、空白原文、重复字典键、大小写不同键）各成功一次，另六条非法值/元数据/根 null 参数各失败一次并进入 Pending 等待重试；正常任务的结果回调各生成一条且成功。另两条邮件任务验证公共元数据变更：合法列表进入 SMTP 空配置错误，非法根 $id 在参数读取时被拒绝。所有记录执行次数为一且租约释放。

随后运行现有 client_webapi，使用临时 EC 密钥和同一隔离存储，健康端点返回 200。实际调用 SendSMSVerifyCode 返回 200/true，生成的任务包含对象 $id=1、字典 $id=2、六位验证码、空签名/模板及保留两侧空格的原始号码；现有腾讯任务宿主消费后成功完成且执行一次。未请求真实短信平台，没有添加测试代码、模拟提供商或临时 Rust 程序。临时宿主、数据库、Redis、配置、密钥和日志均已清理。

全工作区 all-features 的 check、严格 Clippy、locked build、格式及差异空白检查通过，没有安装本机组件。本轮运行证据覆盖参数读取、服务返回、配置失败及 HTTP 入队链路；没有验证真实平台接受签名或响应、SDK 全部超时/重试/凭据来源、所有异常 JSON 响应转换和通用引用对象图。请求协议变更目前依据代码及官方 SDK 对照，不能以空凭据运行认定短信平台发送完全一致。框架其他模块全量复核及此前列出的行为差异仍需继续，整体目标未完成。

## 云存储签名、传输校验与下载期限

对照 NetEngine 当前两个云存储组件，三个 Rust 宿主把共享 HTTP 客户端工厂传给存储适配器。阿里云改为 OSS V4，签名包含区域、桶和对象路径；上传对象键使用远程目录加本地文件名，显示文件名只设置 Content-Disposition，ACL 随请求签入。腾讯云临时地址不签 Host，按原始对象路径计算摘要，已含 AppId 后缀的桶名不再重复追加。两者的临时地址均保留配置前缀原文，不自动补斜杠；公开文件继续返回无签名地址。

前一轮隔离客户端运行生成的回执经复核，两家各包含四条实际文件接口结果，覆盖中文、空格、加号、百分号路径、inline 与公开文件。该轮已离线核对三条私有地址签名，并验证公开地址不带签名。这些回执只支持临时地址生成链路，不能证明真实云平台接受上传、下载和删除，也不覆盖新增的传输检查。对应宿主进程和隔离数据库、缓存已停止；复核后临时目录、回执和临时密钥均已清理。

当前 NetEngine 引用 AlibabaCloud.OSS.V2 0.3.0。本轮按该版本的 [文件传输实现](https://raw.githubusercontent.com/aliyun/alibabacloud-oss-csharp-sdk-v2/0.3.0/src/AlibabaCloud.OSS.V2/Client.Extensions.cs)、[默认参数](https://raw.githubusercontent.com/aliyun/alibabacloud-oss-csharp-sdk-v2/0.3.0/src/AlibabaCloud.OSS.V2/Defaults.cs) 与 [CRC64 实现](https://raw.githubusercontent.com/aliyun/alibabacloud-oss-csharp-sdk-v2/0.3.0/src/AlibabaCloud.OSS.V2/Internal/Crc64.cs) 补齐：

- 上传和下载逐块计算实际内容的 CRC64，在响应包含校验头时按十进制原文比对；算法采用 CRC-64/XZ 参数，不误用非反射的 CRC-64/ECMA-182。上传不预读或整文件缓冲。
- 下载使用共享工厂的流式客户端，响应头单独限制 20 秒；正文采用 81920 字节缓冲，单次读取和写入共享 20 秒期限，每次推进后重置。正文不再受普通客户端整包 100 秒超时限制。
- 文件创建、读取、写入或校验失败时，文件拷贝层最多三次尝试；每轮重新获取对象、覆盖文件并重置摘要，最终失败保留文件。返回重试前等待 Tokio 后台文件写入收尾，避免上一轮写入污染下一轮内容。阿里云不代建父目录。
- 腾讯云下载在网络请求前按源组件的最后一个正斜杠拆分并创建目录，缺少目录部分直接报错；不再等云端响应成功后才创建目录。

新增依赖复用工作区已锁定的 crc 与 futures-util，由容器 Cargo 离线同步依赖边。全工作区 all-features/locked 的 check、严格 Clippy 与 build 已通过；没有新增或运行测试，也没有安装本机组件。CRC、读写超时及失败重试目前经过源码核对和编译检查，尚无真实 OSS 传输运行证据。

剩余差异明确保留：OSS 请求层的服务错误重试、退避与时钟校正、上传重试及超时、自动 MIME 推断、配置校验时机和签名有效期边界；COS TransferManager 的分片、完整性校验、重试与凭据缓存行为。腾讯云源上传把传入目录直接作为完整对象名，而调用方传的是目录；此前已提出该源代码矛盾的处理选择，在收到答复前未改动其上传对象键分支。当前 Rust 分支仍存在显示文件名参与对象键的问题，不能认定腾讯云上传已对齐。整体框架目标继续进行，Admin.App 始终不在范围内。

## OSS 请求重试、上传读取与时钟校正

继续核对 NetEngine 所引用的 SDK 0.3.0，补齐上节中的请求层重试和超时差异。上传、下载和删除共用适配器内部请求执行入口，宿主和应用契约不增加厂商协议分支。

依据 [错误分类](https://raw.githubusercontent.com/aliyun/alibabacloud-oss-csharp-sdk-v2/0.3.0/src/AlibabaCloud.OSS.V2/Retry/ErrorRetryableImpl.cs) 和 [请求执行顺序](https://raw.githubusercontent.com/aliyun/alibabacloud-oss-csharp-sdk-v2/0.3.0/src/AlibabaCloud.OSS.V2/Internal/ExecuteMiddleware.cs)，请求至多三次尝试：网络失败、SDK 读写超时、上传 CRC 不一致，以及 401/408/429/5xx、RequestTimeTooSkewed、BadRequest 和指定签名日期错误可重试。普通 403/404、请求构造错误和非法错误头编码不重试。非成功状态包括 3xx，不能被 error_for_status 忽略后返回普通 false。Put/Delete 消费完响应正文后再判断完成；Get 成功响应正文继续交给独立文件拷贝层。请求重试不占用或重置外层的三次文件拷贝计数。

请求错误从 XML 的 Error 根节点读取直接子字段，重复字段取最后值并保留 InnerText；正文为空时读取 Base64 的 x-oss-err。解析失败退回按 HTTP 状态判断，服务端正文不写入错误消息。时钟修正只处理 SDK 指定的两种日期错误，优先读取严格 GMT 日期头，再读取 ISO ServerTime；400 且偏差小于十五分钟不修正。每个请求保留自身修正量，只有相对缓存变化超过一分钟才更新共享偏移。后续请求重新签名，临时地址也采用缓存偏移并保持原始过期时刻。

上传复用同一个打开的文件，每次重试先回到起点并创建新摘要。流式读取使用有界缓冲，取消或失败后停止旧请求的读取；文件互斥覆盖实际读操作，下一轮 seek 等待已有文件操作结束。按 [NopCloseRequestContent](https://raw.githubusercontent.com/aliyun/alibabacloud-oss-csharp-sdk-v2/0.3.0/src/AlibabaCloud.OSS.V2/Internal/NopCloseRequestContent.cs) 的阈值，至少 256 KiB 的文件每次读取有进度时重置二十秒期限，小文件保持单次期限；同时保留宿主 HTTP 客户端的整体期限。期限更新后继续等待同一个发送过程，不把旧计时器到期误判为新一次请求。

退避按版本实现而非注释推测：0.3.0 [StandardRetryer](https://raw.githubusercontent.com/aliyun/alibabacloud-oss-csharp-sdk-v2/0.3.0/src/AlibabaCloud.OSS.V2/Retry/StandardRetryer.cs) 将 MaxBackOff 与 BaseDelay 反序传给 FullJitterBackoff，因此当前默认实际等待位于 [0, 200ms)。本实现保留这一行为，没有擅自按通常的指数退避算法改写。

容器内离线同步已有依赖的直接引用后，全工作区 all-features/locked 的 check、严格 Clippy 与 build 通过。未添加或运行测试代码，未创建模拟服务，也未请求真实云端；本轮证据为代码对照、编译和差异检查，不能据此认定网络异常、时钟偏差和上传重放已通过真实运行验证。OSS 自动 MIME、配置校验时机、七天及子秒签名边界、全部 XML/日期格式和特殊 203 响应处理仍需复核；腾讯云上传选择及 TransferManager 行为也仍未解决。整体目标继续保持未完成。

## 临时地址错误契约与 OSS 调用时校验

对照 NetEngine 的 IFileStorage 与 FileService，AppFileStorage::get_file_url 改为 Result<Option<String>, AppError>，三个存储适配器和文件应用服务同步更新。只有组件正常返回 None 才产生“文件临时授权地址获取失败”的业务异常；配置、签名、时间溢出和 URL 构造错误原样作为内部异常向上交付，不再通过 .ok() 吞掉。公开文件和本地存储地址仍直接拼接前缀，两个宿主的共享异常响应规则不变。

依据 SDK 的 [静态凭据](https://raw.githubusercontent.com/aliyun/alibabacloud-oss-csharp-sdk-v2/0.3.0/src/AlibabaCloud.OSS.V2/Credentials/StaticCredentialsProvider.cs) 与 [操作校验](https://raw.githubusercontent.com/aliyun/alibabacloud-oss-csharp-sdk-v2/0.3.0/src/AlibabaCloud.OSS.V2/Extensions/StringExtensions.cs)，OSS 构造阶段只保存配置，实际调用才检查桶、对象、区域和凭据。桶名采用 SDK 的 3 至 64 字符边界、小写 ASCII 字母/数字/连字符与首尾约束；对象名采用 1 至 1024 个 UTF-16 码元，不能误按 UTF-8 字节数限制中文。空字符串凭据无效，不把非空凭据 trim 后改写。

OSS 过期时刻与签名时刻分别读取，按两者差值截断秒数；签名时刻遵守缓存时钟修正，过期时刻不随修正延长。过期时刻超出 .NET 日期范围或生成后剩余时间超过七天时返回内部错误。DateTime 时刻按 100ns 精度处理，正常十分钟文件地址在实际运行中产生 599 秒签名参数。

使用现有 client_webapi 二进制、临时 EC 签名密钥、独立 PostgreSQL/Redis 和七条隔离文件记录，依次运行五个宿主配置，共调用十八次现有 GetFileUrl 接口：

- 空凭据 Development：healthz 为 200/Healthy；私有文件产生 500 和空凭据诊断，公开文件仍 200；空对象名及过长对象名产生 500；不存在的文件仍为 400 业务错误。
- 有效格式的虚构凭据：普通路径、1024 个中文字和 512 个非 BMP 字符均返回签名地址，三条签名通过独立离线计算；1025 个中文字及 513 个非 BMP 字符被拒绝。没有向云端发送请求。
- 空凭据 Production：healthz 正常，私有文件返回 400/系统全局内部异常；公开文件成功，不存在文件仍保留业务错误。
- 无效桶名 Development：宿主仍启动且健康，私有文件调用时返回 500，公开文件成功。
- 腾讯云虚构凭据：契约调整后私有 inline 地址和公开地址仍成功，私有地址的 SHA1 签名离线核对一致，签名头列表为空。

所有临时宿主已正常退出，验证未调用真实云端、邮件或短信，也未新增测试程序或测试依赖。工作区 all-features/locked 的 check、build 与严格 Clippy 已通过；验证只覆盖上述实际 HTTP 分支，七天/极大有效期边界仅完成源码对照和编译，没有为不可由现有接口指定的参数增加临时入口。有符号有效期和 OSS MIME 的当前实现见后文；203 响应、全部 URI/XML/日期边界和腾讯云 TransferManager 等差异继续保留，不能认定框架全部完成。

## 有符号有效期与 COS 默认凭据

进一步复核发现，前文“凭据缓存行为”是待验证假设，不能作为实现依据。[DefaultQCloudCredentialProvider](https://raw.githubusercontent.com/tencentyun/qcloud-sdk-dotnet/master/QCloudCSharpSDK/COSXML/Auth/QCloudCredentialProvider.cs) 每次读取当前时间并派生密钥，不跨请求缓存；[GenerateSign](https://raw.githubusercontent.com/tencentyun/qcloud-sdk-dotnet/master/QCloudCSharpSDK/COSXML/CosXmlServer.cs) 分别取得签名区间与凭据区间，未显式设置密钥时长时使用提供商的 600 秒。已排除缓存假设，没有新增缓存。NetEngine 当前声明的 NuGet 依赖为 5.4.51，已读取该包元数据确认版本；官方仓库没有对应公开 tag，包也未提供源提交标识，以上实现对照使用官方 master，尚未完成该版本二进制方法体与源码的逐项核对。

文件存储契约由标准库 Duration 改为 chrono::TimeDelta，应用层的十分钟调用与三个适配器同步修改。common::date_time_helper 提供 TimeSpan ticks 双向转换，使用 i128 中间值检查 i64 边界，保留负值并将不足 100ns 的部分向零截断；没有将该规则扩展到锁、缓存等其他 Duration 参数。OSS 使用归一化后的有符号时长计算过期时刻，日期范围及七天检查继续生效。

COS 按 [.NET TotalSeconds](https://raw.githubusercontent.com/dotnet/runtime/release/10.0/src/libraries/System.Private.CoreLib/src/System/TimeSpan.cs) 先将 ticks 转为 double 秒数，再按中点到偶数取整。正签名秒数使用调用时的签名区间，零或负值使用凭据区间；密钥始终由独立读取的当前时间加 600 秒派生，不再错误地使用请求的有效期派生密钥。正文请求签名和临时地址共用该凭据派生方法，inline 查询项置于签名查询串之前。

配置校验按官方提供商调整：先拒绝空区域及包含 @ 的区域，再拒绝空字符串 SecretId/SecretKey；凭据检查后才 trim，不把纯空白提前等同于空字符串。AppId 和桶名不由构造函数额外要求非空，区域和对象地址仍沿用原始配置。此处保留源组件实际行为，并不意味着这些配置能被真实 COS 服务接受。

隔离 PostgreSQL/Redis 中运行现有 client_webapi，带空白的虚构凭据、空 AppId/桶名、纯空白凭据三种配置均启动且健康。共九次现有 GetFileUrl 调用全部成功：六条私有地址分别覆盖 inline 与普通展示，密钥和签名区间均为默认 600 秒，凭据按源规则裁剪，所有签名独立离线核对一致；三条公开地址直接返回。包含 @ 的区域及空 SecretId 两种配置在启动阶段按预期报错退出。所有临时宿主已停止，没有请求真实云端或增加模拟服务。

依赖只增加工作区已存在 chrono 的直接引用，锁文件在容器离线同步；全工作区 all-features 的检查、严格 Clippy 和 locked 构建通过。非默认时长、负值、中点取整、TimeSpan 极值和跨秒的两个起点目前只有代码及编译证据，现有 HTTP 接口固定十分钟，本轮没有新增验证专用入口或测试代码。COS TransferManager 的分片/重试/校验、上传对象键待选项及更广泛框架差异仍待继续处理，整体目标未完成。OSS 203 的当前实现见后文。

## OSS 上传内容类型

NetEngine 的 `AliCloudStorage.FileUploadAsync` 未显式设置 Content-Type，交由 AlibabaCloud.OSS.V2 0.3.0 的 `Serde.AddContentType` 根据对象键推断。Rust 的 OSS 适配器使用同版本 [MimeUtils](https://github.com/aliyun/alibabacloud-oss-csharp-sdk-v2/blob/0.3.0/src/AlibabaCloud.OSS.V2/MimeUtils.cs) 的全部 552 项默认映射，保留源类型值（例如 js 为 application/x-javascript，xml 为 application/xml），并附带 Apache-2.0 许可。该表不依赖 WebAPI 的 ASP.NET 静态文件映射，也不新增 MIME 库依赖。

上传类型使用远程目录与本地文件名组成的对象键，最后一个扩展名忽略 ASCII 大小写；目录中的点不参与识别，未知扩展名、无扩展名及末尾点回退 application/octet-stream。展示名称仅写入 Content-Disposition。Content-Type 在请求签名前生成，继续参与 OSS V4 的规范请求。源码对照覆盖类型表与调用路径；未使用真实 OSS 凭据执行上传，服务端对象元数据尚无运行证据。

## OSS GET 响应与期限边界

按 AlibabaCloud.OSS.V2 0.3.0 的 [TransportExecuteMiddleware](https://github.com/aliyun/alibabacloud-oss-csharp-sdk-v2/blob/0.3.0/src/AlibabaCloud.OSS.V2/Internal/ExecuteMiddleware.cs) 对齐 GET 处理：普通成功响应保持流式；HTTP 203 在请求层完整缓冲后交给文件拷贝层，缓冲失败计入请求重试，不提前创建或截断目标文件。内存正文仍经过文件层的 CRC 校验与读写期限，状态码仍保留为 203，成功写入也不返回 true。

GET 使用 ResponseHeadersRead 语义，收到响应头后停止宿主 HTTP 超时的计时作用；特殊成功正文与错误正文的后续缓冲继续受当前 OSS 请求期限约束，不因收到响应头重置期限。PUT/DELETE 保持整个响应读取期间的宿主 HTTP 期限。旧计时器到期但已不适用时继续等待同一操作，不重发请求。

全工作区 all-features/locked 的 check、严格 Clippy、build 与格式检查通过；没有添加或运行测试、模拟服务及真实云请求。这些变化目前具有源码对照和编译证据，特殊响应、网络故障及非默认 HTTP 超时仍缺少真实云运行证据，不能据此认定整体云存储或框架对齐完成。

COS 对照需区分源码与包版本：当前官方 master 的 COSXML-Netcore.csproj 声明 5.4.50.0，NetEngine 引用包为 5.4.51，不能只凭 master 宣称该包行为已验证。公开 TransferManager 默认上传阈值为 5 MiB（严格大于才分片）、上传片大小为 1 MiB；下载先 HEAD，默认阈值 20 MiB、片大小 10 MiB。下载关键方法的包内核对及实现见下节；上传仍待继续。

## COS 目标包与默认分片下载

已在容器中读取 [Tencent.QCloud.Cos.Sdk 5.4.51](https://www.nuget.org/packages/Tencent.QCloud.Cos.Sdk/5.4.51) 的 `lib/netstandard2.0/COSXML.dll`，程序集版本为 5.4.51.0，SHA-256 为 `4fd3bd6cf95a1bacac3bc24b80a5d08fca2638f593e5702b5ec03fe3f973cf74`。直接解析方法体，核对 TransferConfig 构造函数、COSXMLDownloadTask 构造函数/ComputeSliceList/Download 回调/ConcurrentGetObject、GetObjectRequest.SetRange、HeadObjectResult 头部解析、CosResult.IsSuccessful、CommandTask.AsyncResponseCallback、CosRequest 默认签名头和 ResponseBody 文件打开分支。此证据覆盖这些方法，不能推广为整个 SDK 已逐项等价。

默认下载先 HEAD，长度小于 20 MiB 时单片，达到阈值则按 10 MiB 分片，最多五路并发。HTTP 异步取响应最多尝试四次；分片传输最多三轮，已成功分片不再下载，耗尽 HTTP 重试后的 4xx 终止，客户端错误及 5xx 留给后续轮次。本轮分片全部发起后，长时间无任务完成会取消剩余请求；Rust 等待已发起的文件写入刷新后才开始下一轮，避免旧写入破坏重试文件。

分片沿用 `.<文件名>.cosresumable.<序号>`，全部完成后按数字顺序合并目标，按 HEAD 长度判断是否截断既有文件并检查最终长度。合并后才清理同目标分片；失败保留相应现场。默认 CRC 开关为 false，没有擅自增加 CRC 拒绝分支。零长度对象也保留 SDK 的 bytes=0- 请求，Range 与 content-length:0 进入请求签名。原始流式客户端由宿主工厂统一创建、复用并轮换，不自动解压正文，普通宿主客户端的解压策略保持不变。

本轮新增 futures-util 的直接引用，版本已在工作区锁文件中，容器内离线同步依赖边。没有新增测试、模拟服务或真实云请求；分片与异常分支目前只有目标包方法体、代码审查及编译证据，尚无真实云端运行证据。HTTP 层仍使用宿主响应头期限且不跟随 3xx，SDK 原始 HttpWebRequest 的全部重定向、超时、路径规范化和错误正文语义尚未对齐；分片上传、上传对象键待选项及其他框架差异继续保留，整体目标未完成。

容器内全工作区 all-features/locked 的 check、严格 Clippy、build、格式及差异空白检查通过。临时 DLL、Python 分析模块和反汇编文件均已从本轮专用容器临时目录清理，没有安装 Windows 本机组件，也没有向项目加入分析工具或验证程序。

## COS 分片上传与续传

继续读取同一 5.4.51 包及相同 SHA-256 的程序集，核对 COSXMLUploadTask 构造函数、MultiUpload、UploadPart、ComputeSliceNums、CheckResumeblaUpload、CheckAllUploadParts、OnFailed 及查找未完成上传的实际回调。默认 UseResumableUpload 为 true；MaxConcurrent 为 2，但等待条件是 activeTasks > MaxConcurrent，Rust 采用最多三个并发上传。默认 5 MiB 阈值为严格大于；分片数量按长度除以 1 MiB 向下取整，余数并入最后一片，商达到 10000 时拒绝，而非自动扩大分片大小。

Rust 上传先列出对象前缀对应的未完成会话，在返回的首批结果中从后向前寻找同名对象，再读取该会话的分片。已有分片全部与本地相同范围的 MD5 比较，ETag 仅去掉两端引号；任意不匹配均重新初始化，有效匹配才纳入续传。没有自动补充源 SDK 未执行的分页查询；服务端未返回的分片仍重新上传。分片编号无效、XML 响应错误或初始化缺少编号返回内部错误，不记录服务端正文及凭据。

小文件 PUT、分片 PUT 和完成 XML 均发送正文 Content-MD5；每次请求签入实际元数据与查询参数。参数名规范化为小写并排序用于签名，URL 保留 partNumber/uploadId 原名。上传文件与 XML 字节不强行设置源 RequestBody 未声明的 Content-Type。文件摘要按 64 KiB 流式计算；重试重新打开文件、定位分片起点并限制发送长度，不复用失败后的读取偏移。ACL 和 Content-Disposition 随简单上传或初始化请求提交，完成清单按编号排序，HTTP 200 中的非完成 XML 也不被当作成功。

已确认一处源实现缺陷：查找未完成上传的结果非空、但所有结果都只是前缀匹配而非同名时，5.4.51 回调直接结束，既不初始化也不完成任务。Rust 在这个分支新建上传，避免永久等待；这是明确修正的差异，不声称逐指令复现挂起行为。普通失败保留服务端 uploadId 和已有分片，不自动 Abort 或删除对象；Rust 结束当前操作时丢弃未完成的本地请求，不保留后台上传任务。

新增的 base64、md-5、quick-xml 与 serde 均使用工作区已有版本，容器离线同步直接依赖边。上传对象键的待确认选择保持不变；宿主 HTTP 期限、重定向、全部 SDK 错误解析/路径规范化以及云端真实传输仍未完成等价验证。此次实现依据目标包与官方源码，没有新增测试、模拟云服务或发送真实云请求，整体框架对齐仍在进行。

最终容器内全工作区 all-features/locked 的 check、严格 Clippy、build、格式与差异空白检查通过；用于核对的临时 DLL、反汇编和 Python 模块已从专用容器目录清理，没有安装本机组件。

## COS 原生删除的请求链

读取同一 5.4.51 包的 CosXmlServer.ExecuteAsync/Schedue 及 DeleteObjectRequest/DeleteObjectResult 方法体，确认 NetEngine 的原生删除调用由 TaskCompletionSource 包装 SDK 异步调度，并非另走同步请求。DeleteObjectRequest 只设置 DELETE 方法，没有自定义正文；沿用已核对的异步请求层重试和零长度请求头签名。程序集 SHA-256 与前述上传、下载核对一致。

删除已接入组件共享的 `request.rs`，与上传及控制请求共用签名、最多四次请求尝试和缓冲响应读取；原 `upload_request.rs` 改为通用命名，没有复制另一套重试代码。成功响应正文完整读取后返回 2xx 成功，204 仍为 true；正文读取失败不重发已取得成功响应的删除。移除了只签 Host、单次发送且不等待响应正文的旧删除路径。业务 FileService 的删除仍只软删除记录，本轮未增加云删除调用方。

容器内全工作区 all-features/locked 的 check、严格 Clippy、build、格式与差异检查通过。没有创建测试代码或模拟服务，也未执行真实云对象删除；验证证据限定于目标包、调用链审查和编译。COS 的全部重定向/超时/错误正文/路径语义、上传对象键待确认项以及全框架对齐仍未完成。

## 公共数据工具与 TOTP 复核

重新读取当前提交的 `Infrastructure/Common/DataHelper.cs`、`PropertyHelper.cs` 和 `TotpHelper.cs`。Excel 导入现在只接受 `.xls`/`.xlsx`，扩展名大小写不影响格式选择；工作表不存在返回 None，空工作表或第 0 行无有效单元格返回空表，不把较后出现的首个数据行提升为表头。有效列数按第 0 行末尾非空单元格确定；有表头时跳过空白标题并保留原始列位置，无表头时使用 column1 等名称。只按实际选中列过滤空行，公式存在时即使缓存值为空也保留该行；缺失、空白及错误单元格转为空字符串。日期单元格在当前 JSON 数据表表示中输出 ISO 日期时间，而非 Excel 日期序号。

模板导出字段逐段使用已有 OrdinalIgnoreCase 比较；中间对象为 null 时输出空格，不存在的字段返回包含完整路径的错误。整数超过正负 999999999999999 时写为文本，沿用源实现的 15 位边界，替换旧的 JavaScript 安全整数范围。没有把普通字符串推断为日期或数值。

TOTP 移除了源实现没有的十步漂移上限；漂移与周期参数采用源 int 范围，周期必须为正、漂移不能为负。生成与校验接收有符号 Unix 秒数，计数器除法向零截断，窗口加法与源 unchecked long 一致；负计数器以同样的八字节补码参与 HMAC，不直接跳过。没有改动默认 20 字节密钥、6 位验证码、30 秒周期与三种哈希算法。

本阶段仍不是 Common 全量等价证明。DataTable 到模型的类型转换、默认构造值、枚举描述、DisplayName 元数据，以及默认 Excel 导出的属性顺序、空列表表头、属性筛选、decimal 和日期类型输出仍需补齐。当前表头重复检查使用共享 OrdinalIgnoreCase，尚未覆盖源 DataTable 的全部区域比较行为；日期/数字表头的完整 NPOI 格式化也未实现。[NPOI DataFormatter 源码](https://github.com/nissl-lab/npoi/blob/master/main/SS/UserModel/DataFormatter.cs) 用于核对无求值器时的公式标题规则，不作为目标 2.8.1 包全部格式化行为的等价证据。

容器内全工作区 all-features/locked 的 check、严格 Clippy（lib/bins、-D warnings）、build、格式与差异空白检查均通过。没有新增或运行测试，也没有生成临时验证程序；本阶段验证限于当前源代码对照及编译和静态检查，未进行 Excel 文件或 TOTP 运行验证。整体目标继续保持未完成。

## 已确认的云上传对象命名

用户明确选择两种云组件统一使用“远程目录 + 本地文件名”，修正 NetEngine 腾讯云组件直接以目录作为完整对象名的缺陷。COS 现在沿用 OSS 的本地 basename 和目录组合规则，展示文件名仅进入 Content-Disposition；普通和分片上传、续传查找及完成请求均复用同一对象键。移除旧的展示名称替代 basename 与额外裁剪分支，文件服务生成的相对路径与上传对象命名保持一致。此前各阶段记录中的“上传对象键待确认”已解决，不再作为阻塞项；未修改 NetEngine 源仓库或发送实际云上传。

## 属性元数据与默认 Excel 导出

现有 PropertyHelper 只转 JSON 的实现无法保留声明顺序和原始日期类型，显示名称方法也没有独立行为。新增 common 的 PropertyModel 契约与 source_generator_core 的同名派生宏，runtime 重新导出宏；字段可声明 name、display_name、value_type、skip。宏解析调用方实际 common 依赖名称、保留泛型约束，按公共字段声明顺序生成读取，拒绝重复属性名与冲突标记。ExcelTemplate 与 ColumnProperty 使用该能力，容器编译实际展开这两个生产类型。

公共属性字典使用 IndexMap；属性比较不再把日期形状的字符串截短，也不对字段排序，decimal 按数值、DateTimeOffset 按同一时间点比较。默认导出改为 PropertyModel + Default，以默认实例构造表头，空列表保留列；新增显示名称导出入口，引用集合不混入标量列。原始类型元数据区分可空/非可空 NaiveDateTime 的源格式分支，decimal 按源有效位规则选择数值或文本，DateTimeOffset 使用 Asia/Shanghai 时区输出。时区由 [chrono-tz](https://docs.rs/chrono-tz/0.10.4/chrono_tz/) 的 IANA 数据提供，不用恒定 UTC+8 代替历史时区规则。

indexmap、rust_decimal 的版本原已在锁文件中；common 新增它们及过程宏 crate 的直接依赖，chrono-tz 与所需 phf 依赖由容器内 Cargo 解析下载。没有安装 Windows 组件。公共 API 的使用与现有限制见 common-data.md。

仍未完成：DataTable 的类型转换、默认值赋值与显示名称导入，枚举描述映射，模板导出的原始类型/名称元数据，完整 NPOI 表头格式化，以及一般对象的引用相等与区域化 ToString。Rust 的属性派生要求公开字段可序列化，自定义值类型需显式标记，尚未覆盖所有 .NET 反射形态。上述公共工具目前只有源码对照与编译证据，没有新增测试、验证程序或实际 Excel 运行检查；整体目标未完成。

本轮最终容器内全工作区 all-features/locked 的 check、严格 Clippy（lib/bins、-D warnings）和 build 均通过，包含新的属性过程宏、运行时导出与腾讯云上传对象键修改；格式及差异空白检查通过。未操作实际业务数据库或云对象。

## 数据表导入与类型保留

继续对照当前 `DataHelper.DataTableToList`、`DataTableToListDisplayName`、`SetPropertyValue` 及 `ListToDataTable`。数据行由无序 JSON 字典改为 IndexMap<String, DataCell>，保留列序及 decimal、DateTime、DateTimeOffset、DateOnly、TimeOnly 类型；Excel 日期单元格不再先转成普通字符串，模型转表也沿用属性名称和声明顺序。这样可以区分真正的日期单元格与日期形状的文字，并使同一属性被多列赋值时顺序确定。

新增 DataModel 派生，与 PropertyModel 共用字段标记解析；字段名先于显示名称，匹配区分大小写，重复显示名称选第一项。只读字段的名称仍参与匹配但不赋值，不错误落入另一个同名显示字段。导入逐行创建 Default 模型，忽略未知列、空值和只读字段；空白可空字段清空，空白普通值类型保留默认值。字段直接使用 FromDataCell，不借用 JSON 字段重命名或宽松绑定器；ColumnProperty 作为生产类型实际展开新派生。

基础转换覆盖布尔、整数、浮点、decimal、Guid、char 与日期/时间类型。数值到整数采用中点取偶并在强制转换前检查边界，字符串到整数保持严格整数解析；f32 直接转换，避免先到 f64 的双重舍入。[.NET 10 Convert 源码](https://github.com/dotnet/runtime/blob/release/10.0/src/libraries/System.Private.CoreLib/src/System/Convert.cs) 用于核对整数舍入与窄化顺序。日期单元格到 DateOnly/TimeOnly 取对应部分，无偏移日期到 DateTimeOffset 使用宿主本地时区。

Rust 非可空 String 不能保存源反射可写入的 null，因而空白输入明确返回错误；需允许清空的模型使用 Option<String>。当前数字/日期文字解析覆盖固定格式，并非完整 CurrentCulture；浮点转 decimal 的全部边界、所有 Guid 格式、DateTime 100 ns 舍入与时区无效/重叠时间、枚举描述、对象赋值等仍需核验。IFormFileExtension.ExcelToList 已定位但尚未提供 Rust 宿主对应封装，模板导出与完整 NPOI 格式化差异继续保留。没有添加测试或临时验证程序，整体目标仍未完成。

最终容器内全工作区 all-features/locked 的 check、严格 Clippy、build、格式及差异空白检查均通过，覆盖新单元格表示、导入契约及过程宏/运行时。没有新增依赖或安装本机组件，没有进行 Excel 文件运行验证。超过 64 位的 JSON 整数保留为 JSON 数字，不隐式降为浮点。

## 上传 Excel 入口与模板属性路径

补齐 `IFormFileExtension.ExcelToList` 对应的 `webapi_core::uploaded_file::excel_to_list`，只接受非空 xlsx，使用可执行文件旁 files 目录和 UUID v4 文件名。按块写入，不将整个上传内容合并为内存数组；同步工作簿解析与模型转换放在阻塞任务内。创建临时文件与其拥有者在同一任务完成，解析任务持有 TempPath，避免请求取消时漏建清理句柄或提前删除尚待读取的路径。固定格式/内容业务错误与源入口一致，IO 和工作任务内部失败仍走共享宿主错误处理。

使用工作区现有 uuid 版本增加 webapi_core 直接依赖并开启 v4，在容器离线同步锁文件依赖边。复用既有 tempfile 和 Tokio，不添加验证专用 HTTP 路由。已开始的阻塞解析不因请求 future 被释放而强行中止，收尾后释放临时路径；这一生命周期依据 [Tokio spawn_blocking](https://docs.rs/tokio/1.54.1/tokio/task/fn.spawn_blocking.html) 与 [TempPath](https://docs.rs/tempfile/3.27.0/tempfile/struct.TempPath.html) 的所有权行为。

模板导出移除按 JSON 对象路径查找的实现，改为 PropertyModel 的 read_property 和 get_property。派生逐段使用共享 OrdinalIgnoreCase，只在匹配的字段上创建快照；nested 标记的对象继续读取内部属性，支持 Option/引用/Box/Arc/Rc。可空中间对象直接返回空单元格，非空对象缺失字段保留完整路径报错。复用现有递归类型识别规则，避免给直接递归嵌套字段生成自循环 trait 约束。DataModel 与 PropertyModel 共用标记解析，nested 不改变导入赋值规则。

模板保留实际 decimal、DateTime 和 DateTimeOffset 类型并共用单元格输出：模板 DateTime 统一到秒，默认属性字典导出的非可空 DateTime 仍保留毫秒，按源两种调用路径区分。任意对象/集合叶子的 ToString、运行时反射的全部内置计算属性、枚举描述、完整区域转换与 NPOI 表头格式仍未对齐；本轮不据此宣称 Common 或全框架已完成。

最终容器内全工作区 all-features/locked 的 check、严格 Clippy（lib/bins、-D warnings）、build、格式和差异空白检查均通过。没有新增或运行测试，没有实际上传 Excel 或生成验证专用程序；新增入口、取消清理及嵌套路径仍缺少运行证据。未修改 NetEngine，未安装本机工具链或其他组件。

## 数据表枚举导入

重新核对干净的 NetEngine `23a7d0002938d0308c975dc48c04ac66aae0dda8`：`DataHelper.SetPropertyValue` 的枚举路径先 Int32/Enum.ToObject，再 Enum.TryParse，最后整段 DescriptionAttribute。依据 [.NET 10 Enum 源码](https://github.com/dotnet/runtime/blob/release/10.0/src/libraries/System.Private.CoreLib/src/System/Enum.cs) 核对数值窄化、名称组合及溢出分支。名称组合按位或，不要求 Flags；描述是组合解析失败后的整体匹配，不逐项匹配描述。

新增 `common::data_helper::{DataEnum, EnumMember, EnumRepr, EnumValue}` 和 DataEnum 派生。底层整数覆盖 C# 的八种整数类型，必须显式指定 repr；元数据名称独立于 Serde，可声明 description。派生按源声明顺序生成描述映射并拒绝有数据字段、重复名称或不支持的底层类型。已有 Algo 和 AppLlmRole 使用该派生，既验证 common 内部宏路径，也验证其他 crate 的 common 路径；未给既有业务模型新增导入字段或改变 JSON 角色校验。

导入字段使用 EnumValue，原始整数与 Rust 已声明枚举分开：未知值和按位组合不强制变成 Rust 枚举，不使用 unsafe；known 只返回精确匹配值。Int32 路径保留源 Enum.ToObject 的截断和符号扩展，超出 Int32 时按实际底层整数范围解析；溢出数字不回退名称，但仍允许整段描述匹配。默认零不要求声明对应成员，空白与 Option 保持既有 DataModel 规则。JSON 保留底层数值。

本轮只补齐枚举导入和可复用元数据：属性/Excel 输出仍需要 value_type 标记且尚未提供 Enum.ToString/Flags 名称格式化；固定区域文本解析、重复数值别名的自动派生、任意对象反射与已记录的 Excel 差异仍未完成。派生可由手动 DataEnum 实现补充别名元数据。没有新增测试、验证专用程序或接口，数字/描述/组合分支目前缺少运行证据，整体目标继续保持未完成。

最终容器内全工作区 all-features/locked 的 check、严格 Clippy（lib/bins、-D warnings）、build 和格式检查均通过，git diff --check 通过；没有新增依赖或安装本机组件。新增宏同时由公共组件和应用模型实际展开，未运行测试或修改 NetEngine 源仓库。

## 枚举属性与 Excel 输出

继续对照同一干净的 NetEngine 提交。`PropertyHelper.ComparisonEntity` 对枚举使用类型和值的 Equals，并通过 ToString 输出差异；`DataHelper.ToExcelSetValue` 的枚举走文本分支，不能按序列化后的底层整数写数值单元格。依据 [.NET 10 Enum 实现](https://github.com/dotnet/runtime/blob/release/10.0/src/libraries/System.Private.CoreLib/src/System/Enum.cs) 补充普通枚举和 Flags 名称格式化：完整值优先，无符号位模式排序、从大到小清位、反向连接名称，不能完全覆盖时回退原始有符号或无符号整数。描述仍只参与导入。

DataEnum 类型可声明 flags；EnumValue 实现 Display，新增 EnumSnapshot 保存类型身份、底层数值与名称文本。PropertyModel 的 enumeration 标记通过 EnumProperty 读取原生枚举、包装枚举、可空字段和引用，不借用 Serde 表示。枚举的比较依据类型与数值，默认和模板 Excel 输出统一为文本，null 输出空单元格。AppLlmMessage 与 AppLlmStreamDelta 实际派生 PropertyModel，并使用源 Role、Content 名称，覆盖非可空和可空枚举的生成代码；原 HTTP JSON 契约保持原样。

属性字典改为 IndexMap<String, DataCell>，保留 decimal、可空日期、带时区日期和枚举快照；非可空 DateTime 仍按源字典规则预先格式化到毫秒。模型转数据表保留 DataCell::Enum，随后导入枚举/字符串使用名称，导入数值/bool/char 使用底层数字并保留目标范围检查。这样不再因为 JSON 中转而把枚举错误当成普通数值或字符串。

前节枚举 ToString/Flags 尚未接入的差异由本节实现更新。仍未声称全框架或全部 Common 完成：完整区域转换、NPOI 表头显示格式、任意对象反射/ToString、时间边界等差异继续保留；重复数值别名只能手写元数据，名称选择不保证匹配 .NET 的未指定别名顺序。没有新增测试代码、测试入口或临时验证程序；Flags 分解及 Excel 生成目前仅有源代码对照和编译证据，缺少实际文件运行证据。

最终容器内全工作区 all-features/locked 的 check、严格 Clippy（lib/bins、-D warnings）、build、格式检查以及 git diff --check 均通过。无新增依赖、无本机安装、无测试执行，NetEngine 源仓库保持未修改。

## Guid 转换、短编码与空对象行

继续复核干净的 NetEngine `23a7d0002938d0308c975dc48c04ac66aae0dda8`。数据导入原先直接调用 UUID 库解析，缺少 P/X 格式且会接受源 Guid.Parse 不接受的 URN。新增数据导入专用解析，按 [.NET 10 Guid 源码](https://github.com/dotnet/runtime/blob/release/10.0/src/libraries/System.Private.CoreLib/src/System/Guid.cs) 的入口格式选择支持 N/D/B/P/X；保留 D 固定宽度分量兼容路径和 X 的内部空白、前导零、UInt32 范围及 short 截断行为。D 的最后八位沿用源专门整数解析路径，区别于其他分量的前缀处理；依据 [Number.Parsing](https://github.com/dotnet/runtime/blob/release/10.0/src/libraries/System.Private.CoreLib/src/System/Number.Parsing.cs) 核对该路径的末尾 NUL 规则。

顺带核对 StringHelper 发现已存在的 Guid 短编码实际使用 UUID 网络字节序，与源 Guid.ToByteArray 的字段小端顺序不一致。压缩与解码现在使用 UUID 库的 to_bytes_le/from_bytes_le，compress_uuid/压缩别名共享源字节顺序。decompress_guid 调整为 Result：源捕获的 Base64 格式失败返回空 Guid，源未捕获的合法 Base64/非 16 字节数组返回错误；decompress_uuid 保留 Option 便利入口。长度用 UTF-16 计数，Base64 过滤规则依据 [Convert](https://github.com/dotnet/runtime/blob/release/10.0/src/libraries/System.Private.CoreLib/src/System/Convert.cs)。仓库中未找到这些短编码助手的业务调用，无需更改业务记录或宿主流程。

ListToDataTable 源实现跳过 null 对象，非空列表即使全部为 null 也返回空表。PropertyModel 新增 is_null_model，Option 与指针包装委托该判断；模型转表过滤 null，不把零属性的有效对象误判为空。空输入列表仍返回 None，非空且全空对象的列表返回 Some(empty)。现有 DataTable 仍仅保存行集合，独立表名、空表列结构与完整 DataColumn 类型约束尚未实现；这一差异已补记，不能把空行修正视为整个数据表契约完成。

没有增加测试代码或临时验证程序；Guid 兼容分支、跨语言短编码及空行结果尚无运行验证，证据限定于源码、依赖实现与容器编译。完整区域转换、Excel 表头格式、日期时间边界以及更广泛框架对齐继续保持未完成。

最终容器内全工作区 all-features/locked 的 check、严格 Clippy（lib/bins、-D warnings）、build、格式检查及 git diff --check 通过。没有新增依赖、安装本机组件、执行测试或修改 NetEngine 源仓库。

## 数据表的独立列结构

重新核对干净的 NetEngine `23a7d0002938d0308c975dc48c04ac66aae0dda8` 的 ListToDataTable 与 ExcelToDataTable。DataTable 不再只是 Vec<Row>：新增独立表名、DataColumn 列清单和数据行。Excel 所选列使用 Object 类型，只有表头也保留列，缺失第 0 行或无有效列仍是零列空表；无工作表保持 None。数据表转模型继续遍历列顺序，不因输入字典顺序改变同属性多列覆盖结果。

模型列表在首个非空对象出现时从 Property 声明建立列类型，不通过该行单元格的值推断。整数宽度和符号、浮点宽度、decimal、日期、Guid、枚举及自定义类型名称独立保存，可空字段为 None 也有其底层声明；EnumProperty 增加静态类型元数据以覆盖可空枚举。DataColumn 默认允许 DBNull，不把非可空模型字段错误转换为禁止空值的列约束。

PropertyModel 增加模型名，派生支持类型级 property(model_name)，Option 和指针包装委托内部类型。当前 LLM 消息与增量显式采用源 DTO 名称。非空但全部为 null 的列表保留表名且不创建列；零属性的有效对象仍产生零列数据行。属性类型去除 Option/引用的规则集中复用，避免导出分类与表结构分别猜测。

手工组表经显式列结构创建，push_row 按列顺序接收并补齐空值，拒绝未知列及不允许的空值，失败时不提交半行；取值不依赖逐次移动所有剩余键，避免宽表按列重排的平方开销。列与已接收的行只提供只读访问。本轮保存了源助手实际需要的表结构，尚未声称实现完整 .NET DataColumn 的赋值转换、动态反射或所有自定义类型行为。

没有新增测试代码、验证专用程序或路由；空表/首行空值/全空对象的结果目前是源码与编译证据，未生成实际 Excel 做运行验证。完整区域转换、NPOI 表头显示格式、时间边界及其他框架差异继续保留，整体目标未完成。

最终容器内全工作区 all-features/locked 的 check、严格 Clippy（lib/bins、-D warnings）、build、格式检查及 git diff --check 均通过。新模型名与枚举类型元数据在现有生产模型中实际展开，上传 Excel 调用方通过编译。没有新增依赖、安装本机组件、运行测试或修改 NetEngine 源仓库。

## 字符串过滤与 URL 参数

继续核对 NetEngine StringHelper：URL 参数改用 PropertyModel 的属性名称和顺序，枚举使用底层值，布尔、日期时间、偏移与 Decimal 使用对应文本规则；编码采用 HttpUtility 的安全字符、空格加号和小写百分号形式。有限浮点按默认 G 格式范围选择普通或指数文本。未声称覆盖任意 IFormattable、非有限浮点和所有舍入边界。

标点与数字过滤按 BMP Unicode 类别处理，避免把数学符号当标点或仅接受 ASCII 数字；RemoveEmoji 按源实际白名单处理，全部中文判断保留空字符串为 true。局域网范围收敛为 IPv4 私有网段和 IPv6 ULA，公网判断保持独立。现有 Rust IP 解析与 .NET 文本语法仍有差异，HTML、转义、UTF-16 截断和压缩解码也尚未完成对齐，详见 [公共字符串工具](common-string.md)。

再次确认用户已选择云上传对象名统一为远程目录加本地文件名：两个 Rust 提供商与文件记录路径一致，COS 普通、分片及续传共用该键；AGENTS.md 已保存这一约定。此处修正 Rust 翻版行为，NetEngine 对照仓库保持未修改。没有发送实际云请求或编写验证程序，整体目标仍未完成。

本轮容器内全工作区 all-features/locked 的 check、严格 Clippy（lib/bins、-D warnings）、build 与格式检查通过，git diff --check 通过。未安装本机组件、添加或运行测试。

## HTML 清理、文章摘要与正则转义

继续核对未修改的 NetEngine `23a7d0002938d0308c975dc48c04ac66aae0dda8`。RemoveHtml 原先采用 HTML5 解码加全局空白压缩，会改变文章自动摘要。现改为源助手的十五步清理顺序，保留正文空白和单个换行，不递归解码实体。依据 [.NET WebUtility](https://github.com/dotnet/runtime/blob/release/10.0/src/libraries/System.Private.CoreLib/src/System/Net/WebUtility.cs) 提取 253 个实体映射并保留 MIT 许可证；数字实体按 UInt32 的十进制/十六进制规则解析，非法标量保留。后续正则显式表达 [zh-CN 大小写等价](https://github.com/dotnet/runtime/blob/release/10.0/src/libraries/System.Text.RegularExpressions/src/System/Text/RegularExpressions/RegexCaseEquivalences.cs)，避免 Rust case folding 额外接受 long s，或遗漏大写带点 I。移除已无调用的 html-escape 依赖，容器离线刷新锁文件。

文章创建与更新共用的摘要解析改为源 IsNullOrEmpty 条件：显式非空摘要不裁剪，正文缺失时保留原摘要的 null/空字符串差异，清理成空文本时保存空字符串。自动摘要按 UTF-16 截取 255 个单元。项目实际依赖 Npgsql 10.0.3；核对该版本的 [写入编码](https://github.com/npgsql/npgsql/blob/v10.0.3/src/Npgsql/Internal/NpgsqlWriteBuffer.cs) 和 [文本转换器](https://github.com/npgsql/npgsql/blob/v10.0.3/src/Npgsql/Internal/Converters/Primitive/TextConverters.cs) 后，截断代理项的情况返回内部错误，保持源数据库严格编码失败的结果。失败检测发生在 Rust 应用层，不声称异常堆栈或执行时机逐字相同。

UnicodeToString 改为可失败入口，依据 [RegexParser](https://github.com/dotnet/runtime/blob/release/10.0/src/libraries/System.Text.RegularExpressions/src/System/Text/RegularExpressions/RegexParser.cs) 补齐控制字符、八进制、定长十六进制、标点与非法转义处理。UTF-16 入口保留源允许的孤立代理项，String 入口显式报错，避免丢失或伪装原始结果。仓库中未找到该助手的业务调用，不添加验证用调用点。

没有编写测试、临时验证程序或执行数据库写入；上述行为目前是源码和编译证据，尚无跨语言运行验证。通用截断/隐藏、压缩解码、IP 文本语法以及全框架剩余差异继续核对，整体目标未完成。

最终容器内全工作区 all-features/locked 的 check、严格 Clippy（lib/bins、-D warnings）、build 与格式检查均通过，git diff --check 通过。未安装本机组件或修改 NetEngine 对照仓库。

## 字符串压缩、读取边界与 UTF-16 截断

本轮再次确认 NetEngine `23a7d0002938d0308c975dc48c04ac66aae0dda8` 工作区干净。SubstringText 改为 i32 长度和可失败结果，负值报错，按 UTF-16 计数并仅在超长时追加省略号；PartiallyHidden 也按 UTF-16 单元计算中间组。新增原始 UTF-16 入口保留源允许的孤立代理项，String 入口对此明确返回错误。当前 Rust 无业务调用点，不添加验证用代码。

字符串压缩使用 flate2 的 gzip 内存接口，显式启用已有 zlib-rs 后端能力；压缩保持 UTF-8、不加文本 BOM、默认级别和 Base64 输出。解码与短 Guid 共用 Convert.FromBase64String 的四种空白和非零填充位规则。依据 [.NET DeflateStream](https://github.com/dotnet/runtime/blob/release/10.0/src/libraries/System.IO.Compression/src/System/IO/Compression/DeflateZLib/DeflateStream.cs) 与 [Inflater](https://github.com/dotnet/runtime/blob/release/10.0/src/libraries/System.IO.Compression/src/System/IO/Compression/DeflateZLib/Inflater.cs)，gzip 继续解相邻段、忽略完整段后不具 gzip 标识的内容，保留默认未开启严格验证时的 EOF/截断行为；格式与已读到的校验和错误仍报错。NetEngine 未发现 UseStrictValidation 配置。

依据 [StreamReader](https://github.com/dotnet/runtime/blob/release/10.0/src/libraries/System.Private.CoreLib/src/System/IO/StreamReader.cs) 补齐 UTF-8/16/32 BOM 与非法文本替换。解压读取保留 8192 字节压缩输入与 1024 字节文本缓冲边界，先处理可跨读取补齐的 UTF-8 preamble，再判断当前块的其他 BOM；编码切换时丢弃旧 decoder 尚未完成的 UTF-8 序列。避免把所有 gzip 段合并后重新解释原先分开的 BOM。WebAPI 原有缓冲正文的 BOM 解码移动到 common 复用，宿主自身的 gzip 限制和读取入口保持原流程。

没有编写测试、临时验证程序或执行真实业务请求，结果证据限定于源码审查及容器编译。尚未验证与 .NET 原生压缩后端的字节级一致性及所有异常输入运行结果；IP 文本语法、属性格式化与其他框架差异仍待推进，整体目标未完成。

最终容器内全工作区 all-features/locked 的 check、严格 Clippy（lib/bins、-D warnings）、build、格式检查与 git diff --check 均通过。锁文件无需变更，未安装本机组件、运行测试或修改 NetEngine 对照仓库。

## 地址文本与模板参数

本轮确认 NetEngine 仍为干净的 `23a7d0002938d0308c975dc48c04ac66aae0dda8`。地址字符串助手原先完全依赖 Rust 标准解析，拒绝源 IPAddress 接受的缩写和进制形式。依据 [.NET IPAddressParser](https://github.com/dotnet/runtime/blob/release/10.0/src/libraries/System.Net.Primitives/src/System/Net/IPAddressParser.cs) 和 [IPv4AddressHelper](https://github.com/dotnet/runtime/blob/release/10.0/src/libraries/Common/src/System/Net/IPv4AddressHelper.Common.cs)，统一支持 IPv4 一至四段、十六进制/八进制以及末段剩余位宽；继续拒绝多余分段、溢出、空白、端口和 URL 后缀。

依据 [IPv6AddressHelper](https://github.com/dotnet/runtime/blob/release/10.0/src/libraries/Common/src/System/Net/IPv6AddressHelper.Common.cs) 补齐方括号、可选作用域和括号后端口校验。当前助手只比较地址位，不返回或解析接口编号；源未知接口名亦不导致解析失败。内嵌 IPv4 单独校验，保留源 canonical 校验只在分隔点拒绝单个前导零、末段仍可接受的行为。CIDR 的标准符号、ASCII 数字空白和末尾 NUL 依据 [Number.Parsing](https://github.com/dotnet/runtime/blob/release/10.0/src/libraries/System.Private.CoreLib/src/System/Number.Parsing.cs) 处理，保留地址族检查和零长度前缀。

公网/局域网的地址位分类保持既有源规则；已用于 Ueditor 抓取的 typed IP 分类入口没有改动。本次字符串地址助手未发现 Rust 业务调用，不增加验证路由或调用点。

ReplacePlaceholders 改为按匹配顺序遇到首个缺失参数立即返回源错误文本，去掉原先多出的空格。允许空前后缀，替换值保持字面内容；两端均为空时按 UTF-16 单元推进，而不是按 Rust Unicode 标量插入。结果包含孤立代理项时 String 入口明确返回错误，未声称支持任意非良构 UTF-16 输出。

未新增测试或临时验证程序，没有发送实际业务请求。本轮证据限定于源码审查与容器编译；自定义区域数字符号、属性格式化及其他框架差异仍未完成，整体目标保持进行中。

最终容器内全工作区 all-features/locked 的 check、严格 Clippy（lib/bins、-D warnings）、build 和格式检查均通过，git diff --check 通过。没有新增依赖、安装本机组件或修改 NetEngine 源仓库。

## 云上传源组件修正与公共时间工具

用户确认将云上传统一为远程目录加本地文件名，并明确修正源项目缺陷。本轮修改 NetEngine 的 `Infrastructure/FileStorage.TencentCloud/TencentCloudStorage.cs`：与阿里云组件共用相同的 Path.GetFileName、Path.Combine 与正斜杠规范化规则，PutObjectRequest 使用完整对象名。`IFileStorage.FileUploadAsync` 的参数注释明确 remotePath 为目录；fileName 仍仅决定下载展示名。FileService 传入 basePath 并记录 basePath 与本地文件名的组合，两者一致。Rust 两个提供商已采用该规则，无需重复修改。此前“NetEngine 未修改”的描述仅代表各阶段当时状态，本轮已按用户授权修改源仓库；未迁移既有云对象，也没有向真实云端上传。

公共日期助手按源实现调整：年龄使用本地日期，未来生日返回零，闰日周年在非闰年落到二月二十八日；带偏移时间抹零保留输入偏移，不重新应用午夜时区规则。Guid V7 时间读取只验证版本位，保留源允许的 variant，毫秒时间限制在 DateTimeOffset 的年份范围。NTP 选择首个 IPv4 地址并按整数毫秒截断，不增加地址重试或 IPv6 回退。

系统地址助手改为解析本机主机名，按 DNS 返回顺序选择首个对应地址族并传播错误；不再遍历网卡、去重或优先非回环地址。容器离线同步锁文件，以已有 hostname 包替换 if-addrs。该实现与 .NET 的完整主机名解析流程、特殊 IPv6 文本格式仍需进一步核对。

日期字符串解析仍使用 chrono 格式，尚未对齐 DateTimeOffset.ParseExact 的格式与 CurrentCulture；Shell 的显式取消、Windows 临时脚本锁定和退出排空等差异也尚未完成。公共日期及地址助手目前没有对应业务调用，不添加验证调用点、测试或临时校验程序；本轮不据此声称全部框架能力对齐。

验证通过：Rust dev 容器内全工作区 all-features/locked 的 check、严格 Clippy（lib/bins、-D warnings）、build 与格式检查；两个仓库的 git diff --check 均通过。NetEngine 腾讯云组件及其 FileStorage 引用在 `mcr.microsoft.com/dotnet/sdk:latest` 临时容器内编译成功，零警告、零错误；源目录只读挂载，源码副本、还原包和编译产物仅位于临时容器，退出后自动移除。未安装本机工具链、运行测试或调用实际云服务；云端上传结果仍无实测证据。

## Shell 取消与退出等待

继续核对 NetEngine SystemHelper 的同步 Shell 入口及内部进程等待循环。Rust 保留默认六十秒入口，补齐接受超时和 CancellationToken 的同步及异步入口。执行前先校验超时再检查取消；启动后按一百毫秒等待周期先检查退出，再判断取消和时限。标准输出和错误输出并行读取，取消或超时终止后等待根进程退出和两路读取完成，随后优先报告取消。正常根进程退出后等待输出期间不再应用执行超时，保持源流程。管道读取错误等待两路读取和进程结束后传播；进程等待或终止失败立即返回，保留清理守卫。

取消错误使用 Interrupted 并保留 OperationCancelled 类型，不按错误消息推断。非零退出保留原始 UTF-8 错误输出，包括尾部空白。Unix 信号退出码按 [.NET 原生进程实现](https://github.com/dotnet/runtime/blob/release/10.0/src/native/libs/System.Native/pal_process.c) 映射为 128 加信号编号；循环使用 [Tokio Child.wait](https://docs.rs/tokio/latest/tokio/process/struct.Child.html#method.wait) 的取消安全等待，不在每次等待到期时丢弃 Child。显式取消会等待退出；调用方直接丢弃 future 时仍只能尽力终止和由运行时回收，不能声称等价于完成一次有序取消。

Bash 去掉源实现没有的额外启动参数，继承标准输入；PowerShell 移除额外的 Set-StrictMode，继续保留终止错误及原生命令失败策略。临时脚本独占创建、写入不带 BOM 的 UTF-8，关闭写句柄后保持读取句柄；Windows 共享模式只允许读取，禁止写入或删除。脚本路径通过 Base64 传给固定包装器，子进程和 taskkill 使用 CREATE_NO_WINDOW。清理守卫只接管成功创建的文件，写失败先关闭句柄再清理。

当前无业务代码调用 Shell 助手，本轮未添加验证程序、测试或专用路由。Windows 条件分支、PowerShell/Office 实际运行仍未验证；Unix 清理仍按进程组发送信号，离开进程组的后代与 .NET 整树枚举存在差异，需继续完善。系统地址解析与日期格式解析的既有差异继续保留，整体框架目标尚未完成。本轮未继续修改 NetEngine 源仓库，保留上一轮授权的两个云上传相关文件修改。

本轮最终 dev 容器内全工作区 all-features/locked 的 check、严格 Clippy（lib/bins、-D warnings）、build 和格式检查均通过。未安装本机组件，未添加依赖或运行测试；上述退出行为的证据为源码对照与 Linux 编译，不作为跨平台运行验证结论。

## Linux Shell 后代进程清理

对照 [.NET Process.KillTree](https://github.com/dotnet/runtime/blob/release/10.0/src/libraries/System.Diagnostics.Process/src/System/Diagnostics/Process.Unix.cs)，Linux 清理改为暂停当前进程、枚举直接子进程、终止当前进程，再逐层处理已枚举后代。移除 Linux 启动时额外建立进程组的行为；子进程切换组或会话不再影响父子关系匹配。采用显式栈遍历，不因深层进程树增加 Rust 调用栈深度；暂停信号仍与源实现一样是尽力阻止继续派生，并非原子冻结整树。

独立模块 `common::system_helper::linux_process_tree` 依据 [/proc/pid/stat](https://man7.org/linux/man-pages/man5/proc_pid_stat.5.html) 读取父 PID 和开始时间，正确跳过含空格、换行或括号的 comm 字段。先前枚举的身份在发信号前重新核对；可用时通过 [pidfd_open](https://man7.org/linux/man-pages/man2/pidfd_open.2.html) 与 [pidfd_send_signal](https://man7.org/linux/man-pages/man2/pidfd_send_signal.2.html) 保持信号目标身份。不支持 pidfd 的旧内核按 ENOSYS 回退到逐次核对开始时间的 PID 信号；该回退仍存在核对与发送之间的竞争窗口，不声称完全消除所有 PID 复用风险。

忽略已经消失的进程，单个后代失败后继续处理其他分支并汇总错误。枚举失败时仍尝试终止当前已暂停进程；不因根进程已退出就吞掉其他后代的清理失败。清理守卫只尝试一次，随后仍由 Child 等待或尽力回收根进程，避免错误路径重复使用旧 PID。

本轮未调用实际 Shell 助手，没有新增测试、辅助校验程序或业务入口，也未添加依赖或安装组件。Windows 的 taskkill 路径与实际 PowerShell/Office 行为仍需核对，其他 Unix 平台仍使用进程组路径；此前全框架对比中的日期格式、地址解析、业务和部署等未完成项继续有效，整体目标未完成。NetEngine 仍保留上一轮授权的云上传修正，本轮未再修改源项目。

最终容器内全工作区 all-features/locked 的 check、严格 Clippy（lib/bins、-D warnings）、build 和格式检查均通过，git diff --check 通过。上述证据覆盖 Linux 分支的编译与接线，尚不能证明竞态、权限失败和多级进程树在实际运行中均与 .NET 相同。

## 图形验证码绘制与运行依赖

核对 NetEngine ImgHelper 和 BaseService 的实际调用链，移除图形验证码原来的 8×8 位图字体、固定字符间距和简化背景点。新增 skia-safe 0.157.0，按源绘制顺序使用白底、面积 1.5% 的随机背景线、七种亮色字符和面积 60% 的前景点；噪点 ARGB 取值保留 Random.Next(Int32.MaxValue) 对应的透明度范围。前景点覆盖目标像素，不与底图按 SrcOver 混合。长度和画布宽度按 UTF-16 单元计算，字体大小 45，基线为高度的 0.86 倍，字符位置按测量宽度前移；默认字体使用系统字体管理器，不再限制为内置位图字库。

字体入口依据 [SkiaSharp SKFont](https://github.com/mono/SkiaSharp/blob/v4.154.0/binding/SkiaSharp/SKFont.cs)，编译和运行依赖依据 [rust-skia](https://github.com/rust-skia/rust-skia)。三个宿主 Dockerfile 及云效模板补齐容器内 Clang、Ninja、Python 和 Fontconfig 开发依赖；运行镜像提供 Fontconfig、C++ 运行库和 DejaVu 字体。开发镜像已重建，stable 工具链及 latest 基础镜像约定保持不变。部署生成器已在容器执行，三个 generated 云效文件由模板同步；服务器运行依赖记录在部署文档，未连接服务器或执行部署。

在隔离的临时 PostgreSQL、Redis 和客户端宿主上调用现有 `/Base/GetVerifyCode`：返回 200、image/png、158×45、8 位 RGBA 图像，实际预览确认字符与噪点已绘出。Redis 存储四位数字，TTL 为 300 秒；使用同一 sign 再次调用，值被替换且 TTL 刷新为 300 秒。动态库检查确认 Fontconfig 与 libstdc++ 均解析成功。临时宿主已正常退出，数据库与 Redis 容器、临时配置、密钥、日志和预览图片均已清理；没有新增测试、模拟服务、Rust 校验程序或专用路由。

目前只验证现有四位数字业务入口。源代码逐个 UTF-16 char 传给 Skia，非 BMP 字符被拆成无效代理项；[Skia UTF-16 计数](https://github.com/google/skia/blob/chrome/m154/src/core/SkUTF.cpp) 对此返回无效结果。Rust 对不能独立绘制的代理项明确返回错误，未声称复现源原生库的全部失败形式。两边 Skia 版本、系统字体和随机序列不同，未作 PNG 字节级等价声明。二维码的字符编码/矩阵生成及裁剪、缩放的全部解码边界仍需核对；TOTP 和其余框架未完成项继续保留。NetEngine 源仓库本轮未修改。

最终容器内全工作区 all-features/locked 的 check、严格 Clippy（lib/bins、-D warnings）、build 和格式检查均通过，git diff --check 通过。没有安装本机组件或运行测试。未重新生成三个 Linux Release 发布包或实际构建完整运行镜像，不将开发容器中的接口实测扩大为全部部署环境验证。

## 裁剪、缩放与二维码编码

继续对照当前 NetEngine 的 `ImgHelper.Screenshot`、两端 `FileController.GetImage` 和 Common 的依赖声明。裁剪参数改为 i32，先检查文件及解码，再依次拒绝负偏移、非正宽高和越界区域；范围相加使用 i64。解码与绘制改用已有 Skia 依赖，保留解码颜色信息、预乘透明度以及 IncompleteInput 的处理，不自动应用 EXIF 旋转。目标使用平台 N32/Premul、默认最近邻采样和 PNG 编码；裁剪采用源 SkiaSharp 原生绑定的 Fast 源矩形约束。依据为 [SKBitmap.Decode/Resize](https://raw.githubusercontent.com/mono/SkiaSharp/v4.154.0/binding/SkiaSharp/SKBitmap.cs) 和该版本固定的 [sk_canvas.cpp](https://raw.githubusercontent.com/mono/skia/993e88f35c9d107669824cf5575c002964e4b087/src/c/sk_canvas.cpp)。

缩放保留先判断任一目标边超出原图、再计算缺失边的顺序。两个单边条件按源接口独立执行，使用 f32 比例与 i32 截断；非正结果进入共享内部异常响应。通过源和目标 Pixmap 的 ScalePixels 执行缩放，不再使用 image crate 的另一套解码与采样实现；现有公开错误返回类型保持不变。

在独立临时 PostgreSQL 中通过正式 repository_tool 执行业务迁移，配合临时 Redis 和现有客户端宿主验证 `/File/GetImage`。输入为现有验证码接口生成的 158×45 PNG：宽 79 返回 79×22，高 22 返回 77×22，指定 60×20 返回对应尺寸，均为 200 image/png 且无附件头。0×0 和超尺寸 159×20 保留原始字节及附件响应；-1×20、1×0 返回 Development 的共享 500 错误；-1×46 因高度先超限而返回原文件。未为验证新增路由、Rust 程序或测试。宿主收到 SIGTERM 后退出码 0，临时数据库、Redis、配置、密钥、图片、日志及工具副本已清理。

读取 NuGet `SkiaSharp.QrCode` 1.2.0 的 nuspec，确认源码提交为 `08ddc4fc2d467a9004b10130b5e02c071301eb84`。对照该提交的 [TextAnalyzer](https://raw.githubusercontent.com/guitarrapc/FeatherQR/08ddc4fc2d467a9004b10130b5e02c071301eb84/src/SkiaSharp.QrCode/Internals/TextAnalyzer.cs)、[QRCodeGenerator](https://raw.githubusercontent.com/guitarrapc/FeatherQR/08ddc4fc2d467a9004b10130b5e02c071301eb84/src/SkiaSharp.QrCode/QRCodeGenerator.cs) 和 [QRCodeRenderer](https://raw.githubusercontent.com/guitarrapc/FeatherQR/08ddc4fc2d467a9004b10130b5e02c071301eb84/src/SkiaSharp.QrCode/QRCodeRenderer.cs)，二维码改用整段 Numeric/Alphanumeric/Byte 模式，空文本明确写零长度 Byte 段。ASCII 不写 ECI，Latin-1 使用单字节及 ECI 3，其他 Unicode 使用 UTF-8 与 ECI 26，不添加 BOM；L 级从版本 1 至 40 选择能容纳内容的最小版本。绘制改用 Skia 的 500×500 白底、四模块留白和连续黑模块矩形，避免先生成低分辨率灰度图再缩放。

本轮未实际调用裁剪助手，也未对二维码执行扫码或逐模块比较；二维码的掩码评分、同分选择与全部矩阵边界仍需核对，不能据此声明输出完全等同。Skia 原生版本和平台解码差异、其他图像格式及不完整输入的运行边界仍未逐一实测。其余全框架差异清单继续有效，整体目标未完成；NetEngine 源仓库未新增修改。

最终容器内全工作区 all-features/locked 的 check、严格 Clippy（lib/bins、-D warnings）、build 及格式检查全部通过，git diff --check 通过。未添加依赖、安装本机组件或运行测试；Rust stable 和 Docker latest 的约定保持不变。

## 二维码掩码与双宿主实际对照

进一步核对 `qrcode` 0.14.1 的默认掩码评分，发现其黑白比例使用线性偏离分数，定位图案相似度允许把边界外视为留白，均不同于 NetEngine 引用的 SkiaSharp.QrCode 1.2.0。新增私有 `img_helper::qr_mask`，继续使用现有库的纠错码、交错和矩阵布局，通过公开 Canvas 接口分别应用 0 至 7 号掩码。评分包含候选的格式信息与版本信息：连续同色从五格开始计分、每个同色 2×2 方块计分、仅矩阵内完整的十一格正反定位图案窗口计分，黑白比例按 double 百分比两侧的 5% 档位取较小偏离；同分保留较小掩码编号。规则依据目标包对应提交的 [ModulePlacer.Masking.cs](https://raw.githubusercontent.com/guitarrapc/FeatherQR/08ddc4fc2d467a9004b10130b5e02c071301eb84/src/SkiaSharp.QrCode/Internals/StandardQR/ModulePlacer.Masking.cs)，未改动 Cargo 缓存中的库代码。

在 `mcr.microsoft.com/dotnet/sdk:latest` 临时容器中只读挂载 NetEngine，复制源码后编译并发布其现有 Client.WebAPI，未包含 Admin.App。首次 Linux 构建暴露源目录 `Logger.DataBase` 与引用 `Logger.Database` 的大小写差异；仅在容器副本中规范目录名称后发布成功，源仓库未改名。发布保留因未复制 Git 元数据产生的仓库定位警告。Fontconfig 仅安装到临时容器。两边宿主使用新生成的临时密钥、临时 PostgreSQL 和 Redis，无真实服务连接。

直接调用两个现有 `/Base/GetQrCode` 接口并比较响应字节，以下十组均返回 200、image/png、500×500 且 PNG 字节完全一致：20 位数字、`HELLO WORLD 123`、带长数字查询参数的 URL、Latin-1 的 Café/Noël/ÿ、中文/emoji/日文混合、400 个 A、1300 个 A、7089 个数字、4296 个 A、2953 个小写 a。后三组覆盖数字、字母数字、Byte 模式的 L 级最大容量，其中数字内容达到版本 40。结果验证这些实际输入，不扩大为所有字符串、所有模式和平台的穷尽证明。

边界调用发现 Rust 两端二维码生成失败被错误归为业务参数错误，已改为共享内部异常。修正后实际验证 7090 位数字、4297 个 A、2954 个小写 a：Development 两边均返回 500；Production 两边均为 400，JSON 的 errMsg 为 `系统全局内部异常`，Cache-Control、Pragma、Expires 一致。缺少 text、空 text 和空白 text 均返回 400，但 Rust 与 .NET 的模型验证文案仍有差异；Development 异常正文格式以及 JSON Content-Type 的 charset 也仍不同，后续应在共享绑定/响应层统一处理，不为二维码路由增加局部字符串替换。

这次临时数据库未建立日志分区，NetEngine 后台日志和维护会报告缺表；二维码调用不依赖这些表，未据此声称数据库日志、维护或健康检查已验证。所有临时宿主通过 SIGTERM 正常退出，源码副本、还原包、编译产物、配置、密钥、图片和日志随临时目录/容器清理，临时 PostgreSQL 与 Redis 已移除。没有添加测试代码、辅助验证入口或执行测试，未安装本机组件。

最终容器内全工作区 all-features/locked 的 check、严格 Clippy（lib/bins、-D warnings）、build 与格式检查通过；完整框架仍有已记录的业务、基础设施和响应行为差异，目标继续保持未完成。

## Query 整数解析与结构化绑定错误

公共输入组件保留错误类别、字段路径及原始消息，避免在逐层传播时只拼接文本而丢失分类。共享 WebAPI 绑定层将空白必填字符串转换为字段验证响应；缺少字段仍独立分类，不把所有缺失标量统一改为必填。字段匹配复用现有 ordinal-ignore-case 帮助函数，保留克隆模式的精确匹配，不修改字段值或字典键。Unicode 字段名尚未做双端运行验证。

根据 [.NET BaseNumberConverter](https://raw.githubusercontent.com/dotnet/runtime/release/10.0/src/libraries/System.ComponentModel.TypeConverter/src/System/ComponentModel/BaseNumberConverter.cs) 和 [ParseNumbers](https://raw.githubusercontent.com/dotnet/runtime/release/10.0/src/libraries/System.Private.CoreLib/src/System/ParseNumbers.cs)，Query 的 8/16/32/64 位整数支持 `#`、`0x`/`0X`、`&h`/`&H` 十六进制前缀，按目标位宽检查范围并保留有符号数的补码解释。外层前缀后的可选加号及 `0x` 前缀沿用源解析规则。显式可空 Query 标量转换器复用共享解析；JSON 数字字符串继续使用十进制规则，数组、对象和未声明转换器的可空字段不泛化为空值。

在隔离的 PostgreSQL、Redis 和两边现有 Client.WebAPI 上直接调用接口。`/Base/GetQrCode` 的 `text=`、空白 text 和大写 `TEXT` 空白值，两边均返回 400，正文均为 `{"errMsg":"text : The text field is required."}`。`/api/Demo/ARef` 无参数均返回 0；`a=0x10&b=2` 返回 18，`a=%23FF&b=1` 返回 256，`a=%26H10&b=1` 返回 17，`a=0xFFFFFFFF&b=1` 返回 0，`a=0x80000000&b=0` 返回 -2147483648，`a=%23%2B0xFF&b=1` 返回 256，重复 a 参数保留首值。上述结果两边一致。

`a=0x100000000` 与 `a=0x-1` 两边均拒绝并返回 400，但错误文案仍不同。现有 `/api/Demo/Add2` JSON 模型的 sort 为数字 16 或字符串 `"16"` 时，两边均成功；字符串 `"0x10"` 两边均返回 400，模型验证正文仍不同。缺少 QR text 的既有错误文案差异、模型错误聚合、JSON Content-Type 的 charset、其他整数位宽及可空转换器的实际接口边界尚未全部验证，不将上述案例视为整个绑定层完全等价。

.NET 使用 latest SDK 容器中的源码副本发布，仍仅在副本中规范 Logger.DataBase 目录大小写；源仓库保留已授权的腾讯云上传对象路径及接口注释两项修改。临时数据库未建日志表，后台缺表消息不影响本次调用，但不能据此证明日志或健康检查正常。两边宿主均经 SIGTERM 以 0 退出，临时容器、配置、密钥和日志已清理。未新增测试代码、验证路由或运行测试，未在本机安装组件。

容器内全工作区 all-features/locked 的 check、严格 Clippy（lib/bins、-D warnings）、build 和格式检查通过。框架其余已记录差异继续保留，整体目标尚未完成。

## Query 浮点数与标量边界

继续核对共享绑定器而非修改单个接口。ASP.NET Core 的 [FloatingPointTypeModelBinderProvider](https://raw.githubusercontent.com/dotnet/aspnetcore/release/10.0/src/Mvc/Mvc.Core/src/ModelBinding/Binders/FloatingPointTypeModelBinderProvider.cs) 使用 Float 和 AllowThousands；[DoubleModelBinder](https://raw.githubusercontent.com/dotnet/aspnetcore/release/10.0/src/Mvc/Mvc.Core/src/ModelBinding/Binders/DoubleModelBinder.cs) 直接解析原始值，不经过整数 TypeConverter 的预先 Trim。Rust Query 的 f32/f64 现按此区分：逗号只能在已有整数数字之后、小数点或指数之前出现，不强制三位分组；去除合法分隔符后仍由目标浮点类型校验完整数字。支持大小写不敏感且可带符号的 NaN、Infinity 以及数字溢出产生的无穷值；拒绝 Rust 接受而 invariant .NET 不接受的 inf 缩写。

依据 [.NET Number.Parsing](https://raw.githubusercontent.com/dotnet/runtime/release/10.0/src/libraries/System.Private.CoreLib/src/System/Number.Parsing.cs)，普通浮点数字的空白限定为 ASCII 的空格及 U+0009 至 U+000D，特殊符号单独按 Unicode 空白裁剪；数字允许尾部空白之后的连续 NUL，不能把 NUL 泛化为任意位置的空白。8/16/32/64 位整数仍先执行 TypeConverter 的 Unicode Trim，再按进制规则解析；十进制补齐尾部 NUL 处理，十六进制仍严格拒绝 NUL。按 [Boolean.TryParse](https://raw.githubusercontent.com/dotnet/runtime/release/10.0/src/libraries/System.Private.CoreLib/src/System/Boolean.cs)，Query 布尔文本允许两端 Unicode 空白与 NUL，内部字符仍必须构成 true 或 false。JSON、内部对象转换及字符串内容不应用这些 Query 规则。

错误响应的全面对齐仍未完成。[ModelStateDictionary](https://raw.githubusercontent.com/dotnet/aspnetcore/release/10.0/src/Mvc/Mvc.Abstractions/src/ModelBinding/ModelStateDictionary.cs) 根据参数与 DTO 属性的元数据生成不同消息，并区分格式/溢出异常与其他异常；现有 Rust 错误结构尚未包含全部元数据，没有用一套文本替换覆盖所有字段。上一节记录的非法整数、缺失字段、可空转换器和错误聚合差异继续保留。

本轮证据为源代码对照及 dev 容器内全工作区 all-features/locked 的 check、严格 Clippy、build 和格式检查通过，未对新增边界执行双宿主 HTTP 实测。当前现有请求模型没有直接声明 f32/f64 的 Query 接口，未为验证添加入口、辅助程序或测试。未安装本机组件、修改 NetEngine 源项目或启动临时基础设施；Rust stable 与镜像 latest 约定保持不变。完整框架目标仍未完成。

## 接口参数默认值与栏目删除约束

重新核对 NetEngine 当前提交 `23a7d0002938d0308c975dc48c04ac66aae0dda8` 的 Controller 参数声明。文件、栏目、文章、用户、角色、链接、LLM 和任务操作的 Query ID，以及文章 uploadKey，在各自请求模型中显式补齐缺失时的零值；只影响已核对的 Query 声明，传入空字符串、空白或错误数字仍拒绝。两端上传 key 使用显式可空 Query 转换器，sign 恢复必填。客户端 GetFunctionList 使用独立的必填 sign 请求模型，管理端继续使用可空 sign，共享业务服务契约不变。

沿实际调用链移除栏目、文章、链接详情查询及对应删除操作中额外的正数 ID 限制，按源服务查询活跃记录后判断结果。栏目不存在时删除返回 false；文章和链接不存在时删除仍返回 true。栏目存在时依次检查活跃子栏目与文章，命中时按源消息拒绝删除，再通过现有 xmin 写入机制软删除。文章编辑与文件批次绑定移除额外的 uploadKey 正数限制，保留原来的文件归属筛选和正文引用校验。EditArticleDto 的 isDisplay 缺省值改为源模型的 false；创建栏目、文章、链接的单值 ID 改为共享 JSON 字符串响应，修正原先纯文本返回。

使用独立 PostgreSQL、Redis、临时签名密钥以及现有 Rust 两端宿主实际调用接口。业务表由正式 repository_tool 迁移。缺失 fileId 与显式 0 均进入业务查询并返回文件不存在消息，空值和空白仍在绑定阶段返回 400；客户端缺失/空白 sign 返回 400，管理端对应请求返回 200。两端文件上传的空白 key、十六进制 key 均通过参数绑定并进入空文件校验，非法 key 仍返回 400。第一次创建栏目因临时令牌的用户 ID 没有对应记录而触发外键约束；随后通过正式 Development InitData 接口准备账号，并使用该隔离账号继续验证，没有移除约束或修改业务库。

实际创建父子栏目后，删除父栏目返回“当前栏目下存在子栏目，无法删除”；软删除子栏目后，省略 uploadKey 创建文章成功，详情返回 isDisplay=false；此时删除父栏目返回“当前栏目下存在文章，无法删除”。删除文章后父栏目删除成功，再次删除返回 false。最后重新调用三个创建接口，均返回 200 application/json，正文为带引号的 ID 字符串。临时宿主均经 SIGTERM 以 0 退出，临时数据库、Redis、配置、密钥、令牌、日志及迁移工具副本已清理。

本轮没有同时启动 .NET 宿主，源端证据为 Controller、应用服务和模型声明，不能据此声称所有 HTTP 细节一致。尤其是详情无记录时，Rust 当前仍返回 200 和 JSON null，而 ASP.NET Core 默认 [HttpNoContentOutputFormatter](https://raw.githubusercontent.com/dotnet/aspnetcore/release/10.0/src/Mvc/Mvc.Core/src/Formatters/HttpNoContentOutputFormatter.cs) 对空对象使用 204；需继续在共享响应层核对，不能把本次应用层空结果修正视为响应协议已对齐。缺失字段/非法数字的错误文案、完整 DTO 校验、并发下的栏目关系变化及其他清单项目继续保留。

最终 dev 容器内全工作区 all-features/locked 的 check、严格 Clippy（lib/bins、-D warnings）、build、格式检查与 git diff --check 通过。没有新增测试代码或验证路由、运行测试、安装本机组件；NetEngine 源仓库只保留先前授权的两项云上传修改。整体目标尚未完成。

## 空对象响应与缓存对照

共享 ObjectResponse 对顶层 null 返回空正文，不再序列化为 JSON null。业务路由外层在处理器完成状态码装配后，将空对象的 200 转为 204，并移除 Content-Length；其他显式状态保留。对象标记仍供缓存与 ETag 判断，处理顺序早于 CORS、压缩和请求日志。空数组、字符串及对象内部的 null 不受该规则影响。

使用临时 PostgreSQL、Redis、密钥与两边现有 Client.WebAPI 实际核对。业务及日志表由正式 repository_tool 迁移，NetEngine 使用 latest SDK 容器中的源码副本发布，仅在副本中规范 Logger.DataBase 目录大小写。查询不存在的用户，Rust Development、Rust Production 与 NetEngine Development 在 Accept 为 application/json、text/plain、application/xml 时均返回 204，正文为空，没有 Content-Type、Content-Length、Content-Encoding 或 ETag，允许来源的跨域头和 Vary 保留。

在最后一次空查询后十五秒内向隔离数据库写入对应用户，再以相同令牌及 URL 查询，三者立即返回 200，证明这些请求的空结果没有占用六十秒响应缓存。用户 JSON 正文逐字段一致，email 与 roleIds 仍为 null。Rust 两个环境另外验证空地区列表和空流式数组仍返回 200 与 []，不存在的文件仍为业务错误 400，未认证查询仍为空体 401。非空 JSON 响应头的 charset 仍与 NetEngine 不同，未将本次空结果修正视为整个响应层完全对齐。

再次核实云上传命名已按用户决定落实：NetEngine 腾讯云采用目录与本地文件名组合，接口注释说明远程参数为目录，Rust 两种云适配器使用相同约定。NetEngine 客户端发布成功，包含修改后的 FileStorage.TencentCloud；没有发送真实云上传或迁移既有对象。

容器内全工作区 all-features/locked 的 check、严格 Clippy、build 与最终格式检查通过，两仓库 git diff --check 通过。三个临时宿主均经 SIGTERM 以 0 退出，临时容器、数据库、源码副本、密钥、配置和日志已清理。未添加测试或验证入口、运行测试或安装本机组件；其余差异清单继续有效，整体目标尚未完成。

## JSON 类型头与错误结果边界

继续核对相同 NetEngine 提交的 ExceptionFilter、SignVerifyFilter、GlobalError、CacheDataFilter 与 QueueLimitFilter。共享对象响应及流式数组统一使用 application/json; charset=utf-8；错误输出复用同一类型头，但不携带可缓存对象标记。业务异常、内部异常、请求绑定错误、签名错误、请求参数读取失败及限流响应均经过共享错误正文构造。源 QueueLimitFilter 的拒绝结果是 BadRequestObjectResult，因此保留 Rust 对应对象结果身份，不把所有带 errMsg 的对象都归为异常或改变缓存筛选规则。

使用临时 PostgreSQL、Redis、签名密钥和两份现有 Rust Client.WebAPI 分别在 Development、Production 环境直接调用接口。普通数字对象、流式数组、非法 Query、非法 JSON、文件不存在业务错误及二维码容量溢出内部错误，均返回 UTF-8 JSON 类型头；生产环境 gzip 压缩继续生效。二维码溢出仍为 Development 500、Production 400，后者保留系统全局内部异常正文与禁止缓存头。纯文本 Echo 仍为 text/plain; charset=utf-8，未认证用户查询仍为空体 401，无记录查询仍为空体 204，均未被追加 JSON 类型头。

在正式迁移创建的隔离业务库插入用户并查询，再修改数据库名称字段，六十秒内相同令牌与 URL 仍返回第一次结果，两份宿主的缓存命中响应均带相同 UTF-8 类型头。另在该临时库短暂改名 User 表，对新的用户查询触发数据库异常，Development 返回 500、Production 返回 400，均无 ETag；恢复表名并插入对应用户后，同一查询立即返回 200 和新对象，证明本次异常没有进入对象缓存。

本轮未重新发布 NetEngine 参考宿主，默认 JSON 类型头的对照来自上一轮双宿主实际响应和当前源代码，不能扩大为内容协商、UTF-16、全部错误正文或全部过滤器组合已等价。签名仅做编译与共享调用链核对，未使用 Debug 请求验证生产签名校验。参数错误文案、开发环境异常格式及其余完整差异清单仍待继续处理。

dev 容器内全工作区 all-features/locked 的 check、严格 Clippy（lib/bins、-D warnings）、build、格式检查通过。两份临时宿主均经 SIGTERM 以 0 退出，临时数据库、Redis、密钥、令牌、配置、日志和迁移工具副本已清理。没有新增测试代码或辅助验证入口、运行测试、安装本机组件或修改 NetEngine 源项目；整体对齐目标仍未完成。

## Query 标量错误、分页属性与模型前缀

继续对照相同 NetEngine 提交的控制器及分页 DTO。公共输入错误新增 Query 标量类别，保留原始尝试值及是否为空；数字格式/范围与布尔转换失败不再只有字符串消息。显式可空 Query 转换器通过内部反序列化协议复用当前 Input，保留错误类别和字段路径；普通 JSON 反序列化器仍沿用原转换路径，未把空白转 null 泛化到其他字段。独立参数、分页属性及 PageSize 的范围错误由共享宿主分别格式化，依据 [ModelStateDictionary](https://raw.githubusercontent.com/dotnet/aspnetcore/release/10.0/src/Mvc/Mvc.Abstractions/src/ModelBinding/ModelStateDictionary.cs) 与 [绑定资源](https://raw.githubusercontent.com/dotnet/aspnetcore/release/10.0/src/Mvc/Mvc.Core/src/Resources.resx) 区分格式无效和空值无效，不反向解析旧错误字符串。

分页契约显式声明模型参数名、独立参数和额外属性名。现有分页模型统一按源声明选择 request 前缀：存在该前缀时只绑定带前缀的模型属性，不逐字段回退无前缀值；没有前缀时使用原来的属性名称。TaskSetting 的 category 仍是独立参数，不能改由 request.category 提供。错误键使用源模型的 Page、PageSize、IsEnable、IsSuccess 及选中的前缀。两端 GetFile 的 Query 名称显式对应源参数 fileid，大小写不敏感匹配仍保留。重复参数继续取首值转换；错误中的多值文本按 [StringValues](https://raw.githubusercontent.com/dotnet/runtime/release/10.0/src/libraries/Microsoft.Extensions.Primitives/src/StringValues.cs) 的合并规则跳过空项，保留空白项。

在隔离 PostgreSQL、Redis 和两边现有 Admin.WebAPI 上实际调用。NetEngine 在 latest SDK 容器内复制源码发布，未包含 Admin.App，仅在副本中规范 Logger.DataBase 目录大小写；业务和日志表使用正式 repository_tool 迁移。首轮核对独立整数与布尔的非法值、空值、空白、溢出、重复值和大小写，可空 long 的非法输入、十六进制溢出及空值，以及分页整数、范围和可空布尔错误，确认对应响应。实际对照发现并修正 fileid 大小写、重复空项和模型前缀处理差异。

更新后的二十五组双端请求状态码与解析后的 JSON 正文全部一致，包含 request.page 的非法值、request.pageSize 的范围错误、大写 REQUEST.PAGESIZE、带前缀与普通参数混用，以及 request 自身、未知属性、方括号前缀触发的整体选择。临时库插入两条禁用任务定义后，带前缀分页加独立 category 正确筛出对应一条；只有 request.category 时两边都忽略该值并返回两条，同时提供两者时独立 category 生效。可空 userId 的空白和十六进制仍通过绑定，空用户结果为 204；可空 IsSuccess 空值仍进入正常查询。

仍存在明确差异：多个字段同时出错时 Rust 只返回首个错误，NetEngine 聚合各字段第一条错误；缺失必填 text 仍缺少完整类型/验证元数据；日期转换、嵌套集合、其他模型验证和 JSON 错误正文尚未全部对齐。本轮仅修正 Query 错误中的 StringValues 合并，签名与请求参数快照中重复空字段的合并仍应继续核对，不能据此声称所有多值处理一致。

最终 dev 容器内全工作区 all-features/locked 的 check、严格 Clippy、build、格式检查与 git diff --check 通过。两边临时宿主均经 SIGTERM 以 0 退出，临时容器、数据库、源码副本、配置、密钥、令牌和日志已清理。没有新增测试代码、验证入口或运行测试，未安装本机组件；NetEngine 只保留此前授权的两项云上传修改。整体框架目标继续保持未完成。

## 多值表单、认证头与文化排序

继续对照 NetEngine 提交 23a7d0002938d0308c975dc48c04ac66aae0dda8。公共字符串辅助方法统一实现 StringValues 文本合并：跳过空字符串，保留空白字符串与非空值顺序，仅在非空项之间添加逗号。请求签名、参数快照、认证及缓存/请求队列的完整 Authorization、Query 错误中的尝试值、Ueditor action 与涂鸦内容复用该规则。表单键按 OrdinalIgnoreCase 合并并保留第一次出现的拼写；multipart 文件仍分别计算摘要，不合并文件。

实际 Release 对照发现 B、a 两个字段的排序会使原来的字节序签名与源端不一致。公共辅助组件改用 ICU 文化稳定排序，普通表单字段、multipart 文件字段和参数快照共同调用；等价键保留输入顺序，不补充字节序比较。Linux 默认文化从 LC_ALL、LC_MESSAGES、LANG 依次选择第一个已定义值，C/POSIX 与空值映射为 invariant。新增 icu_collator 和 icu_locale_core 依赖并在容器更新锁文件。依据包括 [.NET ICU 排序实现](https://raw.githubusercontent.com/dotnet/runtime/release/10.0/src/native/libs/System.Globalization.Native/pal_collation.c)、[默认文化映射](https://raw.githubusercontent.com/dotnet/runtime/release/10.0/src/native/libs/System.Globalization.Native/pal_locale.c) 和 [ICU 环境变量选择](https://raw.githubusercontent.com/unicode-org/icu/main/icu4c/source/common/putil.cpp)。

两边 Admin.WebAPI 与 Client.WebAPI 均以 Release 构建、Production 环境在容器运行，使用隔离 PostgreSQL、Redis 和临时签名密钥，业务及日志表经正式 repository_tool 迁移；没有添加验证入口。普通表单的前置/中间/全部空值、空白保留、大小写和 Unicode 大小写合并均接受相同正确签名，旧的带空逗号签名均被拒绝。multipart 对应空值样例及超过内存回放阈值的文件同样通过，空 Token/Time/Authorization 请求头与正常值并存时也一致。Ueditor 的空 action 加 config 请求成功返回配置。

更新排序后，B/a、ä/z、组合与分解重音字符的稳定顺序，以及分别带文件的 B/a 字段在默认环境均得到相同状态；错误字节序签名返回 401。将两个管理端的 LANG 切换为 sv_SE.UTF-8 后，z 排在 ä 之前的签名均返回 200，默认文化的相反顺序均返回 401。客户端实际查询用户后修改隔离数据库名称，再去掉表单/Query 空项并追加空 Authorization，六十秒内两边都返回原名称，证明这些参数变体命中原缓存；另一组请求调整表单字段顺序后也命中原结果。

单独构建管理端 Release 时还发现 application_service 使用 RequestBuilder.query 却没有声明 reqwest 的 query 特性；此前全工作区特性合并掩盖了问题。已在所属 crate 显式启用该特性，管理端和客户端单独 Release 构建以及后续两个宿主的 Release 构建均通过。全工作区 all-features/locked 的 check、严格 Clippy、build、格式检查及两仓库 diff 检查通过。

本轮不代表所有平台文化完全一致：POSIX @ 修饰符尚不支持，ICU/CLDR 版本差异、全部 Unicode 文本及 .NET 每线程文化覆盖仍需继续核对。此前多字段错误聚合、必填元数据和其他功能差异继续有效。临时宿主已停止，临时数据库、Redis、源码副本、密钥、配置与日志已清理，只保留开发容器。未新增测试、运行测试或安装本机组件；NetEngine 源项目仍仅有用户授权的两项云上传修改，整体目标尚未完成。

## 缺失 Query 字段与验证码 Guid 参数

继续对照相同 NetEngine 提交的 BaseController 和统一 InvalidModelStateResponseFactory。共享 Query 错误格式化将 MissingField 转为带字段路径的必填提示，不再暴露 Serde missing field；不通过错误字符串推断类型、不为缺失字段补值，也不修改 JSON 绑定。DTO 中允许省略的值类型参数仍通过显式默认值声明。

实际请求进一步发现客户端 GetVerifyCode 的 sign 在源端为 Guid，而 Rust 原先先绑定 Option<String>，再在处理器中使用 UUID 库解析；空值提示、非法格式及支持的格式集合均不同。现改为带默认空 Guid 的 Uuid 参数，并显式使用公共 deserialize_query_guid。转换协议保留 Query 错误类别、原始尝试值和字段路径，复用已有数据导入 Guid 解析器，覆盖 N/D/B/P/X、兼容分量和空白规则，不接受额外的 urn:uuid 前缀。数据导入与请求绑定没有复制两套解析逻辑，验证码缓存及业务服务无需改动。

使用正式 repository_tool 迁移隔离 PostgreSQL，配合临时 Redis 和现有两边 Client.WebAPI 实际调用。Rust 使用本轮 Debug 二进制，NetEngine 使用 Release 发布，两者均设为 Production；这轮只验证输入绑定和返回结果，不用 Debug 结果证明生产签名。最初参考镜像缺少 libfontconfig，合法二维码请求进入源图像代码后失败；随后从 latest SDK 构建含图像依赖和字体的临时镜像，重新发布参考宿主，依赖仅安装于容器镜像。一次 NuGet 下载 EOF 经现有发布进程重试后恢复，没有切换本机工具链或固定版本。

最终二十五组双端请求全部符合预期：二维码 text 缺失、无关参数、大小写空值、空白、首个重复值为空均返回相同 400 正文；合法文本及首项合法、末项空值返回 200 image/png、500×500。验证码覆盖缺失 sign、空值、空白、非法文本、N/D/B/P/X、X 内部空白、外部空白、URN 拒绝、重复值与大小写别名，并核对 X 的 short 截断及 D 的旧前缀分量。错误状态和 JSON 正文一致，合法输入均返回 200 image/png、158×45。隔离 Redis 中两端业务键都使用规范化 Guid；未逐像素比较随机验证码或二维码。

更新后的全工作区 all-features/locked check、严格 Clippy（lib/bins、-D warnings）、build、格式检查及两仓库 diff 检查通过。两份临时宿主均经 SIGTERM 以 0 退出，临时容器、数据库、Redis、参考镜像标签、源码副本、密钥、配置与日志已清理。没有新增测试代码、验证路由或运行测试，也没有安装本机组件；NetEngine 源仓库仍只有此前授权的两项云上传修改。多字段错误聚合、完整模型验证元数据、JSON 错误正文和其余差异清单仍待完成，整体目标继续保持未完成。

## Query 多字段错误与分页验证汇总

源端仍为 NetEngine 23a7d0002938d0308c975dc48c04ac66aae0dda8。本轮将 Query 绑定从遇到首个错误就返回，改为由 QueryModel 派生宏逐字段绑定并收集错误。宏实现位于 source_generator_core，公共契约和运行时字段适配位于 common；现有 Query/PageQuery 模型显式派生，RsaQuery 使用相同契约。字段名、默认值、可空转换器和 Guid 转换器复用 Serde 声明，未知参数不复制到每个字段，全部字段通过后才构造业务请求对象。没有用删除错误参数并反复解析整个模型的方式模拟聚合。

字段支持 query(validate = "函数路径")。分页 page_size 使用模型层 validate_page_size_query，转换成功时执行范围校验，即使 Page 或 IsEnable 转换失败，仍保留 PageSize 范围错误；转换失败的字段不再追加范围错误。业务服务原有分页防御性验证保持不变，JSON 反序列化没有加入这套 Query 校验。

宿主汇总各字段第一条错误，以竖线分隔，保留独立参数名称、分页属性显示名和模型前缀。顺序依据 [ModelStateDictionary 的节点排序和枚举实现](https://raw.githubusercontent.com/dotnet/aspnetcore/release/10.0/src/Mvc/Mvc.Abstractions/src/ModelBinding/ModelStateDictionary.cs)：按树的广度遍历，同级先比较 UTF-16 长度，再按 OrdinalIgnoreCase 比较；不使用请求参数顺序或签名的文化排序。分页整体前缀选择与独立 category 仍沿用原规则。

使用隔离 PostgreSQL、Redis、临时密钥与令牌，通过正式 repository_tool 创建业务和日志模型。NetEngine 两端从只读源仓库复制到 latest SDK 容器，排除 Admin.App，仅在副本规范 Logger.DataBase 大小写，发布 Release 宿主；Rust 使用本轮 Debug 二进制，四个宿主均设置 Production。本轮范围为请求绑定和业务结果，不以 Debug 请求证明签名一致性。

二十八组实际双端请求状态与解析后的正文全部一致。覆盖 Add 的双参数错误、请求逆序、大小写重复、空值、默认零与正常加法；地区参数同时无效；缺失二维码文本和非法 Guid 的回归；Page/IsEnable/PageSize 多重转换错误、范围错误混合、大写前缀、整数溢出、页码负数、页大小边界和未知前缀触发的整体回退。正常 LLM 分页保留 null 列表，正常任务分页保留空数组。客户端上传使用真实 multipart 文件及有效认证，缺失 business/sign 和同时无效的 key/isPublicRead 均汇总出与源端相同的错误，未进入文件业务写入。

完整工作区 all-features/locked check、严格 Clippy（lib/bins、-D warnings）、build、格式检查和两仓库 diff 检查通过，覆盖过程宏及运行时辅助 crate。四个临时宿主均经 SIGTERM 以 0 退出，临时容器、数据库、Redis、源码副本、配置、密钥、令牌与日志已清理。没有新增测试代码、验证入口或运行测试，未安装本机组件；NetEngine 源仓库仍只保留授权的两项云上传修改。

当前 QueryModel 宏限定非泛型具名字段，不支持 flatten；嵌套 Query 模型、集合索引、日期错误的完整元数据及 Query 与正文跨提取器错误合并仍未完成。不能将本轮多个独立字段与分页属性的验证扩大为全部模型绑定完全一致，整体框架目标继续保持未完成。

## Query 日期转换与时间范围

源端仍为 NetEngine 23a7d0002938d0308c975dc48c04ac66aae0dda8。日志、队列和 LLM 会话分页模型显式使用公共 deserialize_optional_query_date_time，并声明 StartTime、EndTime、StartCreateTime、EndCreateTime 的属性名称。空白绑定为无值，日期格式或范围错误保留原始 Query 尝试值、前缀和多字段错误汇总。JSON 日期、计划任务正文模型和响应序列化不使用这套 Query 协议。

转换按 invariant 日期规则处理完整年月日、英文月份、数字短日期、时分秒、AM/PM 和时区偏移；两位年份以 2049 为分界。ISO 的 T 分支单独检查两位时分秒，普通日期可使用 GMT、只写小时的 AM/PM 和点分日期。秒的小数按双精度累积后舍入至百纳秒，再检查进位；拒绝闰秒、超过正负十四小时的偏移，以及本地或 UTC 时间超出 1 至 9999 年的值。规则参考 [DateTimeParse](https://raw.githubusercontent.com/dotnet/runtime/release/10.0/src/libraries/System.Private.CoreLib/src/System/Globalization/DateTimeParse.cs)，不把 Chrono 默认 RFC3339 解析等同于源转换器。

使用隔离 PostgreSQL、Redis、临时密钥和现有两边 Admin.WebAPI 实际调用。业务与日志模型经正式 repository_tool 迁移，隔离会话记录覆盖相邻微秒与次日边界。Rust 使用本轮 Debug 二进制，NetEngine 从只读源码复制到 latest SDK 容器并发布 Release，排除 Admin.App，仅在副本规范 Logger.DataBase 大小写；两边设为 Production。本轮验证请求绑定与查询结果，不用 Debug 请求证明生产签名流程。参考宿主发布包含此前获准的腾讯云上传修正，没有执行真实云上传。

首批三十八组请求中三十五组一致，覆盖空白、无效日期、日期格式、微秒过滤、百纳秒舍入、闰年、日期与偏移边界、重复值、模型前缀和错误聚合；成功响应比较状态码、总数及结果 ID，错误比较完整 JSON 正文。另三组合法的 +08:00、+8、+0800 在 Rust 正常返回，NetEngine 因 Npgsql 拒绝非 UTC DateTimeOffset 而返回内部异常。源 LLM 查询直接传入偏移值，日志与队列查询也存在直接传值的调用点。已提出源端查询前转 UTC 的修正选项，尚未获该项决定；没有修改这些 NetEngine 文件，也没有让 Rust 人为抛出同样异常。

后续通过已有会话接口附带非法 pageSize，专门核对三十八组日期绑定格式；修正英文月份连字符、点分日期、小时 AM/PM 和 ISO 位数检查后，三十八组错误正文全部一致。再将两边 LANG 改为 sv_SE.UTF-8，十三组实际日期查询的状态、错误或筛选结果一致，包含 invariant 月日顺序、AM/PM、星期校验与 ISO 分支；该组没有改变本地时区。省略年月日的自由日期形式、其他未覆盖语法与夏令时无效/重叠时间仍待核对，不能声称 DateTimeOffset.Parse 全部行为已等价。

最终 dev 容器全工作区 all-features/locked check、严格 Clippy（lib/bins、-D warnings）、build、格式检查和两仓库 diff 检查通过。临时宿主均经 SIGTERM 以 0 退出，临时容器、数据库、Redis、源码副本、配置、密钥、令牌与日志已清理，只保留开发容器。没有新增测试代码、验证入口或运行测试，也未安装本机组件。NetEngine 源仓库仍仅有用户授权的两项云上传修改；完整框架目标尚未完成。

## 正文模型验证与源端 UTC 查询修正

公共 JsonModel 契约和派生宏为请求模型声明验证元数据，复用 Serde 的命名、别名与默认值，不在处理器中逐个检查字段。管理端和客户端二十七个请求及嵌套模型显式接入；登录、用户、角色、文章、链接、LLM 和创建定时任务保留源 Required 的中文消息，用户角色列表保留 MinLength 与 null 的不同含义。共享 Json、JsonWithBody 和 RsaJson 在类型转换成功后汇总模型错误，复用 ModelState 的属性路径和排序；格式错误不追加尚未成立的模型验证结果。

嵌套文件列表保留数组索引，支付通知保留源属性的小写及下划线；缺少整个资源对象只报父属性，提供空对象则验证子字段。数值 ID 和日期虽然在部分返回 DTO 中保存为字符串，仍通过 json(value) 保留原转换器，不误报普通字符串必填。没有将校验扩展到内部 JsonHelper、响应序列化或动态字典键。

对照源 DTO 补齐设置角色/功能请求的零值和 false、编辑任务的 false、创建定时任务的 true，以及 LLM 调用模型 ID 的零值。文章 CategoryId 按源 DTO 改为字符串，应用服务解析为整数；编辑先查询文章，再转换栏目值。移除源服务没有的栏目正数与活跃记录预查询，仍由数据库外键约束引用关系；非法数值字符串通过共享内部异常处理，缺少文章仍优先返回无效 articleId。

隔离 PostgreSQL 和 Redis 通过正式 repository_tool 准备业务与日志模型。NetEngine 源码只读挂载后复制到 latest SDK 容器，排除 Admin.App、仅在副本规范 Logger.DataBase 大小写，并发布两端 Release；Rust 两端使用本轮 Debug 二进制，均设为 Production。这轮验证正文绑定、业务结果与日期查询，不以 Debug 请求证明生产签名。NuGet 包缓存保留于 Docker 命名卷，不安装 Windows 组件。

管理端二十六组正文请求、客户端规范路径的十三组请求及三组支付通知请求，状态码与解析后的 JSON 正文一致。覆盖多个必填错误、中文提示、隐式必填、空白/null/省略、角色集合长度、重复大小写字段取最后赋值、嵌套文件索引、缺少与空资源对象；合法 Demo Add2 仍返回 3。另核对非法栏目字符串、无效文章优先级及省略 LLM 模型 ID 的业务结果。支付、短信、微信和 LLM 的无效请求停在绑定或配置检查阶段，没有调用真实外部服务。

用户随后明确授权修正 NetEngine 的非 UTC 查询。源 LlmConversationManageService、LogQueryService 和 QueueTaskManagement 在构造查询条件前将上下界转换为 UTC 局部值，不修改请求 DTO 或同一时刻的含义。重新发布两端后，对三个接口分别用 +08:00、+8、-05:00 和 Z 查询隔离记录，十二组请求全部返回 200，数量及结果 ID 与 Rust 一致；开始、结束边界及相邻微秒记录均符合预期。此前非 UTC 查询异常已关闭，NetEngine 工作区新增这三项获准修改。

仍有经实际请求确认的差异：格式错误如字符串字段传数字、角色数组传字符串时，Rust 尚未输出源端的正文参数必填、JSON 路径和字节位置组合提示；路径大小写变化时，源路由可匹配而 Rust 当前要求登记的大小写。两次拼错微信路由的 404 不计为模型验证证据，之后已按规范路由重做请求。根 null、空正文、完整嵌套空元素与 Query/正文跨提取器合并仍需继续处理，整体框架目标未完成。

## StoredFile 表映射与无损重命名迁移

实际创建文章时，参考宿主报 StoredFile 不存在，而 Rust 旧迁移创建 File。对照 NetEngine 的 DatabaseContext.StoredFile 与 StoredFile 实体后，将 repository::file 的物理表、当前索引声明和工具默认值所属表改为 StoredFile；Rust 模块路径保持原有分层。再次核对两边业务表名清单，二十七张表均有对应映射，此证据仅覆盖表名，不能证明全部列、约束和查询完全等价。

通过容器内 repository_tool 离线生成 20261107105933_align_stored_file_table。模型对比确认唯一变化为 File 重命名，其他表和扩展未变化。生成器最初输出删除和重建步骤，审查后改为 Up/Down 各十三条重命名语句，覆盖表、主键、两个外键和九个索引；没有执行生成器的删除/建表方案，也未改旧迁移或历史快照。新版本已由 generated/index.rs 登记，migrate 与 migration-script 共用定义。

隔离旧库先插入文件记录，再通过正式迁移执行升级、回退至前一版本及重新升级，三个阶段的完整记录摘要与 xmin 均保持不变。另建隔离空库执行九项完整业务迁移，最终只有 StoredFile、没有 File；migration-pending 确认当前模型与最新快照一致，json-index 只读检查成功。实际业务数据库未执行迁移，物理文件和云对象均未操作。

升级隔离库并重启新 Rust 宿主后，两边使用字符串栏目 ID 实际创建文章都返回 200 与字符串 ID，数据库确认两条记录保留同一栏目、默认 IsDisplay=false。栏目夹具为软删除记录，证明源服务依赖外键的行为与修正后的 Rust 一致；第一次参考端缺表失败没有被当作成功证据。

最终容器内完整工作区 all-features/locked check、严格 Clippy（lib/bins、-D warnings）、build、格式检查通过，两仓库 git diff --check 通过。四个临时宿主均经 SIGTERM 以 0 退出，参考源码副本、临时 PostgreSQL/Redis、两个隔离数据库、配置、密钥、令牌、日志及工具副本已清理，只保留运行中的开发容器和包缓存。没有新增测试代码、验证入口或运行测试；后续需要继续处理正文格式错误、路由匹配和其余功能差异，未标记整个目标完成。


## 控制器路径与 HTTP 方法匹配

继续核对源 HEAD 23a7d0002938d0308c975dc48c04ac66aae0dda8，确认控制器路由不区分大小写、允许一个末尾斜杠；普通 GET 不自动响应 HEAD，方法不匹配先返回 405。此前 Rust 使用 Axum 默认匹配：混合大小写返回 404，HEAD 执行 GET，受保护接口的错误方法会先触发权限守卫而返回 401。这轮均在共享宿主修正，没有修改 NetEngine 源码。

新增 webapi_core::routes::WebApiRoutes 与方法构建器，实际注册时收集路径和 HTTP 方法，十八个 Feature 的一百一十处路由声明改用同一入口。OpenAPI 和权限清单不充当端点来源，未提供 OpenAPI 的 UEditor 也参与；移除旧的小写 ueditor 专用别名。相同路径的不同方法先合并再装配，只有大小写不同的重复模板拒绝启动。当前支持实际使用的字面量段和完整命名参数段，复杂模板显式拒绝，不静默按错误规则匹配。

最外层根据模板选择规范路径，内层选中端点后在所有原有宿主中间件之前恢复原 URI 与 OriginalUri；原始查询串、路径大小写、末尾斜杠和参数值均不改写。静态端点优先于参数端点，参数模板按较早字面量段排序。健康检查单独登记，Swagger 和文件路径不套用控制器匹配，不通过全局小写化路径实现兼容。

方法声明保留显式 Allow，控制器 HEAD 在 Action 守卫之外拒绝。权限、签名、响应缓存、ETag 与请求队列包装器改用 route_layer，只作用于实际处理器；错误方法返回空体 405。CORS 已完成的预检带内部标记，最外层移除 Axum fallback 后补的 Allow，不影响普通 405。UEditor 仍允许 GET、POST，健康检查保留任意方法。

用正式 repository_tool 在临时 PostgreSQL 创建业务和日志模型，并运行临时 Redis。源项目只读复制到 latest SDK 容器，在副本规范 Logger.DataBase 大小写后发布两端 Release；Rust 客户端使用最新 Debug，管理端使用最新 Release，环境均为 Production。客户端二十八组请求的状态、解析后正文、Allow、认证质询和跨域来源一致，覆盖混合大小写、单/双/中间斜杠、正确和错误方法、HEAD、未知路径、支付参数路由、查询验证、登录必填和已认证改手机号的必填验证。没有发送短信或支付通知到外部服务。

管理端十六组对比全部一致：Release 下原始大小写和末尾斜杠参与签名时正常查询，只用登记路径签名则返回 JSON 401；同时核对空体认证 401、错误方法 405、UEditor 的 GET/POST Allow、健康检查及静态文件大小写。静态文件只有原文件名成功，大小写变化或额外末尾斜杠为 404。最初一组请求误用了小写摘要并附带非法模型参数，不计为通过证据；随后改为协议要求的大写摘要与合法分页参数，重新执行并确认完整的十六组结果。

额外请求确认尚未处理的差异：/api/%44emo/Echo 与 /%61pi/Demo/Echo 在源端匹配，Rust 仍为 404；已认证请求同时包含非法分页值与错误签名时，源 ApiController 的模型验证先返回 400，而 Rust 签名中间件先返回 401。前一轮 JSON 格式错误的参数名、路径与字节位置提示等差异也仍未关闭。现有证据不代表路由和模型管线已完全等价，整体目标继续保持未完成。

最新代码通过容器内全工作区 all-features/locked check、build、严格 Clippy（lib/bins、-D warnings）、格式检查，以及管理端 all-features Release 编译。四个临时宿主均经 SIGTERM 以 0 退出，临时数据库、Redis、参考源码副本、配置、密钥、令牌、日志及工具副本已清理。保留开发容器和已有包缓存；没有本机安装、新增测试代码或运行测试。


## 编码路径、URI 表示与缓存日期

本轮继续使用 NetEngine HEAD 23a7d0002938d0308c975dc48c04ac66aae0dda8，没有新增源项目修改。前轮已确认的百分号编码字面量路由差异已修正。共享 request_path 在路由之前解码一次路径，保留编码斜杠及其十六进制大小写，保留无效/过长/代理区/截断 UTF-8，消除完整点段并保留中间空段；编码 NUL 返回空体 400，HTTP/1 关闭连接。查询串不参与路径解码，普通加号不成为空格。

路由、响应缓存键和静态文件使用解码值；签名、日志完整地址和 HTTPS 重定向使用 PathString 风格的 URI 表示。源签名中的 Path + QueryString 会调用 PathString 的 URI 转义，不是直接拼接解码字符串，也不是原请求行照抄。Axum 路由临时 URI 转义全部百分号，以避免参数二次解码；完成匹配后恢复原始传输 URI。静态文件不再自行额外百分号解码，编码斜杠对应字面文件名，不能意外访问子目录。

隔离环境通过正式工具迁移业务与日志模型，创建临时用户及静态文件。参考源码只读挂载并复制到 latest SDK 容器，仅在副本修正 Logger.DataBase 大小写，发布两端 Release；Rust 客户端 Debug、管理端 Release 均为 Production。二十八组路由/绑定请求、三十一组静态文件请求、十组 Release 签名请求和三十五组 HTTPS 重定向的状态、正文或 Location 与源端一致。包括编码 ASCII/中文、空格与加号、编码斜杠、重复编码、非法 UTF-8、问号/井号文件名、点段、中间空段、NUL，以及原查询串与重新转义路径对签名的影响。支付参数路由使用非法正文止于绑定，没有调用提供商；该证据验证路由与绑定结果，不宣称验证了真实商户业务。

实际缓存验证进一步发现：源 CacheDataFilter 使用公共 JsonHelper 序列化原始对象，日期在缓存命中后为 yyyy/MM/dd HH:mm:ss zzz，而首次 MVC 响应为 ISO。此前 Rust 直接复制 HTTP JSON，导致缓存命中日期格式不同。UserDto.create_time 现保留 DateTime<FixedOffset>，HTTP 序列化保留显式偏移；公共对象 JSON 根据原始日期类型或显式日期标记选择内部格式，不猜测普通字符串。对象克隆单独选择 ISO 日期策略，对应源 cloneOpts 没有日期转换器的行为。

对象响应只在缓存过滤器的异步任务作用域内额外生成内部 JSON；普通响应继续只生成 HTTP JSON，ETag 使用 HTTP 表示。缓存序列化失败只跳过写入并记录固定诊断，不改变已生成的业务响应。缓存命中按 JSON 文档返回，不再次推断字段类型。此前缓存键实验的首次结果暴露日期差异，未计为通过；修正后用新 Authorization 重做五组连续请求：修改隔离用户后，编码别名仍命中旧值，大小写不同路径读取新值，首次/命中日期格式和完整对象均与源端一致；名字设置为 ISO 外观的字符串也保持原样。之后在最新 Release 再核对十组签名请求，全部一致。

最新代码通过完整工作区 all-features/locked check、build、严格 Clippy（lib/bins、-D warnings）、格式检查，以及管理端 all-features Release 编译。没有新增测试文件、测试依赖、验证端点或运行测试，没有本机安装。输入模型验证先于签名的顺序、JSON 格式错误完整提示及其他未覆盖功能继续保留为待办，整体翻版目标未完成；本轮也不宣称已覆盖全部 Kestrel 请求行/HTTP2 边界或复杂路由模板。

收尾确认：四个临时宿主均经 SIGTERM 以 0 退出，参考源码副本、隔离数据库、Redis、静态文件夹具、配置、密钥、令牌、日志和工具副本已清理，只保留开发容器及已有包缓存。源项目仍仅保留此前五项获准修改。

## 参数绑定先于 Action 过滤器与共享正文回放

继续核对 NetEngine HEAD 23a7d0002938d0308c975dc48c04ac66aae0dda8，源仓库仍只有已授权的云上传两处及 UTC 查询三处修改。本轮将控制器调用拆为认证/续签/权限、参数提取与验证、Action 过滤器和业务调用。共享 BoundHandler 保存成功绑定的参数，签名、缓存、队列和 ETag 放行后才调用业务函数；过滤器提前返回时释放参数。没有在各业务处理器重复解析或预校验，也没有把权限守卫移到绑定之后。相同路径的各方法分别保留绑定与守卫，方法拒绝继续在守卫之外。

ReplayBody 在实际读取时才填充缓存，各读取流保留独立偏移。JSON 绑定后的签名读取同一份原文，流式 Multipart 的签名与后续处理也各自从起点读取；超过 30 KiB 转匿名临时文件，不将文件整体载入内存。缓存文件读写失败由共享内部异常响应处理，避免 JSON 提取器将底层系统信息作为参数错误返回。最后一个回放句柄释放后关闭临时文件；实际上传结束后检查宿主没有遗留匿名文件句柄。

通过正式 repository_tool 为临时 PostgreSQL 准备业务和日志模型，运行隔离 Redis。参考源码只读复制到 latest SDK 容器后发布两端 Release，仅在副本规范 Logger.DataBase 目录大小写。Rust 管理端使用本轮 Release，客户端使用本轮 Debug，全部设置 Production；客户端请求不用于证明 Release 签名行为。Windows 未安装任何组件。

十四组顺序对比中十三组的状态、解析后正文、认证质询与 Allow 一致，覆盖无效分页结合缺失/错误/过期签名、必填正文结合错误签名、认证失败优先、有效参数的签名拒绝和业务错误，以及错误 HTTP 方法。剩余一组非法 userId 的两端状态均为 400，但 Rust 仍缺源端的正文参数必填、JSON 路径和字节位置组合提示，未计为通过。最初运行暴露签名包装器尚未切换到 Action 阶段，修正并重新编译后才取得上述结果。

超过 30 KiB 的有效 JSON 正文在正确签名下到达无效 articleId 的业务错误，错误签名返回 401，两端一致。连续三次向带队列过滤器的 CreateRole 提交空对象，均先返回相同模型错误，没有返回频繁操作提示。客户端 GetUser 首次与缓存命中对象一致，日期分别保持 ISO 和内部缓存格式，匿名请求仍返回空体 401；另外核对 Echo 参数、Add2 的 Query 与 JSON 同时绑定和流式数字响应。误用 GetFunctionList 路径产生的 404 未作为功能证据，按正确路径补查后，两端均返回缺少 sign 的参数错误，此请求不证明功能列表缓存。

两端分别上传 11 MiB 文件：错误签名返回 401，正确签名返回字符串文件 ID，随后下载的长度与 SHA-256 均等于各自原文件。文件及记录只写入隔离环境，没有操作实际业务库或云服务。此结果覆盖签名、上传正文回放及下载，不证明全部 Multipart 自动模型绑定已等价。

将临时目录设为不存在的路径后，无效分页携带大正文仍直接返回相同的 400 参数错误，证明绑定没有提前读取无关正文。大 JSON 触发缓存失败时，修正后的 Rust 与源端都返回 400 和系统全局内部异常，不暴露目录信息。另一个合法分页携带大 GET 正文的异常场景仍不同：源端返回空体 500，Rust 遵守现有统一内部异常约定返回 400；保留此差异，不擅自修改源项目或改变既定异常规则。

最终容器内完整工作区 all-features/locked check、build、严格 Clippy（lib/bins、-D warnings）、格式检查，以及管理端 all-features Release 编译通过，两仓库 git diff --check 通过。四个临时宿主均经 SIGTERM 以 0 退出，参考容器、数据库、Redis、源码副本、配置、密钥、令牌、日志、上传文件及工具副本已清理，保留开发容器和包缓存。没有新增测试代码、验证入口或测试依赖，没有运行测试。JSON 格式错误完整提示、Query/正文跨提取器错误合并、缺少文件时的 Multipart 自动绑定顺序等仍未完成，整体翻版目标继续保持进行中。

## 普通上传文件绑定、媒体类型与文件头

本轮继续核对源 HEAD 23a7d0002938d0308c975dc48c04ac66aae0dda8，没有新增 NetEngine 修改。普通上传从两端处理器中的重复 Multipart 解析改为共享 uploaded_file::FormFile：认证后、Action 过滤器前读完整个表单，绑定首个名称匹配 file 的文件。后续同名文件不替换首项，文本字段不占文件位置；尾部截断时不能先进入签名或保存已经读取的首个文件。缺少文件属于模型错误，命名空文件在绑定后由业务返回请勿上传空文件。

绑定、签名和业务复制共用 ReplayBody，各自保持独立读取位置；客户端没有签名时由文件提取器创建同样的缓存。绑定只保存文件索引、名称和长度，不增加独立的整文件内存副本，业务调用才将选中的文件复制到原有 temps 目录。原文缓存和业务临时文件分别由各自作用域管理。普通文件端点增加媒体类型选择：非空且不匹配 multipart/form-data 的 Content-Type 在认证之前返回空体 415，没有 Content-Type 则继续绑定；其他 HTTP 方法仍返回 405 与 Allow: POST。

实际对比进一步发现 filename 空字符串属于普通表单字段，会参与签名而不是被忽略；NAME/FILENAME 参数大小写和 filename* 也不能依赖 Multer 的默认读取结果。提取既有公共下载文件名解析为 Content-Disposition 解析器，保持下载名称选择规则，补充表单 name 属性；文件绑定、签名、表单字段和参数快照共用它。支持首个同名参数、含分号的引号值、MIME Base64 名称、扩展文件名优先以及未知扩展编码回退普通名称。只有精确 form-data disposition 被源端当作表单处理，合法的其他类型跳过。文件服务移除额外的文件名 trim 判空，源端允许的纯空白名称可以原样保存，远程上传 DTO 的显式必填验证保留。

参考依据为 ASP.NET Core release/10.0 的 [FormFileModelBinder](https://github.com/dotnet/aspnetcore/blob/release/10.0/src/Mvc/Mvc.Core/src/ModelBinding/Binders/FormFileModelBinder.cs)、[FormFeature](https://github.com/dotnet/aspnetcore/blob/release/10.0/src/Http/Http/src/Features/FormFeature.cs)、[ContentDispositionHeaderValue](https://github.com/dotnet/aspnetcore/blob/release/10.0/src/Http/Headers/src/ContentDispositionHeaderValue.cs) 与实际 NetEngine 宿主请求。未将这些规则泛化到 UEditor 的业务上传处理。

隔离环境通过正式工具迁移业务与日志模型，参考源码只读复制到 latest SDK 容器后发布两端 Release；仅在副本规范 Logger.DataBase 目录大小写。Rust 管理端使用最新 Release，客户端使用最新 Debug，全部为 Production。两端各十二组无效请求的状态、正文及媒体类型与源端一致，包括空表单、文本 file、空文件名、非匹配字段、错误媒体类型、缺少 boundary、截断文件和损坏尾部。四组媒体类型与认证/缺少 Query 组合、十组空类型/空正文组合以及三组错误 HTTP 方法也一致。客户端无效请求前后文件记录数量不变，大正文错误签名没有写入文件记录。

八组文件选择和成功上传场景分别在两端完成对比：命名空文件、空名称占位后跟文件、文本同名字段后跟文件、首个空文件、多个非空文件、字段大小写、空白文件名和接近 11 MiB 的文件。有效请求都返回字符串 ID，下载正文逐字节等于预期首个文件；首个命名空文件不会被后续有效文件替换。最后的共享头解析修改后，重新执行了全部二十四组无效请求及八组两端上传场景。

文件头最初两项差异在修正后重做：编码文件名和大写参数名均先通过绑定再由错误签名返回 401。八组正确签名的特殊文件头上传在管理端成功，数据库确认两边保存的中文、大小写、扩展优先、未知编码回退、MIME 名称、含分号名称和重复参数首值均符合预期。另向四个宿主发送部分正文后断开连接，未产生 cancelled.bin 记录，Rust 两端无残留匿名文件句柄，temps 目录为空。

完整工作区 all-features/locked check、build、严格 Clippy（lib/bins、-D warnings）、格式检查、管理端 all-features Release 编译及两仓库 diff 检查通过。四个临时宿主均经 SIGTERM 以 0 退出，参考容器、隔离数据库、Redis、配置、密钥、令牌、日志、文件及工具副本已清理，保留开发容器和下载缓存。未安装本机组件，未新增或运行测试。

普通上传缺少文件与首文件选择的顺序差异已关闭；整体目标仍未完成。Query/正文跨提取器错误汇总、JSON 格式错误完整提示、所有 MIME 头语法和 FormOptions 数量/长度限制等尚需继续核对，不能将本轮通过结果当作全部请求绑定或整个框架的等价证明。

## JSON 正文参数身份、空值与空白诊断

继续对照 NetEngine HEAD 23a7d0002938d0308c975dc48c04ac66aae0dda8，源仓库仍仅有此前五项获准修改。新增共享 json_body 读取入口，Json 与 JsonWithBody 共用一次正文读取和模型绑定。控制器方法构建器通过 json_body("参数名") 保存正文参数身份，管理端二十二处、客户端十一处声明逐一对照源接口。相同 DTO 的创建与更新可以分别使用 createUser/updateUser 等名称，不把 HTTP 参数名放进模型，也不从 Rust 类型名猜测。

支持的 JSON 媒体类型下，空正文和根 null 返回非空正文错误及该接口参数必填错误；带前后空白的 null 使用同一规则。仅含 JSON 空白的正文返回 $ 语法错误及参数必填错误，保留从零开始的行号、字节位置。空对象继续执行已有模型字段验证，不被替换为缺少正文。名称与错误排序复用共享 ModelState 规则。初轮客户端对比发现接入排序函数时漏了响应字段前缀，修正并重新编译后才取得通过结果，初轮差异不计为成功。

媒体类型在读取正文之前解析，接受 application/json、text/json 和 application/*+json。显式依赖锁文件中已存在的 mime 0.3.18，仅在容器离线更新该依赖关系，没有升级其他包。UTF-8 BOM 从 JSON 解析视图移除，原始正文保留；只包含 BOM 仍是无 JSON token 的语法错误。JsonWithBody 通过现有 read_signature_text 复用源 GetRequestBody 的 BOM/gzip 文本规则，不重新序列化模型作为回调验签原文。此文本规则由源 HttpContextExtension 和共享签名实现核对；没有宣称本轮完成了真实商户回调的密码学联调。

隔离 PostgreSQL 通过正式 repository_tool 准备业务和日志模型，配合隔离 Redis、临时用户及新生成的签名密钥。NetEngine 只读源码复制到 latest SDK 容器后发布两端 Release，仅在副本规范 Logger.DataBase 目录大小写。Rust 管理端使用最新 Release、客户端使用最新 Debug，均设为 Production；签名行为只使用管理端 Release 作为证据。

三十三个 JSON 入口分别发送空正文、null、带换行的 null 和多行空白：管理端八十八组、客户端四十四组的状态、解析后正文、认证质询及 Allow 全部一致。覆盖用户、角色、文章、栏目、站点、链接、LLM、任务配置、登录、短信/微信入口、远程上传、Demo 以及微信通知。请求均停在绑定阶段，没有发送真实短信、调用支付或 LLM 提供商，也没有写入业务用例数据。

客户端另核对四种媒体类型与七种空值/BOM/空对象组合，二十八组全部一致。有效 Demo 的 Query 与 JSON 同时绑定仍返回 3，登录必填及重复大小写字段仍保持原有规则，微信通知的空对象与 BOM 空对象返回相同嵌套模型错误。管理端五组请求核对模型验证先于错误签名、未认证优先返回空体 401、有效正文在正确签名下到达业务错误及错误签名返回 401；另外正确签名的 BOM 正文和约 180 KiB 正文均与源端到达相同的无效 articleId 业务错误，证明绑定没有破坏原文回放。

本轮重新确认的待办包括：根数组、字符串字段传数字及截断 JSON 的完整类型名/字段路径/字节位置提示，Query 与正文跨提取器合并，非 UTF-8 charset 的完整转换，以及不支持媒体类型的 ProblemDetails 正文和追踪标识。Rust 当前在这些格式错误上仍保留部分 Axum 文案；不能把空值诊断修正视为整个 JSON formatter 已等价。

## JSON 编码与不支持媒体类型的响应

继续对照 NetEngine HEAD 23a7d0002938d0308c975dc48c04ac66aae0dda8，源目录仍只有已授权的上传两处与 UTC 查询三处改动。重新核实会话、日志、队列三个查询都在构造条件之前调用 ToUniversalTime，未修改日期 DTO 或同一时刻的含义。本轮不再修改源仓库。

共享 JSON 绑定按 Content-Type 选择 UTF-8 或 UTF-16 小端，覆盖源运行时支持的编码别名；UTF-16 大端、ASCII、Latin1、utf8 等名称返回不支持编码。规则依据 [TextInputFormatter](https://raw.githubusercontent.com/dotnet/aspnetcore/release/10.0/src/Mvc/Mvc.Core/src/Formatters/TextInputFormatter.cs)、[SystemTextJsonInputFormatter](https://raw.githubusercontent.com/dotnet/aspnetcore/release/10.0/src/Mvc/Mvc.Core/src/Formatters/SystemTextJsonInputFormatter.cs) 和 [EncodingData](https://raw.githubusercontent.com/dotnet/runtime/release/10.0/src/libraries/System.Private.CoreLib/src/System/Text/EncodingData.cs)，并在源宿主实测。严格 UTF-16 解码遇到奇数字节或非法代理项时进入共享内部异常；转码与 BOM 去除只作用于模型解析视图，不改写签名和 JsonWithBody 的原始字节。

方法绑定器在包装回放流之前保存正文存在标记。媒体类型不支持时先返回 415；媒体匹配但传输层无正文时先报必填，跳过编码选择。空分块流仍经过编码与 JSON 解析，因此未知编码返回 415，而受支持编码返回没有 JSON token 的错误。charset 按源参数读取规则选择第一个值，保留引号内空白，遇到无效参数停止；空引号与缺失参数不同。媒体类型匹配与参数解析分开，避免参数不规范时误报整个媒体类型不支持。

新增共享 ProblemDetails 响应，415 使用 application/problem+json; charset=utf-8，包含 RFC 9110 类型地址、标题、状态与 traceId。普通 55 字符有效 W3C traceparent 继承 trace，当前响应生成新 span；全零、非法大小写或版本不继承。标志位按源 Activity 保留低七位，依据 [Activity](https://raw.githubusercontent.com/dotnet/runtime/release/10.0/src/libraries/System.Diagnostics.DiagnosticSource/src/System/Diagnostics/Activity.cs) 并实测。该实现只用于此类错误响应，未宣称完成日志与对外调用的分布式追踪传播。

验证使用隔离 PostgreSQL、Redis 和 latest .NET SDK 容器；业务与日志模型由正式 repository_tool 创建。源码只读挂载后发布两端 Release；Rust 客户端使用 Debug，管理端编码矩阵最终在 Release 重做。宿主均为 Production，不连接外部支付或云服务，不在 Windows 安装组件，不添加测试文件、依赖或验证入口。

最终通过 312 组请求对比：两端各 117 组编码、BOM、空正文、空分块流、媒体类型和非法 UTF-16，共 234 组；两端各 18 组媒体参数及各 10 组追踪标识，共 56 组；管理端 Release 八组签名与认证/绑定顺序；客户端六组成功 Add2、六组支付回调必填校验，以及最后两组空参数名称。415 比较完整固定正文与 Content-Type，动态 traceId 验证格式、父 trace 继承、flags 和新 span，不比较随机值。首轮 234 组中空引号 charset 导致的两组差异已修正，未把首轮失败算作通过。

签名核对使用超过 30 KiB、包含中文和补充平面字符的 UTF-16 小端正文。带 BOM 时签名文本按 BOM 解码；无 BOM 时依源 GetRequestBody 的默认 UTF-8 文本规则签名。两种正文的正确签名均到达无效 articleId 的业务错误，错误签名均返回非法 Token。匿名请求先返回空体 401，已认证但不支持编码返回 415，模型无效仍先于签名失败。没有用客户端 Debug 运行结果证明生产签名。

本轮仍未关闭根数组、字段类型不符及截断 JSON 的源异常文案/路径/位置，Query 与正文错误跨提取器合并，所有畸形媒体头与内容协商，以及旧 Request-Id 和扩展版本的层级追踪格式。已观察到有效的扩展版本 traceparent 在源端可能产生层级 ID，当前实现只覆盖普通 W3C ID；HTTP/2 正文存在标记尚未做单独运行验证。这些差异仍属于整体对齐待办。

最终容器检查通过 fmt、全工作区 all-features locked check、严格 Clippy、全工作区构建及管理端 Release 构建，两边 git diff --check 通过。四个临时宿主均正常停止，三个隔离服务容器和临时配置/密钥/日志已删除，只保留原有 dev 容器与命名缓存卷。

## JSON 类型错误与首次失败顺序

本轮继续核对同一源 HEAD 23a7d0002938d0308c975dc48c04ac66aae0dda8。源仓库仍只有已授权的上传与 UTC 查询修改，没有新增源端变更。前一轮形成了编码和媒体类型规则，本轮继续处理 JSON 根类型、字符串、集合和嵌套对象诊断。

公共输入层新增 read_json_model，用模型元数据驱动同一次 JSON 值读取，并区分形态、语法、转换器与模型验证错误。发生字符串或对象/集合形态错误时立即停止，保留原始属性名称、数组索引和位置；无需从 Serde 错误文案猜测类型，也不先反序列化一个业务模型再重复绑定。既有模型转换器仍负责实际构造模型，内部 ObjectInput 与克隆流程不引入 HTTP 模型验证。

这修复了行为差异而不只是替换文案：源端对较早的 userName 数字立即拒绝，即使后面重复写了正确字符串，或后续正文已经截断。原 Rust 先收集完整 JSON，再按末值去重，可能掩盖较早错误或先报告后面的语法问题。当前在字符串、字符串数组、对象和集合这些已声明形态上与源端保持首次失败顺序；正常重复字段的末值、空白可空字符串和模型验证规则继续保留。

27 个 JSON 模型显式声明源协议类型全名，覆盖不同命名的 Rust Request、源站点 SiteDto 与支付通知内部 Resource 类型。宏继续生成元数据，集合默认 List，源 string[] 在角色 ID 字段声明 json(array)。管理端与客户端共享 HTTP 错误装配，类型转换消息同时包含源类型名、原字段路径、零起始行号和 UTF-8 字节位置。

Serde 在 visit_map/visit_seq 拒绝类型后仍会读取空白和结束符，首次 38 组对比中九组容器错误因此偏后一字节。读取层现按实际失败的开始 token 校正位置；只在这种错误发生时定位，不重读模型。补充覆盖多行空白、嵌套开始符、前面字符串含开始符、转义属性名、大小写字段、中文及补充平面字符，最终均一致。

隔离环境通过正式 repository_tool 创建业务和日志模型，使用临时 PostgreSQL、Redis 与 latest .NET SDK 容器。NetEngine 两端发布 Release，Rust 客户端使用 Debug，最终管理端使用新 Release；均配置 Production。只对正式接口发起内联请求，不添加测试文件、辅助入口或依赖，不连接外部支付/云服务，不在 Windows 安装组件。

最终通过 585 组请求对比：33 个 JSON 入口各八种根类型/根 null 共 264 组；字符串、重复字段及嵌套类型 38 组；转义名称、复杂容器与位置 13 组；前轮编码和媒体类型回归 234 组；UTF-8/UTF-16、BOM 与类型错误位置 24 组；合法重复值、最后空值及有效嵌套模型四组；管理端 Release 签名和认证/绑定顺序八组。比较状态、Content-Type 和完整错误 JSON；415 的随机 traceId 沿用格式检查。首轮九组位置差异未计为通过；另一个拟作成功输入的文件 DTO 因缺少 Path、Sign、LengthText 在两端同样验证失败，补齐后重做成功组。

最终容器内 fmt、全工作区 all-features locked check、严格 Clippy、全工作区构建和管理端 Release 构建全部通过，两边 diff 检查通过。四个临时宿主正常退出，三个隔离服务容器及临时配置、密钥、日志已删除，保留既有 dev 与缓存卷。

尚未对齐一般截断 JSON、非法字面量及标点的完整诊断，数值、布尔、日期等转换器的错误类型、首次失败和位置，以及 Query 与正文跨提取器汇总。语法错误与转换器错误已保留独立类别，但 HTTP 层仍有通用 Serde 文案，不能将本轮形态诊断视为完整 System.Text.Json 等价实现。前轮记录的内容协商、扩展层级追踪等待办继续保留，整体目标仍未完成。

## 非可空布尔与任意精度数字读取

源 HEAD 仍为 23a7d0002938d0308c975dc48c04ac66aae0dda8，源目录只有之前授权的五处修改。本轮核对十个 JSON 模型的十二个非可空 bool 字段，分布在十三个正式端点；当前可空 bool 属于 Query 模型，不将其空白规则套入非可空 JSON 字段。

JsonModel 元数据增加 Boolean 分类，读取时仅接受 true/false。null、字符串、数字、数组和对象保留 System.Boolean 类型诊断、原字段名和字节位置。较早的错误布尔值不能被后面的合法重复字段掩盖，也不会让后续字符串类型错误先返回。缺失字段沿用 Serde 声明的默认值，合法重复布尔值保留末值。可空与显式自定义转换继续由其转换器处理，不在通用 Query 或响应层增加限制。

首轮对比发现小数 1.5 的位置错误地落在根对象开始处。检查实际依赖树与容器内 serde_json 源码确认：source_generator_runtime 启用 arbitrary_precision，经 Cargo 特性合并后，小数、指数和超出机器整数范围的值通过内部 MapAccess 数字协议提供，旧读取器却将其当成对象。common 现显式声明该特性，并在 HTTP JSON 读取器和内部值读取器中还原 Number，保留精度及原数字表示，不降为 f64。

新增的属性读取适配器利用真实 JSON MapKey 的 newtype 边界区分普通属性和 NumberFieldDeserializer 的内部标记，不通过 Rust 私有类型名判断，也不只看属性文本。普通对象使用相同标记名称仍作为对象读取；类型错误中的真实对象位置与数字结束位置分别保留。此修复仅保证该读取阶段，后续 serde_json::Value 转换的保留名称冲突另行记录。

使用独立 PostgreSQL、Redis、latest .NET SDK 容器及临时配置，正式 repository_tool 创建业务和日志模型。NetEngine 两端为 Release，Rust 管理端最终为新 Release、客户端为 Debug，运行环境均为 Production。没有新增测试代码、测试依赖、辅助入口或本机组件；没有访问真实支付、云或 LLM 服务。

最终通过 349 组配对请求：十三个端点的十六处布尔字段使用十五种非法值/重复值/截断后文/多行正文，共 240 组；任意精度数字与真实对象的类型及位置 27 组；合法模型、未知属性和重复值三组；上一轮类型诊断回归 38 组；五轮文章更新与回读共十组；UTF-8/UTF-16/BOM 布尔诊断 24 组；首错顺序四组；认证和 Release 签名顺序两组；包含小数和大指数的 LLM 扩展正文绑定一组。最后一组只到无效模型 ID 的业务错误，没有调用 LLM 提供商。

合法布尔更新分别使用两条隔离文章，核对 true/false 组合、合法重复字段末值和省略时的默认 false。每次更新后通过正式查询接口回读旗标，五轮结果均一致，最后还直接读取 PostgreSQL 确认两条记录均为 false/false。没有用 Debug 结果证明管理端生产签名。初始小数位置失败已修正并重做，不算作通过。

本轮确认的未关闭差异包括：一般畸形对象可能先触发源 JSON 语法诊断，而当前形态读取仍先返回类型错误；例如 isRecommend 的值以 {bad 开始。根对象截断和非法字面量的完整消息仍未对齐。这些探索请求不计入通过数。另对 /Operations/TestLlmApp 提交 extraBody.x 为包含 $serde_json::private::Number 普通属性的对象时，源端保留对象并进入无效模型 ID 的业务错误，Rust 后续 serde_json::Value 转换仍误认标记并报 invalid number。底层读取已区分，但动态字段的后续转换链路仍需继续修复。整体目标保持未完成。

最终容器内 fmt、全工作区 all-features locked check、严格 Clippy、全工作区构建和管理端 Release 构建通过，两边 diff 检查通过。四个临时宿主正常退出，三个隔离服务容器和临时配置、密钥、日志已移除，仅保留原 dev 及缓存卷。

最新代码通过容器内完整工作区 all-features/locked check、build、严格 Clippy（lib/bins、-D warnings）、格式检查，以及管理端 all-features Release 编译，两仓库 diff 检查通过。四个临时宿主均经 SIGTERM 以 0 退出，退出前 Rust 两端没有遗留匿名回放文件句柄。参考容器、隔离数据库、Redis、配置、密钥、令牌、日志及工具副本均已清理，保留开发容器和包缓存。未安装本机组件，未新增或运行测试；整体翻版目标继续保持进行中。

## 动态 JSON 的保留属性名与大整数

源 HEAD 仍为 23a7d0002938d0308c975dc48c04ac66aae0dda8，NetEngine 本轮没有新增修改。修复上一轮确认的动态字段问题：普通对象的 `$serde_json::private::Number` 或 `$serde_json::private::RawValue` 属性在后续 `serde_json::Value` 转换中被误读为内部协议。common 增加动态值读取与字符串解析入口，先使用已有的属性协议识别，再直接构造 JSON 值树；对象和数组递归保留原始键名，不再次通过 Value 反序列化解释这些键。

LLM 调用请求的 extraBody 与 parameters 复用该入口，extraBody 直接取出对象映射，避免嵌套 Value 的二次转换。可空请求标量也复用安全读取，普通对象不能借内部标记冒充数值或 null。配置字符串的对象校验和 LLM 运行时加载使用同一解析入口，并在根值之后检查多余内容；原有空白、非对象、无效 JSON 分类及运行时空键/null 过滤保持原有职责。

首轮实际请求还发现超过 64 位的整数在 Value 转换中进入 visit_i128/visit_u128，原读取器缺少这两个入口。补齐后重新构建并重做全部动态请求，覆盖有符号/无符号 128 位边界及超过该范围的任意精度整数、高精度小数和正负大指数。首次失败没有计为通过。

隔离 PostgreSQL、Redis 与 latest .NET SDK 容器继续使用正式迁移和正式接口。最终完成 276 组配对检查：动态对象/数组/普通保留属性名/数值 76 组、字符串模板字典 20 组、配置字符串校验 76 组、无效配置形态/语法/尾随内容 12 组、既有数字和合法模型回归 30 组、类型诊断回归 38 组、UTF-8/UTF-16/BOM 与生产签名 16 组、认证和签名拒绝两组，以及模型创建、四份配置创建和列表回读六组。前 270 组比较状态、Content-Type 和完整返回；六组持久化操作分别检查成功状态、字符串 ID 和各自配置原文，生成 ID 与时间不作跨宿主相等断言。

四份配置均经真实业务写入并从正式列表接口原样读回。LLM 请求使用无效模型 ID，仅验证绑定后进入相同业务错误；未调用外部模型服务。运行时加载修改经过编译及共享解析路径的校验覆盖，本轮没有实际执行一次完整 LLM 调用，不能据此声称提供商协议全链路已验证。

另有三组可空整数对象输入，两端均在绑定阶段返回 400，Rust 已拒绝标记对象冒充整数或 null，但完整诊断仍不同，未计入上述一致数量。此前的一般畸形 JSON、可空标量和日期转换诊断、首次失败顺序、内容协商与扩展追踪差异仍待推进；此次修复不代表所有泛型 Value 消费点或整个项目已经完全等价。

最终容器内格式检查、工作区 all-features/locked check、严格 Clippy、完整工作区 build 和管理端 Release build 均通过。NetEngine 两端为 Release，Rust 管理端为新 Release、客户端为 Debug，均使用 Production 配置。四个临时宿主正常退出，三个隔离服务容器及配置、密钥、日志、工具副本已清理；只保留原开发容器与缓存。没有新增测试文件、辅助入口、依赖或本机组件，整体目标保持进行中。

## 提供商流式错误与会话删除人字段

源 HEAD 仍为 23a7d0002938d0308c975dc48c04ac66aae0dda8，源目录没有新增修改。继续追踪动态值时发现 OpenAI-Compatible 的 error 字段也通过 serde_json::Value 读取；普通保留属性名会把合法错误对象误报为无效 JSON。LLM 组件现从已有的响应值树读取错误，不增加 common 依赖，不改变提供商 DTO 的字段匹配规则。message/code/type 保持精确大小写、同名末值、只接受字符串及 code/type 去重顺序，其他属性和非字符串值不参与提示。

动态模型工厂将模型标识作为提供商诊断名称，流式默认错误与无效 JSON 提示带上该名称；根 null 单独报告 empty JSON payload，与源端区分。直接创建兼容客户端仍可使用默认名称或显式指定名称。此处保留 Rust 应用层和错误类型的包装信息，不声称 Development 诊断页面与 .NET 完全相同。

使用 latest PostgreSQL、Redis、Nginx 和 .NET SDK 容器，以 Nginx 静态 SSE 文件提供隔离响应，通过已有 `/api/Demo/TestLLMStream` 调用正式应用服务和提供商客户端。没有新增测试代码、测试入口或辅助服务器程序。参考宿主第一次 Development 启动被复制来的环境配置覆盖而失败，移除参考容器内的该配置副本后才开始有效对照；未修改 NetEngine 源配置。最终两端均运行 Release 二进制。

本轮实际调用又暴露持久化缺陷：NetEngine 的 LlmConversation 继承 CD_User，包含 DeleteUserId，而 Rust 实体和迁移缺失该列，导致 .NET 正常流结束后保存会话失败。已在业务实体补充可空删除人 ID 与 User 关系，创建记录显式置空；通过正式工具生成并登记 `20261107135818_align_llm_conversation_audit`，新增列、索引与限制删除的外键。历史迁移和快照保持原样。新迁移自动生成的 Down 原有 121 项操作，审查后收窄为本次外键、索引、列的三项逆操作，避免无关外键重建。

在隔离库验证了已有数据升级、重复执行、回退到前一版本、再次升级，以及空库完整十项业务迁移；检查列可空性/类型、索引、外键与历史记录。回退和再次升级保留已有会话。正式 migration-pending 确认当前模型与最后快照一致，migration-script 可离线导出本次幂等升级脚本。没有对用户已有数据库执行迁移。

最终 Release 完成 46 组配对调用：Development 下 37 组提供商错误信息，以及 null error、同名末值为 null、空对象、未完成文本流、正常完成五组；Production 下三组错误与一组正常完成。37 组比较提取后的提供商错误内容和 500 状态，包含保留属性名、大小写、重复键、非字符串、空白、数值/对象/数组、根 null、畸形正文、默认模型名，以及先收到 finish_reason 后再发生错误。Development 的 Rust JSON 包装与 .NET 纯文本异常页面不同，不能将这 37 组表述为完整 HTTP 返回相等；空异常信息的 .NET 页面没有冒号，首次提取方式已修正后重新完成全部比较。Production 三组错误的状态、Content-Type 和完整 JSON 均一致，保持 400 与系统全局内部异常。

数据库核验不只看返回文本：最终错误组和未完成组执行前后会话数保持为 2；正常完成后先调用 Rust、再调用 NetEngine，分别变为 3、4；Production 错误组仍为 4，正常完成两端后变为 6，所有新增记录的内容均为 fixture-ok、删除人为空。此前缺列时源端失败的保存不计为成功；补齐迁移后单独确认源端恢复写入。

最终容器内格式、全工作区 all-features/locked check、严格 Clippy、完整工作区 build、管理端与客户端 Release build 均通过，两仓库 diff 检查通过。临时宿主正常退出，四个隔离服务容器以及两个隔离数据库、静态响应、配置、密钥、日志和工具副本均已移除，保留原 dev 与镜像/下载缓存。未在本机安装组件。

本轮没有证明所有提供商协议、HTTP 错误格式和业务表结构完全一致。流式错误已实际核验，但其他协议与非流式响应仍需继续对照；本次发现的继承列遗漏也要求后续从源模型系统核对其余业务表，不能仅以编译或已有模型快照一致判定数据库已全面对齐。整体目标继续保持进行中。

## 三个写模型的结构审计与写入约束

本轮从当前源 HEAD 23a7d0002938d0308c975dc48c04ac66aae0dda8 的正式 Repository.Tool 设计时工厂导出 Business、Log、EventBus 模型。只在 latest .NET SDK 容器安装 dotnet-ef（解析得到 10.0.14），使用 Debug 编译与 dbcontext script；工具副本配置为不可用的回环数据库地址，导出过程不连接数据库。Rust 使用正式 model-snapshot 和迁移工具。两边 SQL 分别在 latest PostgreSQL 的隔离空库执行，再读取 PostgreSQL 系统目录比较实际建模结果，避免只依赖源码字段名或手写预期。

三个源写模型合计 30 张非分区子表、275 个普通字段、30 个主键、54 个外键和 180 个索引（含主键索引）。首轮 Rust 没有再缺列，但 Order.Price、Order.PayPrice、Product.Price 为 numeric(38,2)，源为 numeric(10,2)；QueueTask 缺 ParentTaskId 自关联外键及索引；QueueTask.Status 与 StoredFile.Sort 额外设置了数据库默认值 0。源初始化值属于对象构造行为，未声明数据库默认值。

已按所属组件修正三个金额字段与队列自关联，移除工具中额外注入的两个默认值。检查正式入队和文件上传流程，均已显式写入 Pending/0，不依赖数据库默认值。新增并登记 `20261107141007_align_source_column_constraints`，迁移包含三处类型调整、两处默认值移除、父任务索引和外键；Up/Down 审查后均收窄为七项相关操作，不保留生成器对无关外键和索引的重建。没有修改旧迁移或源项目。

迁移在隔离库经过两种失败验证：旧数据存在 100000000.00 金额时升级因 numeric 溢出失败，数据未被裁剪；旧数据存在不存在的父任务时升级因外键失败。两次失败后，三个金额列仍为原精度、两个默认值仍在、新迁移历史仍为零，证明整项事务回滚。仅修正隔离样本后升级成功，样本金额保持 1.23；随后回退、再次升级及重复执行成功。另一个空库通过全部 11 项业务、两项日志和两项事件迁移，migration-pending 确认当前业务模型与最后快照一致。

最终重新比较正式迁移建库的结构：30 张表和 275 个字段在名称、类型、可空性、数据库默认值、自增标记上无差异；30 个主键与 54 个外键按定义语义比较无差异，所有源索引均存在。两端当前日志子分区名称均为 Log_p2026110100，边界均为 319746893414400000 至 325182711398400000。迁移历史表按模型独立存在，不列入业务表统计；xmin 是 PostgreSQL 系统列，也不作为普通字段统计。

执行了 38 组配对数据库操作：三个金额字段各十组零值、舍入、正负边界及溢出输入；可空支付金额一组；有效父任务、缺失父任务、删除被引用父任务、软删除父任务和缺失 Status 五组；文件缺少 Sort 与显式 Sort 两组。比较成功后的实际值或 PostgreSQL SQLSTATE，结果全部一致。所有操作使用隔离数据库和事务，不新增测试文件、测试依赖或辅助程序，也没有迁移用户已有数据库。

审计仍有未关闭差异：Rust 额外拥有 31 个非唯一筛选索引；源的 30 张表和 275 个字段有模型文档注释，Rust 当前未输出；主外键的生成名称仍采用各自框架约定。本轮约束一致指实际定义，不包含这些名称和注释。索引性能、迁移元数据与模型文档能力仍需继续核对，不能把列/约束结果扩大为全部持久化功能或整个项目已经一致。

最终代码通过容器内格式检查、全工作区 all-features/locked check、严格 Clippy、完整工作区构建，以及 admin_webapi、client_webapi、task_service 三个 Release 宿主构建。两仓库 diff 检查通过，未在 Windows 安装工具或组件，整体目标保持进行中。

## 索引声明对齐与迁移生成范围

源 HEAD 仍为 23a7d0002938d0308c975dc48c04ac66aae0dda8，当前源工作区五处既有修改均不涉及模型或索引。对照上一轮从三个正式 EF 模型导出并实际建库得到的索引定义，Rust 当前模型移除 31 个源端未声明的非唯一筛选索引：业务实体 24 个、队列组件五个、日志组件两个。保留已有普通关系/审计索引、实体明确声明的索引，以及 TaskSetting 有效定义唯一索引。任务配置唯一规则再次与源 ConfigureQueueTasks 核对。

生成业务迁移时初始 Up 含 137 项操作，其中 108 项只是无关外键的删除和重建。repository_core 的差异生成器此前把任意索引变动都当作关系重建条件；现明确识别 CREATE INDEX 普通索引，跳过这种无关重建。唯一索引、无法明确识别的索引 SQL 和其他可能影响关系的结构变化保留原有保守处理，不通过忽略约束来让迁移成功。

修正后重新生成实际业务迁移，Up/Down 均直接得到 29 项索引操作，内容与人工审查结果相同。替换并移除了本轮尚未应用的初稿；最终登记 `20261107142136_align_source_business_indexes`。日志迁移由日志组件登记 `20261107141851_align_source_log_indexes`，Up/Down 各两项，按父表索引处理当前及后续分区。没有改动历史已应用迁移、模型基线或 NetEngine 源文件。

在 latest PostgreSQL 的独立升级库和空库验证正式迁移。升级库预先执行旧迁移并写入父子队列、任务定义及日志分区记录，随后完成升级、回退、再次升级和重复执行。54 个外键和应保留的 180 个索引的 PostgreSQL 对象 OID 均保持不变，三个样本表的数据摘要也保持不变，证明没有重建无关关系或重写业务数据。空库完成 12 项业务、三项日志和两项事件迁移，三个模型的 migration-pending 均无待生成变化。

升级库和空库最终均有 180 个模型索引（包括主键索引，排除迁移历史和分区子索引），按表、唯一性、索引方法、列/表达式和谓词比较，与源定义集合完全一致。索引名称按各框架约定另行记录，不借数量相同推断定义相同。日志父表与当前子分区各保留三个索引，全库没有无效索引。

另通过六组隔离事务验证任务定义唯一性：重复活跃队列定义、重复无参定时定义，以及无参定义与参数模板同名均返回唯一冲突；软删除定义不占用有效名称，带参实例允许同名，参数模板与带参实例可共存。这些验证只使用正式数据库和工具，不添加测试代码、测试入口、依赖或辅助程序。

最终代码通过容器内格式检查、工作区 all-features/locked check、严格 Clippy、完整工作区 build，以及三个宿主 Release build；两仓库 diff 检查通过。索引定义差异已关闭，但模型文档注释和约束命名等此前记录的差异仍待推进，不代表整个持久化框架或项目已经完全对齐。未在本机安装组件。

## 实体文档与数据库注释迁移

源 HEAD 仍为 23a7d0002938d0308c975dc48c04ac66aae0dda8。本轮核对 Repository.Core 的 ModelDocumentation：源设计时模型从实体及声明属性的程序集 XML 文档读取说明，缺少文档回退成员名称。Rust 通过公共 `model_documentation` 属性把现有实体和字段文档转换为 SeaORM 注释，复用 TableModel 的注释快照及差异生成，不在数据库工具维护另一份业务说明表。属性由过程宏实现，repository_core 和 source_generator_runtime 导出；28 个 SeaORM 实体显式启用，事件组件的两张 SQL 模型在自身声明中补充注释。

显式 SeaORM comment 优先于文档；无文档回退映射名称，多行文档保留换行。系统列 xmin 与非持久化字段不生成注释。保留 Rust 现有中文注释，未批量改写成源 XML 文本；因此这里对齐的是自动文档建模和迁移能力，并非注释逐字相同。Rust 在 Debug/Release 均生成相同模型，避免离线工具因构建模式丢失注释。

三个模型分别生成并登记 `20261107143359_align_model_documentation`：业务 Up/Down 各 281 条、日志各九条、事件各 15 条。审查所有操作均为 COMMENT ON TABLE/COLUMN，没有结构变更或无关关系重建；Down 将此前为空的注释恢复为 NULL。历史已登记快照保持不变。当前模型共 30 张表、275 个普通字段，全部拥有注释，三个 migration-pending 均通过。

使用 latest PostgreSQL 临时容器验证正式迁移工具。升级库从三个模型各自上一版本开始，写入父子队列、日志分区、事件状态及消息记录；完成 Up、Down、再次 Up 和重复执行。30 张父表及其字段的 305 处实际数据库注释与当前快照逐项一致，Down 后 305 处均为空。214 个表/索引对象（包括当前日志分区及子索引）和 54 个外键 OID 全程不变，四个有数据表的内容摘要不变。独立空库全量迁移并重复执行，最终同样得到 305 处匹配注释。没有对用户已有数据库执行迁移。

容器内格式检查、工作区 all-features/locked check、严格 Clippy、完整工作区 build，以及三个宿主 Release build 均通过。未添加或运行测试代码，未在 Windows 安装组件。模型文档缺失已补齐；约束命名、此前记录的 HTTP/协议细节等仍需继续核对，整体对齐目标尚未完成。

## 主外键和索引命名对齐

再次确认源 HEAD 为 23a7d0002938d0308c975dc48c04ac66aae0dda8，源工作区仍只有既有五处授权修正，没有模型变动。沿用该源版本正式模型导出后实际建库得到的系统目录，定位剩余名称差异：30 个主键、54 个外键，以及 TaskSetting 的有效定义唯一索引。定义本身已一致，本轮补齐名称与公共迁移生成能力。

TableModel 新增可选的主键名称元数据，旧快照缺少该字段时仍解释为原有 table_pkey；当前默认生成 PK_表名，显式主键名称得到保留。SeaORM 默认外键名转换为 FK_表名_引用表名_列名，显式名称不覆盖。任务配置索引名称改为 IX_TaskSetting_Category_Name，过滤条件和唯一规则未改。生成初始 SQL、改变主键定义及名称、读取旧快照均使用同一元数据，不在业务迁移中硬编码一份名称映射。

公共差异生成器识别纯名称变化：主键列不变时直接改名；外键和索引要求除名称外的声明完全相同，并可唯一配对。未知 SQL、歧义及结构变化保留原有保守处理，唯一索引纯改名不再触发全部外键重建。首份未应用草稿暴露多行索引名称后的换行未被识别，修正后移除该草稿的四个文件及登记，再重新生成。最终三个模型分别登记 `20261107144333_align_constraint_names`，业务 Up/Down 各 82 条、日志各一条、事件各两条，全部为 RENAME 操作，没有 DROP/CREATE。

使用 latest PostgreSQL 的三个独立库验证正式工具。升级库从上一版本 20261107143359 开始，写入父子队列、任务配置、日志分区、事件状态和消息；完成升级、回退、再次升级与重复执行。214 个表/索引对象的 OID、物理文件标识，以及含日志子分区主键在内的 85 个主外键对象 OID 和定义均未变化，五个有数据表的内容摘要保持一致。Down 后逐项确认 84 个父表约束和 180 个父表索引恢复原名称与定义。

独立空库全量执行三条迁移链并重复执行；第三个空库直接执行当前 model-snapshot --sql 导出的三个模型。升级、完整迁移与直接建模三条路径均得到 30 个主键、54 个外键、180 个索引；按表、名称、种类、唯一性和数据库实际定义逐项比较，全部与源系统目录相同。比较排除迁移历史表与自动分区子对象，子对象在升级过程中另外验证没有重建。没有迁移用户已有数据库。

三个 migration-pending 均无待生成变更，容器内格式检查、工作区 all-features/locked check、严格 Clippy、完整工作区 build 及三个宿主 Release build 均通过。两仓库 diff 检查通过，未添加或运行测试代码，未在本机安装组件。此前发现的父表约束/索引命名差异已关闭；文档说明仍保留 Rust 原有文字，HTTP、提供商协议等未完成项继续对照，整体目标尚未完成。

## 三种 LLM 协议的非流式响应编码

源 HEAD 保持 23a7d0002938d0308c975dc48c04ac66aae0dda8。重新读取 Chat Completions、Responses、Anthropic 的正式客户端：非流式响应均通过 ReadFromJsonAsync 读取。依据 [.NET HTTP JSON 读取实现](https://github.com/dotnet/runtime/blob/v10.0.0/src/libraries/System.Net.Http.Json/src/System/Net/Http/Json/HttpContentJsonExtensions.cs)、[字符集解析](https://github.com/dotnet/runtime/blob/v10.0.0/src/libraries/System.Net.Http.Json/src/System/Net/Http/Json/JsonHelpers.cs) 和[内置编码名称](https://github.com/dotnet/runtime/blob/v10.0.0/src/libraries/System.Private.CoreLib/src/System/Text/EncodingData.cs)，该路径使用 Content-Type 的字符集转码，再交给流式 JSON 读取器；没有字符集时按 UTF-8 读取。

通过两边原有 /api/Demo/TestLLM 和临时 nginx 静态响应确认实际差异：源端接受 UTF-8 BOM、UTF-16/32 大小端及 Latin-1，Rust 原先直接解析字节而失败；源端拒绝未知字符集和默认禁用的 UTF-7，Rust 原先忽略响应字符集、接受其中的 UTF-8 内容并保存会话。JSON 根 null 也只有源端给出协议对应的空响应诊断。全部调用指向隔离网络内的静态文件，没有调用真实模型服务。

新增 llm::http_response 公共读取器，三种协议的非流式入口统一使用。字符集从媒体类型参数读取，支持默认 UTF-8、UTF-16/32 大小端、ASCII、Latin-1 与内置别名；未知名称报错，UTF-7 保持源默认禁用行为。按声明转码后只去掉流起始的一个 BOM，不根据未声明的 BOM 猜测 UTF-16，不接受空白后的或重复 BOM。UTF-8 保留原字节交给 JSON 解析，其他 Unicode 编码与 ASCII 使用源对应的替换行为。根 null 通过独立 InvalidResponse 错误报告协议名称；空字节流仍属于 JSON 解析失败。错误状态检查、请求超时与取消路径沿用原实现。

验证使用 Release 宿主和 latest SDK 容器构建的当前 .NET 客户端，数据库、Redis、上游和配置均隔离。Development 下完成 54 组配对调用：三种协议各 13 组基础编码/空响应，加 15 组大小端 BOM、Unicode 非法代理项、ASCII 高位字节、Latin-1 控制字符、非法 UTF-8、别名、引号空格、重复 charset、BOM 位置等边界。36 组成功响应逐字一致，18 组失败均为 500；每组同时读取会话数量，成功时各新增一条，失败时不新增。另重复核对三种协议根 null、三种协议未知字符集及 UTF-7 共七组的底层诊断消息一致；Development 外层错误表示仍分别为 Rust JSON 与 .NET 异常页面，不据此宣称完整错误 HTTP 响应相同。

Production 下完成十组配对调用：三种协议各一组 BOM 成功、根 null 失败和未知字符集失败，加 UTF-7 失败。状态、Content-Type 和完整响应正文一致，错误保持 400 与系统全局内部异常，不暴露诊断。正式矩阵开始前隔离库已有探索调用产生的十条会话，Development 后为 82 条，Production 后为 88 条；失败组和七组诊断复核均不增加记录。

最终容器内格式检查、全工作区 all-features/locked check、严格 Clippy、完整工作区 build、三个宿主 Release build 与两仓库 diff 检查通过。未添加测试代码、验证端点或测试依赖，未在本机安装组件。该轮关闭非流式编码与根 null 诊断差异；DTO 重复属性的逐项转换、一般 JSON 错误诊断、复杂 Content-Type 语法以及其他协议行为仍须继续核对，整体目标保持进行中。

## LLM 响应字段的逐项转换与可空引用集合

源 HEAD 仍为 23a7d0002938d0308c975dc48c04ac66aae0dda8。使用当前源客户端和两边现有 Demo 接口确认：三种协议收到先为数字、后为合法字符串的 model 重复字段时，源端立即拒绝第一项类型错误，Rust 原先提前合并字段，错误地接受并保存会话。有效重复字符串应继续取末值，不能简单改成拒绝所有重复键。另确认 Responses 存在有效 output_text 时，源端可以不访问含 null 的备用 output 数组，而 Rust 原先在读取数组时提前拒绝。

新增公共 ProtocolModel 派生宏，由 source_generator_core 实现、source_generator_runtime 导出，三种协议共 19 个私有响应 DTO 显式采用。宏逐项读取并转换属性，合法赋值才覆盖前值；保留 ASCII 大小写不敏感、字段 rename、裸 default、Option 缺省及未知字段忽略规则。重复映射名在编译时拒绝；泛型模型、模型级 Serde 属性及其他字段选项明确报错。未将 HTTP 请求模型的空白转换或业务验证规则用于提供商协议。

wire 的输入对象保留字段原名和出现顺序，不再预先去重。动态错误对象继续按 JsonElement 方式保留属性，错误提取仍按精确大小写取同名末值，不把动态 message 的早期非字符串值当作强类型 DTO 错误。引用类型数组使用可空元素表示；未使用或被后续同名数组覆盖的 null 元素允许存在，实际访问 null 元素时才失败。非法非空类型仍须在读入时拒绝，即使其外层对象或数组稍后被覆盖。

同时将 Responses、Anthropic 流式根 null 与 JSON 转换失败分别映射到源端对应的空 JSON/非法 JSON 诊断。这里保留了统一错误通道，没有增加忽略坏片段或失败后保存会话的路径。

使用 latest SDK 构建的 .NET Release 客户端与 Rust Release 客户端，连接隔离 PostgreSQL、Redis 和 nginx 静态响应。Development 共完成 96 组配对调用：68 组非流式、28 组流式，覆盖先坏后好、合法末值、大小写、可空字段、默认值、计数溢出与浮点、嵌套对象/数组覆盖、备用输出、未知字段、动态错误和 serde_json 保留属性名。38 组成功文本逐字一致，58 组错误均返回 500；18 组流式诊断还逐项比较底层消息。源端非流式 JsonException 的完整路径/字节位置与 Rust 的错误格式尚未对齐，错误组的状态一致不能作为完整 Development HTTP 等价证明。

Production 另完成 14 组配对调用，包含三种协议的非流式/流式成功与重复字段错误、Responses 备用数组及动态错误。状态、Content-Type 和完整正文均一致，失败保持 400 与系统全局内部异常。每组均检查持久化：正式矩阵前为十条探索记录，Development 后为 86 条，Production 后为 100 条；只有成功调用新增记录，两边各一条，失败调用不保存。

最终格式检查、工作区 all-features/locked check、严格 Clippy、完整工作区 build 和三个宿主 Release build 均通过，过程宏与运行时辅助组件一起参与工作区验证。两仓库 diff 检查通过，NetEngine 源仓库仍只有既有五处授权修正。未添加测试文件、测试入口或测试依赖，未在本机安装组件。重复属性转换与引用集合语义的已确认差异已修正；一般 JSON 错误诊断、完整协议事件与尚未覆盖的框架行为继续核对，整体目标未完成。


## Query 与 JSON 跨参数模型错误汇总

源 HEAD 仍为 `23a7d0002938d0308c975dc48c04ac66aae0dda8`，源仓库继续只有既有五处授权修正。重新调用现有 `/api/Demo/Add2` 确认：查询参数非法且正文为空、模型缺字段或类型错误时，源端同时返回两部分错误，Rust 原先只返回 Query 错误；Query 非法且正文媒体类型不支持时，源端为 415，Rust 原先提前返回 400。管理端隐式绑定的文章、栏目操作也适用此问题，不限于显式标记 FromQuery/FromBody 的 Demo。

共享 `request_binding::ModelErrors` 保留字段身份与错误文本，经响应扩展传递给 `BoundHandler`；普通 Query、PageQuery 和 JSON 绑定统一提供此标记。控制器继续绑定其余参数，最后按已有 ModelState 排序及同字段首错规则合并。失败参数只保存为 None，不创建默认模型或调用业务方法。认证、415、正文读取故障及其他非模型拒绝不带此标记，仍立即走各自响应；签名、缓存和请求队列等 Action 过滤器仅在所有绑定成功后运行。JSON 原文字节及 ReplayBody 的读取失败处理保持原有路径。

隔离 PostgreSQL、Redis 与 latest SDK 源宿主中，Development 客户端完成 94 组配对调用：六种查询参数组合与十五种正文，共 90 组，另四组检查媒体类型、缺失类型、未知编码及非法 UTF-16。覆盖合法/重复 Query、必填验证、根 null/空体/空白、根类型、字段类型、嵌套字段、UTF-16 和 UTF-8 BOM。前三类媒体错误返回 415，随机 traceId 校验格式后比较其余正文；非法 UTF-16 的两端底层异常文本不同，该组仅核对 500 及未返回 Query 错误，其余比较状态、完整正文和 Content-Type。

Development 管理端另完成 24 组配对调用：栏目更新、文章创建和更新的 Query/正文组合十五组，独立分页绑定五组，认证、媒体类型和错误签名优先级四组。全部一致；无令牌仍为空体 401，有令牌的模型错误在错误签名前返回 400。Production 客户端十八组再次核对组合错误、合法调用及媒体/编码拒绝，全部一致，非法 UTF-16 返回统一 400 与“系统全局内部异常”。合计 136 组配对调用，不新增验证端点或测试代码。

容器内全工作区 all-features/locked check、严格 Clippy、完整 build、格式检查和三个运行宿主 Release build 均通过，两端 .NET Release 宿主也在容器构建通过。本项关闭的是共享 Query/JSON 参数错误合并；一般截断/非法字面量 JSON、数值/日期转换器的完整诊断及其他未覆盖绑定来源仍不能视为完全等价。整体对齐继续进行。



## 事件后端的无效记录与清理日期边界

源 HEAD 仍为 `23a7d0002938d0308c975dc48c04ac66aae0dda8`。复核 EventBus、Redis 和 Database 实现后，使用正式 Release 客户端宿主、独立 PostgreSQL/Redis 和 latest SDK 源宿主复现两项差异：Redis 重复字段被 Rust 的 StreamRangeReply/HashMap 覆盖，源端 ToDictionary 拒绝该记录；源 TimeSpan 最大保留时间下，Rust 使用 Chrono 负年份生成截止日期，实际 PostgreSQL 持续报告 timestamp out of range，源端则将截止日期限制到 0001-01-01 UTC。

Redis 读取现保留原始字段序列，在逐条解析时拒绝所有重复字段（包括未知字段），错误记录仍携带原位置供游标推进。Redis 字符串按源客户端规则以替换字符处理无效 UTF-8，解码后重名也拒绝；记录时间限定在 DateTimeOffset 的公元 1～9999 年范围。共享运行时将清理截止日期下限限制到公元 1 年，选项中的完整毫秒保留时间限制为 TimeSpan 可表示范围，Scope 长度按 UTF-16 码元计算。未修改 NetEngine 源文件、业务监听注册或发布 API。

实际 Redis 对照包含 22 条记录：普通记录、四类重复字段、三类缺失字段、无效 UUID、空事件名、解码后重名、无效 UTF-8 载荷、大小写不同的未知字段、重连后的记录及日期上限/溢出。两边各自记录相同的 12 个无效位置，其余记录未触发格式拒绝。Redis MONITOR 证实两进程各从启动前已有末尾开始，并推进到同一最后位置；主动断开两个事件连接后，两边各建立新连接并继续读取，HTTP 入口仍返回正常结果。核对脚本初次未剥离 Rust 控制台颜色标记，修正读取方式后逐项位置完全一致，不把初次观察失败计作实现失败或通过。

数据库端在最大保留时间下重启两宿主，已有软删除状态行的 Position=10 保持不变，含 0001-01-01、0001-01-02 及软删除消息的三条记录均保留，两边清理循环无错误。普通 500 毫秒保留时间使用彼此独立的 Scope：各自删除两条过期记录（含软删除），保留两条未来记录（含软删除），另一个 Scope 的四条记录不变；两条软删除状态行仍保持 Position=40。该验证通过正式宿主执行读取/清理，消息样本由隔离 SQL 写入，不据此声称已验证发布事务。

配置边界另核对 ASCII 200/201 字符、补充平面字符 100/101 个，以及保留时间零值/超出上限。两个合法 Scope 均完成启动，四个非法配置均拒绝启动；Rust 合法实例也实际响应 HTTP 后正常退出。源 .NET 和 Rust 的配置错误退出码不同，不将退出码或异常文本视为逐字一致。

容器内全工作区 all-features/locked check、严格 Clippy、完整 build、三个运行宿主 Release build、格式检查和两仓库 diff 检查通过。当前两边业务代码未注册实际事件监听方法，本轮不能证明监听回调的反序列化、资源释放、失败重试或取消行为，也未执行发布事务的并发验证。事件契约与这些运行时范围继续保留，整体对齐目标未完成。



## 队列缺参、重试与双实例领取

2026-11-08 继续对照源 HEAD `23a7d0002938d0308c975dc48c04ac66aae0dda8`。源 QueueTaskBackgroundService 在方法需要参数且记录 Parameter 为数据库 NULL 时抛出缺参异常；JSON 文本 `null` 才交给目标类型解析。Rust 共享参数入口原先用 `unwrap_or("null")` 合并这两种状态，与源不符，早期仅运行 Rust 所得的十六条演示成功记录不构成等价证据。

实际基线使用同样两条 ShowName 和两个隔离数据库：数据库 NULL 的任务在旧 Rust 成功并产生子任务、电话和回调，源端则 Count=1、Pending、没有子任务；显式 JSON null 的主任务两边均成功，但源端派生的缺参子任务/电话失败，父子完成时间为空，回调未产生。现共享 `deserialize_parameter` 在缺失时返回错误，显式 JSON null 仍按 Option<T> 等目标类型解析，无参处理器不受影响。没有修改源项目或将空引用改成空字符串。

修复后，正式 Rust/.NET Release 任务宿主分别运行 34 条初始记录：四个演示队列各包含数据库 NULL、JSON null、空字符串、正常姓名、数字、对象、数组和空文本，另两个消息任务各一条缺参记录。最终各得到 42 条记录，18 条成功、24 条失败待重试；没有调用真实短信或邮件提供商。逐项比较名称、参数、种子编号/父编号、状态、次数、成功及子树完成标记、回调参数、计划时间和租约清理，结果一致；随机生成 ID 和 WorkerId 不作逐字比较。

仅在隔离样本中将失败任务 LastTime 调整到一小时前，以推进已有退避条件，不等待真实五/十分钟。第二轮仍为 24 条待重试，第三轮全部成为 Failed、Count=3，18 条成功记录保持 Count=1；记录总数保持 42，没有失败重试额外生成子任务或回调。两边首次执行时间的逐库摘要未变化，原计划时间保留，失败租约全部清空。该操作验证重试状态流转，不声称测量了实际退避时长。

另加入十条状态控制记录，实际核对计划未到、创建间隔未到、退避未到、过期执行租约恢复、第三次过期租约收敛、有效租约不抢占、软删除过滤、已成功不重跑及最大次数内最后一次成功。源端与 Rust 的状态、执行次数、租约和所有者清理结果逐项一致。随后各启动第二个正式任务宿主并各投递一百条 CallPhone；两个数据库均由两个不同 Worker 分担，100 条全部成功且 Count=1，子树完成标记及租约清理均正确。

容器内格式检查、全工作区 all-features/locked check、严格 Clippy、完整 build 和三个宿主 Release build 均通过，源 TaskService 也在 latest SDK 容器发布通过。未添加或运行测试代码，未在本机安装组件，未修改真实业务数据库。异常文本未逐字对齐，长时间业务执行中的续租、停机期限取消及完整任务管理并发仍需继续核验，整体目标未完成。



## 定时任务在线配置与独立触发

2026-11-08 对照相同源 HEAD 的 ScheduleTaskBackgroundService、ScheduleTaskBuilder 和 TaskSettingSyncBackgroundService，运行两个 Rust Release task_service 及两个源 Release TaskService。两边分别使用独立数据库，共用隔离 Redis；只有既有 ShowTime 定时定义启用，六个队列定义均保持禁用。使用正式任务每次投递十七条 ShowName 的行为观察调度，不添加任务、验证入口或测试代码。

初始 Cron 为 `0/5 * * * * ?`：两个实例完成启动后，已完成的每个五秒触发点各得到 34 条记录，证明每个实例独立执行。在线改为 `0/2 * * * * ?`，等待既有 60 秒配置刷新周期后，两边连续八个已完成触发点均为每两秒 34 条。读取时排除尚未完成的当前秒；一次源端当前秒中途读取到十九条，后续完成窗口按三十四条核对，不把中间状态视为差异。

在线禁用后，两边在各实例下一次配置刷新后停止生成记录，四个进程仍保持运行。禁用稳定期间分别两次读取，Rust 总数保持 1904，源端保持 1921。随后将数据库 Cron 设置为 NULL 并重新启用，等待刷新后，两边连续八个已完成秒均为 34 条，恢复声明中的每秒默认值；数据库 Cron 仍为 NULL，没有被默认值回写。整个运行按秒统计未超过每秒两个实例各十七条的上限，未观察到补跑禁用期间任务的突发批次。

所有生成记录均为未执行的 ShowName（Pending、Count=0），自动定义始终只有七条，其中六个队列定义禁用。宿主启动次序及配置刷新相位不同，因此不比较跨数据库累计条数或动态姓名/ID/时间的逐字相等，只比较完整稳定窗口的频率、每次投递数量、状态及配置保持。未执行短信、邮件或其他外部服务调用。

本轮没有发现上述流程需要进一步修改的生产行为，补充了在线配置的使用说明和实际核验证据。容器内全工作区 all-features/locked check、严格 Clippy、完整 build、三个宿主 Release build、格式和两仓库 diff 检查通过；源 TaskService 在 latest SDK 容器发布通过。四个临时宿主均经 SIGTERM 正常退出。带参动态实例、skip_if_running 的长执行场景、Cron 时区边界及其余框架能力仍需继续核验，整体目标未完成。

## 数据库日志清理的最小日期边界

2026-11-08 对照同一源 HEAD 的数据库日志清理任务。原 Rust 在极大 `save_days` 下回退到 Chrono 的最小日期，超出 PostgreSQL 时间戳范围。在独立 PostgreSQL/Redis 中使用正式 Release TaskService、`save_days = 2147483647` 实际复现一次 cleanup 失败；源 Release 宿主没有该失败。源代码将截止日期限制到 DateTimeOffset.MinValue，隔离数据库的实际语句进一步确认 Npgsql 将其编码为 `-infinity`，而不是有限的 `0001-01-01`。

数据库日志组件现对超出下界的保留时间使用固定 SQL 时间戳 `'-infinity'::timestamptz`，正常范围仍使用参数化日期，继续按当前项目、未软删除和包含截止时刻的条件清理。没有更改源仓库、通用日期转换器、日志记录或分区维护规则。

两边分别迁移独立业务与日志库，为十条隔离数据显式建立样本 ID 分区，覆盖有限最小日期、相邻微秒、普通过期时间、未来时间、负无穷时间，以及软删除和其他项目。极大保留天数下，两边仅删除当前项目未软删除的负无穷记录，其余九条 ID 一致；重新补回该样本，配置 -1 后均保留全部十条；再改为一天，两边均删除四条过期有效记录，保留同样六条。三轮修正后运行均没有 cleanup 诊断，所有正式宿主经 SIGTERM 返回 0。未调用外部提供商，未修改实际业务数据库。

容器内格式检查、全工作区 all-features/locked check、严格 Clippy、完整 build 和三个宿主 Release build 通过，源 TaskService 在 latest SDK 容器发布通过；没有添加或运行测试代码，也没有安装本机组件。本轮验证的是日期清理边界与筛选条件，不代表磁盘故障、文件恢复、缓冲耗尽和停机超时行为已完成运行核验，整体目标仍未完成。

## 数据库日志积压恢复、损坏文件与连接故障

2026-11-08 对照相同源 HEAD 的 LogSaveTask、DatabaseLogWriter 和 LogFileName，在两个正式 Release 任务宿主、两个独立数据库及隔离 Redis 中核验日志消费。通过正式 repository_tool 迁移业务与日志模型，仅为样本 ID 显式建立隔离分区，没有添加任务、运行入口、测试代码或额外依赖。

初始十个文件覆盖 UTF-8 BOM、CR/LF/CRLF、末行无换行、无效 UTF-8 字节、同文件重复 ID、已软删除 ID、损坏行混合有效行、空批次、过期临时文件、恢复目标同名、未到五分钟的临时文件、相似但不符合命名的文件和既有 rejected 文件。两边均新增七条有效记录，连同一条预置软删除记录得到相同八条 ID 与内容；重复 ID 保留首条内容，软删除记录的原文保持不变。含损坏行的完整文件移为 rejected，空批次删除，过期临时文件及其同名目标的不同记录都入库。初始剩余五个文件逐个摘要与原始样本一致；未到恢复年龄和不属于组件的文件保持不动。

两次仅对隔离数据库关闭新连接并终止现有连接，两个宿主均报告数据库阶段失败且继续运行。第一次恢复连接后，有效记录入库，混合损坏文件的完整字节保留。第二次连接仍被禁止时，直接核对两边待入库文件仍为原 log 名称，SHA-256 均与原样本一致，没有提前隔离或删除；恢复连接并重启后，该记录亦入库且完整文件保留为 rejected。

另投递含有效行与损坏行的批次，并预置同名 rejected。两边有效记录均已提交，但两份文件都保持原始字节并报告失败，没有覆盖已有 rejected。随后在隔离目录中将旧样本改为不参与消费的归档名，下一次扫描成功隔离当前批次；两边该有效记录的 xmin 和内容均保持不变。此前八条记录的逐库 ID、xmin、内容摘要在故障、恢复与重启前后也未变化。最终九个保留文件的名称及字节摘要逐项相同。

上述行为未发现需要修改的生产差异，补充了冲突文件的处理说明。容器内格式检查、全工作区 all-features/locked check、严格 Clippy、完整 build、三个宿主 Release build 及源 TaskService 发布通过；所有临时宿主经 SIGTERM 返回 0。没有安装本机组件、修改源仓库或接触实际业务库。仍需核验实际写盘失败、占用锁的临时文件、缓冲耗尽、强制终止与停机超时；不能由本轮消费路径验证推断这些行为已完成，整体目标保持未完成。

## 数据库日志目录故障与停机排空

2026-11-08 对照同一源 HEAD 的 DatabaseLogWriter 和宿主退出顺序。两个正式 Release TaskService 分别连接隔离数据库，共用隔离 Redis。只启用已有 CallPhone 队列，以缺失参数记录产生 Error 日志，不添加任务或验证代码，也不调用外部短信、邮件或电话服务。每阶段五条记录，均观察到 Pending、Count=1 和五次执行错误；为排除后续自动重试干扰，仅将已观察样本的 LastTime 调整到未来。

宿主正常启动后，把各自日志目标目录保留到旁边，再用普通文件占据原目录路径，实际阻断目录创建与日志落盘。第一阶段两边均报告 spool 失败，五条错误日志尚未入库；恢复原目录后，两边均补写并入库五条 Error，证明失败批次未因该目录错误被提前丢弃。

第二阶段再次阻断目录并产生五条错误，发送 SIGTERM 后仍维持阻断约五秒，再恢复目录。两边均在停机期限内返回 0，各留下五条完整日志及五个不同日志 ID，数据库当时仍是此前五条。重启后积压完成入库，两边 Error 总数均为十。源端日志正文没有任务 ID，故该阶段按独立的任务执行数量、日志级别、条数、完整文件和唯一日志 ID 核对，不声称正文或每条源日志的任务 ID 映射相等。

第三阶段保持目录不可写，再产生五条错误后发出 SIGTERM。Rust 在发信号后约 31 秒观测到退出，源端也在排空期限耗尽后退出；两边均记录一次 shutdown 失败并返回 0，没有无限重试。恢复目录并重新启动后，两边仍只有此前十条日志，保存目录中没有该阶段的批次文件，说明未落盘的五条日志没有恢复。这与有限内存缓冲在持续故障及停机超时下可能舍弃的既有行为一致，不是可靠审计持久化保证。

本轮未发现上述流程需要修改的生产行为，修正了数据库日志文档的停机预算说明：任务执行、事件关闭和日志排空共用自停止信号开始的一份三十秒期限。全工作区容器格式检查、all-features/locked check、严格 Clippy、完整 build、三个宿主 Release build 以及源 TaskService 发布通过；无本机安装，无新增或运行测试代码。目录错误发生在实际文件写入之前，因此尚不能证明磁盘写满、部分写入失败后的稳定 ID、容量耗尽或本地小时文件的恢复行为；这些仍需继续核对，整体目标未完成。

## 事件名称边界与 Redis Guid 信封

2026-11-08 对照同一源 HEAD 的 EventPublisher、EventSerialization、RedisEventTransport 和 DatabaseEventTransport。源公共事件名称取具体类完整名称，Redis 读取仅拒绝空白名称；1024 字符限制属于数据库 EventName 字段。原 Rust 将该限制放入公共验证器、Event 派生宏和传输追加入口，导致 Redis 的长名称被错误拒绝。现公共契约及派生宏只校验非空白名称，直接追加入口不再重复增加源端没有的名称校验，数据库字段约束保持不变。

Redis 原先直接使用 uuid::Uuid::parse_str，缺少源 Guid.Parse 的 P/X、外部空白和旧式分量格式，同时会接受源端拒绝的 urn:uuid 前缀。现公开并复用现有 `common::data_helper::parse_guid`，数据导入与请求绑定仍使用同一实现，没有复制另一套解析器。格式选择参考 [.NET Guid 实现](https://raw.githubusercontent.com/dotnet/runtime/release/10.0/src/libraries/System.Private.CoreLib/src/System/Guid.cs)，本轮没有改变公共 Guid 解析算法，仅增加生产使用入口及 Redis 组件依赖。

在独立 PostgreSQL/Redis 中运行两个正式 Release 任务宿主，向同一事件流写入 26 条信封。基线有十二条判定差异：旧 Rust 错误拒绝十一条源端允许的记录，并错误接受 urn:uuid。覆盖 N/D/B/P/X、ASCII/Unicode 外部空白、X 内部空白、大小写、全零标识、空值、非法内部空格、分量溢出、旧式加号和十六进制前缀，以及 1024/1025/8192 字符名称、1025 个非 BMP 字符、空白及带外部空格的名称。判定按完整 Redis 位置精确匹配告警，不使用位置字符串的前缀匹配。

修正后重放同一批 26 条样本，两边均仅拒绝相同七条：urn:uuid、空标识、坏标识、Guid 内部空格、X 首分量溢出、空名称和空白名称，其余十九条没有格式拒绝。Redis MONITOR 确认两边后续读取均已推进到最后一条有效记录的位置，错误记录没有卡住游标。当前业务没有实际事件监听器，因此这证明信封解析与位置推进，不证明负载反序列化或监听调用；直接发布与派生宏名称规则的修改通过源代码对照和编译检查，未增设运行验证接口。

容器内格式检查、全工作区 all-features/locked check、严格 Clippy、完整 build、三个宿主 Release build，以及源 TaskService 发布均通过。内部生产依赖变化在容器离线更新锁文件，未安装本机组件，未添加或运行测试代码，源项目仍仅保留此前五项授权修改。两个临时宿主均通过 SIGTERM 返回 0，临时环境已清理。整体对齐目标仍未完成。

## 事件负载的独立 Web JSON 读取入口

2026-11-08 继续核对同一源 HEAD。EventSerialization 使用 JsonSerializerDefaults.Web，EventListener 在解析后才调用监听方法；原 Rust 的订阅闭包直接使用 serde_json::from_str，未提供 Web JSON 的字段大小写和数字字符串规则。HTTP 请求绑定还包含验证、空白转换及重复字段合并，不能直接用作事件协议入口。

公共 JSON 输入层新增独立 WebProtocol 模式，由事件订阅闭包调用。它按模型字段进行大小写匹配，保留字典原始键，数值字段接受未经裁剪的数字字符串，字符串和可空值不套用 HTTP 的空白规则；对象和数组嵌套上限为 64。该模式保留输入顺序及全部重复属性，不在反序列化前覆盖错误值。浮点数字字符串的固定 NaN/Infinity/-Infinity 拼写依据 [.NET 带引号数字读取实现](https://raw.githubusercontent.com/dotnet/runtime/release/10.0/src/libraries/System.Text.Json/src/System/Text/Json/Reader/Utf8JsonReader.TryGet.cs)，其余非有限文本或溢出结果拒绝。

ProtocolModel 继续负责逐次转换已知字段、最后有效赋值生效、未知字段跳过与 Option 缺省；增加模型级 snake_case/camelCase/PascalCase 命名声明，并复用现有命名辅助函数。事件文档改用 Serialize + ProtocolModel + Event，说明嵌套模型也应使用协议派生，数值等缺省值显式声明 default。没有改变原有 LLM 协议的读取模式，没有把内部 JsonHelper 的日期/long 转换器或 HTTP 验证引入事件。

本轮是源代码级修复和编译核对。当前两个项目的业务层仍没有实际事件发布/监听用例，未新增生产外验证入口或测试辅助程序，因此不能声称已在真实监听调用中验证这些负载规则。手写 Deserialize 的模型仍负责自己的重复字段语义；日期、枚举、缺省初始化、发布端序列化以及监听作用域和取消行为仍需进一步核对，整体目标未完成。

容器内格式检查、全工作区 all-features/locked check、严格 Clippy、完整 build 和三个宿主 Release build 均通过；过程宏与运行时辅助 crate 随工作区一起编译。新增内部依赖离线更新锁文件后恢复 locked 检查，无本机安装，无新增或运行测试代码，源仓库未新增修改。

## 事件发布的非有限浮点数处理

2026-11-08 对照同一源 HEAD 的 EventPublisher、EventSerialization 和 Common.JsonHelper。事件使用 JsonSerializerDefaults.Web，数字字符串读取选项不允许把非有限浮点数作为 JSON 数字写出；源内部 JsonHelper 的输出配置也没有开启命名浮点数写入。原 Rust 直接调用 serde_json::to_string，会将 NaN 和正负无穷静默转换为 null，丢失原始无效值并继续写入事件后端。

公共输出包装器增加独立 Web 模式，事件发布改用该入口：long 保留数字类型，日期交给模型原有 Serialize 声明，不套用内部 long 或日期转换器。递归序列化原始 f32/f64 时检查有限性，错误在调用传输追加之前返回；内部 object_to_json 和 object_clone_json 同样拒绝非有限浮点数。字段、可空值、新类型、序列和映射值继续复用同一递归包装，没有先转成 Value 再猜测原始数值类型。已构造的 JSON 文档值与自定义字符串转换仍保留自身语义，无法恢复模型此前已经丢弃的类型或数值。

本轮属于源代码对照与编译验证，当前业务没有实际事件发布/监听用例，未为验证新增宿主路由、事件或测试辅助程序。尚未证明事件 JSON 的转义、浮点文本格式、写出深度、日期和枚举等全部细节一致，也没有覆盖监听作用域、重试与取消的完整运行链路；整体对齐目标仍未完成。

容器内格式检查、全工作区 all-features/locked check、严格 Clippy、完整 build 和三个宿主 Release build 均通过，两个仓库 diff --check 通过。未安装本机组件，未添加或运行测试，未新建临时运行环境；源仓库仍仅保留此前五项授权修改。

## 数据库事件读取与无限时间边界

2026-11-08 对照同一源 HEAD，在独立 PostgreSQL、Redis 和 latest SDK 容器中运行两边正式任务宿主。使用 repository_tool 分别迁移两套隔离业务、日志和事件模型；源码只读复制后发布源 TaskService，未连接默认业务数据库。先核对极长保留时间，再通过数据库写入信封验证读取与游标，不新增业务事件、监听器或验证接口。

第一批十条历史记录覆盖负无穷、软删除负无穷、公元 1 年及其后一微秒、公元前日期、普通过期记录、软删除过期记录、未来记录和其他范围记录。最大保留时间下，旧 Rust 删除三条、源端全部保留。源 Npgsql 将 DateTimeOffset.MinValue 作为 -infinity 参数发送，旧 Rust 使用有限公元 1 年。数据库适配器现在将最小日期用于 `< '-infinity'` 清理条件，仍执行数据库语句；两边重放后全部保留。随后将保留时间改为一小时，两边均删除相同七条，仅保留未来及其他范围的三条；清理过程中软删除的范围状态仍为 7777，原 xmin 分别为 767/769，均未改变。各清理阶段没有清理错误。

继续写入位置 7778/7779 的事件时，旧 Release Rust 报 `no column found for name: position` 并保留位置 7777。原 MessageRow 错用了实体的 column_name 属性；FromQueryResult 接受的是 from_alias。五个查询字段改为实际列名映射。仅改别名后运行正式 Debug 任务宿主，正无穷时间进一步触发 SQLx/Chrono 的日期加法溢出，事件后台任务失败并结束宿主；源 Release 宿主正常读取。该阶段仅验证数据库读取，不涉及生产签名规则。

现查询先在 PostgreSQL 侧区分无限时间及有限越界值，避免把溢出值交给日期驱动。正负无穷分别还原为 DateTimeOffset 的最大、最小 UTC 时间，最大值保留百纳秒末尾；有限的公元 1～9999 年值按普通日期读取，其余返回整批读取错误。正式 Release 两边重放普通时间、正负无穷、公元 1 年与 9999 年末五条信封，均无读取错误，PostgreSQL 查询参数确认后续位置均为 7782。

随后加入位置 7783 的 10000 年日期及位置 7784 的正常记录，两边均读取失败、保留 7782 并重试。将同一坏记录改为公元前日期后，两边继续报错且位置不变，Rust 不再 panic。将日期改为有效值后，两边均推进至 7784；额外的软删除记录 7785 和其他范围记录 7786 未推进本范围游标。游标证据来自 PostgreSQL 的实际查询参数，而非仅凭进程存活或没有错误日志。这个运行证据覆盖信封读取、错误恢复和范围/软删除过滤；业务仍没有实际事件监听器，不能据此证明监听作用域、负载转换、回调及重试副作用均已等价，整体目标仍未完成。

最终生产代码的容器格式检查、全工作区 all-features/locked check、严格 Clippy、完整 build、三个宿主 Release build 均通过，源 TaskService 容器发布通过。两个仓库 diff --check 通过；未安装本机组件，未添加或运行测试代码。最终两个临时宿主 SIGTERM 均返回 0，临时容器和目录已清理，源仓库仍只有此前五项授权修改。

## 事件监听的独立处理器与异步释放

2026-11-08 继续对照同一源 HEAD 的 EventListener.InvokeAsync。源端先创建异步作用域并解析服务，再反序列化负载和执行方法；await using 确保正常返回、参数解析失败、处理异常时均等待作用域释放。原 Rust 只有接收已解析事件的闭包入口，无法统一保证服务在参数解析前创建，以及失败和展开型 panic 后的异步清理。

event_bus 增加 EventHandler 契约与 subscribe_factory 注册入口，沿用现有任务处理器的工厂及 close 约定。每次尝试同步构造新的处理器，再解析负载并调用 handle；取得处理器后，无论解析或执行结果如何，均等待 close。展开型 panic 先捕获，释放后再交给原监听运行时报告与重试；同步析构在恢复 panic 展开之前完成。释放失败优先于原结果，只有执行及释放都成功才完成该监听，因此释放失败也可能重跑已经执行过的业务。普通 subscribe 闭包适配到同一入口，保留其捕获共享状态的生命周期，应用接口层重新导出处理器契约。

正常完成不主动取消处理器令牌；到期或停止仍由已有运行时发送取消，等待处理及释放，并共享宿主剩余停机预算。没有额外后台释放任务，也不承诺强制结束后仍能完成异步清理。工厂未能成功返回处理器时，已取得资源的释放仍由工厂负责；这一点没有通过新的依赖注入容器自动托管。

本轮为源码对照与编译验证。重新检查两边应用和宿主代码，仍无实际业务事件发布/监听用例；没有添加假业务监听器、验证接口或测试辅助程序，因而没有真实回调的释放次数、重试副作用或停机时序证据。注册反射、服务代理及自定义协议等其余语义也不能因此宣称完全等价，整体对齐目标仍未完成。

容器内格式检查、全工作区 all-features/locked check、严格 Clippy、完整 build 以及三个宿主 Release build 均通过；两个仓库 diff --check 通过。本轮无依赖更新、无本机安装、无新增或运行测试、无临时宿主，源仓库仍保留此前五项授权修改。

## 本地小时日志的只读清理与尾部恢复

2026-11-08 回看公共能力清单后，核对同一源 HEAD 的 LocalFileStore、LocalFileLogWriter 和 LogClearTask。源清理按最后写入时间删除文件，不要求先读写打开或取得文件锁；原 Rust 清理复用追加用的 open_locked，导致目录允许删除但文件不可写时无法清理。现改为读取元数据后删除，仍在本写入器状态锁内运行，并跳过等待重试文件。

使用独立 PostgreSQL/Redis 和两边正式 Release 任务宿主，均以 UID/GID 1000 运行，避免 root 绕过文件权限。源工作区只读挂载，在容器副本中将已有数据库日志注册替换为已有本地文件日志注册，读取 LocalFileLogger 配置；没有改日志组件实现、增加验证路由或辅助宿主，也未改原 NetEngine 工作区。默认数据库日志注册无法验证本地文件组件，因此该装配切换是本轮运行证据的明确前提。

基线确认 Rust 保留了过期的 0444 小时文件并报告 cleanup 故障，源端删除该文件。初批过期未完成文件还揭示启动顺序差异：源清理可能先于恢复执行，Rust 则在清理入口强制等待恢复，从而先截短文件并改变其最后写入时间。已移除清理入口的额外恢复调用，使恢复与清理独立调度、通过同一状态锁互斥；不承诺已经过期文件一定先保存尾部。后续恢复验证使用未过期文件，避免把不同后台调度顺序误算为尾部恢复差异。

重放十个文件与一个目录，覆盖过期只读/可写/空文件、旧文件名但最近修改的可写和只读文件、非法日期名称、普通外来名称、已有 rejected 文件、同名目录，以及 CRLF 完整行后的 12 字节尾部和没有换行的 65541 字节文件。修正后，两边保留的七个固定文件内容哈希一致，均删除过期只读、可写和空文件，保留目录；两个尾部保留文件按原小时文件名和内容比较，字节数及 SHA-256 一致，GUID 文件名不要求相同。

将日志目录设为 0555 后，两边恢复均报告权限失败，原始 38 字节文件的 SHA-256 保持一致且没有截短，也没有生成尾部保留文件。恢复目录写权限并重新启动后，两边均保留相同的完整行和原始尾部。确认全部前序宿主退出后再次写入同一未完成尾部并重放，均生成新的同内容 rejected 文件，原保留文件没有覆盖。该证据覆盖尾部跨缓冲区读取、原始字节保存、保存失败及后续恢复，不覆盖磁盘写满或正在追加的批次部分写入。

最终代码重新通过容器格式检查、工作区 all-features/locked check、严格 Clippy、完整 build 和三个宿主 Release build；源任务宿主副本发布通过。最终 Release 重放十文件/一目录仍得到相同保留内容及尾部哈希。临时宿主停止返回 0，临时容器和目录已清理；两个仓库 diff --check 通过。未安装本机组件、未添加或运行测试代码，NetEngine 原工作区仍只有此前五项授权修改。全部日志故障和整体框架功能等价尚未证明，目标继续保持进行中。

## 本地小时日志的写满、部分写入与重试

2026-11-08 对照同一源 HEAD，复用正式 Release 任务宿主及现有 CallPhone 缺参失败流程，在独立 PostgreSQL/Redis 环境的两套业务库中启用本地文件日志。源容器副本仅切换已有日志提供者注册并读取 LocalFileLogger 配置，未修改原源码或日志组件。两边以 UID/GID 1000 运行，日志目录使用各自 64 KiB tmpfs，落盘间隔为五秒；Rust 容器只读挂载已有 Cargo 产物卷。未引入测试宿主、任务、辅助业务代码或第三方服务调用。

第一轮投入十条数据库 NULL 参数任务，两边均执行一次后回到 Pending。Rust 文件已有 3072 字节完整确认行，源文件起始为空；分别占满剩余空间后，两边日志均停在 4096 字节，包含四条可解析 JSON 行和一条不完整尾行，且没有末尾换行。写入错误被报告，任务调用没有被磁盘失败阻塞。释放占位文件后，两边各产生完整十条 Error 记录，无非法 JSON；Rust 原确认行保留。两边日志正文和长度本来不同，不以文件整体哈希判断跨语言日志等价。

保存这一轮全部确认字节后，再次填满空间并投入另十条缺参任务。Rust 停在 8192 字节、源端停在 12288 字节，末尾均为半行；原有 5702/8460 字节前缀逐字节未变。随后在隔离样本中分别外部截短为 5701/8459 字节，再释放空间。经过至少一轮完整重试等待，两边继续报告写入故障且文件长度不变，没有静默重建或在缺失内容后追加。

将各自保存的完整前缀通过同目录原子替换恢复后，两边继续完成待写批次。最终 Rust 为 8332 字节，源为 16920 字节，各有二十条完整 Error 记录，原确认前缀仍逐字节一致、末尾有换行，未生成启动恢复用的 rejected 文件。Rust 正文携带的二十个 task_id 恰好覆盖 995001～995020 且各一次；源错误正文没有对应 ID，因此只验证源日志数量及数据库中二十条记录各执行一次，不声称逐条身份已比对。为防止五分钟后的业务重试干扰写盘核验，仅将隔离任务的 LastTime 暂设为未来时间。

本轮未发现需要修改的生产差异，补充写满和外部截短的维护说明。该证据证明同一小时段的部分写入会从未确认偏移重写、此前确认内容不重复，以及截短后拒绝自动补齐；不覆盖跨小时失败段、断电持久性、强制终止丢失、容量耗尽或数据库日志批次的全部故障行为，整体对齐目标仍未完成。

本轮重新通过容器格式检查与工作区 all-features/locked check，未改 Rust 生产代码，沿用上一轮已通过完整编译的 Release 产物；源 TaskService 副本重新发布通过。所有临时宿主停止返回 0，临时容器及目录已清理，两个仓库 diff --check 通过。未安装本机组件、未新增或运行测试代码，NetEngine 原工作区仍只有此前五项授权修改。

## 发布包、运行镜像与生成的 HTTPS 代理

2026-11-08 对照同一源 HEAD 的部署结构，实际在开发容器依次执行 deployment/build-release.sh，重新生成 admin_webapi、client_webapi、task_service 三个 Linux amd64 发布包。三个默认功能 Release 编译均成功；随包默认配置与各自源码逐字节一致，运行库清单没有缺失项。产物保留在 dist 下，本轮没有修改 Rust 生产代码、构建脚本或模板。

另外直接使用管理端 Dockerfile 构建 runtime 阶段，生成 rust-engine-admin-alignment:latest。运行镜像中的动态库齐全、字体存在且不含 Cargo；默认配置、可执行文件旁的 InitData/AuthorizeInitData.json 及 ASP.NET Core 许可证与源码哈希一致。管理端使用实际镜像入口启动；客户端和任务服务则将上述发布包复制到同一 Ubuntu 运行环境，按 /package 工作目录与各自默认配置路径启动。后两者没有单独构建其 Dockerfile，不能据此声称三个运行镜像都已验证。

运行使用独立 PostgreSQL/Redis，正式迁移工具分别初始化业务、日志和事件模型；只有临时容器内的配置替换为隔离连接及密钥。管理端通过 APP_CONFIG_PATH 读取外部配置，客户端和任务服务使用随包默认路径。任务服务补齐七个默认禁用定义，未产生队列任务，不调用短信等外部服务。

将生成的 nginx.conf 与两份 API 代理配置直接放入 nginx:latest，使用临时自签名证书覆盖两个示例域名，并让代理与 API 共用容器网络命名空间，以保留模板的回环上游地址。HTTPS 健康接口两端均返回 200/Healthy；管理端静态样本可读取，客户端同路径为 404。客户端 GetVerifyCode 返回 200、image/png，文件签名与 158×45 尺寸有效，证明发布环境能实际完成字体/图像输出。代理支持 HTTP/2；当前 Nginx 对旧式 listen http2 语法报告弃用提示，但配置可正常运行。生成配置只监听 HTTPS，本轮没有验证或增加 HTTP 重定向。

三个正式宿主均通过 Docker 停止信号正常退出，退出码为 0，没有 OOM 或错误级日志。临时宿主、代理、数据库、缓存及隔离目录已清理，发布包和管理端镜像保留。两个仓库 diff --check 通过；未安装本机组件、未添加或运行测试代码，NetEngine 原工作区仍只有此前五项授权修改。本轮覆盖本机容器交付链路，不代表真实服务器 systemd、云效上传部署、其他 Linux 发行版兼容性或整体功能等价已经完成。

## 公共 XML 读取的 DTD 禁止规则

2026-11-08 重新读取同一源 HEAD 的 Common/XmlHelper.cs。源 XmlToObject 明确使用 DtdProcessing.Prohibit 和空 XmlResolver；原 Rust 仅调用 quick_xml::de::from_str。检查锁定的 quick-xml 0.42.0 本地依赖源码，默认 PredefinedEntityResolver.capture 对 DocType 返回成功，既不会解析外部资源，也不会拒绝声明，因此“没有外部实体解析”不能替代源端的“禁止 DTD”规则。

公共 xml_to_object 现在先遍历原字符串的 XML 事件，发现 DocType 返回固定错误，再进入原反序列化路径。使用解析事件而不是字符串匹配，避免误判注释或 CDATA 中的相同文字；全输入预读同时避免未知字段整棵子树被反序列化器跳过时遗漏声明，遇到预读解析错误也直接返回。未引入网络或文件实体解析器，没有新增依赖或改变 XML 输出实现。

当前两边业务与宿主均未发现公共 XmlHelper 的调用点，本轮证据为源代码、实际依赖实现对照及编译检查；没有新增演示端点、临时验证宿主或测试代码，不能宣称已有业务 XML 请求运行证据。一般 XML 文档完整性、命名空间、特殊字符、对象属性映射及源 XmlSerializer 的全部读取边界仍需继续核对，整体功能对齐尚未完成。

最终容器格式检查、全工作区 all-features/locked check、严格 Clippy、完整 build 和三个宿主 Release build 均通过；两个仓库 diff --check 通过。本轮未重建 dist 发布包或上一轮的管理端运行镜像，未在本机安装组件，未新增或运行测试，NetEngine 原工作区仍只有此前五项授权修改。另复核了两个宿主在日志创建后的提前返回路径，以及源日志 StopAsync/Dispose 与 Rust shutdown/Drop 的区别，启动失败是否应排空仍需结合实际宿主调用顺序核验，未据此直接修改生命周期。

## 公共 XML 字符与引用校验

2026-11-08 继续核对同一源 HEAD 的 XmlHelper，以及 .NET 10 的 [XmlReaderSettings](https://raw.githubusercontent.com/dotnet/runtime/release/10.0/src/libraries/System.Private.Xml/src/System/Xml/Core/XmlReaderSettings.cs)、[XmlWriterSettings](https://raw.githubusercontent.com/dotnet/runtime/release/10.0/src/libraries/System.Private.Xml/src/System/Xml/Core/XmlWriterSettings.cs) 和 [XmlTextReaderImpl](https://raw.githubusercontent.com/dotnet/runtime/release/10.0/src/libraries/System.Private.Xml/src/System/Xml/Core/XmlTextReaderImpl.cs)。源端读写默认启用字符检查，数字引用同样校验，未声明实体在跳过内容前也会失败。锁定的 quick-xml 0.42.0 数字转换拒绝零及无效 Unicode 标量，但接受 XML 1.0 禁止的部分控制字符和 U+FFFE/U+FFFF；默认字符串输出也没有等价的字符范围校验。

Rust 公共工具现按 [XML 1.0 Char 范围](https://www.w3.org/TR/REC-xml/#charsets) 检查原始输入与序列化结果；不使用 Unicode 控制字符分类，以免误拒合法制表符、回车、换行及规范允许的其他字符。输入预读同时解析每个元素的属性和文本字符引用，对解析后的字符再检查，未知字段也执行同一规则。属性仅使用五种 XML 预定义实体，文本中的未声明实体返回固定错误；注释和 CDATA 内的引用样式仍只是普通文字，转义后的字面量不做二次实体展开。错误不附带原始 XML 内容，未新增依赖或外部资源读取。

这仍是公共工具的源码与编译核验，两边业务/宿主没有 XmlHelper 调用点，未增加辅助端点或验证程序。XML 声明、文档边界、命名空间、模型映射、输出换行归一化及完整诊断顺序尚不能认定与 XmlSerializer 等价。本轮未改 NetEngine 原工作区、未在本机安装组件、未添加或运行测试代码；整体目标仍未完成。

最终容器格式检查、工作区 all-features/locked check、严格 Clippy、完整 build 和三个宿主 Release build 均通过；两个仓库 diff --check 通过。没有临时运行服务，本轮未重建 dist 发布包及先前的运行镜像。

## 公共 XML 输出换行规则

2026-11-08 继续对照同一源 HEAD 的 XmlHelper.ObjectToXml。源工具未覆盖 [XmlWriterSettings](https://raw.githubusercontent.com/dotnet/runtime/release/10.0/src/libraries/System.Private.Xml/src/System/Xml/Core/XmlWriterSettings.cs) 的默认 NewLineHandling.Replace 和 Environment.NewLine；[XmlUtf8RawTextWriter](https://raw.githubusercontent.com/dotnet/runtime/release/10.0/src/libraries/System.Private.Xml/src/System/Xml/Core/XmlUtf8RawTextWriter.cs) 对元素文本将 CR、LF、CRLF 改为平台换行，对属性则分别转义 CR/LF/Tab。quick-xml 0.42.0 的普通文本输出会将 CR 写成字符引用，读回仍是 CR，与源工具不同。

object_to_xml 在已有字符校验后按 XML 事件整理自身生成的结果。文本及文本中的换行字符引用统一为平台换行，CRLF 合并一次；普通实体和标记原样写回，属性中的字符引用保留。CR 与其后 LF 分属两个解析事件时保留连续状态，遇到标记或其他实体则结束这一状态，避免跨元素合并。CDATA 内容独立处理，既有转义后的字面量不再次解码。读取用户 XML 的入口不应用此输出规则，合法的显式 CR 字符引用仍由原反序列化路径读取。

本轮仍没有业务 XML 调用点或运行对照，未增加辅助程序、端点和测试代码。Linux 编译覆盖平台换行选择的当前目标，不代表 Windows 运行验证；模型映射、声明、文档边界、完整诊断，以及自定义序列化产生的调用边界仍需继续核对，不能据此认定公共 XML 能力或整体目标全部完成。本轮没有新增依赖、没有修改 NetEngine 原工作区，也没有重建 dist 或运行镜像。

最终容器格式检查、全工作区 all-features/locked check、严格 Clippy、完整 build 与三个宿主 Release build 均通过；两个仓库 diff --check 通过。未在 Windows 安装任何组件，没有新增或运行测试，也没有临时服务需要保留。

## 事件后端阻塞时的宿主启动与取消

2026-11-08 核对同一源 HEAD 的三个 Program、EventBusHostedService 及日志提供者生命周期。源 StartingAsync 在所有托管服务启动前等待事件后端位置，StartedAsync 才启动读取/清理；日志 StopAsync 等待排空，Dispose 只取消。Rust PreparedEventBus 同样先建立位置再进入监听，相关运行时已有取消/释放守卫。本轮没有根据“所有启动失败都应排空”的假设改写日志生命周期。

实际使用三个正式 Release 二进制、独立 PostgreSQL/Redis 和现有 Ubuntu 运行镜像。业务模型先迁移，事件模型故意暂不迁移；仅隔离配置切换为本地文件日志及数据库事件，三个宿主共享同一 ID 前缀，节点租约为十五秒。没有添加业务任务、监听器、辅助宿主或测试代码。

三宿主在事件表缺失时持续重试，超过初始节点租约时长后仍有三个有效租约；管理端/客户端端口连接失败，TaskSetting 为空，没有进入运行态。Docker 停止信号使三进程及时退出，均记录“事件总线启动已取消”，退出码 101、没有 OOM。此处是入口 expect 对取消错误产生 panic，不是退出码 0 的正常运行期停止。停止后三个租约仍有约十三秒 TTL，随后全部自然消失，没有立即删除节点键。

重新启动同样三个容器，确认各自再次重试十次且尚未启动后，通过正式 repository_tool 补齐日志和事件迁移。三个进程无需重启即完成启动，两端 /healthz 返回 200/Healthy；任务服务补齐七个默认禁用定义，队列为空。此阶段正常停止三个宿主均返回 0。

随后用隔离数据库的独立事务持有 EventBusStream 排他锁，再次启动三个宿主。pg_stat_activity 确认三个事件查询等待关系锁，两个 API 仍未监听。发送停止信号后宿主均迅速以 101 退出且记录启动取消，但数据库中仍有三个等待查询；取消隔离锁事务后等待查询才消失。这证明进程及时退出不等于 PostgreSQL 中的在途查询已立即取消。

源 DatabaseEventTransport 的 EnsureStreamAsync 将 CancellationToken 传入上下文创建和查询；Rust 通过丢弃等待 future 处理取消，当前 SeaORM/SQLx 路径没有同等的显式查询取消。本轮只确认 Rust 的实际服务器状态，未运行源端锁等待对照，因此尚不能量化两端取消时延。检查本地 SQLx 0.9 PostgreSQL 源码未找到公开 CancelRequest 入口；后续需要落实适配器层的服务器查询取消方案及源端实际对照，不以任意缩短查询超时或放弃停机期限替代。整体目标仍未完成。

本轮没有生产代码改动，正式 Release 产物沿用上一轮已完成的三宿主构建；容器工作区 all-features/locked check 再次通过，两个仓库 diff --check 通过。全部临时宿主、数据库、缓存、锁事务及配置目录已清理；未安装本机组件、未添加或运行测试，NetEngine 原工作区仍只有此前五项授权修改。

## 事件数据库操作的服务器端取消

2026-11-08 继续对照源 HEAD 23a7d0002938d0308c975dc48c04ac66aae0dda8。本轮用 latest SDK 在容器内发布原 TaskService，代码不作临时行为替换；隔离配置使用数据库事件后端。源工作区仍只有云上传两处和 UTC 查询三处授权修改，三处日期查询均在构造条件前转换 UTC，保留输入所表示的同一时刻。

先在同一隔离 PostgreSQL 上持有 EventBusStream 排他锁，并运行源 TaskService 与改动前 Rust Release 宿主。数据库确认双方各有一条等待查询；停止后源查询消失，Rust 查询仍在锁队列中，直到释放锁才结束。源进程退出码 139，Rust 为 101，均无 OOM；该结果只证明服务器端取消差异，不把源进程退出称为正常停止。

公共 EventTransport 新增位置、读取、清理的可取消入口，保留既有调用入口及其他后端默认实现。数据库适配器为操作保留事务连接，以后端 PID、启动时间和随机 application_name 标记身份；正常提交归还连接，失败、取消或外层丢弃均禁止复用。取消通过独立控制连接精确终止该连接，沿用原认证与 TLS 配置，不依赖业务池可用连接，也不批量终止其他请求。采用 [PostgreSQL 带等待参数的终止函数](https://www.postgresql.org/docs/current/functions-admin.html#FUNCTIONS-ADMIN-SIGNAL) 确认结束，服务器等待五秒、整个控制过程最多十秒，仍受宿主三十秒总预算约束。启动收到停止信号时，两个共享宿主先通知取消并等待适配器清理，不再直接丢弃准备过程。

改动后使用非超级用户 cancel_app 验证，仅授予隔离数据库的普通表读写与序列权限，没有授予 pg_signal_backend。连接池最大值为一时，启动等待中的一条查询在停止后归零，持锁会话仍然活跃；启动取消仍由入口 expect 以 101 退出。正常启动补齐七个任务定义，单连接池运行期的读取被锁阻塞后，停止同样清除查询，宿主返回 0。将池上限调整为二后，数据库同时观察到一条读取、一条清理等待；停止后两条均消失，持锁连接仍在，宿主返回 0。各轮均在释放验证用锁之前查询数据库状态，未出现服务器取消失败诊断。

恢复正常运行后，隔离数据中的本 Scope 过期记录被清理，新记录及其他 Scope 的过期记录保留；没有读取/清理错误，观察时没有遗留 idle in transaction 操作，正常停止返回 0。SQL 日期范围、正负无穷映射、软删除过滤及位置排序保持原规则；本轮没有重做此前全部日期边界对照，也没有业务监听器调用证据。

容器内全工作区格式检查、all-features/locked check、严格 Clippy、完整 build 和三个宿主 Release build 均通过，两个仓库 diff --check 通过。临时宿主、参考构建容器、数据库、缓存和配置目录已清理，未重建 dist 或既有运行镜像。没有新增依赖、测试代码或测试运行，没有安装本机组件。发布与健康检查的调用方取消、网络不可达或数据库拒绝控制连接时的全部故障行为仍未覆盖，整体对齐目标继续保持未完成。


## 事件发布的调用方取消契约

2026-11-08 继续核对同一源 HEAD 的 IEventPublisher、EventPublisher、两种 EventTransport 及 PostgreSqlServiceCollectionExtensions。源发布入口在名称校验和序列化前检查 CancellationToken，再传入后端；数据库初始化、状态更新、信封写入和提交都接收该令牌。Rust 原先仅在启动位置、读取和清理中传递取消，发布入口无法表达调用方取消。

EventPublisher 现提供 publish_with_cancellation，保留无令牌 publish 并委托到同一实现。取消先于名称与 Web JSON 序列化检查，不启动已取消的发布。EventTransport 新增 append_with_cancellation，默认实现适用于 Redis 等等待确认的后端：取消结束调用方等待，不声称撤回已发送命令。

数据库发布复用 OperationConnection。范围状态通过共享 ensure_stream 独立提交，之后在新的独立事务内原子增加 Position、生成消息 ID、写入信封并提交；同一 Scope 的行锁持续到提交，未借用业务事务或以 ID 替代位置。原先发布专用的非取消初始化实现已删除，启动与发布使用同一条初始化路径。调用方取消覆盖连接获取、状态查询/创建、位置更新、ID 等待、消息写入和提交；查询间隙或 ID 等待中观察到取消时，也等待服务器端清理后返回。

取消等待沿用上一轮的独立控制连接、精确后端身份、禁止连接复用和有界等待规则；直接丢弃 future 无法等待清理。提交与取消可能竞争，取消错误不构成“消息一定没有写入”的证明。源注册未启用 EnableRetryOnFailure，本轮不额外加入无依据的自动发布重试；状态递增仍使用已有原子条件更新处理并发。

当前双方业务层均未发现事件发布调用，也没有业务监听器，本轮没有新增验证路由、临时发布程序、事件或测试辅助代码。证据为源代码与编译检查，不能把上一轮监听/清理锁等待的运行结果当作本轮发布事务取消、提交竞争或消息幂等的实测证据。健康检查的调用方取消与完整网络故障场景仍待核对，整体对齐目标未完成。NetEngine 原工作区仍仅保留此前五项授权改动，本轮没有修改源项目。

本轮容器内格式检查、全工作区 all-features/locked check、严格 Clippy、完整 build 以及三个宿主 Release build 均通过；两个仓库 diff --check 通过。没有新增依赖、测试代码或测试运行，没有安装本机组件，也没有临时服务需要清理；dist 与运行镜像未重建。

## 健康检查的协作取消与超时清理

2026-11-08 继续核对同一源 HEAD 23a7d0002938d0308c975dc48c04ac66aae0dda8。源 Repository.Core.DatabaseHealthCheck 将令牌传给 CanConnectAsync；[Npgsql EF Core 10.0.3 的 ExistsAsync](https://raw.githubusercontent.com/npgsql/efcore.pg/v10.0.3/src/EFCore.PG/Storage/Internal/NpgsqlDatabaseCreator.cs) 将它继续传给连接打开和 SELECT 1。Rust 原先由 timeout/select 丢弃健康检查 future，未将取消传给数据库操作；这与已对齐的事件操作取消路径不一致。

数据库操作连接移至 repository_core::postgres_operation::PostgresOperation，事件与健康检查共用后端身份登记、独立控制连接、取消确认及禁止连接复用逻辑。基础仓储仅依赖已有的 cancellation 契约和 UUID 组件，不反向依赖事件或业务组件；Cargo.lock 同步新增的直接依赖关系。DatabaseHealthConnection 只开放探测方法，ReadDatabaseConnection 在内部委托底层连接，不暴露写连接或改变只读查询接口。

通用数据库检查达到三十秒时通知子令牌并等待清理，调用方取消也传入同一操作。HealthState 增加可取消入口，AppState 将令牌传给业务写/读库、日志写/读库、事件后端及缓存等待。事件数据库健康检查与事件发布分开，不建立范围状态或发布消息；Redis 取消等待 PING。后台每轮使用宿主令牌的子令牌，超时或停止后等待该轮清理，仍受宿主剩余停机预算约束，不以直接丢弃整轮 future 代替取消。旧入口保留；非协作的自定义 HealthState 实现仍需自行实现新契约。

容器中使用正式客户端 Release 二进制、隔离 PostgreSQL/Redis、本地文件日志、数据库事件后端及正式工具迁移。初始 /healthz 返回 200/Healthy 和禁止缓存响应头，覆盖业务只读连接及选定日志/事件目标。暂停数据库后，实际 HTTP 健康请求在 29.999826 秒返回 503/Unhealthy；保持暂停跨过后台一轮超时时刻，日志出现一次“后台健康检查超时”。恢复数据库后，同一宿主无需重启即再次返回 200/Healthy，观察时 idle in transaction 为零，队列和事件消息均为空。

另一次重新启动后立即暂停数据库，保持暂停跨过首次后台检查的十秒时刻，再发送停止信号：宿主约 0.56 秒以 0 退出，无 OOM；数据库恢复后宿主连接计数为零。该暂停场景验证故障期间的超时、退出和恢复，不等于在数据库暂停时已经确认服务器取消成功，也不代表人为长时间阻塞 SELECT 1 的对照。没有加入代理、替代宿主、测试代码或测试运行；本轮没有运行 NetEngine 健康端点对照。

HTTP 请求断连后的完整取消链、无法建立控制连接/执行取消时的全部故障场景，以及第三方不配合取消的检查仍未全部验证。没有以本轮后台检查证据覆盖这些差异，整体对齐目标保持未完成。NetEngine 原工作区没有新增修改，仍只保留此前授权的云上传两处与 UTC 查询三处变更。

容器内格式检查、全工作区 all-features/locked check、严格 Clippy、完整 build 与三个宿主 Release build 均通过；repository_core 独立 locked check 也通过，确认共享能力不依赖事件 crate 的特性合并。两个仓库 diff --check 通过。隔离宿主、数据库、缓存和临时配置已清理；没有安装本机组件，dist 和既有运行镜像未重建。

## 请求断连后的协作清理与宿主排空

2026-11-08 对照同一源 HEAD 23a7d0002938d0308c975dc48c04ac66aae0dda8，继续核对健康请求与现有 LLM 请求入口。源 [HealthCheckMiddleware](https://raw.githubusercontent.com/dotnet/aspnetcore/release/10.0/src/Middleware/HealthChecks/src/HealthCheckMiddleware.cs) 将 HttpContext.RequestAborted 交给健康服务；[Kestrel HttpProtocol](https://raw.githubusercontent.com/dotnet/aspnetcore/release/10.0/src/Servers/Kestrel/Core/src/Internal/Http/HttpProtocol.cs) 等待应用请求处理结束，再完成请求生命周期。Rust 已有独立请求令牌和响应体取消守卫，但健康端点没有提取令牌；请求 future 被连接释放时，数据库清理 future 也可能一起被丢弃。

共享 WebAPI 宿主现持有 RequestRuntime，用 TaskTracker 跟踪请求处理任务。连接 future 释放仍由原守卫发送取消，处理任务继续到返回，允许协作的服务完成异步清理。响应体的正常完成、HEAD、204、304 与信息响应仍按原规则处理；没有把同连接的所有请求合并为一个令牌。请求任务保留当前 tracing span，业务对象响应的 task-local scope 仍在任务内由原中间件建立。健康端点提取 RequestCancellation 并调用上一轮增加的可取消接口；已有 LLM 入口继续显式传递自己的令牌。

HTTP 服务停止接收并排空后，宿主按同一三十秒截止时间等待请求任务，再完成事件和日志排空。任务等待超时会通知强制取消并返回停机错误；运行时守卫异常释放也通知取消。没有独立追加一段停机预算。单独使用 Router 构建器而没有共享宿主管理时，保留在原请求 future 内执行，不能据此宣称所有自定义宿主都自动获得任务排空。

实际运行正式客户端 Release 宿主与隔离 PostgreSQL/Redis，将各数据库池上限设为一、获取连接超时设为六十秒。初始 GET/HEAD /healthz 均为 200，禁止缓存头保留；/Base/GetRegion 仍返回 JSON 空数组，未丢失对象响应格式。独立事务持有 EventBusMessage 排他锁，数据库确认一条事件读取等待并占用该池。发送带唯一查询标记的健康请求，两秒后主动断开；客户端未收到响应，服务端请求日志记录 2003 毫秒结束、内部状态 503。此时持锁连接和原事件等待查询都仍存在，未靠释放锁或终止其他操作完成请求取消；这是服务端收尾证据，不把 503 当作已交付给断开客户端的响应。

随后释放该锁，另以 RegionProvince 排他锁阻塞没有取消参数的实际 /Base/GetRegion 查询。客户端两秒断开后，数据库查询仍等待；发送停止信号十秒后宿主仍运行，查询仍在。释放锁后停止命令在约 17.1 秒完成，宿主退出码 0、无 OOM，数据库无残留宿主连接和持锁会话。该结果验证无取消参数的处理仍可完成，以及宿主等待断开连接后的处理；不是未配合处理的强制超时证据。

新增的持续持锁、直至停机期限耗尽的验证命令被自动审批以 blocked by policy 拦截，未提供更具体原因。该命令没有执行，也未以变形命令重试；强制超时分支本轮仅有代码与编译核对。尚未实测所有流式接口、HTTP/2 单流取消、响应体中断、全部异常路径及强制截止行为，也未运行 NetEngine 对应请求对照；整体对齐目标仍未完成。源工作区没有新增修改，仍只有此前五项授权变更。

容器格式检查、全工作区 all-features/locked check、严格 Clippy、完整 build 与三个宿主 Release build 均通过；两个仓库 diff --check 通过。临时宿主、数据库、缓存、锁会话与配置/日志目录已清理。只显式启用已有 tokio-util 的 rt 特性，没有引入新外部包，没有新增或运行测试，没有安装本机组件；dist 和既有运行镜像未重建。

## HTTP/2 正文存在状态的入口快照

2026-11-08 继续对照同一源 HEAD 23a7d0002938d0308c975dc48c04ac66aae0dda8。源 [Kestrel BodyControl.Start](https://raw.githubusercontent.com/dotnet/aspnetcore/release/10.0/src/Servers/Kestrel/Core/src/Internal/Infrastructure/BodyControl.cs) 在应用处理前保存 CanHaveBody；[Http2Stream](https://raw.githubusercontent.com/dotnet/aspnetcore/release/10.0/src/Servers/Kestrel/Core/src/Internal/Http2/Http2Stream.cs) 依据当时的接收状态选择正文实现。这不是仅保存原始 HEADERS 的 END_STREAM 位。Rust 原来在鉴权后的方法绑定阶段才创建 BodyPresence，现提前到共享请求中间件入口，在任务调度和内层鉴权等待前保存；绑定器和 JSON 读取复用已有快照，单独调用绑定器仍可补充快照。

本地 Hyper/h2 实现的 is_end_stream 同时考虑接收状态与待取出的帧队列，空 DATA 也会入队。因此本轮没有宣称仅空 DATA/END_STREAM 提前到达就已复现缺失正文误判；本次修正依据入口快照的生命周期差异，尚需真实 HTTP/2 请求验证。没有更改鉴权顺序、媒体类型规则、正文解析或单流取消令牌。

已在 SDK 容器中成功发布原 NetEngine 管理端 Release，并准备隔离数据库、缓存、HTTPS 配置和静态文件。启动 Rust HTTP/2 宿主并请求健康端点的组合命令被自动审批以 blocked by policy 拦截，未提供更具体原因；没有执行或换方式重试。没有获得 HTTP/2 多路复用、单流取消或并发下载的运行证据，整体对齐目标仍未完成。

Rust 容器内格式检查、全工作区 all-features/locked check、严格 Clippy、完整 build 和三个宿主 Release build 均通过。隔离容器及临时配置、证书和文件已清理，仅保留开发容器；源项目仍只有此前五项授权变更。没有新增或运行测试，没有安装本机组件，dist 与既有运行镜像未重建。

## ProblemDetails 的扩展追踪标识

2026-11-08 重新确认源 HEAD 23a7d0002938d0308c975dc48c04ac66aae0dda8，源 WebAPI 使用 net10.0，未发现覆盖默认 Activity/传播器的配置。沿 [DistributedContextPropagator](https://raw.githubusercontent.com/dotnet/runtime/release/10.0/src/libraries/System.Diagnostics.DiagnosticSource/src/System/Diagnostics/DistributedContextPropagator.cs)、[W3CPropagator](https://raw.githubusercontent.com/dotnet/runtime/release/10.0/src/libraries/System.Diagnostics.DiagnosticSource/src/System/Diagnostics/W3CPropagator.cs) 和 [ActivityCreator](https://raw.githubusercontent.com/dotnet/aspnetcore/release/10.0/src/Shared/Diagnostics/ActivityCreator.cs) 核对：默认传播器忽略旧 Request-Id，接受合法较新版本的扩展 traceparent；Activity 对此使用层级格式。此前已记录的扩展父标识差异尚未实现，本轮补齐。

共享 problem_details 保持普通 W3C ID 与低七位 flags 规则，扩展父标识生成层级子 ID。[Activity](https://raw.githubusercontent.com/dotnet/runtime/release/10.0/src/libraries/System.Diagnostics.DiagnosticSource/src/System/Diagnostics/Activity.cs) 的长度上限、层级边界截断、溢出标记及无边界时的新根形式已对应实现。重复请求头按源 HostingApplicationDiagnostics 的 StringValues 转字符串行为合并，跳过空值，合并后再校验；不把重复头一律丢弃，也不添加 Request-Id 回退。实现仅服务现有 415 ProblemDetails，不代表日志、对外调用、baggage 或请求级 Activity 传播已经完整对齐。

本轮证据为当前生产代码、官方框架源码及容器编译检查；未启动 HTTP 宿主，未新增验证端点、辅助程序或测试代码，未运行测试。上一轮运行命令的自动审批拦截没有重试；本轮扩展、多值及超长标识仍缺少两端实际请求对照，整体对齐目标未完成。源项目没有新增修改，没有安装本机组件，没有新增依赖或临时运行容器，dist 与运行镜像未重建。

最终容器格式检查、全工作区 all-features/locked check、严格 Clippy、完整 build 和三个宿主 Release build 均通过；两个仓库 diff --check 通过。既有普通追踪标识的历史运行结果未作为本轮新增分支的运行证据。

## 响应字符集与客户端公钥接口

2026-11-08 对照源 HEAD 23a7d0002938d0308c975dc48c04ac66aae0dda8。源 AddMvc/AddControllers 沿用默认输出格式化器；[TextOutputFormatter](https://raw.githubusercontent.com/dotnet/aspnetcore/release/10.0/src/Mvc/Mvc.Core/src/Formatters/TextOutputFormatter.cs) 按 Accept-Charset 选择编码，[SystemTextJsonOutputFormatter](https://raw.githubusercontent.com/dotnet/aspnetcore/release/10.0/src/Mvc/Mvc.Core/src/Formatters/SystemTextJsonOutputFormatter.cs) 和 StringOutputFormatter 支持 UTF-8 与 UTF-16。Rust 原先固定 UTF-8，本轮增加共享字符集选择与按帧 UTF-16 小端转换，不写 BOM，不预先收集 JSON 数组流。跨帧未完成 UTF-8 字节单独保留，流错误继续终止响应，尾部字段透传；删除转换前的 Content-Length，之后仍由原压缩和生命周期包装器处理。

对象、415 ProblemDetails、JsonStream 显式标记为文本格式化结果。控制器字符串结果通过 Text 或已有文本响应适配声明，覆盖两端登录、文件 URL、客户端支付、Demo 和 UEditor。缓存命中仍从对象 JSON 重新格式化，ETag 继续按转换前对象计算；不向业务错误、健康检查、文件、静态内容或任意二进制响应推广。字符集名称、质量排序与通配符顺序对照源格式化器，包括无显式非零质量时不进入排序的原有分支；复用既有质量参数解析，不添加编码包。

逐项核对源字符串 Action 时发现客户端 AuthorizeController.GetPublicKey 尚无 Rust 路由。本轮在授权服务契约和实现增加 get_public_key，由 webapi_support 注入可选 RSA 公钥及配置存在状态，客户端新增匿名 GET /Authorize/GetPublicKey 和 OpenAPI。未配置 RSA 返回业务错误 RSA公钥加载异常；已配置但缺少公钥返回 204；空公钥字符串仍为 200 空文本。只返回配置中的公钥，不转换格式、不回退 JWT 公钥或从私钥推导；没有给现有登录模型自动添加解密。

本轮未启动实际宿主，没有重试此前被自动审批拦截的运行命令。字符集、跨帧转码和公钥端点证据为生产代码核对与编译，尚无新增路径的两端 HTTP 运行对照。完整 Accept 媒体类型选择、所有畸形协商头以及更广泛的功能与生命周期对齐仍未完成；没有把本轮编码处理视为完整内容协商。源项目无新增修改，没有新增依赖、测试代码、测试运行、本机组件或临时容器，dist 和既有运行镜像未重建。

最终容器格式检查、全工作区 all-features/locked check、严格 Clippy、完整 build 与三个宿主 Release build 均通过；两个仓库 diff --check 通过。开发容器继续保留，其他容器未启动。

## 两端路由清单与权限同步复核

2026-11-08 对照源 HEAD 23a7d0002938d0308c975dc48c04ac66aae0dda8，重新读取两端 16 个 Controller、方法上的 HTTP 声明、控制器/方法的 Authorize 与 AllowAnonymous、XML summary，以及 Rust Feature 的注册、守卫与权限元数据。没有发现未声明 HTTP 方法的公开 Action。修正前管理端 71 个方法/路径对应，客户端源 42 个而 Rust 41 个，遗漏 GET /Authorize/UpdateRoute；本轮补齐客户端入口及 OpenAPI。两端沿用源控制器的匿名调用声明，成功统一返回 200 空正文，修正管理端原先返回 JSON true 的差异。

源同步只收录需要鉴权且未标记匿名的 Action。Rust 权限清单误收录管理端 UpdateRoute、GetFile、GetImage，以及客户端七个支付创建入口；现移除这些同步元数据。实际请求守卫此前已按源声明装配，本轮没有更改这些入口的守卫或签名规则。另将管理端 27 项、客户端 9 项同步备注改为源方法 summary 的文本，避免把本地概括写入权限表。

修正后静态清单比较：管理端 71 个、客户端 42 个方法/路径均对应；鉴权路径及同步清单分别为管理端 59 项、客户端 9 项，守卫、权限项与备注比较均无差异。比较将路径占位符名称统一以检查路由形状，不据此宣称参数命名、绑定、所有动态路由行为或请求运行结果已经一致。源与 Rust 的默认 Feature 装配均已核对，本轮清单检查未写入测试或验证程序文件。

应用层权限同步改为 find_active 读取、在一个事务中以 xmin 条件完成新增、更新和软删除；参照源 UpdateRoutesAsync 的一次 SaveChanges，其[默认写入事务语义](https://learn.microsoft.com/en-us/ef/core/saving/transactions)要求整批成功或回滚。空清单不再直接跳过清理；已删除路径重新出现时新增无功能绑定的记录，不恢复旧记录及其绑定。源方法调用普通 SaveChanges 而非 SaveChangesWithUpdateLog，本轮删除 Rust 额外的 DataUpdateLog 写入。有效记录只在备注变化时更新，其他模块不参与。

本轮没有调用同步端点或修改运行数据库，没有重试此前被自动审批拦截的宿主启动命令。事务失败、并发版本冲突、软删除重建、空清单和空响应仍需实际两端运行验证；整体功能与架构对齐目标继续保持未完成。NetEngine 原工作区仍仅有此前五处授权修改；没有新增依赖、测试代码、测试运行、本机组件或临时容器，dist 与既有运行镜像未重建。

最终容器格式检查、全工作区 all-features/locked check、严格 Clippy、完整 build 与三个宿主 Release build 均通过；两个仓库 diff --check 通过。

## 响应媒体类型选择

2026-11-08 继续对照相同源 HEAD，源仓库仍只有云上传两处、UTC 查询三处授权修改。补充共享 response_format：普通对象与异步数组支持 application/json、text/json；显式字符串默认 text/plain，选择 JSON 时重新序列化为带引号及转义的字符串。ProblemDetails 保留服务端声明的 application/problem+json。文件、静态资源、健康端点及直接 JsonResult 错误不参与；空对象与 304 不新增正文。格式选择位于缓存和 ETag 之后、字符集转换与压缩之前，异步数组保持原有流，只收集已经声明为完整字符串的正文。

规则依据 ASP.NET 10 的 [DefaultOutputFormatterSelector](https://raw.githubusercontent.com/dotnet/aspnetcore/release/10.0/src/Mvc/Mvc.Core/src/Infrastructure/DefaultOutputFormatterSelector.cs) 与 [OutputFormatter](https://raw.githubusercontent.com/dotnet/aspnetcore/release/10.0/src/Mvc/Mvc.Core/src/Formatters/OutputFormatter.cs)：源默认忽略含有效 */* 的整个 Accept，无法匹配时回退而非 406，不回显任意客户端 vendor+json。按质量降序保留 q=0，匹配不额外按特异度排序。参数读取参照 [AcceptHeaderParser](https://raw.githubusercontent.com/dotnet/aspnetcore/release/10.0/src/Mvc/Mvc.Core/src/Formatters/AcceptHeaderParser.cs) 和 [MediaType](https://raw.githubusercontent.com/dotnet/aspnetcore/release/10.0/src/Mvc/Mvc.Core/src/Formatters/MediaType.cs)，只让首个 q 之前的必需参数影响匹配，重复 q 取最后值；带引号参数中的逗号不直接切开。普通十进制质量及特殊浮点符号不另行限制到 0～1。

同质量条目的次序影响最终格式选择，因此实现源 [ArraySortHelper](https://raw.githubusercontent.com/dotnet/runtime/release/10.0/src/libraries/System.Private.CoreLib/src/System/Collections/Generic/ArraySortHelper.cs) 的小数组、分区及堆排序分支，避免将超过 16 项的源排序误作稳定排序。没有新增测试或依赖，没有在本机安装组件。本轮未启动服务、未重试此前被自动审批拦截的启动动作，也未重建 dist 或运行镜像。媒体协商与字符集组合、异常头恢复、长列表排序和流式结果仍待两端实际 HTTP 验证，不能据编译成功宣称整体对齐完成。

本轮容器格式检查、全工作区 all-features/locked check、严格 Clippy、完整 build 与三个宿主 Release build 均通过，两个仓库 diff --check 通过。仅 dev 容器运行。

## 字符串对象快照与 ETag 内部序列化

2026-11-08 对照当前源 ETagFilter、CacheDataFilter.CachedResponse.ToObjectResult 和 Common.JsonHelper：ETag 使用内部 JsonHelper.ObjectToJson 的摘要，而 Rust 此前使用 HTTP JSON；字符串缓存恢复时源实现将字符串 JsonElement 解包为 string，Rust 此前一律恢复 JSON 对象。源 HEAD 未变，本轮未修改 NetEngine。

ObjectResponse 现在将内部 JSON 快照同时提供给缓存和 ETag。两个过滤器分别用任务作用域声明需求，以免嵌套的禁用缓存分支关闭 ETag 的快照收集；没有这些过滤器时不额外执行内部序列化。ETag 使用内部日期和 long 规则形成摘要，序列化失败进入共享内部异常响应，不回退 HTTP JSON。缓存仍在内部序列化失败时保留业务响应并跳过写入。

Text 现在保留对象快照，缓存命中后字符串恢复为 Text 并保留原状态，再由 Accept 选择纯文本或 JSON 字符串。普通字符串不再在格式选择阶段重读正文；UEditor 的已有响应通过异步 from_response 在过滤器处理结果之前读取完整文本、替换对象快照。JSON 流仍不预先收集，也不加入对象缓存和 ETag；null、文件及直接业务错误的边界保持原样。更新 AGENTS 与请求绑定文档中的约定。

当前源业务路由没有声明 ETag，本轮没有为验证新增接口、测试代码、测试依赖或运行测试，也未启动临时宿主或重试被拦截的动作。结论来自源实现与生产代码核对；日期摘要、字符串缓存状态及格式选择的组合尚无本轮两端实际 HTTP 证据，不能作为整体对齐完成的证明。

容器内格式检查、全工作区 all-features/locked check、严格 Clippy、完整 build 及三个宿主 Release build 均通过；两个仓库 diff --check 通过。开发容器继续保留，没有新增本机组件。

## 控制器长整数与流式 JSON 序列化

2026-11-08 核对源 WebApplicationBuilderExtension 的 AddJsonOptions、Common.LongConverter，以及 FileInfoDto.Length、AppSetting.GroupId 的声明。源控制器全局将 long 写成字符串，Rust 的共享 Json 与 JsonStream 此前直接使用 serde_json，FileInfoDto.length 等仍为数字。已有 ID 字段转为 String 只能覆盖部分模型，不能替代宿主的统一规则。

新增 common::json_helper::write_controller_json，复用现有按原始 Serde 类型递归的序列化器；普通对象和异步数组逐项统一调用它。i64 和可空、集合、嵌套 i64 写为字符串，i32、u64 继续为数字；动态 Value、Number、RawValue 保留文档自身类型，字典键保持目标序列化器行为。日期使用 ISO 分支，未换成缓存/ETag 的内部日期格式。非有限浮点数拒绝序列化，普通对象进入共享内部异常响应，流式数组在失败项终止且不伪造结束括号。

文件大小、应用配置分组 ID、任务执行日志任务 ID 的模型文档同步为字符串或可空字符串。检查当前两端 handler 中提前构造动态 JSON 的位置，仅发现 UEditor 的配置、状态及抓取列表；这些属于已有文本协议，保留其文档值，不批量改写数字。现有数字流声明为 i32，分页 Total 也为 i32，因此继续输出数字。事件使用的 Web JSON 助手、JWT 和第三方协议未接入控制器写入器。

本轮没有新增测试代码、测试依赖或运行测试，没有新增本机组件或启动临时宿主；长整数响应、浮点失败、日期和异步输出的组合仍需实际两端 HTTP 验证。只依据代码核对与编译不能宣称所有序列化边界及整体对齐已经完成。

最终容器格式检查、全工作区 all-features/locked check、严格 Clippy、完整 build 以及三个宿主 Release build 均通过；两个仓库 diff --check 通过。源 HEAD 仍为 23a7d0002938d0308c975dc48c04ac66aae0dda8，仅有此前五处授权修改，只有 dev 容器运行。

## 日期响应精度与 DTO 日期标记

2026-11-08 核对源 DateTimeOffset DTO 声明、Common.DateTimeOffsetConverter，以及 System.Text.Json 的 [JsonWriterHelper.Date](https://raw.githubusercontent.com/dotnet/runtime/release/10.0/src/libraries/System.Text.Json/src/System/Text/Json/Writer/JsonWriterHelper.Date.cs)。源 HTTP 日期最多七位小数秒并去掉尾零，而 Chrono 的默认 RFC3339 常按三、六或九位保留小数。共享日期输出现按百纳秒截取并去尾零，保留显式偏移，零偏移写 +00:00；无小数时省略小数点。拒绝超出 DateTimeOffset 可表示范围、秒级偏移和闰秒的输出，不将其伪装为合法源日期。

UserDto 原本已保留日期类型，其输出随共享实现修正。另为六个模型文件的十九个 String/Option<String> 字段增加显式日期序列化标记，覆盖源 LogDto、RoleDto、LinkDto、CategoryDto、ArticleDto、LlmModelDto、LlmAppDto、LlmConversationDto、TaskSettingDto 与 QueueTaskDto。没有改输入绑定、业务模型字段类型或按内容扫描任意字符串；可空日期仍保持 null，第三方支付协议中的时间字符串不参与。

日期标记让内部 JSON 快照重新识别这些 DTO 字段，缓存与 ETag 按源 yyyy/MM/dd HH:mm:ss zzz 格式处理。字符串载体可读回内部日期格式，避免已声明日期的模型反序列化后无法再输出。缓存命中的 RawValue 保留 JSON 文档文本，仍不重新推断日期，符合源 JsonElement 恢复路径；不能将缓存命中值统一改回 ISO。

本轮没有新增测试代码或运行测试，没有新增本机组件、修改 NetEngine 或启动临时宿主。源 HEAD 仍为 23a7d0002938d0308c975dc48c04ac66aae0dda8。日期精度与缓存/ETag 的组合尚无本轮两端实际 HTTP 对照，整体目标仍未完成。

最终容器格式检查、全工作区 all-features/locked check、严格 Clippy、完整 build 与三个宿主 Release build 均通过；两个仓库 diff --check 通过。新增日期字段标记共十九处，源仓库仍仅有此前五处授权修改，只有 dev 容器运行。

## 可序列化模型字段清单与顺序

2026-11-08 核对应用层枚举和对象字段：源 LlmChatRole 为 0～3，Rust AppLlmRole 已通过 i32 转换保持数值映射；LlmParameterComparison 不参与序列化，本轮没有更改枚举。应用模型中未发现 skip_serializing_if，源模型也未声明 JsonIgnore 或 JsonPropertyOrder；未因静态类型的可空注解差异推断实际查询一定返回非空。

对照 [System.Text.Json 属性顺序规则](https://learn.microsoft.com/en-us/dotnet/standard/serialization/system-text-json/customize-properties#configure-the-order-of-serialized-properties)，源 DTO 默认按声明顺序输出，而 ETagFilter 对 JSON 文本求摘要。修正 TaskSettingDto 的 Parameter 位置、QueueTaskDto 的 Status/WorkerId/Parameter/PlanTime 顺序、ArticleDto 的 Digest 位置、FileInfoDto 的 Sign/Path 顺序，以及 LlmConversationDto 中创建人/时间与消息内容的顺序。字段类型、名称、默认值、输入转换和日期标记随字段保留；没有全局排序 JSON 键。

只读清单核对覆盖二十四个两端同名且 Rust 声明 Serialize 的模型：SendSMSDto、UserRoleDto、UserFunctionDto、UserDto、SendEmailDto、RoleSelectDto、RoleFunctionDto、RoleDto、TaskSettingDto、PageListDto、QueueTaskDto、LogDto、RegionDto、CategoryDto、ArticleDto、CategorySelectDto、SiteDto、ServerInfoDto、FileInfoDto、LinkDto、LlmModelSelectDto、LlmModelDto、LlmConversationDto、LlmAppDto。将字段命名规范化后，声明字段清单及顺序无差异。这个范围不覆盖所有请求模型、记录类型、继承成员、命名策略、实际值和动态字典，也不能证明各模型的最终 JSON 或 ETag 逐字节一致；异常转义等边界仍需核验。

清单核对仅在内存读取现有源码，没有写入核对程序、测试代码或测试依赖。没有新增业务端点、启动临时宿主或重试此前被拦截的动作；本轮仍无新增 HTTP 对照证据，整体目标保持未完成。

容器格式检查、全工作区 all-features/locked check、严格 Clippy、完整 build 和三个宿主 Release build 均通过；两个仓库 diff --check 通过。源 HEAD 仍为 23a7d0002938d0308c975dc48c04ac66aae0dda8，仅保留此前五处授权修改，只有 dev 容器运行。

## JSON 字符串编码与原始文档写回

2026-11-08 核对 Common.JsonHelper 的 UnsafeRelaxedJsonEscaping，以及 EventPublisher 的 JsonSerializerDefaults.Web。源宽松编码仍限制部分 Unicode 字符，Rust serde_json 默认字符串输出不能直接替代。依据 [DefaultJavaScriptEncoder](https://raw.githubusercontent.com/dotnet/runtime/release/10.0/src/libraries/System.Text.Encodings.Web/src/System/Text/Encodings/Web/DefaultJavaScriptEncoder.cs) 与 [AllowedBmpCodePointsBitmap](https://raw.githubusercontent.com/dotnet/runtime/release/10.0/src/libraries/System.Text.Encodings.Web/src/System/Text/Encodings/Web/AllowedBmpCodePointsBitmap.cs) 增加共享默认/宽松编码器：控制字符使用对应短转义或大写十六进制，禁用 BMP 字符与补充平面字符按 UTF-16 单元转义；默认编码另外处理非 ASCII 和 HTML 敏感字符。

生产字符表从上游 UnicodeHelpers.generated.cs 的 8192 字节位图转换为 343 个连续禁用范围，保留 MIT 许可证、源 URL 及文件 SHA-256。位图 SHA-256 为 97392397b3101dfb6d6c3b25e1d26343b91808d7bbf6eb7ca5ad3d4d98af105f；未增加 Unicode 依赖，运行时不下载数据。内部 JSON、克隆 JSON 和控制器输出使用宽松编码，事件 Web JSON 使用默认编码；Text 改用控制器写入器，直接业务错误及 ProblemDetails 也接入对应编码，仍不额外加入对象缓存。

依据 [JsonDocument 写回实现](https://raw.githubusercontent.com/dotnet/runtime/release/10.0/src/libraries/System.Text.Json/src/System/Text/Json/Document/JsonDocument.cs)，RawValue 逐个重编码字符串并去除结构外空白，保留数字原文、重复键和字段顺序；缓存命中的文档也经过这个过程。没有先转成 Value 后丢弃重复键或数字词法。异步数组仍逐项写出，未预先收集完整响应流。

原生 [DateTimeOffset 写入器](https://raw.githubusercontent.com/dotnet/runtime/release/10.0/src/libraries/System.Text.Json/src/System/Text/Json/Writer/Utf8JsonWriter.WriteValues.DateTimeOffset.cs) 直接写已格式化 ISO 文本，不按普通字符串转义时区正号。共享序列化器通过每次调用独立的日期上下文保留这个区别，退出或异常展开时恢复状态；普通文本和 JsonElement 不享受该例外。日期类型仍使用明确标记，不靠文本猜测。

本轮没有新增测试代码、测试依赖或运行测试，没有安装本机组件、修改 NetEngine 或启动临时宿主。上述结论来自源代码核对和容器构建；特殊字符、缓存/ETag 与默认事件编码的实际两端运行对照仍待完成，不能宣称全部 JSON 字节行为或整体目标已完成。

容器格式检查、全工作区 all-features/locked check、严格 Clippy、完整 build 和三个宿主 Release build 均通过；Release 构建耗时 1 分 40 秒。两个仓库 diff --check 通过，NetEngine HEAD 仍为 23a7d0002938d0308c975dc48c04ac66aae0dda8，仅保留此前五处授权修改。发布包与运行镜像未重新生成。

## 原生浮点 JSON 格式

2026-11-08 对照源 Common.JsonHelper、MVC 和事件默认选项，确认均未替换 Single/Double 的默认输出转换器。源 [Double 写入器](https://raw.githubusercontent.com/dotnet/runtime/release/10.0/src/libraries/System.Text.Json/src/System/Text/Json/Writer/Utf8JsonWriter.WriteValues.Double.cs) 和 [Single 写入器](https://raw.githubusercontent.com/dotnet/runtime/release/10.0/src/libraries/System.Text.Json/src/System/Text/Json/Writer/Utf8JsonWriter.WriteValues.Float.cs) 交给 Utf8Formatter 的默认 G 格式。旧 Rust 共享写入器仍使用 serde_json 原生浮点格式，整数浮点值的小数后缀与科学计数法形式会产生不同的 JSON 字节。

依据 [最短可往返数字精度选择](https://raw.githubusercontent.com/dotnet/runtime/release/10.0/src/libraries/System.Private.CoreLib/src/System/Number.Formatting.cs) 及 [FormatGeneral](https://raw.githubusercontent.com/dotnet/runtime/release/10.0/src/libraries/Common/src/System/Number.Formatting.Common.cs)，在共享 JsonFormatter 增加 write_f32/write_f64：从现有 serde_json 浮点算法取得最短有效数字，仅重排小数点与指数，不重新解析为浮点或增加舍入。f32/f64 的科学计数法上界分别为十进制指数 9/17，下界均为 -4；指数使用大写 E、显式正负号及至少两位数字。去掉整数值末尾 .0，保留浮点负零 -0。此规则覆盖普通控制器、内部/克隆 JSON 与默认事件 JSON，不增加依赖或单独宿主分支。

确认当前锁定 serde_json 的 arbitrary_precision Number 通过特殊数字文本入口序列化，RawValue 通过原始文档入口写回，均不调用新的浮点格式化分支；原有数字文本不会被本次规则改写。非有限值仍由原始类型包装器拒绝，decimal 不使用原生二进制浮点规则。异步数组仍按项序列化，没有新增整个响应的内存收集。

本轮为源代码核对与生产实现修正，没有新增或运行测试，没有创建核对程序或启动临时宿主，没有安装本机组件。实际两端浮点响应、所有舍入临界值及缓存/ETag 组合尚未穷尽核验；不能据此声称所有数字 JSON 字节或整体框架已经完全对齐。NetEngine HEAD 仍为 23a7d0002938d0308c975dc48c04ac66aae0dda8，源工作区只有此前五处授权修改。

容器格式检查、全工作区 all-features/locked check、严格 Clippy、完整 build 及三个宿主 Release build 均通过；Release 构建耗时 1 分 36 秒。两个仓库 diff --check 通过。发布包及运行镜像未重新生成，整体目标保持未完成。

## JSON 容器深度与流式数组

2026-11-08 核对 NetEngine 的 JsonHelper、事件默认 Web 选项及 MVC 配置，未发现覆盖 MaxDepth 的设置。[JsonSerializerOptions](https://raw.githubusercontent.com/dotnet/runtime/release/10.0/src/libraries/System.Text.Json/src/System/Text/Json/Serialization/JsonSerializerOptions.cs) 的默认有效深度为 64，并传给 JSON 读写器。旧 Rust 只在默认 Web 协议读取后检查 64 层，内部/克隆读取和共享输出没有相同限制，HTTP 正文还沿用 serde_json 的递归阈值。

common 统一声明容器深度上限。共享输出在 begin_array/begin_object 进入下一层之前检查，结束时恢复；原始 RawValue 文档扫描结构括号并合并外层深度，完整跳过字符串及其转义，不会把文本括号计为容器。普通对象、内部快照、克隆、事件与缓存文档写回均使用该写入器。新增 write_controller_json_array_item，流式数组单项从深度 1 开始；仍逐项消费，失败终止流且不伪造结束括号，不收集完整异步数组。

内部和克隆读取在转换目标模型之前复用容器检查，保留字典键与重复字段的已有处理。HTTP 模型读取在对象/数组 token 边界计数；任意精度数字的 MapAccess 特殊协议不算对象。超深返回独立 JsonReadError::Depth，HTTP 层按 [源诊断资源](https://raw.githubusercontent.com/dotnet/runtime/release/10.0/src/libraries/System.Text.Json/src/Resources/Strings.resx) 组装绑定错误并返回 400。只在 token 读取已确认超深时回扫原始解析字节定位起始括号，不用预扫描抢先覆盖更早的模型形态错误；正文和签名读取保持原样。

本轮只对齐容器上限及这些入口的接线，没有新增测试代码、测试依赖或运行测试，没有启动临时宿主、安装本机组件或修改 NetEngine。输入诊断路径还需与动态文档、跳过未知字段的源读取栈逐项核验；源 [JsonConverter.TryWrite](https://raw.githubusercontent.com/dotnet/runtime/release/10.0/src/libraries/System.Text.Json/src/System/Text/Json/Serialization/JsonConverterOfT.cs) 的转换器分派在边界层可能先于底层容器检查，不能把当前写入器限制视为已证明所有第 64 层标量、自定义转换器及 null 行为完全相同。通用 ReferenceHandler.Preserve 的对象身份、引用及循环仍未完成，既有短信/邮件专用元数据不代表完整引用图支持。整体目标保持未完成。

NetEngine HEAD 仍为 23a7d0002938d0308c975dc48c04ac66aae0dda8，源工作区只有此前五处授权修改。

补充核对 [Utf8JsonReader.StartObject/StartArray](https://raw.githubusercontent.com/dotnet/runtime/release/10.0/src/libraries/System.Text.Json/src/System/Text/Json/Reader/Utf8JsonReader.cs)：源端在消费起始括号、增加字节位置之前检查深度，因此超深位置按该括号的零基字节偏移记录。

容器格式检查、全工作区 all-features/locked check、严格 Clippy、完整 build 和三个宿主 Release build 均通过；Release 构建耗时 1 分 37 秒。两个仓库 diff --check 通过。未重新生成发布包或运行镜像，未新增两端 HTTP 运行对照。

## 字典形态与动态文档错误路径

2026-11-08 继续核对 NetEngine TestLlmAppRequestDto：ExtraBody 是 Dictionary<string, JsonNode>，Parameters 是 Dictionary<string, string>；WebAPI.Core 目标仍为 net10.0。旧 JsonModel 把两者都归为任意值，导致错误 token 在模型转换后才报出，也无法区分字典键与动态文档内部结构。

JsonModel 派生宏增加 String 键字典识别和 Dictionary 元数据，共享读取器按声明检查字典形态及字符串/布尔/嵌套模型元素，沿用已有转换器完成实际值转换。类型名称复用源 Dictionary 泛型格式；ExtraBody 通过 dictionary_value_name 明确源元素是 JsonNode。支持 String 键的 HashMap、BTreeMap、IndexMap、Map，非 String 键给出编译诊断，不默默推断错误协议。嵌套字典模型的验证按键进入值对象，未修改宿主专用逻辑或字段实际键值。

依据 [ReadStack.JsonPath](https://raw.githubusercontent.com/dotnet/runtime/release/10.0/src/libraries/System.Text.Json/src/System/Text/Json/Serialization/ReadStack.cs) 与 [JsonReaderHelper 的特殊字符清单](https://raw.githubusercontent.com/dotnet/runtime/release/10.0/src/libraries/System.Text.Json/src/System/Text/Json/Reader/JsonReaderHelper.cs)，属性路径遇到指定字符使用方括号形式，保留原名且不额外转义；普通属性仍用点号。动态文档和跳过的未知字段不为内部节点增加读取栈路径，字典则在每个键处保留路径段，类型化集合保留索引。JSON 语法及容器深度仍持续检查，未跳过文档内容验证。

本轮通过现有生产模型的工作区编译验证宏与运行时协同，没有创建验证程序、测试代码或测试依赖，没有运行测试或启动临时宿主；未安装本机组件，未修改源 NetEngine。两端实际诊断原文、全部字典元素类型和读取栈延续分支仍待运行对照，不能将声明支持或编译成功视为全部边界行为一致。此前通用克隆引用图及第 64 层转换器分派等未完成项保持未完成，整体目标继续进行。

源 HEAD 为 23a7d0002938d0308c975dc48c04ac66aae0dda8，工作区仍只有此前五处授权修改。

容器格式检查、全工作区 all-features/locked check、严格 Clippy、完整 build 及三个宿主 Release build 均通过；过程宏与运行时辅助 crate 同时经过这些检查，Release 构建耗时 1 分 49 秒。两个仓库 diff --check 通过。发布包与运行镜像未重新生成，本轮没有新增两端 HTTP 对照证据。

## 微信用户信息读取与待确认的源查询修正

2026-11-08 重新读取 NetEngine AuthorizeService、GetWeiXinAppUserInfoDto 和 Common.JsonHelper。源用户信息请求使用 JsonHelper.JsonToObject，Rust 仍直接使用 serde_json::from_str，未套用内部 JSON 的大小写不敏感字段匹配。Rust get_wx_app_user_info 已改用 common::json_helper::json_to_object；保持字段原值、缺失 Sex 的零默认值及内部错误处理，不把 HTTP 入参空白转 null 的规则带入第三方用户信息。

源 Privilege 声明为可空 string[]，元素 null 并不会因 C# 引用类型注解自动拒绝；[System.Text.Json 可空注解限制](https://learn.microsoft.com/en-us/dotnet/standard/serialization/system-text-json/nullable-annotations) 也明确区分集合元素。Rust 模型由 Option<Vec<String>> 改为 Option<Vec<Option<String>>>，保留数组为空、空数组和数组中空元素三种状态。该字段只由用户信息 DTO 持有，当前登录流程不遍历它；不再因未使用的空元素拒绝整份登录资料。昵称仍只在 null 时拒绝授权，空字符串和空白昵称沿用现有行为。

源小程序登录查询在 Application/Application.Service/AuthorizeService.cs:87 仍使用 t.OpenId == login.AppId，但创建绑定时写入 openId。Rust 在加锁前及锁内复查均按微信返回的 openId 查询。已向用户提出具体源修正：仅把该查询条件改为 t.OpenId == openId，保留 AppName/AppId 过滤、锁内复查及注册事务。本轮尚未收到该项选择，未修改 NetEngine，也未把疑似缺陷复制到 Rust。

本轮未启动临时宿主、调用微信、使用真实授权码或修改数据库，没有新增测试代码、测试依赖或运行测试，没有安装本机组件。合法响应、异常重复字段和全部第三方载荷组合的两端运行对照仍未完成；源查询差异也仍待确认，整体目标保持未完成。NetEngine HEAD 为 23a7d0002938d0308c975dc48c04ac66aae0dda8，源工作区仍只有此前五处授权修改。

容器格式检查、全工作区 all-features/locked check、严格 Clippy、完整 build 及三个宿主 Release build 均通过；Release 构建耗时 1 分 24 秒。两个仓库 diff --check 通过。发布包与运行镜像未重新生成。

## 日志标准拉取指标

2026-11-08 重新读取 NetEngine Logger.LocalFile 与 Logger.Database 的 LogDiagnostics，确认源组件用独立 Meter 暴露 ObservableCounter/ObservableGauge，而 Rust 只提供 diagnostics::snapshot。两个 Rust 组件在首次成功发布诊断状态时注册 OpenTelemetry 拉取式指标，沿用 Logger.LocalFile/Logger.Database meter 名称、指标名称、单位及 level/operation 标签。业务日志路径仍只更新原子计数，采集回调只读取当前状态；数据库积压沿用后台刷新结果，回调不扫描磁盘、不查询数据库、不输出日志。

依照 [OpenTelemetry Meter API](https://docs.rs/opentelemetry/0.33.1/opentelemetry/metrics/struct.Meter.html) 接入现有计数，两个组件仅新增 opentelemetry 0.33.1 的 metrics 特性。容器解析锁文件只新增一个包；该配置的 cargo tree 没有额外依赖分支。级别标签使用 Information，并保留源 None 零值系列；失败阶段复用组件已有操作清单。注册发生在正常日志初始化后，不在每次日志调用或采集时重复创建指标。

宿主监控集成通过 [set_meter_provider](https://docs.rs/opentelemetry/0.33.1/opentelemetry/global/fn.set_meter_provider.html) 在初始化日志前安装 provider；组件不替宿主配置全局 provider、SDK 或 exporter。未配置采集端时保持无导出及既有快照读取行为。两个日志使用文档补充指标清单、标签、初始化顺序及先排空日志再关闭采集端的生命周期；没有增加公开 HTTP 监控端点。

只读声明比对覆盖本地文件四项、数据库七项指标，名称/Counter或Gauge类型/单位没有差异。该比对只读取已有源码，没有写入核对程序或测试代码。它不证明真实 SDK 采集、时间窗口、导出格式、并发计数及全部启动失败生命周期已经一致；这些运行验证仍待完成。本轮没有运行测试、启动临时宿主或安装本机组件，发布包与运行镜像未更新。

NetEngine HEAD 仍为 23a7d0002938d0308c975dc48c04ac66aae0dda8，源工作区仍只有此前五处授权修改。微信小程序 OpenId 源查询修正继续等待用户选择，未擅自修改。整体框架目标保持未完成。

容器格式检查、全工作区 all-features/locked check、严格 Clippy、完整 build 及三个宿主 Release build 均通过；Release 构建耗时 1 分 13 秒。两个仓库 diff --check 通过，没有添加或运行测试。

## 日志级别配置的启动校验

2026-11-08 继续对照 NetEngine 的日志注册及 .NET 10 日志配置实现。[LoggerFilterConfigureOptions](https://raw.githubusercontent.com/dotnet/runtime/v10.0.0/src/libraries/Microsoft.Extensions.Logging.Configuration/src/LoggerFilterConfigureOptions.cs) 对空字符串不创建规则，对非法枚举值抛出异常；Rust 原实现打印提示后忽略非法值，可能使提供者按其他默认级别继续接收日志。

现由 logger::LoggerSettings::validate_log_levels 在两种提供者创建写入任务前检查全部级别，数据库提供者也在启动分区维护前检查。共享 subscriber 注册返回配置或注册错误，过滤器复用同一个解析函数；不再把非法级别静默忽略。保持现有 info/warn/off 简写，名称忽略 ASCII 大小写；空字符串跳过，纯空白、未知名称、越界整数及非法组合拒绝。[Enum.TryParse](https://learn.microsoft.com/en-us/dotnet/api/system.enum.tryparse?view=net-10.0) 接受的 Int32 数字字符串及逗号名称按位或组合也纳入解析，最低级别比较使用有符号数，不把负数转换为无符号数导致全部过滤。常规事件与 Critical 的六级编号和快照索引保持原有类型。

复核两种 writer、日志运行时和分区维护守卫的 Drop，现有实现已经取消或中止所拥有的后台任务；本轮没有据此假设存在额外泄漏，也没有启动故障注入宿主。尚未通过两端实际启动逐项比较所有非法枚举边界。另从 LoggerRuleSelector 确认类别规则的 OrdinalIgnoreCase、单通配符及等长匹配顺序仍需继续对齐，不能将此次级别解析修正视为整个日志配置能力一致。

本轮没有修改 NetEngine、增加测试代码或依赖、运行测试、安装本机组件；没有生成新的发布包或运行镜像。源仓库保持此前五处授权修改。整体框架对齐目标仍未完成。

容器格式检查、全工作区 all-features/locked check、严格 Clippy、完整 build 和三个宿主 Release build 均通过；Release 构建耗时 1 分 14 秒。两个仓库 diff --check 通过。没有以编译结果替代两端启动和过滤行为的运行对照。

## 日志类别匹配及跨配置源覆盖

2026-11-08 继续核对 NetEngine 宿主的日志注册，源 HEAD 仍为 23a7d0002938d0308c975dc48c04ac66aae0dda8。[LoggerRuleSelector](https://raw.githubusercontent.com/dotnet/runtime/v10.0.0/src/libraries/Microsoft.Extensions.Logging/src/LoggerRuleSelector.cs) 使用 OrdinalIgnoreCase 前缀/后缀匹配，最多一个通配符，按类别 UTF-16 长度选择更具体规则；同长规则取配置枚举的最后匹配项。Rust 原实现裁剪类别、按 UTF-8 字节长度排序并区分大小写匹配前缀，等长规则受 HashMap 随机顺序影响。

新增 logger::filter 的编译规则：保留类别空白与分隔符，无通配符时匹配前缀，有一个通配符时分别匹配前后缀，包括两端重叠的情况；有效多通配符规则在后台任务启动前报错。Unicode 一对一大写映射使用工作区已有 icu_casemap，并与 common::string_helper 保持无点 i、长 s 的特殊处理；没有让基础日志契约依赖 common 的图像和表格工具。最长规则按 UTF-16 长度（包含星号）选择，default 只作无匹配时的回退。

依据 [ConfigurationExtensions.AsEnumerable](https://raw.githubusercontent.com/dotnet/runtime/v10.0.0/src/libraries/Microsoft.Extensions.Configuration.Abstractions/src/ConfigurationExtensions.cs)、[ConfigurationProvider](https://raw.githubusercontent.com/dotnet/runtime/v10.0.0/src/libraries/Microsoft.Extensions.Configuration/src/ConfigurationProvider.cs) 和 [ConfigurationKeyComparer](https://raw.githubusercontent.com/dotnet/runtime/v10.0.0/src/libraries/Microsoft.Extensions.Configuration/src/ConfigurationKeyComparer.cs)，等长规则按配置层级子键顺序选择，不使用 TOML 插入顺序。子键区分数字与文本，文本按统一大小写后的 UTF-16 比较，连续冒号的空层级保留。

同时新增 logger::log_filter_source，由 WebAPI 和任务宿主包装基础文件、环境文件及环境变量，在每个来源合并前统一 logger.log_level 下的类别键。这样不同来源的大小写变体正确按后者覆盖，不能等合并进 HashMap 后再猜测来源顺序。同一来源中的大小写重复项返回配置错误，依据 [JsonConfigurationFileParser](https://raw.githubusercontent.com/dotnet/runtime/v10.0.0/src/libraries/Microsoft.Extensions.Configuration.Json/src/JsonConfigurationFileParser.cs) 的重复键规则；其他配置字段、值和原有加载顺序保留。提供者校验入口改为 validate_log_filter，共享过滤器仍使用同一编译过程。

Cargo 只为 logger 增加对已有 config/icu_casemap 的依赖边，离线解析没有下载或新增包；没有添加测试、验证程序或运行测试，没有安装本机组件、修改 NetEngine 或启动临时宿主。本轮证据为源代码核对及生产工作区编译，尚未完成两端运行对照、全部 Unicode/数字排序边界、提供者专属过滤和配置热更新；整体框架目标保持未完成。源仓库仍只有此前五处获准修改，微信小程序源查询的额外修改仍待用户答复。

容器格式检查、全工作区 all-features/locked check、严格 Clippy、完整 build 以及三个宿主 Release build 全部通过；Release 构建耗时 1 分 14 秒。两仓库 diff --check 通过。发布包与运行镜像没有重新生成，本轮没有新增实际日志过滤或配置覆盖的运行对照证据。

## 提供者独立日志过滤与环境配置

2026-11-08 核对 NetEngine 两种存储提供者及 .NET ConsoleLoggerProvider。文件和数据库提供者没有 ProviderAlias，类型名分别为 Logger.LocalFile.LocalFileLoggerProvider、Logger.Database.DatabaseLoggerProvider；[ConsoleLoggerProvider](https://raw.githubusercontent.com/dotnet/runtime/v10.0.0/src/libraries/Microsoft.Extensions.Logging.Console/src/ConsoleLoggerProvider.cs) 同时接受完整类型名和 Console 别名。此前 Rust 在整个 subscriber 前放置一个全局过滤器，无法单独调整控制台与存储级别。

新增 logger.providers 的可选提供者级别配置，沿用源类型名及控制台别名。ProviderFilter 分别编译全局和适用提供者规则，提供者专属类别或 default 一旦适用就优先于任何全局类别规则；没有适用的专属规则才回退全局。控制台完整类型名和别名同时存在时，按配置节枚举顺序及类别长度选择，不给别名另加优先级。未选中的存储不会被创建，未知提供者的级别仍校验，但不尝试匹配其通配符。两种初始化入口显式传入实际提供者身份，避免依赖可能与调用入口不符的 component 字段。

依照 [tracing-subscriber 分层过滤](https://docs.rs/tracing-subscriber/0.3.23/tracing_subscriber/layer/index.html#per-layer-filtering)，控制台 fmt 层与存储 LoggerLayer 各自挂 with_filter，移除会提前截断两者事件的全局过滤层。两层仍接受全部 span，保留请求/任务范围字段；Critical 继续使用专用字段识别，不与普通 Error 合并。配置未声明 providers 时回退原有全局行为。

复核容器中的 config 0.15.27 环境实现，发现其无条件将键名小写并将点号解析为层级，这会使 Console/full provider 名称失配，也会破坏字面 http.request 类别。新增 logger::log_filter_environment，两个共享宿主均改用此入口：日志环境键直接装配表结构，保留提供者名称、字面点号和类别，类别中的双下划线按[源环境提供者](https://raw.githubusercontent.com/dotnet/runtime/v10.0.0/src/libraries/Microsoft.Extensions.Configuration.EnvironmentVariables/src/EnvironmentVariablesConfigurationProvider.cs) 映射为冒号。环境类别大小写重复按枚举顺序覆盖，文件仍拒绝重复。其余 APP 环境项交回原 Environment 实现；不输出环境值或连接凭据。

文件配置归一化同时覆盖全局和各提供者 log_level，保留基础文件、环境文件、环境变量的加载顺序。使用文档补充提供者名称、优先级及环境变量示例。尚未完成两端实际日志接收/拒绝、所有提供者节大小写组合、环境覆盖边界及配置热更新的运行对照，不能凭编译认定整体日志行为等价。

本轮没有修改 NetEngine、引入新依赖、添加或运行测试、安装本机组件、启动临时宿主或重生成发布镜像。源仓库仍为此前五处授权修改，框架对齐目标保持未完成。

容器格式检查、全工作区 all-features/locked check、严格 Clippy、完整 build 和三个宿主 Release build 均通过；Release 构建耗时 1 分 17 秒。两个仓库 diff --check 通过，只保留开发容器运行。没有用编译结果替代真实双提供者输出和环境覆盖的运行验证。

## 日志过滤运行时替换入口

2026-11-08 继续核对源宿主，管理端与客户端使用 WebApplication.CreateBuilder，任务宿主使用 Host.CreateDefaultBuilder，未找到覆盖文件重载行为的注册。[LoggerFactory](https://raw.githubusercontent.com/dotnet/runtime/v10.0.0/src/libraries/Microsoft.Extensions.Logging/src/LoggerFactory.cs) 订阅过滤选项变化，并更新已创建 logger 的规则；Rust 先前只在注册 subscriber 时编译固定过滤器。

新增 LoggerRuntime::reload_filter 和由运行时持有的 LoggerFilterHandle。两种提供者在注册 subscriber 后将更新入口交给对应 LoggerRuntime；更新只读取全局及提供者过滤配置，始终使用初始化时的真实提供者身份，不重建连接、队列或文件存储。全部规则编译成功后，通过工作区已有 arc-swap 整体发布包含控制台和存储规则的不可变快照，错误返回调用者并保留旧快照；shutdown 开始时移除更新入口。

两层 Filter 显式使用 Interest::sometimes，不缓存某个调用点此前是否被拒绝；最大级别不作固定提示，允许降低阈值后已有调用点重新参与过滤。事件读取当前快照，span 始终保留上下文。类别匹配改用一对一大写字符迭代，保留独立前缀/后缀及重叠语义，避免动态检查时为每条事件分配类别副本。已核对容器 tracing-subscriber 0.3.23 的 Filter::callsite_enabled 契约。

[FileConfigurationProvider](https://raw.githubusercontent.com/dotnet/runtime/v10.0.0/src/libraries/Microsoft.Extensions.Configuration.FileExtensions/src/FileConfigurationProvider.cs) 在重载中对缺失文件清空该来源，解析失败清空后传播错误；[PhysicalFilesWatcher](https://raw.githubusercontent.com/dotnet/runtime/v10.0.0/src/libraries/Microsoft.Extensions.FileProviders.Physical/src/PhysicalFilesWatcher.cs) 的通知通过后台 Task 或捕获异常的轮询回调执行。因此不能仅凭配置解析抛异常推断宿主会立即退出，也不能把本轮更新 API 的失败保留策略等同于完整文件来源重载语义。文件监听、各来源缓存及删除/坏文件恢复行为仍需继续实现和运行对照，本轮没有安装监听依赖或接入自动监听。

Cargo 仅增加 logger 到已有 arc-swap 1.9.3 的依赖边，离线解析没有新增包。没有新增测试、验证程序、测试依赖或运行测试，没有启动临时宿主、修改 NetEngine、安装本机组件或生成新发布镜像。源 HEAD 仍为 23a7d0002938d0308c975dc48c04ac66aae0dda8，源仓库仍只有五处获准修改。此次实现是文件热更新所需的运行时入口，不能视为自动热更新完成或整个框架目标完成。

补充读取 .NET 10 [HostingHostBuilderExtensions](https://raw.githubusercontent.com/dotnet/runtime/v10.0.0/src/libraries/Microsoft.Extensions.Hosting/src/HostingHostBuilderExtensions.cs)：默认基础及环境 appsettings 来源都使用 hostBuilder:reloadConfigOnChange 控制，默认值为 true。后续监听装配需保留这一开关语义，而不是默认只提供手动更新 API。

容器格式检查、全工作区 all-features/locked check、严格 Clippy、完整 build 及三个宿主 Release build 均通过；Release 构建耗时 1 分 17 秒。两个仓库 diff --check 通过；没有新增运行时重载对照证据。

## 宿主日志配置文件自动监听

2026-11-08 在运行时过滤更新入口之上接入文件自动监听。共享 logger::load_host_configuration 按项目 TOML 约定加载基础文件、所选环境文件与 APP 环境变量，宿主反序列化和监听缓存使用同一次读取结果。缓存仅保留 log_level/providers 相关内容，不长期保存数据库、Redis 等连接配置。WebAPI 和 TaskService 均保存该来源信息，两种日志提供者初始化时把监听器绑定到 LoggerRuntime。

logger.reload_on_change 默认 true，三个宿主默认配置均显式声明；设为 false 时不创建监听器，保持手动更新入口。此设置映射源 hostBuilder:reloadConfigOnChange 的启动开关语义，只覆盖日志过滤规则，不重建存储组件、连接或业务设置。源 [FileConfigurationSource](https://raw.githubusercontent.com/dotnet/runtime/v10.0.0/src/libraries/Microsoft.Extensions.Configuration.FileExtensions/src/FileConfigurationSource.cs) 的 ReloadDelay 默认 250 毫秒，当前通知也在 250 毫秒后读取发生变化的文件。

使用 [notify 8.2.1](https://docs.rs/notify/8.2.1/notify/) 监听配置父目录，捕获文件创建、修改、替换和删除，忽略读取自身产生的访问事件。回调只合并原子标记并尝试发送到容量为 1 的队列；文件读取交给阻塞任务，过滤编译与发布在接收端完成。首次注册后主动重读文件，补齐配置加载与监听注册之间的变化。DOTNET_USE_POLLING_FILE_WATCHER 为 1/true 时改用四秒轮询；网络或 Windows 挂载目录可能不发送系统通知，需显式启用轮询。

每个文件分别保留来源缓存，环境过滤规则固定为启动快照。文件删除把该来源置空并发布重载；解析失败把该来源置空但不发布通知，当前生效规则继续保留，另一个来源后续成功变化时会读取清空后的缓存。读取失败保留该来源缓存；内容有效而级别非法时保存来源内容，但过滤快照更新失败并保留旧规则。修复文件后可在后续通知中重试。诊断只输出固定文字，不记录解析正文、环境值或连接信息，不回写日志链路。这些分支依据前轮读取的 FileConfigurationProvider/LoggerFactory 机制实现，尚未完成两端故障场景实际运行对照。

LoggerRuntime::shutdown 先停止监听并取消、等待更新任务，再用剩余停机期限排空日志；初始化中途失败或 Drop 也释放监听并取消任务，不把后台句柄遗留在宿主外。通知库自身线程按其 Drop 协议退出，当前未对所有底层文件系统故障做运行验证。

同时复核任务宿主环境选择：旧实现只读取 APP_ENVIRONMENT，未设置时跳过环境文件。现在按 APP_ENVIRONMENT、DOTNET_ENVIRONMENT 选择并默认 Production，基础文件与 Production 文件都进入加载/监听来源；这是对 [Generic Host 的 DOTNET_ 主机环境配置](https://raw.githubusercontent.com/dotnet/runtime/v10.0.0/src/libraries/Microsoft.Extensions.Hosting/src/HostingHostBuilderExtensions.cs) 的补齐，WebAPI 继续使用其既有环境解析。

容器解析锁文件新增 notify、notify-types、inotify、inotify-sys 及跨平台所需的四个包，共八个新包；Linux 编译只下载所需依赖。没有在本机安装组件，没有新增测试代码、验证入口或测试依赖，也没有运行测试、启动临时宿主或修改 NetEngine。源工作区仍只有此前五处授权变更。自动监听已接入生产宿主，但文件通知、错误恢复和停机的两端实际对照仍未完成，整体框架目标继续保持未完成。

容器格式检查、全工作区 all-features/locked check、严格 Clippy、完整 build 和三个宿主 Release build 均通过；Release 构建耗时 2 分 23 秒。两个仓库 diff --check 通过，仅开发容器运行。发布包与运行镜像没有重新生成，本轮没有新增两端文件事件运行对照证据。

## 克隆参数的引用编号重复校验

2026-11-08 重新核对 Common.JsonHelper、QueueTaskProducer、队列/定时任务的反射反序列化入口和 Rust 的短信、邮件任务参数。源克隆选项使用 ReferenceHandler.Preserve；[PreserveReferenceResolver](https://raw.githubusercontent.com/dotnet/runtime/v10.0.0/src/libraries/System.Text.Json/src/System/Text/Json/Serialization/PreserveReferenceResolver.cs) 在单次反序列化中通过共享字典登记引用，重复 AddReference 会抛出 JSON 错误。Rust 此前只验证 $id 的字符串类型及位置，再移除元数据，没有跨对象登记，因此可能接受根对象和子字典、两个邮件列表之间重复的编号。

common::json_helper::input 的 Clone 来源现在持有单次读取专用的 ReferenceIds，通过 Rc 共享到字段、集合元素和枚举访问器。类型化对象/字典的前导 $id 与集合包装的 $id 登记在同一表中，重复声明返回读取错误。表在每次 ObjectInput<T, true> 创建，读取结束释放，不使用线程全局状态，也不跨队列任务共享；编号按原始字符串比较，不裁剪、大小写转换或解析整数。

普通内部 JSON、HTTP/Query 和 Web 协议来源不创建登记表。动态 JSON 仍走原始文档访问，忽略字段直接跳过，不通过遍历任意 JSON 的全局预处理识别或剥离 $id。短信与邮件生产者现有编号生成规则未改变，合法独立对象、字典和列表继续使用原有序列化形式。

本轮只补齐已类型化读取的编号登记，不实现通用对象图的共享身份、循环引用及 $ref 解析。现有重复业务属性在进入目标反序列化前合并的行为仍可能跳过被覆盖值的转换或引用登记，需要继续对照；不能把这项校验视为完整 ReferenceHandler.Preserve 等价。还未执行两端重复编号实际请求或队列运行对照。

没有修改 NetEngine、新增依赖、测试或验证程序，没有运行测试、启动临时宿主或安装本机组件。整体框架目标保持未完成。

容器格式检查、全工作区 all-features/locked check、严格 Clippy、完整 build 和三个宿主 Release build 均通过；Release 构建耗时 1 分 40 秒。两个仓库 diff --check 通过。发布包和运行镜像未重新生成，未新增两端克隆参数运行对照证据。

## 克隆任务参数的逐次赋值

2026-11-08 继续核对重复业务字段。源 System.Text.Json 的 ObjectDefaultConverter 按出现顺序查找属性并调用 ReadJsonAndSetMember，失败不会等后面的同名属性覆盖。参考源码：https://raw.githubusercontent.com/dotnet/runtime/v10.0.0/src/libraries/System.Text.Json/src/System/Text/Json/Serialization/Converters/Object/ObjectDefaultConverter.cs 。Rust 公共输入此前先合并同名字段，可能跳过错误类型和被覆盖对象的引用登记。

ProtocolModel 现在支持模型级无参数 serde(default) 及显式 protocol(case_sensitive)，生成内部读取标记，让公共输入保留逐次赋值。短信和邮件 DTO 使用此声明，保持 PascalCase、精确大小写和缺省值；先转换每次字段值，最后一次成功值覆盖之前值。类型化字典也保留每次键值读取，让目标字典在转换成功后覆盖。被覆盖对象、列表仍使用单次共享引用登记表，不能通过重复业务字段跳过 $id 唯一性校验。

普通 Serde 结构体仍沿用已有适配行为，动态 JSON 仍走文档读取，Query 仍聚合同名查询值；没有把字段级规则变成全局 JSON 预处理。一般对象图的共享身份、循环引用和 $ref 解析仍未实现，本轮也未执行两端实际队列参数运行对照。源 NetEngine 保持已有五处授权修正，没有新增源项目修改、依赖、测试代码或验证程序；未运行测试，未安装本机组件。整体对齐目标仍未完成。

容器格式检查、全工作区 all-features/locked check、严格 Clippy、完整 build 和三个宿主 Release build 均通过；Release 构建耗时 1 分 48 秒。两个仓库 diff --check 通过，仅开发服务运行。发布包与运行镜像未重新生成，未新增两端队列运行对照证据。

## 克隆固定数组与列表的元数据边界

2026-11-08 对照 System.Text.Json 的 ArrayConverter 与集合元数据读取，确认数组转换器 CanHaveMetadata=false，遇到 $id 不接受列表引用包装。源码：https://raw.githubusercontent.com/dotnet/runtime/v10.0.0/src/libraries/System.Text.Json/src/System/Text/Json/Serialization/Converters/Collection/ArrayConverter.cs ，元数据约束：https://raw.githubusercontent.com/dotnet/runtime/v10.0.0/src/libraries/System.Text.Json/src/System/Text/Json/Serialization/JsonSerializer.Read.HandleMetadata.cs 。Rust 的固定数组经 Serde tuple 入口读取，此前直接委托序列入口，误接受 List<T> 的 $id/$values 对象。

公共克隆输入现在在固定数组/tuple 入口拒绝对象包装，再按原有序列机制读取普通数组；检查也覆盖嵌套固定数组。Vec 列表、邮件 DTO 现有列表字段、普通 JSON、Query 和动态文档保持原有路径。没有引入新的解析器、依赖或测试代码，也未修改 NetEngine。

同时审查 $ref：源语义包含共享对象身份及循环引用，而当前 Rust DTO 使用拥有值的字段；复制或展开原 JSON 会丢失身份，不能用此方法声称实现了 ReferenceHandler.Preserve。本轮没有引入这种替代实现，通用共享引用与循环仍是未完成项。未执行两端实际数组参数运行对照，整体目标保持未完成。

容器格式检查、全工作区 all-features/locked check、严格 Clippy、完整 build 和三个宿主 Release build 均通过；Release 构建耗时 1 分 46 秒。两个仓库 diff --check 通过，仅开发服务运行，没有安装本机组件、运行测试或生成新发布包/运行镜像。另复核 json_body.rs：Syntax 与 Conversion 的通用错误提示仍未达到源端的完整路径、位置及参数必填组合行为，后续继续处理；当前检查不能证明整个框架已完全对齐。

## HTTP 数值字段的首次转换错误

2026-11-08 复核 read_json_model 与宿主 json_body：原有 Shape 声明只约束字符串、布尔、对象和集合。数值走 Any，先读完整棵值再合并同名字段、最后转换模型，可能跳过较早的非法数值，并退回缺少原始位置的通用 Conversion 提示。

JsonModel 派生现在为非可空有符号/无符号 8/16/32/64 位整数及 f32/f64 生成 JsonScalarField 声明，集合和字典的同类元素也有独立声明。公共 Seed 在消费数值、字符串、布尔或 null 时先调用声明的纯标量转换；对象和数组仍在 token 形态阶段拒绝。失败进入既有 Shape 诊断，保留原始字段名、索引、System 数值类型名称及读取位置，由共享 HTTP 层生成路径/行/字节位置和正文参数必填信息。字段声明 deserialize_with 时复用该函数，普通数值及元素走既有 Input；业务模型仍只在读取成功后构造一次，不通过反解析 Serde 错误文本推断目标类型，也不反复重新绑定模型来搜集错误。

同步核对 System.Text.Json 的 Int32Converter、DoubleConverter 和 Utf8JsonReader.TryGet：HTTP 普通数值字符串不裁剪空白，LongConverter 的 i64 单独保留 TryParse 对首尾空白的容忍；HTTP 浮点字符串接受精确拼写 NaN/Infinity/-Infinity，其他解析为非有限值的字符串仍拒绝。Query、内部对象/克隆、Web 协议和动态 JSON 字符串不因这项修改扩大转换。源码参考：
- https://raw.githubusercontent.com/dotnet/runtime/v10.0.0/src/libraries/System.Text.Json/src/System/Text/Json/Serialization/Converters/Value/Int32Converter.cs
- https://raw.githubusercontent.com/dotnet/runtime/v10.0.0/src/libraries/System.Text.Json/src/System/Text/Json/Serialization/Converters/Value/DoubleConverter.cs
- https://raw.githubusercontent.com/dotnet/runtime/v10.0.0/src/libraries/System.Text.Json/src/System/Text/Json/Reader/Utf8JsonReader.TryGet.cs
- https://raw.githubusercontent.com/dotnet/aspnetcore/v10.0.0/src/Mvc/Mvc.Core/src/Formatters/SystemTextJsonInputFormatter.cs

这轮覆盖显式数值类型，未把可空值、以字符串承载的 ID/日期或任意自定义类型推断为同一转换器；它们的空白、内部 Deserialize 路径及错误行为仍需继续对齐。一般语法错误、完整浮点边界、共享引用身份和其他已记录待办仍未关闭，尚未取得两端本轮错误请求的实际运行对照证据。源 NetEngine 保持已有五处授权修正，没有添加依赖、测试代码或验证程序，没有运行测试或安装本机组件。整体目标继续保持未完成。

容器格式检查、全工作区 all-features/locked check、严格 Clippy、完整 build 和三个宿主 Release build 均通过；Release 构建耗时 1 分 48 秒。两个仓库 diff --check 通过，仅开发服务运行。发布包和运行镜像未重新生成，本轮没有启动临时宿主或新增两端运行对照证据。

## 可空 JSON 标量的转换与独立错误范围

2026-11-08 对照 NetEngine 的 NullableStructConverterFactory：null 由可空转换器处理，空/纯空白字符串返回 null，其他值通过 JsonSerializer.Deserialize<T>(ref reader, options) 重新读取。源框架 Read<Utf8JsonReader> 为当前值创建独立 ReadStack 和作用于当前原始 token 的读取器，已有 JsonException 路径不会被外层覆盖；字符串范围包含原始引号和转义，对象/数组先 TrySkip 完整值。参考：https://raw.githubusercontent.com/dotnet/runtime/v10.0.0/src/libraries/System.Text.Json/src/System/Text/Json/Serialization/JsonSerializer.Read.Utf8JsonReader.cs 及 https://raw.githubusercontent.com/dotnet/runtime/v10.0.0/src/libraries/System.Text.Json/src/System/Text/Json/ThrowHelper.Serialization.cs 。

Rust 的公共可空 JSON 转换此前依次尝试目标反序列化、i64/u64/f64 猜测，再将猜测值转换为目标类型。这会额外裁剪普通数值字符串空白，并丢失浮点命名常量。现在空值分支保留，非空值直接交给 Input 的 JSON 来源按目标类型读取；Query 路径保持独立，不修改普通字符串内容或全局扫描 JSON。

JsonModel 标量声明扩展到显式公共可空 JSON 标量转换器的 Option 数值/布尔，以及显式可空 i64 转换器。生产请求中的 EditTaskSettingDto.Semaphore、EditCategoryDto.ParentId、SetFunctionRouteDto.FunctionId 和 RemoteUploadFileDto.Key 等沿用现有字段声明获得逐次读取。首次非法赋值不再等整份对象合并之后才失败。声明包含 scoped 标志：错误类型采用内层 System 类型，路径为 $，位置按当前原始 token 计算；字符串回查原文引号并处理反斜线奇偶，保留转义与 UTF-8 字节长度。对于对象/数组，先读取完整值以保留内部语法/深度错误的先后，再按内层标量读取位置报告类型错误。

可空 String 不生成数值声明，未显式声明转换器的 Option、集合元素、Query、内部克隆和动态文档也不自动开启 scoped 标量规则。日期诊断及普通 JSON 语法错误的完整源文案仍待处理，浮点全部边界及两端当前错误请求尚未实际对照。没有修改 NetEngine、增加依赖、编写测试或验证程序，没有安装本机组件或运行测试；整体目标保持未完成。

容器格式检查、全工作区 all-features/locked check、严格 Clippy、完整 build 和三个宿主 Release build 均通过；Release 构建耗时 1 分 48 秒。两个仓库 diff --check 通过，仅开发服务运行。发布包和运行镜像未重新生成，未新增两端实际请求验证。后续日期定位到 WxPayNotifyDto.create_time：Rust 使用带 date_time 转换器的 String，源属性为 DateTimeOffset，需要明确协议类型声明，不能依靠普通字符串形态诊断覆盖。

## HTTP 显式标量类型与 ISO 日期读取

2026-11-08 复核 ArticleDto、FileInfoDto、WeiXinPayNotifyDto：源 ID 为 long，创建时间为 DateTimeOffset，Rust 部分字段以 String 保存并已有转换器，但 json(value) 仅跳过字符串必填逻辑，没有声明逐项诊断类型。JsonModel 新增 scalar_type，必须同时声明 value 和 deserialize_with，限定非可空标量；生成时拒绝集合、字典、嵌套对象及缺失转换器等无效组合。三个 ID 字段声明 System.Int64，文章/微信通知两个创建时间声明 System.DateTimeOffset。复用既有标量转换、路径和位置机制，较早的非法值不再被后续重复属性覆盖，也不再误报普通字符串类型。

HTTP 日期转换从应用模型中移入 common::json_helper::date_time 的 ISO 读取入口，应用层保留薄转换适配。新入口按固定位置读取 ASCII 日期和时间，支持省略秒数、Z、小时或小时:分钟偏移；小数秒至多十六位，截取前七位为百纳秒，不经 f64 舍入。源读取器在小数点后直接遇到时区标记的行为也保留。校验日历日期、时分秒、十四小时偏移及 UTC 年份范围，不再依赖宽松 RFC3339 解析兜底；未指定偏移继续按本地时区读取。内部对象日期与 Query 的入口未替换。

核对依据为源默认 WebAPI 日期配置及 System.Text.Json 日期读取实现：https://raw.githubusercontent.com/dotnet/runtime/v10.0.0/src/libraries/System.Text.Json/src/System/Text/Json/JsonHelpers.Date.cs 。当前本地时间仍通过 chrono Local 选择映射；夏令时缺口、歧义与历史偏移规则仍需进一步对齐，不能将本轮语法修正视为所有 DateTimeOffset 行为完全一致。尚未新增两端日期请求运行证据。

没有修改 NetEngine、新增依赖、测试代码或验证程序，没有运行测试或安装本机组件。一般 JSON 语法错误、完整日期/数值边界及共享引用身份等未完成项继续保留，整体目标保持未完成。

容器格式检查、全工作区 all-features/locked check、严格 Clippy、完整 build 和三个宿主 Release build 均通过；Release 构建耗时 1 分 49 秒。两个仓库 diff --check 通过，仅开发服务运行。发布包与运行镜像未重新生成，本轮没有启动临时宿主或新增两端日期请求运行证据。

## Unix 本地日期的标准时选择

2026-11-08 核对 DateTimeOffset 构造与 TimeZoneInfo.GetIsDaylightSavings：新的本地 DateTime 没有从 UTC 转换保留的歧义标记，源端对重复时间选择标准时，且 NoThrowOnInvalidTime 允许使用标准偏移解释夏令时缺口。原 Rust 各入口直接 Local.from_local_datetime(...).latest()，缺口返回空值，重复时刻则只按时间先后选择，不能表达负夏令时等标准时选择。参考：https://raw.githubusercontent.com/dotnet/runtime/v10.0.0/src/libraries/System.Private.CoreLib/src/System/DateTimeOffset.cs 和 https://raw.githubusercontent.com/dotnet/runtime/v10.0.0/src/libraries/System.Private.CoreLib/src/System/TimeZoneInfo.cs 。

新增 common::date_time_helper 的内部本地转换模块。唯一映射采用实际偏移，Unix 的缺口/重复时刻通过按输入日期设置 tm_isdst=0 查询标准偏移，再用原始 NaiveDateTime 和固定偏移构造结果，不采用 mktime 归一化后的时分秒。标准查询从 Cron 的现有实现提取，Cron 继续只查询当前标准时并使用原有缓存；HTTP ISO、Query、内部 JSON 和 DataHelper 的本地日期转换共同调用解析入口。返回 timestamp=-1 时比较 localtime_r 回读的已归一化日历字段，区分合法 epoch 前一秒与失败，不根据单一返回值误拒合法时间。

本轮没有实现完整 TimeZoneInfo TZif 规则复制。源 Unix 实现对历史秒级偏移按基础偏移差额归一化至分钟，chrono/libc 原始偏移仍可能不同；相关转换边界、非 Unix 的逐年标准时 API、政治性时区重叠及全部跨年规则仍需进一步对照。当前非 Unix 分支保留既有系统映射。参考：https://raw.githubusercontent.com/dotnet/runtime/v10.0.0/src/libraries/System.Private.CoreLib/src/System/TimeZoneInfo.Unix.cs 。尚未完成两端夏令时实际输入对照，不能把本轮代码与编译结果作为完整时区等价证明。

没有修改 NetEngine、增加依赖、编写测试或验证程序，没有运行测试或安装本机组件。整体目标保持未完成。

容器格式检查、全工作区 all-features/locked check、严格 Clippy、完整 build 和三个宿主 Release build 均通过；Release 构建耗时 1 分 41 秒。两个仓库 diff --check 通过，仅开发服务运行。发布包与运行镜像未重新生成，没有启动临时宿主或新增两端时区输入运行证据。

## JSON 根值之后的额外内容诊断

2026-11-08 复核 read_json_model：之前把根值读取与 Deserializer.end 合在同一 Syntax 分支，宿主丢弃公共路径并输出通用 Serde 提示，缺少正文参数必填组合。源 Utf8JsonReader 在单一根值后遇到非空白字节时使用 ExpectedEndAfterSingleJson，显示当前字节；可打印范围为 ASCII 0x20 至 0x7E，其他值使用大写两位十六进制。参考：https://raw.githubusercontent.com/dotnet/runtime/v10.0.0/src/libraries/System.Text.Json/src/System/Text/Json/Reader/Utf8JsonReader.cs 、 https://raw.githubusercontent.com/dotnet/runtime/v10.0.0/src/libraries/System.Text.Json/src/System/Text/Json/ThrowHelper.cs 和 https://raw.githubusercontent.com/dotnet/runtime/v10.0.0/src/libraries/System.Text.Json/src/Resources/Strings.resx 。

公共读取器现在分开根值读取与 end 检查。只有根值成功读取后才检查尾部；失败时复用原 Deserializer 的 StreamDeserializer.byte_offset 获取当前未消费字节，再按已消费 UTF-8 前缀计算行列。没有重解析正文或依赖 Serde 错误文字判断类别。尾部错误独立保留字节、行、列，HTTP 层输出源端对应诊断并按参数声明补必填信息。根 null 在空值处理之前也经过尾部检查；此前已被 token 读取发现的类型错误不会被尾部错误覆盖。容器中只读核对了当前锁定 Serde JSON 的 end、peek_error 和 into_iter 实现，确认位置来自同一读取器，没有执行测试或建立验证程序。

一般 Syntax 分支也恢复使用公共读取器已经保留的路径，并复用正文参数错误装配；目前通用语法文字仍有差异，未将其记录为完全等价。截断输入、非法字面量和标点的源诊断、全部转换的首次失败顺序，以及两端本轮实际请求对照仍未完成。历史时区和共享引用等前序未完成项继续保留。

没有修改 NetEngine、新增依赖、测试代码或验证入口，没有运行测试或安装本机组件。整体目标保持未完成。

容器格式检查、全工作区 all-features/locked check、严格 Clippy、完整 build 和三个宿主 Release build 均通过；Release 构建耗时 1 分 40 秒。两个仓库 diff --check 通过，仅开发服务运行。发布包与运行镜像未重新生成，未新增两端 JSON 错误请求运行证据。

### JSON 分隔符之后的输入截断诊断

对照 .NET 10 Utf8JsonReader 的 ConsumeNextToken 与 ExpectedStartOfPropertyOrValueNotFound，补充对象属性/数组元素分隔符后的 EOF 分类。使用读取器调用阶段记录是否仍在等待键或元素，实际进入键/值反序列化即清除标记；保留任意精度数字的 MapAccess 协议，不从 Serde 错误字符串猜测。只在已经确认 EOF 且最后非 JSON 空白字节为逗号时转换诊断，未闭合字符串不进入该分支。

源读取器在逗号恰为最后一个字节时回退一字节，有后续空白则报告 EOF；Rust 同步这一区别，并按原 UTF-8 字节计算行列。HTTP 绑定层复用模型路径和参数必填错误装配。此前 NetEngine 三处日期查询的 UTC 修复仍保留，本轮没有增加源项目改动。

参考：https://github.com/dotnet/runtime/blob/v10.0.0/src/libraries/System.Text.Json/src/System/Text/Json/Reader/Utf8JsonReader.cs 。未闭合字符串、其他语法错误文案及完整运行时差异仍待后续核对；本项不代表全部 JSON 行为或整个项目已对齐。

本轮容器验证全部通过：cargo fmt --all -- --check、cargo check --workspace --all-features --locked、cargo clippy --workspace --all-features --lib --bins --locked -- -D warnings、cargo build --workspace --all-features --locked，以及 admin_webapi/client_webapi/task_service 三宿主 Release 全特性编译（终端退出 0，1m42s）。未添加或运行测试，未安装本机组件，未重新启动运行时进行接口对照；发布镜像和部署包未重建。

### JSON 未闭合字符串诊断

源 HEAD 仍为 23a7d0002938d0308c975dc48c04ac66aae0dda8。对照 .NET 10 Utf8JsonReader 的 ConsumeString、ConsumeStringAndValidate 和 ValidateHexDigits：未找到结束引号、转义反斜杠后截断、Unicode 转义已出现的数字合法但不足四位，均在输入末尾报告字符串未结束。公共 Rust 读取器现在只在 Serde 已确认 EOF、且没有更早的字段类型/深度错误时识别这些情况，HTTP 层复用当前路径、UTF-8 字节行列和正文参数错误装配。

诊断遍历只跟踪字符串、转义及四位十六进制边界，不解析模型，不复制正文，也不替代原读取器。非法转义、控制字符、短 Unicode 转义中的非十六进制字符保留原诊断，避免把更早的字符错误误报为缺少引号。已闭合字符串后出现的其他截断不会进入字符串诊断。分隔符、尾部内容和字符串末尾位置共用字节定位助手。

另核实源 JsonSerializer.GetReaderScopedToNextValue 会在 TrySkip 失败时通过新 ReadStack 包装 JsonReaderException，所以现有可空标量的独立根路径规则仍保留，没有提前改回外层字段路径。参考：https://github.com/dotnet/runtime/blob/v10.0.0/src/libraries/System.Text.Json/src/System/Text/Json/Reader/Utf8JsonReader.cs ，https://github.com/dotnet/runtime/blob/v10.0.0/src/libraries/System.Text.Json/src/System/Text/Json/Serialization/JsonSerializer.Read.Utf8JsonReader.cs 。

本轮未修改 NetEngine；源目录仍为此前授权的上传两处和 UTC 查询三处修改。非法字符、未完成字面量、数字语法、代理项错误及完整运行时对照仍未全部完成，本项不代表 JSON 或整体功能已对齐。

本轮容器验证全部退出 0：格式检查、工作区全特性 check、严格 Clippy（lib/bins、-D warnings）、工作区全特性 build，以及 admin_webapi/client_webapi/task_service 三宿主 Release 全特性编译（1m41s），Cargo 均保留 --locked。两仓库 diff --check 通过。未添加或运行测试，未安装本机组件，未启动宿主作运行时对照；发布镜像和部署包未重建。

### LLM 流式响应的增量字符解码

源 HEAD 仍为 23a7d0002938d0308c975dc48c04ac66aae0dda8。本轮重新检查业务和基础设施生产代码，未发现 todo!/unimplemented! 或明确的未实现占位。继续逐项比对 LLM 后，确认 Anthropic、OpenAI Compatible、OpenAI Responses 三种源实现均使用 StreamReader(stream, Encoding.UTF8)，而 Rust 共用读取器原先直接按原始 CR/LF 字节拆行，只在第一行去除 UTF-8 BOM。UTF-16/UTF-32 的带标记正文因此无法按源规则读取事件。

在 llm 组件中新增私有 TextDecoder，先增量解码，再拆行。支持 UTF-8、UTF-16 LE/BE、UTF-32 LE/BE，保留跨网络读取的未完成字符和 UTF-16 高代理项；正常 EOF 才替换解码器尾部。没有聚合完整响应，未完成字符缓存最多三字节，另保留至多两字节 UTF-8 preamble。沿用源 1024 字节输入窗口及 BOM 可见边界：UTF-8 preamble 可以累积，其他标记按当次可见数据识别，不能为了容忍分片而改变源的选择顺序。切换编码时重置旧解码器尾部。流式读取不读取 Content-Type charset；非流式 http_response 规则不变。

同时核对 ReadLineAsyncInternal，源在块末 CR 后先读取下一字符或 EOF，再返回当前行。共享读取器同步该顺序，跨块 CRLF 不额外产生空行，CR 后读取失败不能提前发布该行。已有请求超时、取消、事件映射和三种协议的 data 行处理保持原有职责；没有新增依赖或引入 common 的图像/表格依赖。

依据为三个本地源 LLM 客户端，以及 https://github.com/dotnet/runtime/blob/v10.0.0/src/libraries/System.Private.CoreLib/src/System/IO/StreamReader.cs 的 DetectEncoding、IsPreamble、ReadBufferAsync、ReadLineAsyncInternal。当前证据为源码对照和容器构建，尚未执行真实提供商或跨语言网络分片对照，不能视为 LLM 全部行为已验证。

本轮最终容器验证全部退出 0：cargo fmt --all -- --check、cargo check --workspace --all-features --locked、cargo clippy --workspace --all-features --lib --bins --locked -- -D warnings、cargo build --workspace --all-features --locked，以及 admin_webapi/client_webapi/task_service 三宿主 Release 全特性编译（54.91s）。首轮 Clippy 的固定分组建议已按 stable 工具链修正并重新通过。两仓库 diff --check 通过。未添加或运行测试，未安装本机组件，未启动运行时或访问真实提供商；发布镜像和部署包未重建。

### LLM HTTP 成功状态、错误顺序与请求字符集

源 HEAD 仍为 23a7d0002938d0308c975dc48c04ac66aae0dda8。三种源客户端在读取协议 DTO/事件前调用 EnsureSuccessStatusCode，只接受最终 200–299；Rust 六个入口原使用 reqwest::Response::error_for_status，当前锁定版本仅拒绝 4xx/5xx。最终 3xx（如 304，或未被客户端跟随的重定向）因此可能继续解析 JSON、返回空流或被误认为成功。

现由 llm::http_response::ensure_success 统一检查最终状态。非流式 read_json 先等待正文读取，再报告状态错误，随后才检查 charset 并解析 JSON；保留源 PostAsync/SendAsync 默认缓冲行为下的传输失败、超时优先级。流式在响应头后立即检查，非成功状态不进入事件读取。三种协议的流式/非流式请求均显式声明 application/json; charset=utf-8，与源 StringContent 一致；客户端的重定向策略和请求超时不在这里改写。

状态错误使用已有 ApiError 保留状态码，并按源描述保留 HTTP/1 自定义 reason phrase（Latin-1），空白短语不添加括号；不将错误正文或请求 URL 拼入状态描述。为读取 reqwest 保留的原始 ReasonPhrase，llm 显式依赖锁文件中已有的 hyper 1.12.0；在容器离线同步 Cargo.lock 后，逐包名称、版本、校验和快照确认没有包版本变动。

源码依据：本地 AnthropicMessagesLlmClient、OpenAiResponsesLlmClient、OpenAiCompatibleLlmClient；https://github.com/dotnet/runtime/blob/v10.0.0/src/libraries/System.Net.Http/src/System/Net/Http/HttpResponseMessage.cs ，https://github.com/dotnet/runtime/blob/v10.0.0/src/libraries/System.Net.Http/src/System/Net/Http/SocketsHttpHandler/HttpConnection.cs ，https://github.com/dotnet/runtime/blob/v10.0.0/src/libraries/System.Net.Http/src/System/Net/Http/HttpRuleParser.cs 。本轮没有实际服务调用；跨语言网络行为、其他协议诊断和完整 LLM 功能仍待核验，不能以编译通过代替等价证明。

本轮最终验证全部在容器完成并退出 0：格式检查、工作区全特性 check、严格 Clippy（lib/bins、-D warnings）、工作区全特性 build，以及 admin_webapi/client_webapi/task_service 三宿主 Release 全特性编译（54.66s）。离线同步依赖关系后，最终 Cargo 检查与编译均使用 --locked。两仓库 diff --check 通过。未添加或运行测试，未安装本机组件，未连接真实 LLM 服务；发布镜像和部署包未重建。

### LLM 扩展参数的重复属性与子节点保留

本轮重新核对 LlmInvokeService、LlmAppService、试调用 DTO 以及三种协议的请求装配。源 ExtraBodyJson 在保存时仅检查合法对象；调用时枚举 JsonObject，首次建立字典会按 Ordinal 比较拒绝顶层重复键，随后才过滤空键/null。Rust 原先转成 serde_json::Value，顶层重复键被覆盖，嵌套重复属性也随之丢失。管理端试调用的源 ExtraBody 则是 Dictionary<string, JsonNode>，同名顶层键采用后值，这两条入口不能统一改成拒绝重复。

应用模型与 LLM 请求契约的扩展参数改为有序原始 JSON 子节点。common::json_helper::input 新增对象项读取入口，保留全部顶层项及嵌套子节点，复用共享 64 层深度限制；运行时配置在过滤之前检查重复键。HTTP 试调用继续使用字典赋值规则，子节点经过既有 RawValue 读取协议保留结构。三种提供商共用 Payload 装配：基础字段正常生成，扩展节点直接序列化，冲突仍按大小写敏感规则拒绝，Anthropic 的默认 max_tokens 仍在扩展参数合并后补齐。

配置字段、数据库结构、HTTP 请求结构及 OpenAPI 的字典表示不变；Rust 内部 extra_body 类型从 Value 字典改为 RawValue 字典，直接调用构造器时应使用对应的原始子节点类型。保留的是重复属性、顺序及数字词法，不承诺保持配置 JSON 的空白和字符串转义字节。没有将应用模型反向依赖基础设施组件，LLM 也未引入 common 的重型工具依赖；显式使用已锁定的 indexmap 和 serde_json raw_value 功能，包名称、版本与校验和快照一致。

依据为本地 LlmInvokeService.ParseExtraBodyJson、LlmAppService.ValidateExtraBodyJson、TestLlmAppRequestDto，以及 https://github.com/dotnet/runtime/blob/v10.0.0/src/libraries/System.Text.Json/src/System/Text/Json/Nodes/JsonObject.IDictionary.cs 的 InitializeDictionary 与 https://github.com/dotnet/runtime/blob/v10.0.0/src/libraries/System.Text.Json/src/System/Text/Json/Nodes/JsonObject.cs 的 DeepCloneCore/WriteTo。未物化的嵌套节点克隆和写入均沿用 JsonElement，不应提前去重。本轮未访问真实模型或数据库，完整运行时行为仍待对照。

本轮最终容器验证全部退出 0：格式检查、工作区全特性 check、严格 Clippy（lib/bins、-D warnings）、工作区全特性 build，以及 admin_webapi/client_webapi/task_service 三宿主 Release 全特性编译（1m41s）。离线同步既有依赖关系后，最终 Cargo 检查和编译均使用 --locked。另复核当前 serde_json 的 arbitrary_precision 数字读取，对 -0 保留原文协议而不转换为普通零。两仓库 diff --check 通过，源 HEAD 和五处已授权修改未变。未添加或运行测试，未安装本机组件，未访问模型服务或数据库；发布镜像和部署包未重建。

### 动态配置 JSON 的读取期深度检查与暂停交接

本轮核对 LLM 试调用、模板和配置校验时，发现 ExtraBodyJson 保存入口经 parse_json_value 读取，却没有经过内部对象/运行时配置的深度检查。合法但超过 64 层的配置可能先保存成功，调用时才被拒绝。现将 common 动态值读取改为携带深度的 DeserializeSeed：对象与数组在读取时检查共享 MAX_DEPTH，子值沿用父深度，标量不增加深度；任意精度数字的 MapAccess 仍按数字识别。移除构造整棵树之后重复执行的深度遍历，让动态配置、内部对象、克隆及公共 Web 协议入口共用读取期约束。

此次只完成上述已开始的改动。用户要求最近一次改动完成后停止，最终编译检查结束后暂停目标，不继续开启其他对齐工作。完整功能等价尚未证明；独立 LLM 提供商读取器的深度约束、此前记录的其他协议差异及运行时对照仍留待后续，不以本项通过代表全部完成。

暂停前最终容器检查全部退出 0：cargo fmt --all -- --check、cargo check --workspace --all-features --locked、严格 Clippy（--workspace --all-features --lib --bins --locked -- -D warnings）、工作区全特性 build，以及三个宿主 Release 全特性编译（1m42s）。两仓库 diff --check 通过。未添加或运行测试，未安装本机组件，未进行新运行时对照。按用户要求在本次改动完成后暂停，整个 NetEngine 对齐目标仍未标记完成。
