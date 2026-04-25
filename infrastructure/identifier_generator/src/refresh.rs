use distributed_cache::DistributedCache;
use std::sync::Arc;

use crate::generator::{IdentifierGenerator, SIGN_REFRESH_INTERVAL};

/// 机器标识刷新任务
///
/// 对应 NetEngine 的 `RefreshSignTask`，用于在多节点部署场景下定期刷新雪花 ID 生成器的机器编号
/// 通过外部 `sign_provider` 获取当前节点应使用的机器标识与数据中心标识，并更新至生成器中
///
/// 典型用途：从数据库或注册中心动态分配节点编号，防止多实例产生冲突 ID
pub struct RefreshSignTask {
    /// 雪花标识生成器引用
    generator: Arc<IdentifierGenerator>,

    /// 分布式缓存
    distributed_cache: Arc<dyn DistributedCache>,
}

impl RefreshSignTask {
    /// 创建刷新任务
    pub fn new(
        generator: Arc<IdentifierGenerator>,
        distributed_cache: Arc<dyn DistributedCache>,
    ) -> Self {
        Self {
            generator,
            distributed_cache,
        }
    }

    /// 立即执行一次刷新，并启动后台循环定时刷新
    pub async fn run(&self) {
        self.generator.get_id().await.ok();
        Self::try_refresh(&self.generator, self.distributed_cache.as_ref()).await;

        let generator = Arc::clone(&self.generator);
        let distributed_cache = Arc::clone(&self.distributed_cache);

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(SIGN_REFRESH_INTERVAL).await;
                Self::try_refresh(&generator, distributed_cache.as_ref()).await;
            }
        });
    }

    /// 尝试刷新一次节点占位标记
    async fn try_refresh(
        generator: &IdentifierGenerator,
        distributed_cache: &dyn DistributedCache,
    ) {
        if let Err(error) = generator.refresh_sign(distributed_cache).await {
            eprintln!("[identifier_generator] 节点标识续租失败: {error}");
        }
    }
}
