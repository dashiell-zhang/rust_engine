use std::sync::Arc;

use arc_swap::ArcSwap;
use tracing::{Metadata, Subscriber, subscriber::Interest};
use tracing_subscriber::layer::{Context, Filter};

use crate::{LoggerProvider, LoggerSettings, event_level, filter::ProviderFilter};

/// 更新入口归属 LoggerRuntime；两层过滤器共享同一份不可变配置快照。
#[derive(Clone)]
pub struct LoggerFilterHandle {
    current: Arc<ArcSwap<FilterSnapshot>>,
    provider: LoggerProvider,
}

struct FilterSnapshot {
    console: ProviderFilter,
    storage: ProviderFilter,
}

impl FilterSnapshot {
    fn new(settings: &LoggerSettings, provider: LoggerProvider) -> Result<Self, String> {
        settings.validate_log_filter(provider)?;
        Ok(Self {
            console: ProviderFilter::new(settings, LoggerProvider::Console)?,
            storage: ProviderFilter::new(settings, provider)?,
        })
    }
}

impl LoggerFilterHandle {
    pub(super) fn new(settings: &LoggerSettings, provider: LoggerProvider) -> Result<Self, String> {
        Ok(Self {
            current: Arc::new(ArcSwap::from_pointee(FilterSnapshot::new(
                settings, provider,
            )?)),
            provider,
        })
    }

    pub(super) fn reload(&self, settings: &LoggerSettings) -> Result<(), String> {
        // 两个目标都编译成功后再发布，失败不产生部分更新。
        let next = FilterSnapshot::new(settings, self.provider)?;
        self.current.store(Arc::new(next));
        Ok(())
    }

    pub(super) fn console_filter(&self) -> RuntimeFilter {
        RuntimeFilter {
            current: Arc::clone(&self.current),
            console: true,
        }
    }

    pub(super) fn storage_filter(&self) -> RuntimeFilter {
        RuntimeFilter {
            current: Arc::clone(&self.current),
            console: false,
        }
    }
}

pub(super) struct RuntimeFilter {
    current: Arc<ArcSwap<FilterSnapshot>>,
    console: bool,
}

impl<S: Subscriber> Filter<S> for RuntimeFilter {
    fn enabled(&self, metadata: &Metadata<'_>, _context: &Context<'_, S>) -> bool {
        // Scope 与事件阈值独立，更新前创建的 span 仍能提供上下文。
        if metadata.is_span() {
            return true;
        }
        let snapshot = self.current.load();
        let filter = if self.console {
            &snapshot.console
        } else {
            &snapshot.storage
        };
        event_level(metadata) as i32 >= filter.minimum(metadata.target())
    }

    fn callsite_enabled(&self, _metadata: &'static Metadata<'static>) -> Interest {
        // 不让调用点缓存旧的拒绝结果，否则降低级别后旧调用点仍不会发出事件。
        Interest::sometimes()
    }
}
