use std::collections::{HashMap, HashSet};

use common::string_helper::trim_prefix_name;
use repository::{extensions::SoftDeleteQueryExt, function, function_authorize};
use sea_orm::{
    ColumnTrait, DatabaseConnection, DbErr, DerivePartialModel, QueryFilter, QueryOrder,
    QuerySelect, sea_query::Condition,
};

const MODULE: i32 = 1;
const FUNCTION: i32 = 2;

#[derive(DerivePartialModel)]
#[sea_orm(entity = "function::Entity")]
struct FunctionNode {
    id: i64,
    parent_id: Option<i64>,
    name: String,
    sign: String,
    r#type: i32,
}

pub(crate) async fn load_permission_tree<T>(
    db: &DatabaseConnection,
    owner: Condition,
    to_dto: impl Fn(String, String, bool, String, Option<Vec<T>>, Option<Vec<T>>) -> T,
) -> Result<Vec<T>, DbErr> {
    let nodes = function::Entity::find()
        .not_deleted(function::Column::DeleteTime)
        .filter(function::Column::Type.is_in([MODULE, FUNCTION]))
        .order_by_asc(function::Column::Id)
        .into_partial_model::<FunctionNode>()
        .all(db)
        .await?;
    let mut tree = PermissionTree::default();
    for node in nodes {
        let group = if node.r#type == MODULE {
            &mut tree.modules
        } else {
            &mut tree.functions
        };
        group.entry(node.parent_id).or_default().push(node);
    }
    if !tree.modules.contains_key(&None) {
        return Ok(Vec::new());
    }

    tree.checked = function_authorize::Entity::find()
        .not_deleted(function_authorize::Column::DeleteTime)
        .filter(owner)
        .select_only()
        .column(function_authorize::Column::FunctionId)
        .into_tuple::<i64>()
        .all(db)
        .await?
        .into_iter()
        .collect();
    Ok(tree.children(None, None, &to_dto))
}

#[derive(Default)]
struct PermissionTree {
    modules: HashMap<Option<i64>, Vec<FunctionNode>>,
    functions: HashMap<Option<i64>, Vec<FunctionNode>>,
    checked: HashSet<i64>,
}

impl PermissionTree {
    fn children<T>(
        &mut self,
        parent_id: Option<i64>,
        parent_name: Option<&str>,
        to_dto: &impl Fn(String, String, bool, String, Option<Vec<T>>, Option<Vec<T>>) -> T,
    ) -> Vec<T> {
        self.modules
            .remove(&parent_id)
            .unwrap_or_default()
            .into_iter()
            .map(|node| {
                let children = self.children(Some(node.id), Some(&node.name), to_dto);
                let functions = self
                    .functions
                    .remove(&Some(node.id))
                    .unwrap_or_default()
                    .into_iter()
                    .map(|item| {
                        to_dto(
                            item.id.to_string(),
                            trim_prefix_name(&item.name, Some(&node.name)),
                            self.checked.contains(&item.id),
                            item.sign,
                            None,
                            None,
                        )
                    })
                    .collect();
                to_dto(
                    node.id.to_string(),
                    trim_prefix_name(&node.name, parent_name),
                    self.checked.contains(&node.id),
                    node.sign,
                    Some(children),
                    Some(functions),
                )
            })
            .collect()
    }
}
