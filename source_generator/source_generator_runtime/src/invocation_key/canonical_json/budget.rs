use crate::*;

/// 限制首次参数序列化允许访问的 JSON 深度和项目数量
pub(crate) struct SerializationBudget {
    pub(crate) item_count: Cell<usize>,

    pub(crate) max_depth: usize,

    pub(crate) max_traversal_depth: usize,

    pub(crate) max_items: usize,

    pub(crate) safety_policy: ParameterKeySafetyPolicy,
}

impl SerializationBudget {
    /// 创建已经包含根节点的序列化预算
    pub(crate) fn new(
        max_depth: usize,

        max_traversal_depth: usize,

        max_items: usize,

        safety_policy: ParameterKeySafetyPolicy,
    ) -> Self {
        Self {
            item_count: Cell::new(1),

            max_depth,

            max_traversal_depth,

            max_items,

            safety_policy,
        }
    }

    /// 在序列化子节点前检查并消耗一次预算
    pub(crate) fn enter<E>(&self, depth: usize) -> Result<(), E>
    where
        E: SerializeError,
    {
        self.enter_many(depth, 1)
    }

    /// 在序列化一批同层子节点前检查并消耗预算
    pub(crate) fn enter_many<E>(&self, depth: usize, count: usize) -> Result<(), E>
    where
        E: SerializeError,
    {
        if count == 0 {
            return Ok(());
        }

        if depth > self.max_depth {
            return Err(E::custom(format!(
                "参数 JSON 超过 {} 层深度限制",
                self.max_depth
            )));
        }

        let item_count = self
            .item_count
            .get()
            .checked_add(count)
            .ok_or_else(|| E::custom("参数 JSON 项目计数溢出"))?;

        if item_count > self.max_items {
            return Err(E::custom(format!(
                "参数 JSON 超过 {} 项限制",
                self.max_items
            )));
        }

        self.item_count.set(item_count);

        Ok(())
    }

    /// 检查 Serde 嵌套调用深度
    pub(crate) fn check_traversal<E>(&self, traversal_depth: usize) -> Result<(), E>
    where
        E: SerializeError,
    {
        if traversal_depth > self.max_traversal_depth {
            return Err(E::custom(format!(
                "参数 Serde 遍历超过 {} 层调用深度限制",
                self.max_traversal_depth
            )));
        }

        Ok(())
    }
}
