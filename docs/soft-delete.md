# 软删除查询

软删除保留记录和历史关联，通过 `delete_time` 区分有效数据。为实体派生 `repository_core::SoftDelete` 后，常用查询默认排除已删除记录。

## 实体声明

在标准 SeaORM 的 Model 上添加派生，模型所在模块需包含 `Entity`、`Column::DeleteTime` 和 `delete_time` 字段：

```rust
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, repository_core::SoftDelete)]
#[sea_orm(table_name = "Example")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: i64,
    pub delete_time: Option<DateTimeWithTimeZone>,
}
```

示例省略 SeaORM prelude 导入、Relation 和 ActiveModelBehavior 声明；已有实体可参考[业务实体目录](../repository/src)。

## 查询示例

以下 `Entity` 指派生了 SoftDelete 的具体实体：

```rust
let active = Entity::find().all(db).await?;
let item = Entity::find_by_id(id).one(db).await?;
let history = Entity::find_with_deleted().all(db).await?;
```

| 入口 | 返回范围 |
| --- | --- |
| `find()`、`find_by_id(id)` | 有效记录 |
| `find_active()`、`find_by_id_active(id)` | 有效记录，适合带 SoftDeleteEntityExt 约束的泛型代码 |
| `find_with_deleted()`、`find_by_id_with_deleted(id)` | 包含已删除记录，适合历史、恢复和去重 |

这是实体提供的查询入口，不是全局过滤器。显式调用 `<E as EntityTrait>::find()`、原生 SQL 或普通 SeaORM 关联方法时，调用方负责添加删除条件。

## 关联查询

```rust
use repository_core::extensions::SoftDeleteSelectExt;

let rows = Entity::find()
    .find_also_related_active(RelatedEntity)
    .all(db)
    .await?;
```

这里的 `RelatedEntity` 代表已声明关系的目标实体。目标已删除时返回 `None`，左侧记录仍保留。普通 `find_also_related`、多段关联及多对多桥接表不自动获得此过滤，需要明确每一段的可见性。

## 删除与恢复

软删除通过设置删除时间、审计字段后保存；它不会自动删除磁盘文件或云对象。包含行版本的实体使用 [update_versioned](optimistic-concurrency.md) 保留并发保护。

恢复记录时从 `find_with_deleted()` 查询，按业务规则清空删除字段后保存。批量更新、物理删除和原生 SQL 必须自行声明条件，不自动获得软删除过滤或行版本保护。
