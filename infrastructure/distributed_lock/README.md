# 分布式锁

`distributed_lock` 提供互斥和限并发接口，用于防止多个调用同时处理同一业务资源。

| 实现 | 适用范围 |
| --- | --- |
| `distributed_lock_memory::InMemoryLock` | 同一进程内共享的并发控制 |
| `distributed_lock_redis::RedisLock` | 多进程、多机器共享的并发控制 |

## 添加依赖

以下路径适用于 `presentation/<宿主>` 下的 crate；其他位置按实际目录调整：

```toml
[dependencies]
distributed_lock = { path = "../../infrastructure/distributed_lock" }
distributed_lock_memory = { path = "../../infrastructure/distributed_lock_memory" }
distributed_lock_redis = { path = "../../infrastructure/distributed_lock_redis" }
```

只使用一种实现时，可省略另一种依赖。

## 获取与释放

以下片段在异步函数中执行，业务操作由调用方提供：

```rust
use distributed_lock::DistributedLock;
use distributed_lock_memory::InMemoryLock;
use std::time::Duration;

let lock = InMemoryLock::new();
let guard = lock.lock("order:123", Duration::from_secs(30), 1).await?;
// 在租约有效期内处理订单。
guard.release().await;
```

`lock` 异步等待可用名额，`expiry` 同时指定最长等待时间和取得锁后的租约时长，两者独立计时。传 `Duration::ZERO` 使用 60 秒；超时返回 `LockError::Timeout`。

句柄离开作用域会触发释放；需要等待释放完成时使用 `release().await`。**持有句柄不会自动续期**，租约到期后其他调用仍可能取得锁。

多个任务需要共享同一个内存锁实例或其 `Arc`，各自创建实例不会互斥。

## 不等待与限并发

```rust
match lock.try_lock("report", Duration::from_secs(60), 3).await? {
    Some(guard) => {
        // 最多三个调用同时处理此资源。
        guard.release().await;
    }
    None => {
        // 当前名额已满，按业务选择跳过或稍后重试。
    }
}
```

`try_lock` 只尝试一次，名额满时返回 `None`。第三个参数为并发名额，1 表示互斥，0 为非法参数。同一业务键应使用一致的并发配置。

## Redis 用法

```rust
use distributed_lock::DistributedLock;
use distributed_lock_redis::RedisLock;
use std::time::Duration;

let lock = RedisLock::new("redis://host.docker.internal:6379/0", "Orders:").await?;
let guard = lock.lock("order:123", Duration::from_secs(30), 1).await?;
// 处理共享资源。
guard.release().await;
```

需要互斥的实例使用相同 Redis 目标、库编号和前缀。不同业务可以使用不同前缀；不要按实例名称隔离需要共同协调的锁。

Redis 释放按持有者身份校验，不会删除他人取得的锁。Drop 只触发尽力释放，不等待网络完成；应在 Tokio 运行时结束前显式等待 `release()`。无法完成释放时依赖租约自然到期。

## 长任务续期

```rust
let renewed = lock.renew(&guard, Duration::from_secs(30)).await?;
if !renewed {
    // 已不能确认持有锁，停止依赖互斥保证的操作。
}
```

长任务应在租约到期前定期续期。续期返回 false 表示锁已过期或句柄无效，不能继续把业务当成受锁保护。网络错误同样需要由调用方处理。

明确需要保留冷却时间时，可调用 `guard.leave_until_expiry()`，让租约自然到期。锁不能替代数据库事务或业务幂等；业务执行时间超过租约时仍可能发生重入。
