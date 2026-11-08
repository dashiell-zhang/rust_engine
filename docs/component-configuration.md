# 组件配置归属与宿主装配

AR-12 将具体文件、短信和 SMTP 配置交还组件及表现层，现有 `appsettings.toml` 无需改名或迁移字段。

## 配置从哪里读取

两个公共宿主仍由 `Settings::load(profile)` 调用既有 `logger::load_host_configuration`，读取基础 TOML、环境 TOML 和 APP 环境变量。加载后保留同一份合并结果；`Settings::extension<T>()` 仅从该启动快照解析入口自有的配置，不重新打开文件或读取环境变量。

TaskService 的路径、消息配置与 `FileStorageConfiguration` 分别从同一快照直接解析，再组合为运行态 `TaskComponentSettings`；不对提供商配置使用 `serde(flatten)`，避免中间缓冲丢失配置库的标量转换。例如 `APP__FILE__ALIYUN_OSS__USE_INTERNAL_ENDPOINT=true` 仍按布尔值解析，现有 `[file]` 节和环境覆盖顺序不变。

WebAPI 的环境优先级仍为 `APP_ENVIRONMENT`、`DOTNET_ENVIRONMENT`、`ASPNETCORE_ENVIRONMENT`；任务宿主仍为 `APP_ENVIRONMENT`、`DOTNET_ENVIRONMENT`，默认 Production。`APP_CONFIG_PATH` 继续覆盖宿主默认路径。日志过滤保留原有来源信息和文件监听；此启动快照不引入提供商热切换。

档案的可选 `validate_configuration` 回调在创建连接、HTTP 工厂等资源之前执行。管理端、客户端校验文件配置，任务入口校验文件、短信和邮件配置；资源工厂随后从相同快照取得配置。错误不输出配置值或凭据。运行态 Settings 不再直接实现 Deserialize，必须通过加载入口建立包含快照的完整状态，避免绕过快照而让扩展配置静默使用默认值。

## 配置归属

| 配置 | 所属位置 | 装配位置 |
| --- | --- | --- |
| `file.component` | [file_storage_support](../presentation/file_storage_support/src/lib.rs) 的提供商选择枚举 | 两个 WebAPI 的文件构建函数及任务入口 |
| `file.aliyun_oss.*` | [阿里云文件组件](../infrastructure/file_storage_ali_cloud/src/lib.rs) | file_storage_support |
| `file.tencent_cos.*` | [腾讯云文件组件](../infrastructure/file_storage_tencent_cloud/src/lib.rs) | file_storage_support |
| WebAPI `file.root_path`、`file.public_base_url` | [WebAPI 公共配置](../project_core/webapi_core/src/host/runtime/settings.rs) | 静态资源与业务文件上下文 |
| TaskService `file.root_path`、`file.public_base_url` | [任务入口配置](../presentation/task_service/src/configuration.rs) | 任务业务资源 |
| `sms.component`、短信组合配置 | 任务入口配置 | [任务资源工厂](../presentation/task_service/src/host_support.rs) |
| `sms.aliyun.*`、`sms.tencent.*` | [阿里云短信](../infrastructure/sms_ali_cloud/src/lib.rs)、[腾讯云短信](../infrastructure/sms_tencent_cloud/src/lib.rs) | 任务资源工厂 |
| `message.smtp.*` | [email_smtp::SmtpEmailSettings](../infrastructure/email_smtp/src/lib.rs) | 任务入口的 MessageSettings 与资源工厂 |

`presentation/file_storage_support` 只复用三个入口相同的文件提供商选择和构造，不汇总其他配置，也不依赖公共宿主。它位于表现层，因为这里明确选择具体实现；文件契约和公共宿主都不引用它。后续增加文件提供商，只改组件及选择它的表现层；不向公共 Settings 增加提供商字段。短信和邮件的业务选择目前仅由任务入口负责。

## 兼容映射

| 既有约定 | 当前处理 |
| --- | --- |
| `file.component` 缺省 | 仍为 `local`，另外接受 `aliyun_oss`、`tencent_cos` |
| 云存储字段缺省 | 字符串为空，`use_internal_endpoint` 为 false，由组件类型统一提供默认值 |
| `file.public_base_url` | 仍由装配层写入提供商的运行时 `url`；嵌套配置中的 `url` 继续不参与绑定 |
| `file.*.endpoint` | 保留旧字符串字段的解析；旧装配未使用它，当前仍按原规则构造 API 地址，不因移动配置而启用自定义端点 |
| WebAPI 文件根目录 | 缺省、空白及旧 `data/files` 值仍按档案回退；管理端 `wwwroot`，客户端 `.` |
| 任务文件根目录 | 缺省、空白及旧 `storage` 值仍解析为 `presentation/task_service/storage`；邮件附件内容目录继续取工作目录，不改成此存储根目录 |
| `sms.component` 缺省 | 仍为 `aliyun`，另接受 `tencent`；凭据字段缺省为空 |
| SMTP 缺省 | 端口仍为 587，host/username/password 为空；实际发送时的校验与 STARTTLS 行为不变 |

管理端仍显式公开静态文件，客户端仍不公开文件根目录；选择云提供商不会改变这个策略。文件提供商继续使用宿主传入的共享 HTTP 工厂；任务资源仍只创建一个 HTTP 工厂并传给文件和短信组件。

两个 WebAPI 没有邮件发送用例，因此不再绑定未使用的 `[message.smtp]`；旧配置文件保留此节仍可加载，但未使用节的字段类型不再由公共核心校验。任务入口继续完整校验其 SMTP 配置。配置文件本身的 TOML 语法错误仍在公共加载时失败。

新增组件配置时，先复用组件类型，再在入口声明选择与必要兼容映射；不要复制同一套凭据结构或默认值，也不要重新读取一遍配置文件。可按需要启用自己的验证回调，避免未选择的业务能力成为公共宿主的必需依赖。
