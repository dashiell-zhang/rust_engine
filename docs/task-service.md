# 任务服务

任务服务处理不宜放在 HTTP 请求内完成的工作，例如发送邮件、短信、批量处理和定时作业。队列任务竞争领取，定时任务在各实例独立触发；需要每个实例都收到通知时使用[事件广播](event-bus.md)。

## 启动与启用任务

配置 [task_service/appsettings.toml](../presentation/task_service/appsettings.toml)，完成业务数据库迁移后启动：

```powershell
docker compose exec dev cargo run --locked -p task_service
```

Debug 模式通过菜单选择当前进程启用的任务，不更新数据库任务定义。`start` 关闭菜单继续运行，`quit` 停止宿主；无标准输入时继续运行，未选择的任务保持禁用。

生产运行使用 Release：

```powershell
docker compose exec -T dev cargo run --release --locked -p task_service
```

Release 从数据库读取配置，每六十秒刷新并补齐缺少的定义。新增定义默认禁用，需在任务管理中启用。运行环境依次取 APP_ENVIRONMENT、DOTNET_ENVIRONMENT，默认 Production；它与 Debug/Release 构建模式独立。

## 默认任务

| 类型 | 名称 | 默认并发 / Cron |
| --- | --- | --- |
| 短信队列 | `MessageTask.SendSMS` | 1 |
| 邮件队列 | `MessageTask.SendEmail` | 1 |
| 演示队列 | `ShowName`、`SendSMS`、`CallPhone`、`ShowNameSuccess` | 各 5 |
| 定时演示 | `ShowTime` | `0/1 * * * * ?` |

演示任务展示入队、子任务和完成回调。实际业务应注册自己的名称和处理器。

## 注册队列任务

在任务 Feature 的注册方法中使用现有 registry：

```rust
use source_generator_runtime::register_task_registry;
use task_service_core::registry::QueueTaskDefinition;

register_task_registry! {
    registry = registry;
    queue {
        definition: QueueTaskDefinition::new("ReturnName").with_semaphore(1),
        factory: || Ok(task_service_core::task_handler::queue_action(|context| async move {
            let name: String = context.deserialize_parameter()?;
            Ok(name)
        })),
    }
}?;
```

每次执行通过 factory 创建独立处理器。连接池等资源可共享，执行中的状态留在本次处理器。异步资源可由处理器的 `close` 释放。完整例子见[内置任务注册](../presentation/task_service/src/tasks.rs)。

## 入队与回调

以下 producer 已绑定当前模块数据库和宿主 ID 生成器：

```rust
let task = queue_task_database::NewQueueTask {
    name: "ReturnName".to_owned(),
    parameter: Some(common::json_helper::object_clone_json(&"小明")?),
    ..Default::default()
};
let task_id = producer.create_single(task).await?;
```

需要随业务提交时使用 `create(&transaction, task)`，见[同模块事务入队](database-modules.md#同模块事务入队)。生产者和事务必须来自同一模块连接池。

`parent_task_id` 指定父任务，`callback_name` 指定完成回调。父任务及全部子树成功后才入队回调，状态和回调在同一事务提交。显式 callback_parameter 优先，否则按需序列化处理器返回值。

参数使用 JSON 文本。字符串必须先编码，数据库 NULL 表示缺少参数，显式 JSON null 才可绑定为 Option。一般参数使用 `object_clone_json`，短信和邮件 DTO 使用各自的 `to_queue_json()`；i64 编码为字符串。

## 短信任务

参数类型为 `SendSmsDto`，字段为 `SignName`、`Phone`、`TemplateCode`、`TemplateParams`，区分大小写。配置选择和号码要求见[组件配置](component-configuration.md#短信与邮件)。

内置提供商将发送错误转换为 false，应用服务不将该值转换成队列失败。因此队列成功不代表短信送达，也不会因提供商返回 false 自动重试。

## 邮件任务

示例参数：

```json
{
  "FromDisplayName": "通知中心",
  "ToAddresses": ["reader@example.com"],
  "Subject": "处理完成",
  "Body": "你的任务已完成。",
  "IsBodyHtml": false,
  "FileIdList": []
}
```

使用 `SendEmailDto::to_queue_json()` 生成队列参数，投递到 `MessageTask.SendEmail`。SMTP 使用 `message.smtp` 配置和 STARTTLS，默认端口 587。

附件由 FileIdList 引用有效文件记录，ID 写为字符串。路径相对于宿主工作目录解析。云存储模式每次先下载附件，仅发送成功后删除本次下载文件；失败时保留。本地附件不会在发送后删除。

短信和邮件字段逐次解析，较早的非法重复字段不会被后值掩盖。列表支持普通数组及 `$id/$values` 包装，同次参数中的引用编号必须唯一；不提供一般对象图的共享引用或循环引用恢复。

## 重试、并发与管理

- 真正开始执行时增加次数，自动执行最多三次。失败后按最近开始时间加 `5 × count` 分钟决定重试时间。
- 数据库执行租约用于恢复中断任务；分布式锁限制跨实例并发。共同处理队列的实例须共享锁目标和前缀。
- 手动重试拒绝已成功或仍持有有效租约的记录，恢复 Pending 时清除执行次数和执行时间。
- 带参定时实例从对应模板创建；编辑配置不能将定义、模板和实例互相转换。
- 在线启停和 Cron 修改在下一次配置刷新生效，不补跑禁用期间错过的触发。
- 外部业务副作用可能已发生而完成事务失败，处理器仍需按业务标识实现幂等。

## Cron 配置

格式为 `秒 分 时 日 月 星期 [年]`，日与星期必须且只能有一个为 `?`：

| 表达式 | 含义 |
| --- | --- |
| `0 0 0 * * ?` | 每天零点 |
| `0 */5 * * * ?` | 每五分钟 |
| `0 0 9 ? * MON-FRI` | 工作日九点 |
| `0 0 9 L * ?` | 每月最后一天九点 |
| `0 0 9 ? * MON#2` | 每月第二个星期一九点 |

数据库 Cron 留空时使用注册定义的默认值。时间按宿主本地日历计算，触发采用本地标准偏移。定时任务由每个实例独立触发，`skip_if_running` 只防止当前进程重入。

## 停机

收到停止信号后，宿主停止新领取，等待执行和续租，再处理事件及日志清理。共用三十秒期限，超时取消剩余工作，未完成队列由租约恢复。默认队列轮询一秒、定时轮询 128 毫秒、队列租约五分钟，可通过 poll 和 lock 配置调整。
