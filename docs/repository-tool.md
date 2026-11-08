# 数据库工具

`repository_tool` 用于生成增量迁移、更新数据库、导出 SQL 和查看模型。所有命令在 Docker 开发容器中执行：

```powershell
docker compose exec -T dev cargo run --locked -p repository_tool
```

无参数显示帮助，不连接数据库。宿主启动与部署生成器均不自动执行迁移。

## 配置与目标

编辑 [repository_tool/appsettings.toml](../repository_tool/appsettings.toml)。构建会将其复制为可执行文件旁的 `repository_tool.appsettings.toml`，修改源配置后重新构建。

工具只读取该文件，不接受进程环境或命令行连接覆盖。发布工具时需要同时携带配置。

| --target | 连接字段 | 管理内容 |
| --- | --- | --- |
| business | database.connection | 业务实体、队列和任务配置 |
| log | database.log_connection | 日志表与分区 |
| event-bus | database.event_bus_connection | 广播消息及位置状态 |

目标必须显式指定，缺少连接直接报错。即使共用物理库，也分别迁移各模型。命名连接通过 `--module` 选择，见[模块配置](database-modules.md#仓储工具)。

## 首次建库与升级

```powershell
docker compose exec -T dev cargo run --locked -p repository_tool -- migrate --target business
docker compose exec -T dev cargo run --locked -p repository_tool -- migrate --target log
docker compose exec -T dev cargo run --locked -p repository_tool -- migrate --target event-bus
```

使用 Redis 广播时可省略事件库迁移。重复执行只应用未执行版本，操作和历史记录在同一事务及迁移锁中提交。

历史表分别为 `__EFMigrationsHistory_Business`、`__EFMigrationsHistory_Log`、`__EFMigrationsHistory_EventBus`。初始迁移要求模型表不存在；工具不自动接管其他迁移链的现有库。

业务库需要 citext 扩展支持。唯一约束遇到历史重复数据时会失败回滚，需要先按业务规则处理冲突；工具不自动合并或删除业务记录。

## 修改模型后生成迁移

```powershell
docker compose exec -T dev cargo run --locked -p repository_tool -- migration-add AddOrderRemark --target business
docker compose exec -T dev cargo run --locked -p repository_tool -- migration-pending --target business
```

生成器比较当前编译模型与最后保存的模型，不查询数据库。每个版本包含 Up、Down、模型快照和登记文件，应一起提交。默认位置为：

- 业务：`repository_tool/src/migrator/generated`
- 日志与事件：各自组件的 `src/migrations/generated`

生成后先审查，再重新编译并执行 migrate。删除、类型变化、非空约束和唯一索引尤其需要核对；数据填充或重命名应在新迁移中明确处理。不要修改已应用的迁移或历史快照。

`migration-pending` 有未生成变化时以非零状态退出；默认 migrate 也拒绝这类变化。模型支持表、列、默认值、键、索引、注释和分区元数据；未支持的结构需要显式迁移。普通模型差异不能自动搬迁已有分区数据。

## 查看模型和 SQL

```powershell
docker compose exec -T dev cargo run --locked -p repository_tool -- model-snapshot --target business
docker compose exec -T dev cargo run --locked -p repository_tool -- model-snapshot --target business --sql
docker compose exec -T dev cargo run --locked -p repository_tool -- migration-script --target business --output /workspace/dist/business-migrations.sql
```

这些命令只离线生成内容，不连接数据库。`model-snapshot --sql` 是当前完整建库 SQL，不替代已有库的增量迁移。输出路径为容器内路径，`/workspace` 对应工作区，同名输出文件会覆盖。

## 指定版本与回退

`migrate --to` 接受版本号、唯一名称或完整 MigrationId；省略时升级至最新。例如：

```powershell
docker compose exec -T dev cargo run --locked -p repository_tool -- migrate --target business --to 202606240001
```

目标早于当前版本时逆序执行 Down，目标版本自身保留。`--to 0` 撤销全部版本，会删除模型表及数据。操作前审查迁移并备份数据，Down 重建结构无法恢复已删除数据。

只导出区间 SQL：

```powershell
docker compose exec -T dev cargo run --locked -p repository_tool -- migration-script --target business --from 202604050001 --to 202606240001 --idempotent --output /workspace/dist/business-upgrade.sql
```

from 表示已应用位置，to 表示目标；交换起止位置可导出回退 SQL。`--idempotent` 按对应模型历史跳过已执行升级或已撤销回退，不检查人工修改过的数据库结构。脚本需审查后另行执行。

已登记迁移使用固定 Up/Down，不会根据当前实体猜测旧结构。已有数据库必须与迁移链相符，不能仅凭版本号相同判断可恢复。

## JSON 索引差异

```powershell
docker compose exec -T dev cargo run --locked -p repository_tool -- json-index --target business
```

此命令只读数据库目录并输出建议 SQL，不执行 DDL。范围限于所选模型拥有的表，以及 `jsonbIX_` 加保留十六进制后缀命名的索引；不要手工复用这些名称。当前业务模型未声明 JSON 索引，没有旧工具索引时输出为空。

实体所属组件通过 `JsonIndexedEntity`、`JsonIndexDefinition` 和 `JsonMember` 声明索引与真实 JSON 属性映射。复杂字段需映射到 JSON 列，唯一分区索引必须包含分区键。API 见[json_index](../infrastructure/repository_core/src/json_index.rs)。

## 模型注释

在 SeaORM 模型的 DeriveEntityModel 前添加 `#[repository_core::model_documentation]`，可将结构体和字段的文档注释写入表、列注释。显式 sea_orm comment 优先。

注释变化也需生成 migration-add；它生成 COMMENT 操作，Down 恢复旧注释。模型修改、迁移生成和数据库执行是三个独立步骤。
