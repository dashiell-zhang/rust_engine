# AGENTS.md

## 工作区规则

- 编译、检查、格式化及其他 Rust 工具链操作一律在本机 Docker 容器内执行，不使用 Windows 本机工具链。
- 不要在本机安装 Rust、Cargo、编译器、SDK、构建依赖或其他组件；所需工具和依赖只安装到容器镜像中。
- Rust 工具链始终跟随最新稳定版：[rust-toolchain.toml](rust-toolchain.toml) 使用 `channel = "stable"`，不要改为具体版本号或日期，也不要通过命令或环境变量覆盖为固定版本。
- Dockerfile 的外部基础镜像使用 `latest` 标签（如 `rust:latest`、`ubuntu:latest`、`busybox:latest`），不要固定版本标签或镜像摘要；多阶段构建的阶段引用和 `scratch` 保持原有写法。
- 不要编写测试代码。
- 不要添加 `#[cfg(test)]` 模块、`tests/` 目录或测试辅助代码。
- 不要添加仅用于测试的依赖或 `dev-dependencies`。
- 如果某项改动通常需要包含测试，除非用户明确要求，否则请跳过测试相关内容。

## 快速链接

- 架构与分层职责：[README.md](README.md)
- NetEngine 架构差异与后续范围：[docs/netengine-architecture-review.md](docs/netengine-architecture-review.md)
- 工作区 crate 列表：[Cargo.toml](Cargo.toml)
- 可复用的构建与运行任务：[.vscode/tasks.json](.vscode/tasks.json)
- 容器开发环境：[compose.yaml](compose.yaml)
- 部署生成与发布：[docs/deployment.md](docs/deployment.md)
- 数据库工具入口：[repository_tool/src/main.rs](repository_tool/src/main.rs)
- 数据库工具用法：[docs/repository-tool.md](docs/repository-tool.md)
- WebAPI 共享宿主入口：[project_core/webapi_core/src/lib.rs](project_core/webapi_core/src/lib.rs)
- WebAPI 业务装配：[presentation/webapi_support/src/lib.rs](presentation/webapi_support/src/lib.rs)
- TaskService 共享宿主入口：[project_core/task_service_core/src/lib.rs](project_core/task_service_core/src/lib.rs)
- 软删除查询扩展：[repository/src/extensions.rs](repository/src/extensions.rs)
- 过程宏实现：[source_generator/source_generator_core/src/lib.rs](source_generator/source_generator_core/src/lib.rs)

## 工作区结构

- 这是一个 Cargo workspace，包含三个运行时宿主：`admin_webapi`、`client_webapi` 和 `task_service`。
- 共享的宿主装配代码位于 `project_core/webapi_core` 和 `project_core/task_service_core`。
- 应用逻辑归属 `application/*`；持久化归属 `repository`；可插拔适配器归属 `infrastructure/*`。
- 通用数据库机制位于 `infrastructure/repository_core`；队列任务实体和生产者位于 `infrastructure/queue_task_database`；日志实体由 `infrastructure/logger_database` 持有。这些组件不要反向依赖业务 `repository`。
- `task_service_core` 不依赖具体业务服务或业务仓库；业务任务适配器和处理器由 `presentation/task_service` 装配。
- `webapi_core` 不依赖 `application_service` 或业务 `repository`，续签、权限和路由同步通过 `WebApiApplication` 契约委托。管理端和客户端在宿主档案中显式选择 `presentation/webapi_support` 的业务适配器；请求处理器通过 `app_state.services()` 创建业务服务。
- 队列状态转换通过 `queue_task_database::QueueTaskStore`；开始执行时增加次数，失败不得改写原计划时间来代替退避条件。完成状态、父子链与回调必须在同一数据库事务中提交，父记录按链向上加行锁，不使用宿主内存锁替代事务。
- 队列查询、重试和任务配置管理通过 `queue_task_database::QueueTaskManagement`，应用层只做 DTO 与业务审计装配。手动重试必须在事务行锁内检查成功状态和有效执行租约；编辑配置不得互换无参定义、参数模板及带参实例的身份。
- 任务停止时先停止新领取，使用宿主剩余期限等待执行与续租；超时取消，保留租约恢复路径。定时任务由每个实例独立触发，`skip_if_running` 只做本进程防重入，不添加框架级跨实例触发锁。
- 任务调度读取进程内配置快照；Release 每 60 秒通过任务存储刷新并补齐默认禁用的定义，依赖有效定义唯一约束处理多实例初始化。Debug 选择只启用当前进程的任务，不写数据库配置，也不阻塞事件订阅。
- Cron 校验和触发计算统一使用 `common::cron_helper`，不要另引入一套管理端或宿主专用解析规则。表达式包含秒字段，日与星期其中一项必须是 `?`；留空的实例配置应保留为空，由运行时回退任务声明默认值。
- 任务注册声明 `factory`，每次执行创建独立处理器；只共享宿主连接池等基础设施，不在注册表长期持有执行对象。返回值通过 `QueueTaskResult` 延迟至完成事务中按需序列化，异步资源通过处理器 `close` 释放，遵守同一停机预算。
- 克隆参数读取的引用编号登记表属于单次反序列化，类型化对象、字典和集合包装共用该表并拒绝重复 `$id`；普通 JSON、动态文档和被忽略字段不登记引用编号，不通过全局扫描剥离其业务属性。
- 短信和邮件任务参数通过 `ProtocolModel` 的 `case_sensitive` 声明逐次读取重复字段；先转换再覆盖，不能预先合并而跳过非法值或引用编号登记。类型化字典同样逐次读取，不把该规则泛化到动态 JSON 或 Query 参数。
- 任务参数使用公共 `object_clone_json`/`json_clone_object`，回调结果使用 `object_to_json`。按原始类型处理 i64 字符串约定，不先转 Value 后把所有整数改写；动态 JSON 文档保持自身数字类型。克隆参数字段大小写敏感，不复用普通内部 JSON 的大小写不敏感模式。
- 默认任务清单跟随 NetEngine 的 `MessageTask` 和 `DemoTask`：两个消息队列任务默认并发 1、四个演示队列任务默认并发 5、`ShowTime` 每秒触发；不预设备注、不保留旧任务别名或额外 Token 清理任务。缺失引用参数用 `Option<T>` 表达 null；演示姓名的 null 与空字符串在入队和回调时保持区别。
- 邮件任务通过消息服务契约接收 `SendEmailDto`，保持 PascalCase 字段及列表引用元数据。附件路径以宿主内容目录为基准；选择云存储时每次下载，仅在发送成功后清理下载文件。不得增加备用目录搜索、跳过已有文件下载或在失败路径删除文件。
- 短信任务通过 `SmsService` 契约接收 `SendSmsDto`。内置提供商捕获发送错误并返回 false，应用服务不读取布尔结果；不得把该结果改为队列失败或额外重试。模板字典保留顺序及可空值；阿里云手机号原样传入，腾讯云按源组件直接拼接 `+86`。
- `repository_tool` 是数据库迁移 CLI；不要把 schema 逻辑移动到 presentation 层的 crate 中。
- `deployment` 只生成部署文件，不能在生成器内连接服务器或执行安装、迁移和部署。修改部署内容应改 `deployment/templates`，再在容器运行生成器同步 `generated`；不要只改生成结果。
- Linux 发布包由容器执行 `deployment/build-release.sh`，包含二进制及保持默认路径的配置。运行环境差异通过服务器 env 或外部 TOML 提供，不将真实连接凭据写入部署参数。
- `source_generator/source_generator_core` 和 `source_generator/source_generator_runtime` 提供跨服务使用的过程宏与运行时辅助工具。

## 变更策略

- 仅关注生产代码、构建配置和运行时行为。
- 优先删除或避免测试相关代码，而不是引入新的测试脚手架。
- 变更应保持在现有分层边界内；优先修复所属 crate，而不是在宿主 crate 周围打补丁。
- 保留现有中文注释和命名风格，除非周边文件明显采用了不同约定。
- 优先采用最小化、局部化变更。除非任务明确要求，否则不要重塑模块或 crate 边界。

## 构建与验证

- 使用 [compose.yaml](compose.yaml) 中的 `dev` 服务；在项目根目录执行 `docker compose up -d dev`，首次创建或修改开发镜像配置后执行 `docker compose up -d --build dev`。
- 需要更新稳定版工具链和基础镜像时，执行 `docker compose build --pull --no-cache dev`，再执行 `docker compose up -d dev`，并在容器内重新检查和编译。开发镜像中的 rustfmt、Clippy 应安装到 `stable` 工具链；不要因缓存或兼容性问题改回固定版本。
- 构建或检查工作区时，优先使用 [.vscode/tasks.json](.vscode/tasks.json) 中以 `container` 开头的 VS Code 任务；不要使用直接调用本机 Cargo 的任务。
- 常用命令（从项目根目录调用，实际在容器内执行）：
	- `docker compose exec -T dev cargo check --workspace --locked`
	- `docker compose exec -T dev cargo build --workspace --locked`
	- `docker compose exec -T dev cargo build --locked -p admin_webapi`
	- `docker compose exec -T dev cargo build --locked -p client_webapi`
	- `docker compose exec -T dev cargo build --locked -p task_service`
	- `docker compose exec -T dev cargo clippy --workspace --lib --bins --locked -- -D warnings`
	- `docker compose exec -T dev cargo fmt --all`
- Cargo 下载缓存和编译产物保存在 Docker 命名卷中；沿用容器的 `CARGO_TARGET_DIR=/cargo-target`，不要改到 Windows 本机目录。
- Docker 不可用、镜像下载失败或缺少工具时，优先排查容器服务、网络和镜像配置；无法解决时说明阻塞原因，不得转为在本机安装组件或使用本机工具链。
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
- 工具配置源文件为 `repository_tool/appsettings.toml`，构建复制为可执行文件旁的 `repository_tool.appsettings.toml`，默认按工具位置读取。通过 `--target business`、`--target log` 或 `--target event-bus` 选择写模型；目标缺少连接时不得回退其他模型。同物理库也分别执行各模型迁移，不在宿主启动时自动迁移。
- `migrate --to` 与 `migration-script --from/--to` 共用所选模型的版本解析及固定 Up/Down 定义；0 表示未应用迁移。升级应用 `(from, to]`，回退逆序撤销 `(to, from]` 并删除对应历史，目标版本自身保留。执行与历史修改必须在同一事务和迁移锁内完成，不使用 `CASCADE` 删除模型外对象。
- `migration-script` 只离线导出所选模型的迁移 SQL；不得在导出过程中连接数据库或执行 DDL。`--idempotent` 按该模型历史跳过已应用的升级或已撤销的回退步骤。新增迁移同时提供固定 Down 定义，不从当前实体或运行时数据库猜测旧结构。
- `json-index` 只读比较模型拥有的表与 `jsonbIX_` 保留名称，不执行生成的 DDL。索引声明和实际 JSON 属性映射归属实体组件；索引检查使用当前实体清单，历史迁移使用独立固定定义，分区唯一索引必须包含分区键。
- 常用命令：
	- `docker compose exec -T dev cargo run --locked -p repository_tool -- migrate --target business`

## 分层边界

- HTTP 相关关注点、路由注册和宿主装配应放在 `presentation/*` 与 `project_core/*`。
- 对外 HTTP 调用应复用 `http_client::HttpClientFactory` 这类宿主级工厂，由 `project_core/*` 或运行时宿主统一创建并通过上下文注入；不要在应用服务或基础设施调用点长期持有自行创建的 `reqwest::Client`。
- 业务流程与编排应放在 `application_service*` 系列 crate 中。
- 跨宿主契约应放在 `application_interface`，DTO 和配置模型应放在 `application_model`。运行资源上下文放在 `application_runtime`，JWT 编解码归属 `infrastructure/jwt_auth`，HTTP 工厂归属 `infrastructure/http_client`；不得回填到接口层。DTO/契约使用 common 时关闭默认 runtime 特性。
- WebAPI 业务连接、日志查询和业务服务工厂通过 presentation 的资源工厂显式注册，公共宿主只运行资源健康检查管线。模块通过 `DatabaseModules` 按类型注册，禁止重复覆盖；业务事务使用 `ModuleDatabase::begin`，事务入队必须验证连接归属。
- 文件、邮件、短信提供商契约归属各自基础设施组件，应用接口可以单向兼容导出；基础设施不得直接或间接反向依赖应用 crate。只需文件契约时关闭 file_storage 的默认 local 特性。
- WebAPI 业务能力按 `webapi_support::services::ServiceKey` 独立注册，入口只选择启用功能所需的能力；不要恢复全量 ServiceFactory。新增 Feature 必须在 `validate_resources` 中声明所需能力，处理器通过 `services().能力()?` 或 `get::<Key>()?` 调用。LLM 默认实现需显式启用 webapi_support 的 llm 特性。
- TaskService 业务资源通过 presentation 的可选 `build_resources` 注册；公共调度核心不得固定要求邮件、短信、文件或 HTTP 工厂。具体日志连接由入口构建，继续使用核心传入的 ID 生成器和统一异步停机流程。
- 新增常驻后台服务通过 `common::background_service` 和宿主构建器或 Feature 钩子显式注册；注册时不 spawn，名称不得重复。错误和 panic 交给宿主停机，清理共用宿主绝对期限，超时 abort；不要绕开注册表另写不受管理的监听循环。`webapi.health` 已使用此机制，保留原有检查周期与项目。
- `webapi_core` 不得依赖 application_runtime、repository_core 或 SeaORM。CurrentUser 直接来自 application_model；JWT 编解码使用 jwt_auth，原 application_runtime::auth 仅兼容导出。
- WebAPI 资源工厂接收宿主已配置的共享 HTTP 工厂，文件提供商通过 `FileStorageResource` 显式注册；本地存储注册 `FileStorageResource(None)`，不能把缺少注册当作本地模式。邮件能力由有实际需要的入口装配，不回填到公共 AppState。
- 数据库纯配置位于 `common::database_settings`，连接选项由 `repository_core::connection` 构造；WebAPI、任务和工具保留各自默认参数，None 的 acquire/max_lifetime 不得擅自改为 WebAPI 默认值。
- LLM 运行调用通过 `llm::LlmClientFactory` 与 `llm::LlmModelConfigResolver`；具体实现由入口分别注册到 LlmClientFactoryResource、LlmModelConfigResource。数据库解析器留在 application_service_llm，不在服务内部绕过注入调用默认工厂或查询函数。默认解析器复用业务写池、不增加缓存。
- `application_runtime::context::LlmContext` 需启用 llm 特性；模型 CRUD 使用不含运行依赖的 LlmModelContext。LLM 应用与调用能力在启动阶段检查两个运行依赖，纯模型及会话查询不强制装配它们；保持既有取消包装和流生命周期。
- WebAPI 业务服务契约放在 `application_interface::services`，参数与结果只使用模型或通用类型，不暴露数据库连接、实体和 HTTP 上下文。`application_interface` 依赖 `application_model`，模型层不得反向依赖接口层。
- 请求处理器通过工厂返回的服务契约调用用例，具体实现仅在宿主适配器选择。使用服务工厂宏的 `实现类型 => dyn 契约 + 生命周期` 声明；新增操作同步修改契约与实现绑定，不在 handler 导入具体应用服务。
- `/Base` 的地区、字典和验证码用例统一通过 `BaseService`；地区与字典使用宿主只读连接，不复用写库上下文或额外缓存结果。验证码归属基础服务，保持五分钟有效及相同标记覆盖规则。
- 请求处理器沿用 `webapi_core::extract`；字段大小写和重复参数规则统一放在共享请求绑定层，不通过批量改 JSON 键名处理字典或动态内容。可空请求标量按 JSON/Query 来源声明对应转换器，禁止将空白转 null 的规则泛化到数组、对象、响应或第三方协议模型。
- JSON 请求模型使用 `JsonModel` 声明必填提示、集合最小长度及显式嵌套验证，属性名与默认值必须对照源 DTO；字符串形式的数值或日期用 `json(value)` 保留原转换器，不在处理器中拼接模型错误。只有全部格式转换和模型验证通过后才能调用应用服务。
- JSON 端点通过方法构建器的 `json_body("源参数名")` 声明正文参数身份，同一 DTO 在不同接口中可以使用不同参数名。空正文、根 null 和仅空白正文由共享绑定层处理，不向业务传递默认对象；`JsonWithBody` 保留原文并复用 GetRequestBody 的 BOM/gzip 文本规则，不重新序列化模型用于验签。
- JSON 输入编码由共享绑定层按 Content-Type 选择 UTF-8 或 UTF-16 小端及源端支持的别名；转码和 BOM 去除仅用于解析视图，原始正文继续供签名与回调读取。保留传输层无正文与空分块正文的区别，不支持的媒体类型或编码返回 ProblemDetails，不混入业务错误协议。
- JSON 模型声明 `#[json(type_name = "源协议类型全名")]`；源数组与 List 的差异通过集合字段的 `#[json(array)]` 显式说明。读取字符串、非可空布尔、对象与集合 token 时保留原始属性名和数组索引，先拒绝类型错误，再处理合法重复字段的末值覆盖；不要靠反解析 Serde 错误文本或重新绑定模型来补错误路径。
- 公共 JSON 读取保留任意精度数字；serde_json 的数字内部 MapAccess 协议不是真实对象，须按反序列化边界区分。不要把数字降为 f64，也不要仅凭用户属性名等于内部标记就把对象当作数字。
- HTTP 非可空数值字段及数值集合元素通过 JsonModel 标量声明逐 token 校验，复用纯转换函数并保留首个错误的路径和位置，不能先合并重复属性再校验。普通数值字符串不裁剪空白，i64 按源 LongConverter 单独处理；可空数值和动态文档不要自动套用该规则。
- 显式使用公共可空 JSON 标量/ID 转换器的数值和布尔字段，按源 NullableStructConverter 的独立读取范围生成诊断：路径为 `$`，位置相对于当前原始值，不能强行补回外层字段路径。非空可空值直接使用目标类型的 JSON 转换，不先猜测或转存为另一种数值类型；字符串、Query、动态文档及未声明的 Option 不自动推广这项行为。
- HTTP 日期/ID 以 String 承载时，用 JsonModel 的 `json(value, scalar_type = "协议类型")` 配合字段转换器声明诊断类型，不按字符串内容推断。HTTP ISO 日期复用 common::json_helper::date_time::parse_offset_iso，不回退内部日期或 Query 的宽松解析。
- 无显式偏移的日期统一通过 common 的本地时间转换；Unix 的夏令时缺口/歧义按输入日期查询标准时偏移，保留原始墙上时间，不用系统函数归一化后的时分秒替代。不要在各输入入口各自使用 latest/earliest 选择歧义时刻。
- JSON 完整根值后的额外内容由公共读取器使用实际字节位置识别；类型错误优先，根 null 也必须先完成尾部检查。语法错误保留已记录路径，正文参数提示由共享 HTTP 层装配，不通过扫描或重解析整份正文来替代原读取器。分隔符后的 EOF 根据键/值读取阶段识别，不能仅凭正文末尾的逗号判断；逗号恰为末字节与其后还有空白时分别保留源位置规则。未闭合字符串仅在原读取器确认 EOF 后诊断，非法转义/控制字符不能改报缺少引号，也不得覆盖更早的类型或深度错误。
- Query 模型派生 `common::json_helper::input::QueryModel`，复用 Serde 声明逐字段绑定并汇总错误；字段验证用 `#[query(validate = "函数路径")]`，分页沿用 `validate_page_size_query`。不得向业务传递部分成功模型，也不要通过删除错误参数、重复反序列化来模拟汇总。错误排序归属共享宿主的 ModelState 规则，不与签名文化排序混用。
- 分页返回使用共享 `PageListDto` 的可空列表，按对应源服务区分 `null` 与空数组；不要全局把空数组改成 `null`。控制器 JSON 的 i64 值统一输出字符串，包括 ID、文件大小和创建接口的单值返回；普通对象与异步数组复用 `common::json_helper::write_controller_json`，不要先转成 JSON 文档再猜测整数类型。动态 JSON 文档中的数字、i32 和 u64 保留数值，事件与第三方协议不套用控制器转换。
- 源 DTO 的 DateTimeOffset 字段保留明确日期标记；以 String 承载时使用 `date_time::serialize_offset_string` 或可空版本，不按内容推断普通字符串。HTTP 日期按百纳秒精度去除小数秒尾零，零偏移写 `+00:00`；缓存与 ETag 使用内部日期格式，缓存恢复的 JSON 文档不再次识别和改写日期文本。
- 对齐源 DTO 时保留属性声明顺序；序列化文本会参与 ETag 和内部快照，不能认为字段集合相同就可任意重排，也不要通过全局按键排序消除差异。
- JSON 编码复用 common 的共享写入器：控制器和内部快照使用源宽松编码，事件的默认 Web JSON 使用默认编码；不要直接用 serde_json 默认输出替代。RawValue 写回只重编码字符串并整理结构空白，保留数字原文、重复键和字段顺序；不把它转成 Value 后再输出。日期编码例外依据明确类型，不根据字符串内容猜测。
- 原生 f32/f64 的 JSON 输出遵循源默认 G 格式，保留负零，指数使用大写 E、显式符号及至少两位数字；不得把浮点格式化规则套用到 RawValue/Number 的数字原文或 decimal。
- JSON 容器深度上限统一由 common 声明为 64；RawValue 内部的对象和数组也要计数，字符串中的括号不计数。流式响应数组逐项使用包含外层数组深度的写入入口，不得通过逐项重置规避上限；输入超深不能变成业务成功或内部异常。
- JsonModel 的字典字段使用共享 Dictionary 声明，值类型与源诊断名称由模型/宏提供；JsonNode 等动态值保留文档入口的错误路径，字典键仍是独立路径段。特殊字符的 JSON 路径格式复用共享读取器，不重命名实际键或在业务层拼接绑定错误。
- 分页页码、每页条数和总数使用 `i32`，偏移复用共享 `page_offset` 的环绕运算；只限制每页 1 至 100 条，不统一拒绝非正页码或改写为第一页。普通列表保留先计数再查询的顺序；队列和日志组件在查询前校验偏移。
- OpenAPI 的查询 DTO 显式声明 `#[into_params(parameter_in = Query)]`；分页响应与组件注册使用 `PageListDto<具体 DTO>`，不要使用会丢失泛型名称的类型别名，避免不同列表覆盖同一 schema。
- 对应字符串 `Contains` 的数据库查询复用 `repository_core::extensions::contains_pattern`，将 `%`、`_` 和反斜线按字面匹配；是否裁剪空白由所属用例决定。
- 数据库实体、关系与查询辅助代码应放在 `repository`。
- 与具体提供商相关的集成应放在对应的 `infrastructure/*` crate 中，不要在应用服务内部通过分支处理。

## 改动落点

- 新增或调整管理端/客户端 HTTP 接口时，优先改 `presentation/admin_webapi/src/features` 或 `presentation/client_webapi/src/features`，并检查对应宿主 `src/lib.rs` 的 profile/feature 注册。
- 共享的 WebAPI 中间件、鉴权、签名校验、限流、OpenAPI 装配，优先改 `project_core/webapi_core/src`，不要在单个宿主重复实现。
- 控制器使用 `webapi_core::routes::WebApiRoutes` 与同模块的方法构建器注册，匹配信息来自实际端点声明。静态路径段不区分 ASCII 大小写并允许一个末尾斜杠；匹配后恢复原 URI，签名、缓存、日志和路径参数不能使用小写后的请求值。方法不匹配先返回 405 与显式 Allow，GET 不自动支持 HEAD；Action 守卫使用 route_layer，不拦截方法拒绝。健康检查、Swagger 与静态文件分别保留自身规则。
- 后台任务注册、轮询和宿主级配置装配，优先改 `project_core/task_service_core/src` 与 `presentation/task_service/src/lib.rs`。
- 业务规则、应用编排和用例流程放在 `application/application_service*`；如果只是换基础设施实现，不要把分支逻辑塞回应用服务。
- SeaORM 实体、关系、查询扩展和软删除行为放在 `repository/src`；数据库迁移定义放在 `repository_tool/src/migrator`。
- 横切能力默认行为或宏展开逻辑，优先改 `source_generator/source_generator_core`，必要时同步修改 `source_generator/source_generator_runtime`。

## 配置与宿主约定

- 文件删除只软删除记录，不删除物理文件或云对象。`GetFile`/`GetImage` 只读取本地文件，不自动把云文件下载到公开根目录；云访问使用 `GetFileUrl`。原文件下载与静态资源的范围处理不可混用，图片缩放返回 PNG，未缩放时保留附件响应。
- 云存储复用宿主 HTTP 客户端工厂。OSS CRC64 使用反射 ECMA 多项式及前后取反（CRC-64/XZ），按实际传输字节计算；下载重试必须重置摘要并重新覆盖文件，等待上一轮文件写入收尾，不拼接残片。临时地址按配置前缀原文拼接，不自动修正斜杠。
- 阿里云与腾讯云上传均以远程目录加本地存储文件名作为对象键，展示文件名只设置 Content-Disposition。这是用户明确选择的源项目缺陷修正，不恢复腾讯云直接以目录作为对象名的行为。
- OSS 上传按对象名扩展名使用所属 SDK 的 Content-Type 映射，未知类型回退 `application/octet-stream`；展示文件名只影响 Content-Disposition，不复用 WebAPI 静态文件的类型映射。
- OSS GET 收到响应头后结束宿主 HTTP 期限，后续读取保留 SDK 期限。203 正文在请求层完整缓冲后才能创建目标文件；缓冲失败属于请求层重试，不提前截断本地文件。
- COS 默认下载先 HEAD，再按 20 MiB 阈值和 10 MiB 分片、五路并发传输；仅重试失败分片，全部完成后顺序合并并检查总长度。保持 SDK 默认关闭 CRC，Range 参与签名；使用宿主工厂的原始流式客户端，不能自动解压对象或为每片新建长期 HTTP 客户端。取消重试前等待分片文件写入收尾。
- COS 上传按所属组件默认阈值分片并检查同名未完成上传，以本地分片 MD5 核对 ETag 后续传；不能只凭分片编号跳过数据。正文发送 Content-MD5，ACL、附件名、长度及实际查询参数参与签名；每次重试重新定位文件，完成清单按编号排序，普通失败不自动 Abort 或删除云对象。
- COS 原生删除与上传控制操作共用组件请求层，按 SDK 异步链执行签名、重试和完整响应读取；保留 204 成功语义。业务文件软删除不得因此调用云对象删除。
- 临时地址契约区分 `Ok(None)` 和生成错误，不把签名或配置异常吞成空地址。OSS 的桶、对象、区域和凭据在调用时校验；对象长度按 UTF-16 码元计数。公开文件直接拼接地址，不因未使用的签名配置无效而失败。
- 存储临时地址有效期使用有符号 `TimeDelta`，按 TimeSpan 的 i64/100ns 精度处理，不改回只能表达正值的标准库 Duration。COS 默认永久凭据每次派生 600 秒密钥，签名区间与密钥区间分开；零或负的签名秒数回退密钥区间，不添加凭据缓存。

- `/healthz` 输出纯文本 `Healthy` 或 `Unhealthy` 并禁止缓存；业务读写库、日志写库、业务缓存和事件后端都参与，只有管理端检查日志读库。缓存只写固定十秒键，不增加锁、读取或删除探测。后台检查由共享宿主管理并参与停机取消。
- 静态文件由管理端档案显式启用，使用文件根目录（默认 `wwwroot`）；客户端文件根目录默认工作目录且不公开。不能因选择本地存储就自动开放两端 `/uploads`，也不要让静态文件覆盖 API 或自动返回目录首页。

- 一个宿主只创建一个 ID 生成器，业务、日志、队列和数据库事件共享克隆。自动节点使用 `[identifier_generator]` 的共享 Redis、前缀和租约，不得改回业务缓存占位、宿主专属前缀或无所有者校验的续期。固定编号必须同时提供数据中心和机器编号。
- ID 租约在分配成功后立即独立续期；失效时停止发号，退出保留 Redis 占用至自然到期。不能吞掉发号错误、回退固定节点或在运行时改写节点身份。
- 三个运行时宿主的默认配置路径定义在各自 `presentation/*/src/lib.rs`，环境变量 `APP_CONFIG_PATH` 会覆盖默认值，传入路径不带 `.toml` 后缀。
- TaskService 的配置环境按 `APP_ENVIRONMENT`、`DOTNET_ENVIRONMENT` 选择，默认 `Production`；基础文件与相应环境文件都参与日志过滤配置监听，不能因未设置 APP_ENVIRONMENT 就跳过 Production 覆盖。
- 运行或排查配置问题时，优先从宿主 `appsettings.toml` 和 `project_core/*/settings` 的加载路径开始，不要先怀疑业务层。
- `appsettings.toml` 中的 `component` 字段用于选择基础设施实现；优先通过配置和宿主注册切换实现，而不是在应用层写条件分支。
- `repository_tool` 固定读取可执行文件旁的配置，不接受命令行连接覆盖或环境覆盖；必须显式选择目标，数据库迁移统一通过容器执行：`docker compose exec -T dev cargo run --locked -p repository_tool -- migrate --target business`。

## 持久化与迁移

- PostgreSQL 实体使用系统列 `xmin` 的 `row_version`；普通单行写入统一使用 `VersionedActiveModel` 的 `insert_versioned`、`update_versioned`、`delete_versioned`，批量新增使用 `insert_many_versioned`。保留读取时的原始版本，不能在写入前查新版本来绕过冲突。
- 用户姓名 `User.Name` 使用 `citext`，保留原始文本；用户名、手机和邮箱仍是 `text`。用户名查重归属应用服务，不添加用户名或外部账户绑定的数据库唯一约束；调整实体类型或索引必须同时提供已登记的迁移，兼顾空库及已有库。
- 角色编码、功能标记、授权组合、路由组合、LLM 应用 Code、产品 SKU 和订单号不因服务层查重而增加数据库唯一约束。`UserInfo.UserId` 的一对一唯一约束覆盖软删除记录；迁移发现重复资料时失败回滚，不自动删改资料。任务定义的有效记录唯一约束继续保留。
- 新增与 xmin 回读必须使用同一事务或保存点；版本字段不能写入或创建为普通列。编写新迁移时使用 `repository_core::versioned::create_table_from_entity` 构建并固化建表 SQL，已登记迁移不得再读取当前实体生成结构。集合更新和原生 SQL 仅用于有明确原子条件的操作，不默认获得行版本保护。
- PostgreSQL 分区能力位于 `repository_core::partition`；声明使用 `PartitionTable` 派生宏，新迁移从实体取得并固化分区元数据，运行时维护读取当前声明。历史迁移保留其原始元数据，初始子分区仍按 SQL 生成时刻计算。时间固定 UTC+8，雪花位布局复用 `identifier_generator::layout`，不要复制另一套时间常量。
- 分区维护仅补未来范围，保留已有边界；不得自动转换普通表、填补历史空洞或删除旧分区。同一存储的实例必须共享维护锁连接、前缀和数据库标识，不能使用宿主独有的业务锁前缀。
- 对带 `DeleteTime` 的实体，查询活跃数据时优先使用 `SoftDeleteEntityExt::find_active()` 或 `find_by_id_active()`，不要直接从 `Entity::find()` 开始再忘记补过滤。
- `repository::file` 的物理表名是 `StoredFile`，与 NetEngine 的实体映射一致；旧库通过已登记的重命名迁移升级，不修改历史 `File` 建表快照，也不通过删表重建来迁移文件记录。
- 查询可空父节点或顶层树结构时，显式使用 `is_null()`，不要依赖 `Column.eq(None)` 去表达 `NULL` 语义。
- 当前模型变化通过容器内 `repository_tool migration-add <名称> --target <模型>` 生成固定 Up/Down、目标模型和登记文件；审查后重新编译，再单独执行迁移。业务生成目录为 `repository_tool/src/migrator/generated`，日志和事件归属各组件的 `src/migrations/generated`；生成文件与模型快照一起提交。不要修改已应用迁移或基线，不从当前实体重建历史结构。
- `repository_tool/src/schema.rs` 装配当前业务关系模型和 JSON 索引清单，具体索引声明归属实体组件；旧 `snapshots` 保持不变。`migration-pending` 检查未生成变化，默认 `migrate` 拒绝尚未生成迁移的模型变化。生成不连接数据库，不能以读取线上结构替代模型快照；重命名与数据转换必须审查生成操作，不把自动删除/新增当作无损重命名。
- 业务迁移历史表使用 `__EFMigrationsHistory_Business`，日志使用 `__EFMigrationsHistory_Log`；三个模型历史表均固定在 public，字段为 MigrationId 和 ProductVersion；历史表名由所属模型声明。只改 schema 语句而不登记对应模型版本，会导致环境之间行为不一致。
- 数据库日志调用现场只做快照与非阻塞接收，字节预算必须覆盖队列和重试批次；失败保留批次与稳定 ID，不删除部分临时文件。文件识别统一使用组件的严格命名规则，恢复和发布不得覆盖已有文件。
- 本地文件日志与数据库日志分别配置有界缓冲，均通过异步 shutdown 排空。小时文件重试必须退回未确认段的起始偏移；尾部恢复必须先成功保留原字节再截短，清理按最后写入时间并跳过等待重试的文件。
- Critical 使用 `logger::critical!`，不要用普通 Error 替代独立级别；最低级别为 critical 时不得接收普通 Error。日志目录统一为工作目录下的 `Logs`。
- 日志级别解析由 `logger` 统一提供，非法级别在提供者启动后台任务前报错，不能打印提示后静默忽略。空字符串不产生过滤规则；保留现有 Rust 级别简写和源枚举数字、名称组合语义。
- 日志类别使用不区分大小写的前缀/单通配符匹配，按 UTF-16 长度和源配置枚举顺序选择规则，不裁剪类别或依赖 HashMap 顺序。宿主各配置源在合并前经过 `logger::log_filter_source`，保证不同大小写的环境覆盖仍是同一个类别。
- 控制台与存储分别过滤日志，`logger.providers` 的适用规则（包括 default）优先于全局类别规则，不在它们前面再挂全局过滤器。APP 环境配置使用 `logger::log_filter_environment`，保留提供者名称及类别中的字面点号，不让配置库的小写化破坏名称匹配。
- 日志过滤重载通过 `LoggerRuntime::reload_filter` 整体替换快照，只更新过滤规则。调用点必须保持动态判定，不能缓存旧的拒绝结果；重载不重建连接或切换存储提供者。宿主通过 `logger::load_host_configuration` 捕获来源并监听文件，环境过滤配置保持启动快照；删除来源撤销覆盖，解析失败先清空该来源但不发布更新。停机先停止监听，再用剩余期限排空日志。
- 日志运行时正常退出必须等待 `shutdown`，使用宿主剩余预算排空到文件；后台故障诊断不回写自身日志链路，不输出正文或连接凭据。
- 日志标准指标采用 OpenTelemetry 拉取回调，保留源 meter、指标名称、单位及 level/operation 标签；业务日志调用只更新原子计数。采集端由宿主在日志初始化前装配，回调不得执行磁盘扫描、数据库查询、导出或日志写入。
- 日志查询使用 `logger_database::query::LogQueryService` 的独立只读连接；任务执行日志经过配置的日志组件，不直接写业务库的 `Log` 表。日志保留清理必须按项目过滤，落盘积压目录必须隔离存储目标。
- 事件是所有订阅实例各自消费的广播，不能改成 Redis 消费组或竞争领取。后端由 presentation 选择，公共宿主只依赖 `event_bus` 契约；事件迁移历史为 `__EFMigrationsHistory_EventBus`。
- 事件数据库的 Position 必须遵循提交顺序，不能用雪花 ID 替代游标；状态行包含软删除记录且不能随消息清理删除。监听应幂等并配合取消，发布不加入业务事务，需要事务一致性时先事务入队再发布。
- `llm_app` 当前 schema 与 .NET 对齐时不包含 `ApiKey`、`BaseUrl`，备注列名是 `Remark`；改实体字段前先核对库表。
- LLM 模型和应用配置管理使用业务写库，对话及其关联信息使用宿主业务只读连接。配置锁在成功和错误路径都等待释放；仅裁剪源服务明确裁剪的字段，可选备注、系统提示词和 ExtraBodyJson 保留非空原文。同值编辑不刷新 xmin，删除不覆盖原修改者。
- LLM 扩展参数从应用模型到协议装配保留原始 JSON 子节点，不能经 Value 对象去重后发送。保存的 ExtraBodyJson 在调用枚举时拒绝顶层重复键，之后才过滤空键/null；试调用的 ExtraBody 保持字典同名键后值覆盖规则。嵌套重复属性与数字原文都应保留，基础字段冲突按大小写敏感规则检查。
- LLM 三种协议的流式正文共用增量文本读取器：按源 StreamReader 的读取边界识别 BOM，保留跨分片字符，再按 CR/LF 拆行；不要直接对原始字节拆行或等完整响应后解码。流式编码不从 Content-Type 推断，非流式响应继续使用独立的字符集规则。
- LLM HTTP 最终响应只接受 2xx，不用仅拒绝 4xx/5xx 的检查代替。流式在响应头后检查，非流式先缓冲正文再报告状态或字符集错误；请求明确使用 application/json; charset=utf-8。失败状态不解析为成功事件或空对话结果。

## 鉴权与请求签名

- 控制器执行顺序为认证、续签与权限检查，随后提取并验证参数，再执行签名、响应缓存、请求队列及 ETag 等 Action 过滤器，最后调用业务函数。认证与正文限制通过方法构建器的 `map` 装配，Action 过滤器使用 `map_action`；不要在各 handler 重复预校验或重复绑定参数。
- `ReplayBody` 在参数提取或过滤器实际读取时缓存原请求，各回放流持有独立偏移：30 KiB 内存阈值，超出使用随句柄关闭自动回收的匿名临时文件；multipart 文件摘要必须逐块计算，禁止重新整文件读入内存或套用参数快照的 10 MiB 上限。临时目录遵守 `ASPNETCORE_TEMP`，失败不得隐式换目录；缓存读取失败使用共享内部异常响应，不作为字段格式错误暴露系统信息。
- 普通文件与 Ueditor 上传路由禁用默认 body 限制，Ueditor 业务大小/类型规则由其配置决定。Ueditor 仅挂认证与权限守卫，不使用管理端签名包装器。
- 普通上传使用共享 `uploaded_file::FormFile` 在 Action 过滤器前读取完整表单并绑定首个 `file` 文件，不在两端 handler 重复解析。缺少文件是模型错误，命名空文件是业务错误；Content-Disposition 的参数名、扩展文件名和空文件名识别由绑定、签名及参数快照共用。媒体类型选择使用 `require_multipart`，非匹配的非空 Content-Type 先返回 415，不截获其他方法的 405。

- 路由授权与功能列表的应用服务直接读取业务数据库，不在服务内部缓存授权结果或维护授权版本快照。客户端 `GetUser`、`GetFunctionList` 按 NetEngine 的接口声明使用 60 秒响应缓存，完整 Authorization 参与缓存键；认证、续签和路由权限检查必须在响应缓存之前执行。未绑定功能的路由通过权限检查，但仍遵守所在宿主的登录和签名验证；已绑定路由按有效授权记录判断。
- 响应缓存声明独立且全局唯一的处理器身份，不能用权限模块配置替代；缓存键同时包含 HTTP 方法、路径、路由参数和请求参数。通过共享 `extract::Json` 或 `text_response::Text` 保留对象结果及状态码，不缓存 null、文件、流或异常响应；字符串缓存命中后恢复为字符串结果。ETag 仅按接口显式声明挂载，摘要与缓存共用内部 JSON 快照，不能对媒体协商、字符集转换或压缩后的正文求摘要。
- 请求路径解码、点段归一化和 URI 重新转义统一通过 `webapi_core::request_path`。路由、缓存键、静态文件使用解码值，签名、完整日志地址和重定向使用 URI 表示；查询串原样保留，禁止各处重复百分号解码或把 `%2F` 当路径分隔符。
- 响应缓存按公共对象 JSON 序列化原始类型，不直接复制 HTTP 正文。用户创建时间保留 DateTimeOffset 类型：首次 HTTP 响应为 ISO，缓存值沿用公共日期格式；不得扫描字符串内容并猜测日期。只有缓存过滤器执行期间收集内部 JSON，普通响应与 ETag 保留 HTTP 表示。对象克隆保留 ISO 日期，不套用公共对象日期转换器。
- 请求队列限制按独立处理器身份、可选完整 Authorization 和共享请求参数生成 MD5 键。阻断重复请求返回 400 与 `请勿频繁操作`，等待模式每 200 毫秒重试；正数 `expiry_seconds` 从获取锁开始计算并自然到期，0 使用 60 秒租约且请求完成后等待解锁。不要改为完成请求后再延长冷却窗口，也不要用 `ManuallyDrop` 泄漏锁句柄。
- JWT 使用 `userId`、`tokenId` 声明，公共宿主验证签名、颁发者、受众和有效期；不要另把 UserToken 表或缓存当作令牌黑名单。续签在权限判断前执行，返回 `NewToken` 响应头并向跨域客户端公开；已认证但无权限返回 403。
- 短信验证码按原始手机号构造 `VerifyPhone_` 键，固定五分钟有效，登录和改密不消费验证码。发送使用 `MessageTask.SendSMS` 队列，与缓存写入一起等待完成；缓存失败不取消已入队的任务。短信改密在同一事务中更新密码并物理删除有效 Token 记录，旧密码改密保留 Token；两者不额外写 DataUpdateLog。微信首登的用户和外部绑定必须在同一事务中提交。
- 权限初始化通过管理端 `POST /Authorize/InitData` 显式执行，仅允许 Development 环境；文件定位在可执行文件旁的 `InitData`，业务写入归属授权应用服务并使用单一事务，不加入迁移或宿主启动流程。
- 默认权限模块名为 `Admin.WebAPI`、`Client.WebAPI`；业务适配器在路由同步和权限校验时统一移除 HTTP 路径的前导斜杠。宿主启动不自动同步权限路由，两端通过 `UpdateRoute` 显式同步各自模块，只收录需要鉴权且未标记匿名的路由。
- 权限路由同步在单一事务中提交；空清单也要软删除当前模块已移除的有效路由。已软删除的路径再次出现时创建新记录，不恢复旧功能绑定；沿用源普通保存行为，不额外生成 `DataUpdateLog`。

- JWT 与请求签名相关共享能力位于 `project_core/webapi_core`；涉及 Token 校验、续签或签名验证时，优先在共享宿主层定位实现。
- 请求签名中间件在 `debug_assertions` 下会直接放行；不要用 Debug 运行结果判断生产环境签名流程是否生效。
- 生产签名校验依赖 `Token`、`Time`、`Authorization: Bearer <jwt>` 请求头，并组合请求路径与请求体参与摘要计算。
- 管理端受保护路由先完成 JWT 认证、续签和权限检查，再执行请求签名；摘要不匹配时保留已生成的 `NewToken`，内部异常响应不保留续签头。签名时间按源过滤器的 `Time + 3 分钟 > 当前时间` 判断，不另加未来时间限制。重复表单字段复用 StringValues 合并规则，跳过空字符串、保留空白和顺序；字段名与文件字段名按宿主文化稳定排序，不用字节排序或小写排序替代，文件仍逐个计算摘要。
- JWT 认证失败使用空体 401 和 `WWW-Authenticate`，不要与签名过滤器的 JSON 401 或业务异常混用。内部异常统一经过共享错误响应：Development 返回 500 诊断信息，其他环境返回 400 和 `系统全局内部异常`，不向调用方输出数据库等底层错误。
- WebAPI 环境判断统一使用 `webapi_core::environment`，按 `APP_ENVIRONMENT`、`DOTNET_ENVIRONMENT`、`ASPNETCORE_ENVIRONMENT` 的顺序选择，默认 Production；配置覆盖、初始化权限和 Swagger 使用同一环境。Swagger 仅在 Development 注册，入口为 `/swagger` 和 `/swagger/v1/swagger.json`。
- CORS 由共享宿主统一处理：预检返回 204，按请求回传方法和请求头，只给允许的来源添加凭据、许可头与 `Vary: Origin`。静态资源也使用该策略，不全局公开响应头或添加私有网络许可；续签头只在实际产生时公开。
- 响应压缩只在非 Development 环境由共享宿主挂载，保留流式正文并按 NetEngine 默认 MIME 类型与 Brotli/gzip 协商规则处理；不要直接采用压缩库默认的最小正文大小、406 或响应头移除策略。
- 请求协议、连接地址与客户端 IP 通过共享 `RequestContext` 获取；代理只消费最右侧一跳 For/Proto，Host/Prefix 不转发。支付回调基地址取当前请求，不使用文件存储 URL；不要在业务处理器重新解析 X-Forwarded-For 或另加 X-Real-IP 回退。
- 微信预支付不额外加锁：JSAPI/App 对象缓存 1.8 小时，H5/PC 原始链接分别缓存 4.5/15 分钟。证书回源单独使用默认 60 秒锁并等待释放，验签后缓存并回读；不要将其替换为宏的自动续期策略，或统一对象缓存与字符串缓存的读取失败语义。
- 四个微信创建接口按源模板允许匿名，按未删除的原始订单号读取，不添加归属、支付状态或 trim 条件。订单/创建配置缺失返回空体 204；创建字段按模块首值读取，HTTP 签名配置按商户分组读取，可空 GroupId 必须使用 is_null()。
- 三个支付宝创建接口允许匿名，先读取对应模块各配置键的首值，再按原始订单号查询。PC/H5 只要求 State 为“待支付”，小程序使用订单创建人的有效绑定；缺失必要配置、订单或绑定返回 204。H5 的 quit_url 仅放在 biz_content。服务端响应按原始业务节点验签，不把重新序列化的 JSON 用作验签正文。
- 支付宝通知使用共享 `FormFields`，重复字段按顺序用逗号连接，文件不参与字段字典。回调公钥按 AliPay 前缀应用所在组选择，NULL 组使用 is_null()；验签通过后按源订单保存规则处理，只有“业务逻辑”类型返回 success，其他空结果为 204。保持行版本保护，相同值不重复写入；普通保存不额外产生 DataUpdateLog。
- 微信通知保持提供商的下划线字段，通过共享 `JsonWithBody` 绑定并保留原文验签。证书缓存命中不提前要求商户配置；通知解密读取 WeiXinPay 模块首个 MchApiV3Key。未知事件、无有效订单、验签或解密失败返回 501 与固定 FAIL/失败，成功返回 SUCCESS 且 message 为 null；退款通知先解密和解析再确认。已支付订单直接确认，不增加额外审计写入。
- 退款及退款查询按当前 NetEngine 保留无参模板契约，不自动添加 HTTP 路由、订单归属检查或业务结果映射；接入真实退款用例应先明确业务需求。支付缓存、证书和通知明文使用 `common::json_helper::json_to_object` 的模型字段大小写规则；不要给内部 JSON 套用 WebAPI 必填字符串、所有数字字符串转换或全量字典键改名。内部日期转换与 HTTP ISO 输入规则分别声明。
- HTTP/HTTPS 通过共享监听器提供连接协议与地址，TLS 证书归属可选 `host.tls` 配置；证书失败不得退回明文。重定向端口在显式配置和环境配置之后使用已绑定的 HTTPS 端口，不能把请求行中的协议当成 TLS 连接依据。
- `jsonwebtoken` 11.x 在本仓库需要启用 `rust_crypto` 特性；如果调整 JWT 依赖或相关 crate，先确认不会引入运行时 panic。
- RSA 请求字段解密由 `webapi_core::rsa_decrypt` 按需启用，模型通过 `RsaDecrypt` 派生与 `#[rsa_encrypted]` 标记字符串字段。使用 OAEP-SHA256 与 Base64，密钥来自宿主可选 `[rsa]` 配置；不要自动给现有接口或响应模型添加解密。Debug 对非法密文记录固定消息并保留原值，Release 交给共享内部异常响应，禁止记录密钥、密文或明文。

## 代码生成与宏

- 通用自定义行为使用 `#[behavior]` 与 `InvocationBehavior`，复用稳定方法键；按声明顺序进入、逆序退出。流行为必须显式声明并随枚举生命周期执行，不在后台脱离调用方消费；已有缓存与流日志复用公共管道，禁止另外维护平行实现。使用方式见 [宿主扩展](docs/host-extensions.md)。

- `#[logging]` 不是默认全量挂载；只在需要的方法上显式 `use source_generator_runtime::logging;` 后再添加属性。
- `#[logging]` 支持同步和异步方法，默认处理 `Result`（包括别名），普通返回值或单元值使用 `result = false`；异步序列显式使用 `stream = true`，只在枚举期间记录生命周期，最多捕获前 100 项。`#[retry]`、`#[cacheable]`、`#[concurrency_limit]` 仍仅支持 `async fn`；改方法签名或扩宏时先检查这一前提。
- `#[retry]` 默认最多重试 3 次、每次等待 0 ms；`#[cacheable]` 默认 `ttl_seconds = 60`；`#[concurrency_limit]` 默认使用 `self.lock`、并发数 `1`、非阻塞。
- `#[retry]` 通过 `CancellationError`（或显式 `is_cancelled`）识别取消，取消错误直接返回。调用方令牌通过 `cancellation` 显式传入，等待取消通过 `OperationCancelled` 转换并保留分类；先判断次数耗尽，再观察令牌，不强制中断执行中的方法体。
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
