# 软删除查询约定

AR-14 在实体声明处使用 `#[derive(repository_core::SoftDelete)]`，要求同一模块内有标准 SeaORM `Model`、`Entity`、`Column::DeleteTime`，Model 包含 `delete_time`。业务仓库的 25 个软删除实体以及日志、队列任务、任务配置三个组件实体均已声明，不再维护仓库外的实体名单。

## 默认查询与历史查询

| 入口 | 语义 |
| --- | --- |
| `Entity::find()`、`Entity::find_by_id(id)` | 生成的固有方法默认追加本实体 DeleteTime IS NULL |
| `Entity::find_active()`、`find_by_id_active(id)` | 保留原扩展契约，语义同上；泛型查询应明确约束 SoftDeleteEntityExt 并选择此入口 |
| `Entity::find_with_deleted()`、`find_by_id_with_deleted(id)` | 显式包含删除记录，供历史、恢复与去重流程使用 |
| `Entity::update_many()`、`delete_many()`、原生 SQL | 不受读取默认值影响，必须声明自己的原子条件；不自动获得 xmin 保护 |

固有方法覆盖常见的具体实体查询入口，不是 SeaORM 全局查询过滤器。`<E as EntityTrait>::find()`、仅约束 EntityTrait 的泛型查询、`ModelTrait::find_related`、原生 SQL 会绕过固有方法，不能把它们当作活跃查询。已有 `.not_deleted(...)` 保留兼容；冗余过滤不改变结果，不为此次调整批量重写业务语句。

新实体必须在声明处派生 SoftDelete。未派生的实体不会因恰好有同名字段自动改变查询语义。当前实现保留 Rust 的显式查询和编译期约束，没有引入 EF 变更跟踪、全局状态或数据库触发器。

## 关联查询

`repository_core::extensions::SoftDeleteSelectExt::find_also_related_active` 对关联目标在 JOIN ON 中追加活跃过滤，并保留原有关系条件。左侧记录的可见性由起始查询决定：从 `Entity::find()` 出发只读活跃父记录，从 `find_with_deleted()` 出发包含历史父记录。目标删除时，左侧仍保留且关联值为 None，不在 WHERE 中误删整个左侧记录。结果投影沿用 SeaORM 的 select_also；自关联或带自定义别名的复杂查询仍需显式构造 JOIN、过滤和投影，不将此辅助方法视为任意关联加载器。

普通 SeaORM `find_also_related`、`find_with_related`、模型 `find_related`、多段 Linked 查询不会自动使用该规则。需要历史关联时显式使用原生关联入口，并在调用点说明；需要活跃关联时使用公共扩展或逐段显式过滤。多对多桥接表没有可推断的实体类型，必须额外声明桥接表可见性；公共扩展只保证其类型参数指定的目标实体，不猜测中间表的 DeleteTime。集合关联可分两次活跃查询后按键组装。

授权功能列表和路由绑定列表已接入公共关联扩展。关联功能删除时不再返回该功能的名称或标识，路由自身仍可返回，关联字段为空。LLM 应用的模型名称批量查询也随默认入口排除已删除模型。

## 保留的历史语义

- 权限初始化的六处恢复查询显式使用 `find_with_deleted()`，继续恢复原记录、保留主键和创建审计。
- 数据库日志积压恢复按包括软删除记录在内的全部 ID 去重，避免重复插入已有主键。
- xmin 新增回读和空更新版本校验继续使用仓储核心的原始泛型查询，包含刚被软删除的记录；写入仍遵守原版本条件及事务。
- 数据库事件的状态游标为原生 SQL，包含软删除状态行的规则及提交顺序不变。

没有修改数据库 schema、迁移历史、物理删除行为或事务边界。此次验证为容器编译与源码复核，未连接真实数据库运行联调。
