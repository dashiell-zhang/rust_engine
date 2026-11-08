# 业务模块与服务装配

模块装配将业务用例、运行资源和数据库连接分开。不同宿主可以选择不同服务实现，也可以让多个业务模块使用独立连接。

## 组织业务代码

| 内容 | 位置 |
| --- | --- |
| DTO、请求和返回模型 | `application_model` |
| 业务服务 trait | `application_interface::services` |
| 运行上下文 | `application_runtime` |
| 用例实现 | `application_service*` |
| 实体、关系与模块身份 | 所属仓储 crate |
| 具体实现与资源注册 | `presentation/*` |

业务契约使用模型和通用类型作为参数，不把数据库实体或 HTTP 状态传给调用方。文件、邮件、短信等提供商契约由各自基础设施组件提供。

## WebAPI 服务注册

宿主档案的 `build_resources` 指向资源工厂，工厂接收 `Settings`、日志只读健康检查选项、共享 `Arc<HttpClientFactory>` 和 `&mut ResourceCleanup`。

以下片段放在资源工厂中，为基础接口创建资源并注册默认服务：

```rust
let mut resources = webapi_support::resources::build_resources(
    settings,
    check_log_read_health,
    cleanup,
).await?;
webapi_support::services::Base::register_default(&mut resources)?;
```

Feature 声明所需能力：

```rust
webapi_support::services::require::<webapi_support::services::Base>(resources)?;
```

处理器调用契约：

```rust
use webapi_support::WebApiApplicationExt;

let service = app_state.services().base()?;
```

能力按项注册，缺失依赖会在启动时报告错误，重复注册不会覆盖原值。实现自定义服务 trait 后，可用 `services::register::<services::Base>(&mut resources, create_base, validate_base)` 替换该能力的默认构造函数。自定义能力实现 `ServiceKey`，通过 `services().get::<Key>()?` 获取。

完整示例见[管理端资源工厂](../presentation/admin_webapi/src/business_resources.rs)。文件能力需要显式注册 `FileStorageResource`；本地模式使用 `FileStorageResource(None)`。日志查询按需注册，只有管理端默认启用。

## 模块身份与连接

仓储实现 `repository_core::module::DatabaseModule`，声明模块名和迁移历史表名。模板业务模块为 `repository::BusinessModule`。

`ModuleDatabase<M>` 持有该模块的写连接和只读连接，`DatabaseModules` 按类型登记和获取，拒绝重复类型或名称。配置示例：

```toml
[database.modules.business]
connection = "postgres://writer:password@db/business"
read_connection = "postgres://reader:password@db-reader/business"
```

未声明 business 模块时，宿主使用 `database.connection` 和 `database.read_connection`。声明后写连接必须非空；读连接留空时使用同模块写库地址，但连接仍为只读。

新增模块时：

1. 在所属仓储中声明模块类型、实体和迁移。
2. 在入口读取并校验 `database.modules.<名称>`。
3. 使用 `ModuleDatabase::<M>::connect_managed(write_options, read_options, cleanup)` 创建连接并登记清理。
4. 注册到 `DatabaseModules`，再为应用服务创建对应上下文。
5. WebAPI 可通过 `build_resources_with_modules` 传入额外模块；TaskService 通过 `TaskServiceDatabases::for_module::<M>` 选择队列目标。

配置本身不会自动加载新模块。各模块连接参与宿主健康检查；日志和事件连接单独配置。

## 同模块事务入队

业务写入与任务入队需要一起提交时，从生产者绑定的模块开启事务。以下 `task` 为 `NewQueueTask`，`identifiers` 为宿主 ID 生成器：

```rust
let transaction = module.begin().await?;
let producer = queue_task_database::QueueTaskProducer::from_module(module, identifiers);
// 业务写入使用 transaction.connection()。
producer.create(&transaction, task).await?;
transaction.commit().await?;
```

生产者和事务必须来自同一个模块连接池对象；相同连接字符串创建出的两个池不能混用事务。`create_single` 用于独立入队。

多个服务调用不会自动组成同一事务，跨数据库也不提供原子提交。跨模块任务应结合幂等处理及必要的业务补偿。

## TaskService 资源

`TaskServiceHostProfile.build_resources` 可返回按类型注册的资源集合；不需要业务资源时设为 `None`。Feature 从 `TaskServiceFeatureContext.resources` 获取依赖。

默认任务入口注册短信、邮件及文件资源，每次执行由工厂创建独立处理器。公共任务宿主负责调度和停机。参考[任务资源工厂](../presentation/task_service/src/host_support.rs)与[任务注册](../presentation/task_service/src/tasks.rs)。

## LLM 依赖替换

`LlmClientFactoryResource` 决定协议客户端，`LlmModelConfigResource` 决定运行配置来源。默认实现通过以下入口分别注册：

```rust
webapi_support::llm_resources::register_default_client_factory(&mut resources, http_clients)?;
webapi_support::llm_resources::register_default_model_config_resolver(&mut resources)?;
```

替换其中一项时，注册包装了自定义实现的对应资源，并省略该项默认注册。LLM 应用及调用能力需要两项资源，纯模型管理和会话查询不需要它们。`webapi_support` 的默认 LLM 实现需显式启用 `llm` 特性。用例见[LLM 调用](llm.md)。

## 仓储工具

工具使用 `--target business|log|event-bus` 选择模型，用 `--module` 选择该模型的命名连接：

```toml
[database.modules.business_archive]
model = "business"
connection = "postgres://writer:password@db/archive"
```

```powershell
docker compose exec -T dev cargo run --locked -p repository_tool -- migration-script --target business --module business_archive
```

命名连接不会创建新模型。新增实体模块需要在工具中登记模型、迁移和历史表。详细操作见[数据库工具](repository-tool.md)。
