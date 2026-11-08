# 跨进程事件广播

事件总线把同一消息交给所有已订阅实例，适合缓存失效通知、配置变更通知等广播场景。只希望一个 Worker 执行的工作使用[队列任务](task-service.md)。

## 选择后端

```toml
[event_bus]
provider = "redis"
connection = "redis://host.docker.internal:6379/0"
scope = "Example:Production"
retention_milliseconds = 3600000
poll_interval_milliseconds = 1000
retry_interval_milliseconds = 1000
cleanup_interval_milliseconds = 60000
batch_size = 100
```

互通实例必须使用相同后端、目标、scope 和保留时间。Redis 后端需要 Redis 6.2 或更新版本。

数据库后端将 provider 改为 `database`，填写 PostgreSQL 写连接，并在工具配置中填写事件目标后迁移：

```powershell
docker compose exec -T dev cargo run --locked -p repository_tool -- migrate --target event-bus
```

数据库后端需要 PostgreSQL 14+，取消操作使用服务器终止功能。宿主不自动迁移；事件连接独立于业务事务。未填写 connection 时，入口分别回退到业务缓存的 Redis 地址或业务写库地址。

## 定义事件

使用稳定协议名，字段由共享模型声明：

```rust
use serde::Serialize;
use source_generator_runtime::{Event, ProtocolModel};

#[derive(Serialize, ProtocolModel, Event)]
#[serde(rename_all = "camelCase")]
#[event(name = "Example.Events.ProductChanged")]
pub struct ProductChanged {
    #[serde(default)]
    pub product_id: i64,
}
```

ProtocolModel 已提供反序列化，不再同时派生 Deserialize。需要与其他程序互通时，双方使用相同事件名和 JSON 字段协议。事件负载中的 i64 保留数字形式，不使用控制器的长整数字符串输出规则。

## 发布与监听

应用接收宿主提供的 publisher，在业务提交后发布：

```rust
publisher.publish(&ProductChanged { product_id: 123 }).await?;
```

在 Feature 的 `register_event_listeners` 中注册：

```rust
registry.subscribe::<ProductChanged, _, _>(
    "product-cache.invalidate",
    |message, cancellation| async move {
        if cancellation.is_cancelled() {
            return Ok(());
        }
        // 根据 message.product_id 更新本实例的状态。
        Ok(())
    },
)?;
```

监听标识应稳定且唯一。需要每次尝试创建独立处理器时，实现 `EventHandler<E>`，使用 `subscribe_factory`；处理结束后运行时等待其异步 `close`。

## 交付与重试

| 情况 | 行为 |
| --- | --- |
| 发布成功 | 后端确认保存，不表示所有监听完成 |
| 正常消费 | 每个进程维护自己的游标，发布方也会收到 |
| 启动或重启 | 从当前已提交末尾开始，不恢复上个进程的游标 |
| 监听失败 | 只重试失败监听，阻塞本进程后续事件 |
| 断线恢复 | 从当前进程游标继续；超过保留期的消息可能已清理 |
| 后台循环异常退出 | 通知宿主停机 |

监听必须幂等，释放失败也可能导致处理重跑。这不是持久消费者队列，不能依靠重启恢复未处理广播。

业务写入和事件发布不自动原子提交。需要可靠推进时，可在业务事务中入队广播任务，再由任务发布；重复发布仍需监听方处理。

## 取消与停止

需要响应调用方取消时使用：

```rust
publisher.publish_with_cancellation(&message, &cancellation).await?;
```

发出取消后继续等待方法返回。取消或错误不能证明消息没有写入，重试时可能重复。监听应响应令牌并在返回前完成清理，不另起脱离生命周期的后台任务。

宿主停止时等待处理器与 close，共用剩余停机期限。数据库控制连接或终止权限不可用时，服务器端操作可能无法立即结束。

## 消息格式

模型字段名不区分大小写，合法重复字段取末值，较早的非法值仍报错。字典键原样保留，数值字段可接收数字字符串，容器深度上限为 64。

事件发布拒绝非有限浮点数，不将它们静默转换成 null。事件名必须非空白；数据库后端字段上限为 1024 字符。默认模板只装配广播能力，具体业务事件由应用自行注册。
