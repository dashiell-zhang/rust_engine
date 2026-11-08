use futures_util::Stream;
use serde::Serialize;
use serde_json::Value;

use crate::{
    InvocationBehavior, InvocationContext, InvocationLog, ObservedStream, StreamCompletion,
    StreamObserver,
};

/// 流调用结束时的独立快照，字段与源代理的快照契约一致。
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct AsyncStreamResultSnapshot {
    pub enumerated_count: i64,
    pub completed_naturally: bool,
    pub truncated: bool,
    pub captured_items: Vec<Value>,
}

#[doc(hidden)]
pub enum StreamItemLog {
    Value(Option<Value>),
    Error(Option<Value>),
}

/// 日志属性和外部行为共用同一个延迟枚举管道，不另外维护 poll/Drop 生命周期。
pub struct StreamLoggingBehavior<C> {
    arguments: Vec<(&'static str, Value)>,
    inspect: C,
}

impl<C> StreamLoggingBehavior<C> {
    pub fn new(arguments: Vec<(&'static str, Value)>, inspect: C) -> Self {
        Self { arguments, inspect }
    }
}

impl<S, C> InvocationBehavior<S> for StreamLoggingBehavior<C>
where
    S: Stream,
    C: FnMut(&S::Item, bool, bool) -> StreamItemLog,
{
    type Output = ObservedStream<S, StreamLogObserver<C>>;
    fn invoke(self, context: InvocationContext, source: S) -> Self::Output {
        ObservedStream::new(
            context,
            source,
            StreamLogObserver {
                inspect: self.inspect,
                arguments: self.arguments,
                invocation: None,
                enumerated_count: 0,
                captured_items: Vec::new(),
                failed: false,
            },
        )
    }
}

#[doc(hidden)]
pub struct StreamLogObserver<C> {
    inspect: C,
    arguments: Vec<(&'static str, Value)>,
    invocation: Option<InvocationLog>,
    enumerated_count: i64,
    captured_items: Vec<Value>,
    failed: bool,
}

impl<T, C: FnMut(&T, bool, bool) -> StreamItemLog> StreamObserver<T> for StreamLogObserver<C> {
    fn on_start(&mut self, context: &InvocationContext) {
        let mut invocation = InvocationLog::new(context.method.clone());
        for (name, value) in self.arguments.drain(..) {
            invocation.set_argument(name, || value);
        }
        invocation.start();
        self.invocation = Some(invocation);
    }

    fn on_item(&mut self, _context: &InvocationContext, item: &T) -> bool {
        let invocation = self
            .invocation
            .as_mut()
            .expect("已开始的流必须持有调用状态");
        match (self.inspect)(
            item,
            invocation.capture_stream_items() && self.captured_items.len() < 100,
            invocation.capture_stream_error(),
        ) {
            StreamItemLog::Value(value) => {
                self.enumerated_count = self.enumerated_count.saturating_add(1);
                if let Some(value) = value {
                    self.captured_items.push(value);
                }
                false
            }
            StreamItemLog::Error(error) => {
                self.failed = true;
                invocation.fail(|| error.unwrap_or(Value::Null));
                true
            }
        }
    }

    fn on_finish(&mut self, _context: &InvocationContext, completion: StreamCompletion) {
        if self.failed {
            return;
        }
        if let Some(invocation) = &mut self.invocation {
            let snapshot = AsyncStreamResultSnapshot {
                enumerated_count: self.enumerated_count,
                completed_naturally: completion == StreamCompletion::Exhausted,
                truncated: self.enumerated_count > self.captured_items.len() as i64,
                captured_items: std::mem::take(&mut self.captured_items),
            };
            invocation.complete_value(&snapshot, || {
                serde_json::to_value(&snapshot).unwrap_or(Value::Null)
            });
        }
    }

    fn on_panic(&mut self, _context: &InvocationContext, payload: &(dyn std::any::Any + Send)) {
        if let Some(invocation) = &mut self.invocation {
            invocation.panic(payload);
        }
    }
}

/// 保留既有日志宏的构造入口；实际生命周期交给通用行为与流观察器。
pub struct LoggedStream<S: Stream, C: FnMut(&S::Item, bool, bool) -> StreamItemLog> {
    inner: ObservedStream<S, StreamLogObserver<C>>,
}
impl<S: Stream, C: FnMut(&S::Item, bool, bool) -> StreamItemLog> LoggedStream<S, C> {
    #[doc(hidden)]
    pub fn new(
        source: S,
        method: String,
        arguments: Vec<(&'static str, Value)>,
        inspect: C,
    ) -> Self {
        Self {
            inner: StreamLoggingBehavior::new(arguments, inspect)
                .invoke(InvocationContext::new(method), source),
        }
    }
}

impl<S: Stream, C: FnMut(&S::Item, bool, bool) -> StreamItemLog> Stream for LoggedStream<S, C> {
    type Item = S::Item;
    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        std::pin::Pin::new(&mut self.get_mut().inner).poll_next(cx)
    }
}
