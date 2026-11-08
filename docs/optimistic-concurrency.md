# PostgreSQL 行版本与并发保存

业务实体、任务实体和日志实体都映射 PostgreSQL 系统列 `xmin`。查询得到的 `row_version` 是无符号事务 ID 的文本表示，仅用于持久化层；不要把它当作业务排序号或时间戳，也不要自行写入该列。

## 实体声明

在 SeaORM 模型中声明系统列，并为实体实现公共版本契约：

```rust,ignore
#[sea_orm(column_name = "xmin", column_type = "Text", select_as = "text", save_as = "xid")]
pub row_version: String,

repository_core::impl_versioned_entity!(Entity, Column::RowVersion);
```

字段属于 `Model`；宏放在模型模块中。版本通过数据库的 `xid`/`text` 转换读取和比较，不依赖驱动将 `xid` 映射成有符号整数。新增 `ActiveModel` 的 `row_version` 保持 `NotSet`。

建表使用 `repository_core::versioned::create_table_from_entity`。它在 SeaQuery 语句树上排除系统列，同时保留主键、外键、其他列及注释；不要使用普通建表入口创建名为 `xmin` 的用户列。已有 PostgreSQL 表自带系统列，无需新增版本字段迁移。

## 单行写入

引入 `repository_core::versioned::VersionedActiveModel`，使用以下入口：

- `insert_versioned(db)`：插入后在同一事务内回读完整实体和真实版本。
- `update_versioned(db)`：用读取时的版本作为更新条件，返回更新后的实体和新版本。
- `delete_versioned(db)`：按主键和读取时的版本执行物理删除。

软删除仍通过设置 `delete_time` 等审计字段后调用 `update_versioned` 完成。修改已读实体时使用 `model.into_active_model()`，保留读取时的版本；不要在更新前重新查询版本来覆盖旧版本。

如果记录已经被其他事务更新或删除，更新和删除返回 `DbErr::RecordNotUpdated`。调用方应终止当前操作并按业务需要处理冲突，不能静默重试覆盖他人的改动。没有待修改字段时仍验证版本，不绕过并发条件重读并接受新记录。

保存入口保留 `before_save`、`after_save` 和删除钩子。现有带更新时间的实体仅在调用方未明确设置更新时间时自动填入当前时间。

## 批量新增与分区

批量写入使用 `repository_core::versioned::insert_many_versioned(models, db)`。它在一个事务里分批写入和回读，验证全部主键与回读数量，按输入顺序返回实体。日志入库使用此入口。

所有表采用统一的插入方式：INSERT 不返回 `xmin`，随后按已确定的主键在同一事务里读取版本。因此普通表、已声明分区的表和数据库中已有的分区表都可保存，无需按启动时分区目录切换命令策略。运行时分区维护仍只管理显式声明的模型。

没有外层事务时由保存入口创建并提交事务；传入已有事务时创建保存点，成功仅释放保存点，最终提交仍由调用方决定。写入或回读失败时回滚本次保存，取消 future 时由数据库事务守卫回滚。新增主键必须预先确定，不能依赖数据库自增。只读连接不提供事务保存能力，不能传入新增入口。

`update_many`、原生 SQL 等集合更新不具有实体版本跟踪语义，仅用于明确设计的原子操作；应自行提供完整的业务条件。例如任务领取和续租使用状态、所有者及租约条件。任务结果回写在续租停止后读取当前所有权与版本，再执行受版本保护的更新。
