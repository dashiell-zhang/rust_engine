use std::{
    panic::AssertUnwindSafe,
    pin::Pin,
    task::{Context, Poll},
};

use futures_util::Stream;
use serde::Serialize;
use serde_json::Value;

use crate::InvocationLog;

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

/// 每个流拥有独立日志状态；尚未被消费的流不触发日志生命周期。
pub struct LoggedStream<S: Stream, C> {
    source: Pin<Box<S>>,
    inspect: Box<C>,
    method: Option<String>,
    arguments: Vec<(&'static str, Value)>,
    invocation: Option<InvocationLog>,
    enumerated_count: i64,
    captured_items: Vec<Value>,
    finished: bool,
}

impl<S: Stream, C> LoggedStream<S, C> {
    #[doc(hidden)]
    pub fn new(source: S, method: String, arguments: Vec<(&'static str, Value)>, inspect: C) -> Self
    where
        C: FnMut(&S::Item, bool, bool) -> StreamItemLog,
    {
        Self {
            source: Box::pin(source),
            inspect: Box::new(inspect),
            method: Some(method),
            arguments,
            invocation: None,
            enumerated_count: 0,
            captured_items: Vec::new(),
            finished: false,
        }
    }

    fn start(&mut self) {
        if let Some(method) = self.method.take() {
            let mut invocation = InvocationLog::new(method);
            for (name, value) in self.arguments.drain(..) {
                invocation.set_argument(name, || value);
            }
            invocation.start();
            self.invocation = Some(invocation);
        }
    }

    fn complete(&mut self, completed_naturally: bool) {
        self.finished = true;
        if let Some(invocation) = &mut self.invocation {
            let snapshot = AsyncStreamResultSnapshot {
                enumerated_count: self.enumerated_count,
                completed_naturally,
                truncated: self.enumerated_count > self.captured_items.len() as i64,
                captured_items: std::mem::take(&mut self.captured_items),
            };
            invocation.complete_value(&snapshot, || {
                serde_json::to_value(&snapshot).unwrap_or(Value::Null)
            });
        }
    }
}

impl<S, C> Stream for LoggedStream<S, C>
where
    S: Stream,
    C: FnMut(&S::Item, bool, bool) -> StreamItemLog,
{
    type Item = S::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.finished {
            return Poll::Ready(None);
        }
        this.start();
        let outcome =
            std::panic::catch_unwind(AssertUnwindSafe(|| this.source.as_mut().poll_next(cx)));
        match outcome {
            Ok(Poll::Pending) => Poll::Pending,
            Ok(Poll::Ready(None)) => {
                this.complete(true);
                Poll::Ready(None)
            }
            Ok(Poll::Ready(Some(item))) => {
                let invocation = this
                    .invocation
                    .as_mut()
                    .expect("已开始的流必须持有调用状态");
                match (this.inspect)(
                    &item,
                    invocation.capture_stream_items() && this.captured_items.len() < 100,
                    invocation.capture_stream_error(),
                ) {
                    StreamItemLog::Value(value) => {
                        this.enumerated_count = this.enumerated_count.saturating_add(1);
                        if let Some(value) = value {
                            this.captured_items.push(value);
                        }
                    }
                    StreamItemLog::Error(error) => {
                        this.finished = true;
                        invocation.fail(|| error.unwrap_or(Value::Null));
                    }
                }
                Poll::Ready(Some(item))
            }
            Err(payload) => {
                this.finished = true;
                if let Some(invocation) = &mut this.invocation {
                    invocation.panic(payload.as_ref());
                }
                std::panic::resume_unwind(payload)
            }
        }
    }
}

impl<S: Stream, C> Drop for LoggedStream<S, C> {
    fn drop(&mut self) {
        if !self.finished && self.invocation.is_some() {
            // 提前释放仍是成功结束行为生命周期，但不是自然枚举完成。
            self.complete(false);
        }
    }
}
