use crate::*;
use std::cell::RefCell;

/// 限制首次参数序列化允许访问的 JSON 深度和项目数量
pub(crate) struct SerializationBudget {
    pub(crate) item_count: Cell<usize>,

    pub(crate) max_depth: usize,

    pub(crate) max_traversal_depth: usize,

    pub(crate) max_items: usize,

    pub(crate) safety_policy: ParameterKeySafetyPolicy,

    /// 日志快照复用递归遍历，但不应用缓存键的类型与项目数策略。
    snapshot_path: Option<RefCell<Vec<(&'static str, usize)>>>,
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
            snapshot_path: None,
        }
    }

    pub(crate) fn for_snapshot() -> Self {
        Self {
            snapshot_path: Some(RefCell::new(Vec::new())),
            ..Self::new(64, 256, usize::MAX, ParameterKeySafetyPolicy::default())
        }
    }

    pub(crate) fn is_snapshot(&self) -> bool {
        self.snapshot_path.is_some()
    }

    /// 仅在当前递归路径检测循环；同一对象在其他字段再次出现仍正常记录。
    pub(crate) fn enter_snapshot<T: ?Sized>(&self, value: &T) -> Option<SnapshotPathGuard<'_>> {
        if let Some(path) = &self.snapshot_path {
            let identity = (
                std::any::type_name::<T>(),
                std::ptr::from_ref(value).cast::<()>().addr(),
            );
            let mut active = path.borrow_mut();
            if active.contains(&identity) {
                return None;
            }
            active.push(identity);
        }
        Some(SnapshotPathGuard(self.snapshot_path.as_ref()))
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

pub(crate) struct SnapshotPathGuard<'a>(Option<&'a RefCell<Vec<(&'static str, usize)>>>);

impl Drop for SnapshotPathGuard<'_> {
    fn drop(&mut self) {
        if let Some(path) = self.0 {
            path.borrow_mut().pop();
        }
    }
}
