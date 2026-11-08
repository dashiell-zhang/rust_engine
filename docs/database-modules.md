# 业务模块、服务工厂与宿主装配

本文对应 [AR-01～AR-09 架构调整](netengine-architecture-review.md)。模块机制已接入三个现有宿主，不要求拆分现有业务数据库。

## 契约与运行资源

- `application_model` 保存 DTO，`application_interface` 保存业务服务契约；`file_storage`、`email`、`sms` 拥有提供商契约及请求类型。接口层原模块仅保留单向兼容导出，基础设施不得回引它们。
- `application_runtime` 保存数据库执行上下文；JWT 编解码独立为 `jwt_auth::EcJwtCodec`，旧 auth 模块仅单向兼容导出。共享 HTTP 工厂独立为 `http_client::HttpClientFactory`，提供商无需引入应用上下文、SeaORM 或 JWT。
- DTO、接口和事件契约关闭 common 的默认 `runtime` 特性。只使用文件契约时也关闭 file_storage 的默认 `local` 特性，避免带入本地文件实现。

## WebAPI 实现选择

`WebApiHostProfile.build_resources` 指向运行入口的资源工厂。两个入口各自的 `business_resources.rs` 先调用 `webapi_support::resources::build_resources` 创建模块连接和健康检查，再逐项选择业务能力。例如 `services::Base::register_default(&mut resources)?` 只注册 Base 的默认实现，不要求实现其他业务方法。

资源回调签名为 `fn(&Settings, bool, Arc<HttpClientFactory>) -> ...`，第三个参数是公共宿主已经配置命名客户端的同一个工厂，不能另建工厂。现有两端在回调中创建文件提供商并注册 `FileStorageResource`；本地文件模式显式注册 `FileStorageResource(None)`，缺少资源是装配错误。File 及使用默认文件上下文的 User/Article/Link/Site 能力均检查该资源。AppState 不再固定包含文件或邮件字段；当前 WebAPI 没有邮件用例，已取消无用的邮件实例，TaskService 邮件装配保留。

公共核心保留文件根目录、SMTP、数据库等原配置字段以兼容 TOML，但不因此创建对应业务资源。JWT 验签、续签和权限中间件仍属于公共 WebAPI 流程，编解码实现来自 jwt_auth，CurrentUser 直接来自 application_model。核心不依赖 application_runtime、repository_core 或 SeaORM。

请求处理器通过 `app_state.services().base()?` 获得业务契约。鉴权、续签、权限路由同步、普通业务和 LLM 均通过所选能力；服务生命周期借用 AppState，不借用临时工厂。注册项只保存构造函数和资源校验函数，不长期保存应用服务对象。

替换一项能力时，实现对应业务 trait，并在 presentation 调用 `services::register::<services::Base>(&mut resources, create_base, validate_base)`，不要再注册该项默认实现。构造函数签名由 `ServiceConstructor<K>` 约束，返回借用状态的 `Result<Box<dyn 契约>, AppError>`；校验函数接收 `&WebApiResources`，缺少依赖时返回错误。其他能力的注册不受影响。新增自定义能力可实现 `ServiceKey`，用 `services().get::<自定义键>()?` 获取，无需修改一个全量工厂接口。

默认具体服务构造继续复用 `generate_direct_service_factory!`。宏的 `implements = 契约类型;` 仍可用于完整 trait 实现，例如后台消息任务的 `TaskServiceFactory`，但 WebAPI 不再要求聚合所有业务方法的 ServiceFactory。

`WebApiResources` 使用 common 的按类型资源集合，重复注册报错。注册能力时立即校验资源；每个已启用的 `WebApiFeature::validate_resources` 还必须调用 `services::require::<能力键>(resources)` 声明自己需要的能力。公共宿主在启动日志与事件运行时之前执行业务策略和全部 Feature 的校验。缺少注册不能以空实现代替，访问层也返回共享内部错误而非 panic。

管理端选择授权、权限路由、基础、用户、角色、任务、日志、文章、文件、链接、站点和 LLM 管理能力；客户端仅选择授权、权限路由、基础、用户、文件、支付及 LLM 调用能力。`webapi_support` 默认不启用 `llm` 特性；现有两个宿主确有 LLM 入口，因此显式启用它。未启用时不引入 application_service_llm，契约仍可供自定义实现使用。

日志查询通过 `register_log_query` 单独装配，只有管理端选择它，并与日志读健康检查共享同一只读连接。客户端不创建查询服务或日志读连接。两个宿主仍检查业务读写库、日志写库、缓存和事件；只有管理端检查日志读库。新增模块的读写连接也参与检查，公共宿主并行执行并传递取消令牌。

## LLM 客户端与模型配置来源

`llm::LlmClientFactory` 负责从模型运行配置创建协议客户端；`llm::LlmModelConfigResolver` 负责读取运行配置。两者的签名均不暴露数据库连接。默认客户端工厂使用宿主 HTTP 工厂的 streaming_client，每次调用重新取得客户端，保持原三个协议和期限；默认数据库解析器位于 application_service_llm，克隆原 BusinessModule 写池，保留 ID、启用和软删除过滤，不缓存模型配置。

现有入口在注册 LlmApp/LlmInvoke 前分别调用 `llm_resources::register_default_client_factory` 和 `register_default_model_config_resolver`。替换其中一项时，改为 `resources.insert(LlmClientFactoryResource(Arc::new(自定义实现)))?` 或 `resources.insert(LlmModelConfigResource(Arc::new(自定义实现)))?`，并省略对应默认注册；重复类型会报错。无需修改 handler 或替换整个业务服务。

LlmApp（包含管理端试用）和 LlmInvoke 检查两个运行依赖，并借用资源组成 LlmContext；普通调用与流式调用均经过该选择。纯模型 CRUD 使用 LlmModelContext，会话查询仅借用只读池，不要求客户端工厂和配置解析器。application_runtime 的 llm 特性由 application_service_llm 启用；webapi_support 关闭 llm 时不引入协议组件及 LLM 应用实现。

原应用契约的取消包装继续覆盖配置读取、提供商调用、流消费和会话保存。解析器返回的查询 future 归调用方所有；自定义实现不可将查询脱离到不受管理的后台任务。独立使用解析器时可调用 get_config_with_cancellation。该调整不更改会话保存时机或将业务配置的新增/编辑校验改为外部配置源。

## TaskService 资源选择

`TaskServiceHostProfile.build_resources` 是可选回调，返回 `common::resources::Resources`；不需要业务资源的宿主填 `None`。核心不要求邮件、短信、文件或 HTTP 工厂，Feature 从 `TaskServiceFeatureContext.resources` 获取自己明确注册的资源。

当前入口的 `host_support::build_resources` 注册 `MessageResources`，共享一个 HTTP 工厂供短信和云文件调用，保留邮件及内容根目录。内置 Feature 在注册任务前检查 BusinessModule 与 MessageResources。配置模型保留原有可选/默认字段以兼容 TOML，字段存在不代表核心会创建对应资源。

具体日志提供商和日志数据库连接由 presentation 的 `build_logger_runtime` 选择，继续使用核心传入的同一 ID 生成器。核心保留 LoggerRuntime 的异步关闭、任务取消、执行租约及停机预算；资源重组不改变队列事务或调度规则。

## 模块身份与连接

`repository_core::module::DatabaseModule` 由所属业务仓库实现，声明固定的 `NAME` 和 `MIGRATION_HISTORY_TABLE`。模板业务类型为 `repository::BusinessModule`，名称 `business`。

每个类型对应一个 `ModuleDatabase<M>`，包含独立写池和只读池。只读连接仍启用 PostgreSQL 会话只读设置。`DatabaseModules::register` 拒绝重复类型或名称，`get::<M>()` 按类型查找，注册顺序不会改变目标。

连接池与模块连接的纯配置类型位于 `common::database_settings`，不引用驱动；旧 `webapi_core::startup::DatabasePoolSettings` 和 `repository_core::module::ModuleConnectionSettings` 保留单向类型导出。实际选项统一由 `repository_core::connection::build_postgres_connect_options` 构造。原 WebApiStartup 同名方法已移除，调用方需改用仓储组件。

WebAPI 默认最小连接数 5、连接期限 10 秒、获取期限 5 秒、最大存活 1800 秒；TaskService 保持最小 30、连接期限 50 秒；仓储工具保持最小 1、连接期限 50 秒。任务和工具原本未显式设置获取期限及最大存活，转换时传 None 保留驱动默认行为；所有入口仍保持最大连接数 30、空闲期限 300 秒、连接前检查及 SQL 日志默认关闭。配置显式提供的参数保持优先。

运行时可选配置：

```toml
[database.modules.business]
connection = "<业务写库连接>"
read_connection = "<业务只读连接；留空则使用同模块写库地址>"
```

未声明 `business` 模块时，现有宿主继续使用 `database.connection` / `database.read_connection`。显式声明后写连接为空会启动失败，不回退旧连接。日志、事件原有显式连接和兼容回退规则保持不变；模块配置不自动改写它们，使用独立模块配置时也应明确配置日志和事件目标。

新增模块的接入顺序：

1. 在所属仓库定义模块类型、实体及自己的迁移元数据，在配置的 `database.modules.<名称>` 声明连接。
2. 在 presentation 资源回调中按该名称读取配置、执行 `validate`，调用 `ModuleDatabase::<M>::connect_managed(write_options, read_options, cleanup)`，再注册到 `DatabaseModules`。工厂接收宿主的 `&mut ResourceCleanup`，每个池创建成功后立即登记，读池连接失败时仍会清理写池。配置只提供参数，不会自动推断实体或加载业务模块。
3. WebAPI 使用 `build_resources_with_modules` 传入已通过同一 cleanup 登记的额外模块集合；模板 BusinessModule 由该函数补齐，额外模块不重复登记清理，重复声明会报错。其他业务服务通过所属模块连接创建，在宿主工厂中绑定契约。
4. TaskService 的 `build_databases` 回调装配模块集合，通过 `TaskServiceDatabases::for_module::<M>` 明确选择当前 Worker 的队列目标。任务特性从同一集合取得所属模块。一个 Worker 仍只处理选定模块；需要多个数据库的 Worker 时分别装配宿主实例，不把不同库任务混入同一个默认队列。

模板消息任务只依赖 `TaskServiceFactory`；具体 Message/SMS 实现由 `presentation/task_service/src/service_factory.rs` 创建。注册表持有共享工厂和基础设施，每次执行仍创建新的任务处理器及应用服务。

## 同模块事务入队

业务用例先从明确模块调用 `begin()`，通过返回的 `ModuleTransaction<M>::connection()` 执行业务写入，再调用 `QueueTaskProducer::from_module(module, identifiers).create(&transaction, task)` 入队，最后由业务用例提交或回滚。

生产者校验事务属于同一个连接池对象；即使连接字符串相同，另建的连接池也不能混用事务。应复用注册的模块或其克隆。生产者不会提前提交事务，入队的 ID、实体插入与 xmin 回读仍沿用原有实现。

`create_single` 仍独立保存。队列存储完成任务、父子链和回调的内部事务保持原样，其原始事务入口仅在 queue_task_database crate 内可见。`TaskCenterService::create_queue_task` 现在接收模块事务，调用方应从模块开启事务；这是 Rust 源码级接入调整，不改变 HTTP 接口。

模块之间只通过普通业务契约和 DTO 交互，不传递连接、实体或事务。多个服务调用不会自动形成同一事务，框架也不提供跨库原子性；可靠跨模块推进仍需业务事务入队、接收方幂等和必要补偿。

## 仓储工具

工具保持 `--target business|log|event-bus` 显式选择模型。可选 `--module <名称>` 从工具可执行文件旁的配置选择命名连接：

```toml
[database.modules.business_archive]
model = "business"
connection = "<该 business 模型实例的写连接>"
```

```powershell
docker compose exec -T dev cargo run --locked -p repository_tool -- migration-script --target business --module business_archive
```

模块不存在、连接为空或声明的 `model` 与 `--target` 不一致时直接失败，不回退其他目标。连接不能从命令行或环境覆盖；`migration-script` 仍只离线导出 SQL，不连接数据库。

命名连接不会创造新模型。新的实体模块必须在 repository_tool 登记自己的模型、schema、迁移集合及历史表；不能把它伪装成 business 来执行另一套 schema。现有三个模型的历史表和迁移版本没有改变，宿主启动不执行迁移。
