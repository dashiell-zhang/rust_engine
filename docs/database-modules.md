# 业务模块、服务工厂与宿主装配

本文对应 [AR-01～AR-04 架构调整](netengine-architecture-review.md)。模块机制已接入三个现有宿主，不要求拆分现有业务数据库。

## 契约与运行资源

- `application_model` 保存 DTO，`application_interface` 保存业务服务及提供商契约。
- `application_runtime` 保存数据库执行上下文、共享 HTTP 客户端工厂和 JWT 编解码实现。应用实现与宿主引用它；只使用契约的组件无需引用它。
- DTO、接口和事件契约关闭 common 的默认 `runtime` 特性，保留 JSON、日期、属性映射和错误类型。网络、图像、Excel、系统等运行能力由默认启用的 `runtime` 特性提供；已有运行组件行为保持不变。

## WebAPI 实现选择

`WebApiHostProfile.build_resources` 指向运行入口的资源工厂。模板入口使用 `webapi_support::resources::build_resources`，它装配业务模块、独立日志查询、健康检查及 `BusinessServiceFactory`。

请求处理器继续使用 `app_state.services()`。返回值是 `application_interface::services::ServiceFactory` 契约，具体工厂函数由资源注册决定。鉴权、权限路由同步、普通业务和 LLM 均走该选择。

替换业务实现时，在 presentation 中实现 `ServiceFactory<'a>`，并由档案的资源回调调用 `build_resources_with_factory(settings, check_log_read_health, BusinessServiceFactory(custom_services))`。可用现有 `WebApiApplicationAdapter` 委托未替换的方法。不要修改 handler，也不要在公共宿主内选择具体应用服务。

现有 `generate_direct_service_factory!` 支持可选 `implements = 契约类型;`，自动生成 trait 实现；不写时仍生成原有公开方法。参数和返回值仍由真实契约约束，不能仅修改实现而漏改接口。

`WebApiResources` 按类型保存入口选择的资源，重复注册报错。公共宿主不自动连接业务库或日志库，业务策略在启动时通过 `validate_resources` 检查必要绑定。独立宿主可以使用自己的资源工厂与业务策略；模板业务策略必须配套模板资源。

健康检查由资源工厂显式注册，公共宿主并行运行并传递取消令牌。两个现有 WebAPI 均检查业务读写库和日志写库，只有管理端检查日志读库；缓存与事件检查继续由公共宿主执行。新增模块的读写连接也参与检查。

## 模块身份与连接

`repository_core::module::DatabaseModule` 由所属业务仓库实现，声明固定的 `NAME` 和 `MIGRATION_HISTORY_TABLE`。模板业务类型为 `repository::BusinessModule`，名称 `business`。

每个类型对应一个 `ModuleDatabase<M>`，包含独立写池和只读池。只读连接仍启用 PostgreSQL 会话只读设置。`DatabaseModules::register` 拒绝重复类型或名称，`get::<M>()` 按类型查找，注册顺序不会改变目标。

运行时可选配置：

```toml
[database.modules.business]
connection = "<业务写库连接>"
read_connection = "<业务只读连接；留空则使用同模块写库地址>"
```

未声明 `business` 模块时，现有宿主继续使用 `database.connection` / `database.read_connection`。显式声明后写连接为空会启动失败，不回退旧连接。日志、事件原有显式连接和兼容回退规则保持不变；模块配置不自动改写它们，使用独立模块配置时也应明确配置日志和事件目标。

新增模块的接入顺序：

1. 在所属仓库定义模块类型、实体及自己的迁移元数据，在配置的 `database.modules.<名称>` 声明连接。
2. 在 presentation 资源回调中按该名称读取配置、执行 `validate`，构造 `ModuleDatabase::<M>::connect` 的写/读选项，再注册到 `DatabaseModules`。配置只提供参数，不会自动推断实体或加载业务模块。
3. WebAPI 使用 `build_resources_with_modules` 传入额外模块集合；模板 BusinessModule 由该函数补齐，重复声明会报错。其他业务服务通过所属模块连接创建，在宿主工厂中绑定契约。
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
