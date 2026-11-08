pub use event_bus::{CancellationToken, Event, EventHandler, EventPublisher, EventResult};

/// 宿主向需要广播的应用流程显式提供发布入口。
pub trait EventPublisherProvider {
    fn event_publisher(&self) -> &EventPublisher;
}
