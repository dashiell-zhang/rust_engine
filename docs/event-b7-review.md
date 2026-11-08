# B7 事件总线：调用链与失败处理记录

日期：2026-11-08。本批范围来自 [阶段清单 B7](D:/ZWork/rust_engine/docs/netengine-next-stage.md)。完成有限源码对照与事件组件编译，未发现需要修复的确认缺陷。双方目前均无具体业务事件调用，以下源码结论不等同于监听或发布链路已通过运行验收。

## 本次基线

- Rust：用户已提交 `acd147dccbe26dbb766b1479897c5e5cb1806e97`，包含 B1～B6 改动；本批开始时工作区干净。本批只修改三份文档，不改变生产代码、依赖或配置。
- NetEngine：`23a7d0002938d0308c975dc48c04ac66aae0dda8` 加阶段清单中的五处授权修改。本批没有修改或运行 NetEngine，没有获取新的远端版本。
- 未新增测试、验证程序、模拟服务、业务事件或专用接口；没有启动临时基础设施或重新执行 B1～B6 的运行案例。

## 实际业务接入核对

搜索 Rust 的 application/presentation 源码，未发现具体 `Event` 模型声明、发布调用、`subscribe`/`subscribe_factory` 调用或特性中监听注册的重写。源码生成器只有 Event 派生能力，应用接口只有发布器契约重导出及访问契约。

搜索 NetEngine 的 C# 源码，`IEventPublisher`、泛型 `PublishAsync` 和 `EventSubscribeAttribute` 仅出现在事件框架的定义、注册和反射扫描中；没有业务发布或带订阅属性的方法。健康检查的 `HealthCheckPublisher.PublishAsync` 属于健康报告，不是事件发布入口。

三个宿主均已装配事件后端。Rust 的 [WebAPI 特性](D:/ZWork/rust_engine/project_core/webapi_core/src/host/features.rs) 与 [任务特性](D:/ZWork/rust_engine/project_core/task_service_core/src/features.rs) 提供默认空监听注册方法，实际 Feature 当前沿用该行为。源宿主则注册事件服务，由框架扫描服务方法。

因此，“当前没有业务事件”是双方一致的使用现状，不是本批确认的 Rust 业务遗漏。不能为取得运行结果擅自设计新的业务广播或添加演示监听。

## 有限源码对照

| 关注点 | 源设计与 Rust 当前实现 | 本批结论 |
| --- | --- | --- |
| 发布契约 | 源使用具体事件类型和 Web JSON；Rust 使用显式稳定名称、Event 契约及独立 Web JSON 入口，随后委托传输后端 | 调用顺序和职责已核对；无实际业务发布证据 |
| 启动位置 | 源 StartingAsync 先取得位置，StartedAsync 启动后台循环；Rust prepare 先取得位置，再完成 Feature 装配并 start | 未发现本范围确认差异 |
| 广播与重试 | 每实例持有独立游标；当前事件处理完才推进；成功监听移出待重试集合，失败监听继续重试，清理循环独立 | 源码核对一致，没有业务处理器失败的运行证据 |
| 每次调用与释放 | 源为每次 InvokeAsync 创建异步服务作用域；Rust 每次尝试调用 factory，解析并 handle 后等待 close | 保持独立调用生命周期；DI 反射与 Rust trait/工厂不是同一种实现机制 |
| 失败优先级 | 参数解析或处理失败仍释放；释放失败优先于处理结果；Rust 对展开型 panic 也先尝试 close，再交外层报告 | 已核对源码；不证明进程中止或强制取消下能够异步释放 |
| 取消与关闭 | 事件过期/宿主停止传递取消；当前处理配合退出并完成释放。Rust 两类公共宿主在同一 30 秒预算内等待，耗尽后丢弃后台运行对象 | 正常关闭路径与预算已核对；没有在途处理器的运行验收 |
| 数据库发布 | 源更新状态行并持有事务写锁、写消息后提交；Rust 原子递增 Position 并在同一事务插入消息，不使用雪花 ID 作游标 | 两者均保留提交顺序设计；没有本批发布竞争证据 |
| 数据库取消 | Rust 使用独立控制连接，按 PID、后端开始时间和事务标记确认对象；未完成操作禁止连接回池 | 已核对实现及调用路径；控制连接失败、提交竞争仍需实际业务入口 |

主要源码位置：

- 源发布/监听：[EventPublisher.cs](D:/ZWork/NetEngine/Infrastructure/EventBus/EventPublisher.cs)、[EventListenerRegistry.cs](D:/ZWork/NetEngine/Infrastructure/EventBus/EventListenerRegistry.cs)、[EventBusHostedService.cs](D:/ZWork/NetEngine/Infrastructure/EventBus/EventBusHostedService.cs)。
- Rust 发布/工厂：[lib.rs](D:/ZWork/rust_engine/infrastructure/event_bus/src/lib.rs)，其中 `subscribe_factory` 每次创建处理器，解析、处理和 close 均留在本次调用中；工厂自身失败前取得的资源仍由工厂负责。
- Rust 重试/关闭：[runtime.rs](D:/ZWork/rust_engine/infrastructure/event_bus/src/runtime.rs)，`pending` 仅保留失败监听，正常完成不主动取消处理器令牌，后台任务异常退出由宿主监督。
- 宿主装配：[WebAPI builder](D:/ZWork/rust_engine/project_core/webapi_core/src/host/runtime/builder.rs)、[TaskService builder](D:/ZWork/rust_engine/project_core/task_service_core/src/builder.rs)。两类宿主在进入正常关闭时先取消事件，再按剩余预算等待，未另加事件关闭期限。
- 数据库：[源 DatabaseEventTransport](D:/ZWork/NetEngine/Infrastructure/EventBus.Database/DatabaseEventTransport.cs)、[Rust DatabaseEventTransport](D:/ZWork/rust_engine/infrastructure/event_bus_database/src/lib.rs)、[PostgresOperation](D:/ZWork/rust_engine/infrastructure/repository_core/src/postgres_operation.rs)。取消与提交相遇时，不能仅凭取消结果认定消息没有写入。
- Redis：[适配器](D:/ZWork/rust_engine/infrastructure/event_bus_redis/src/lib.rs) 使用 XADD/XRANGE 与各实例游标，没有改成消费组竞争领取。本批未扩展至 Redis 所有协议边界。

## 证据缺口与结束边界

B6 已有三个宿主在数据库事件后端下启动及正常退出的证据，本批沿用，不重新运行。它只能说明空业务监听下的宿主装配能运行，不能证明具体处理器收到事件、失败重试、释放或发布取消成功。

实际业务链路验收仍需要已确定的事件契约、正式发布用例、实际监听处理器，以及可以观察的业务结果。特别是工厂失败、handle/close 失败与重试的运行证据，无法通过现有空注册表取得；数据库发布与取消竞争也不能用直接插入消息表代替正式发布调用。

本批不通过新建脚手架、直接写事件记录或反复启动空宿主补造这些证据。后续应在业务确实需要事件，或用户另行明确授权专门验证方式时，再建立对应的有限验收目标。V04 保持“源码核对与编译完成，业务运行证据不足”，不标记完全对齐。

## 容器检查

以下在原有 Docker dev 内执行并通过：

- `cargo check --locked -p event_bus -p event_bus_database -p event_bus_redis`
- `cargo clippy --locked -p event_bus -p event_bus_database -p event_bus_redis --lib -- -D warnings`
- `cargo fmt --all -- --check`

没有添加或运行测试，没有本机安装或固定工具链版本。本批没有生产改动，因此没有重新构建全部宿主、镜像或发布包。没有临时运行资源需要清理，未提交或推送代码。
