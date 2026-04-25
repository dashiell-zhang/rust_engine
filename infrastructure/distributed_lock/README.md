# distributed_lock

分布式锁抽象 crate，定义统一的 `DistributedLock` trait 与公共类型

具体实现由独立 crate 提供：

- **`distributed_lock_memory`**：基于进程内信号量，适用于单进程内的并发控制，无需外部依赖
- **`distributed_lock_redis`**：基于 Redis，适用于跨进程与跨机器的分布式互斥

---

## 添加依赖

### 仅使用内存锁

```toml
[dependencies]
distributed_lock = { path = "..." }
distributed_lock_memory = { path = "..." }
```

### 同时使用 Redis 锁

```toml
[dependencies]
distributed_lock = { path = "..." }
distributed_lock_memory = { path = "..." }
distributed_lock_redis = { path = "..." }
```

---

## 核心接口

所有实现均通过 `DistributedLock` trait 提供统一的三个方法：

| 方法 | 说明 |
|---|---|
| `lock` | 阻塞式获取锁，超时前持续重试，超时后返回错误 |
| `try_lock` | 非阻塞式尝试获取锁，立即返回，获取失败返回 `None` |
| `renew` | 续期锁，将失效时长延长至新的 `expiry` |

获取成功后返回 `LockGuard`，持有期间锁保持有效，**离开作用域时自动释放**，无需手动调用释放方法。

---

## InMemoryLock 用法

### 创建实例

```rust
use distributed_lock_memory::InMemoryLock;

let lock = InMemoryLock::new();
```

`InMemoryLock` 实现了 `Default`，也可以通过 `InMemoryLock::default()` 创建。

需要跨任务共享时，包装在 `Arc` 中：

```rust
use std::sync::Arc;
use distributed_lock_memory::InMemoryLock;

let lock = Arc::new(InMemoryLock::new());
```

### 阻塞获取锁

```rust
use std::time::Duration;
use distributed_lock::DistributedLock;
use distributed_lock_memory::InMemoryLock;

let lock = InMemoryLock::new();

// 获取锁，失效时长 30 秒，互斥（semaphore = 1）
// lock 方法会持续重试直到获取成功或超时
let guard = lock.lock("my-key", Duration::from_secs(30), 1).await?;

// 持有 guard 期间锁保持有效
do_work().await;

// guard 离开作用域，锁自动释放
```

`expiry` 传 `Duration::ZERO` 时默认使用 60 秒：

```rust
let guard = lock.lock("my-key", Duration::ZERO, 1).await?;
```

### 非阻塞尝试获取

```rust
use distributed_lock::DistributedLock;
use distributed_lock_memory::InMemoryLock;
use std::time::Duration;

let lock = InMemoryLock::new();

match lock.try_lock("my-key", Duration::from_secs(30), 1).await? {
    Some(guard) => {
        // 获取成功，执行临界区逻辑
        do_work().await;
        // guard drop 时自动释放
    }
    None => {
        // 锁当前被其他持有者占用，可选择跳过或稍后重试
    }
}
```

### 信号量模式（允许多个并发持有者）

`semaphore` 参数控制同一 key 可同时被持有的名额数，传入大于 1 的值即为信号量语义：

```rust
// 允许最多 3 个任务同时持有此锁
let guard = lock.lock("resource-key", Duration::from_secs(60), 3).await?;
```

### 续期锁

适用于执行时间不确定、可能超过初始 `expiry` 的长任务：

```rust
use distributed_lock::DistributedLock;
use distributed_lock_memory::InMemoryLock;
use std::time::Duration;

let lock = InMemoryLock::new();
let guard = lock.lock("long-task", Duration::from_secs(30), 1).await?;

// 任务执行过程中续期，将失效时长重置为 30 秒
let renewed = lock.renew(&guard, Duration::from_secs(30)).await?;
if !renewed {
    // 锁已过期，需要重新获取
}
```

---

## RedisLock 用法

### 创建实例

```rust
use distributed_lock_redis::RedisLock;

let lock = RedisLock::new("redis://127.0.0.1/", "my-service").await?;
```

第二个参数为 Key 前缀，建议使用服务名称，避免不同服务的锁 Key 在 Redis 中冲突。
最终写入 Redis 的 Key 格式为 `{prefix}:{key}:{slot}`，例如 `my-service:order-123:0`。

### 阻塞获取锁

```rust
use distributed_lock::DistributedLock;
use distributed_lock_redis::RedisLock;
use std::time::Duration;

let lock = RedisLock::new("redis://127.0.0.1/", "my-service").await?;
let guard = lock.lock("order-123", Duration::from_secs(30), 1).await?;

do_work().await;

// guard drop 时 fire-and-forget 发送 DEL 命令释放锁
```

### 非阻塞尝试获取

```rust
match lock.try_lock("order-123", Duration::from_secs(30), 1).await? {
    Some(guard) => {
        do_work().await;
    }
    None => {
        // 锁被其他进程占用
    }
}
```

### 续期锁

```rust
let guard = lock.lock("long-task", Duration::from_secs(30), 1).await?;

// 通过 Lua 脚本原子地校验并续期
let renewed = lock.renew(&guard, Duration::from_secs(30)).await?;
```

---

## 错误处理

```rust
use distributed_lock::{DistributedLock, LockError};
use distributed_lock_memory::InMemoryLock;
use std::time::Duration;

let lock = InMemoryLock::new();

match lock.lock("my-key", Duration::from_secs(5), 1).await {
    Ok(guard) => {
        // 获取成功
    }
    Err(LockError::Timeout { key }) => {
        // 在 5 秒内未能获取到锁
        eprintln!("获取锁 {} 超时", key);
    }
    Err(LockError::InvalidArgument { message }) => {
        // 参数错误，如 key 为空或 semaphore 为 0
        eprintln!("参数错误: {}", message);
    }
    Err(e) => {
        eprintln!("其他错误: {}", e);
    }
}
```

---

## 注意事项

**`expiry` 的双重含义**

`lock` 方法的 `expiry` 同时作为两个时长：
- 锁的失效时长（从获取成功时刻起算）
- 等待获取锁的最长时间

两者独立计算，等待了较长时间才获取到锁时，锁的 TTL 仍为完整的 `expiry`。

**RedisLock 的 drop 需要 tokio 上下文**

`RedisLock` 的 `LockGuard` 在 drop 时会通过 `tokio::spawn` 发送 DEL 命令。
如果在非 tokio 异步上下文中 drop（例如普通 `std::thread`），会发生 panic。
正常在 async 函数或 tokio task 中使用时无需关注此限制。

**InMemoryLock 不支持跨进程互斥**

`InMemoryLock` 的锁状态仅存在于当前进程的内存中，多个进程各自持有独立的锁状态，无法实现分布式互斥。跨进程场景请使用 `RedisLock`。
