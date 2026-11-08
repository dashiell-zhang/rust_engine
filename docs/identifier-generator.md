# ID 生成器

ID 生成器为业务实体、日志、队列及数据库事件提供统一的雪花 ID。一个宿主创建一个生成器，其他组件共享其克隆，避免同一进程重复分配节点。

## 自动分配

```toml
[identifier_generator]
redis_connection = "redis://host.docker.internal:6379/0"
instance_name = "Example.Identifier"
lease_duration_milliseconds = 172800000
```

需要共同保证 ID 唯一的所有进程，使用相同 Redis 目标、库编号和 instance_name。自动模式支持 1024 个节点，退出后的未过期租约仍占用名额。

redis_connection 留空时使用 cache.connection 的地址。即使业务缓存选择内存组件，自动 ID 节点仍需要 Redis。

默认租约两天，可配置三秒至三十天。共享范围内实例使用一致租期；节点耗尽或初始化连接失败会启动失败，不回退固定编号。

## 固定编号

能够管理节点唯一性时，同时提供两个值：

```toml
[identifier_generator]
data_center_id = 0
machine_id = 1
```

两者必须同时出现，范围均为 0～31。固定模式无需 Redis。每个实例使用不同组合，不把固定节点混入自动分配范围。

## 获取 ID

WebAPI 使用基础接口即可获得 ID：

```powershell
curl.exe http://127.0.0.1:3001/Base/GetSnowflakeId
```

返回 JSON 字符串，避免客户端整数精度丢失。应用内部复用宿主注入的 IdentifierGenerator，不为每个组件单独创建。

ID 包含时间、数据中心、机器和序列信息，纪元为 2022-01-01 UTC。它是标识，不应作为业务提交顺序或事务完成时间。

## 租约与故障处理

自动节点分配成功后独立续期。短暂故障期间仅在本地确认的有效期内发号；过期后拒绝生成 ID。确认失去所有权后停止发号，需要排查依赖并重启重新分配。

时钟回拨和超出可表示范围会返回错误，调用方不能吞掉错误后使用固定节点替代。

Redis 节点键应避免缓存清理和淘汰。协调数据丢失时，先停止该唯一性范围内的发号实例，再恢复节点分配。

退出前完成仍需 ID 的任务和日志排空，再调用 shutdown。退出不删除占用，等待租约自然到期；运行中不能更换节点身份。
