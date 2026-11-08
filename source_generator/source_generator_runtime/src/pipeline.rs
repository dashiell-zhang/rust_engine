//! 静态类型调用管道。行为可由外部类型或捕获借用的闭包提供，不使用反射、Any 结果或全局容器。
use futures_util::Stream;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

#[derive(Clone)]
pub struct InvocationContext {
    /// 与既有横切属性相同的稳定方法键，不包含参数值或凭据。
    pub method: String,
}

impl InvocationContext {
    pub fn new(method: String) -> Self {
        Self { method }
    }
}

/// N 分别为同步 continuation、异步 Future 或源 Stream。
/// 行为负责调用/等待/枚举下一层，也可短路；输出由入口函数约束为原有类型。
/// 闭包可直接作为行为，配置和参数通过捕获传入，不要求 'static 或 Send。
pub trait InvocationBehavior<N> {
    type Output;
    fn invoke(self, context: InvocationContext, next: N) -> Self::Output;
}

impl<N, R, B: FnOnce(InvocationContext, N) -> R> InvocationBehavior<N> for B {
    type Output = R;
    fn invoke(self, context: InvocationContext, next: N) -> R {
        self(context, next)
    }
}

pub fn invoke_sync<T, N, B>(context: InvocationContext, behavior: B, next: N) -> T
where
    N: FnOnce() -> T,
    B: InvocationBehavior<N, Output = T>,
{
    behavior.invoke(context, next)
}

pub async fn invoke_async<N, B>(context: InvocationContext, behavior: B, next: N) -> N::Output
where
    N: Future,
    B: InvocationBehavior<N>,
    B::Output: Future<Output = N::Output>,
{
    behavior.invoke(context, next).await
}

/// 行为从首次 poll 开始；未枚举直接释放的流不会执行行为。
/// 各层必须将源流保存在返回流内，不得转入脱离调用方生命周期的后台任务。
pub fn invoke_stream<S, B>(
    context: InvocationContext,
    behavior: B,
    source: S,
) -> impl Stream<Item = S::Item>
where
    S: Stream,
    B: InvocationBehavior<S>,
    B::Output: Stream<Item = S::Item>,
{
    BehaviorStream {
        pending: Some(Box::new((context, behavior, source))),
        running: None,
        finished: false,
    }
}

struct BehaviorStream<S, B: InvocationBehavior<S>> {
    pending: Option<Box<(InvocationContext, B, S)>>,
    running: Option<Pin<Box<B::Output>>>,
    finished: bool,
}

impl<S, B> Stream for BehaviorStream<S, B>
where
    S: Stream,
    B: InvocationBehavior<S>,
    B::Output: Stream<Item = S::Item>,
{
    type Item = S::Item;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.finished {
            return Poll::Ready(None);
        }
        if let Some(pending) = this.pending.take() {
            let (context, behavior, source) = *pending;
            this.running = Some(Box::pin(behavior.invoke(context, source)));
        }
        let result = this
            .running
            .as_mut()
            .expect("已开始的行为必须持有流")
            .as_mut()
            .poll_next(cx);
        if matches!(result, Poll::Ready(None)) {
            this.finished = true;
        }
        result
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum StreamCompletion {
    Exhausted,
    /// 观察器在某个元素后要求停止（例如 Result::Err）。该元素仍返回给调用者。
    Stopped,
    /// 已经开始枚举但被调用者提前释放，包括取消。
    Dropped,
}

/// 可复用的流生命周期观察器。钩子必须同步、非阻塞且不 panic。
/// 异步清理必须由返回流自身的正常结束路径完成，Drop 不能等待异步工作。
pub trait StreamObserver<T> {
    fn on_start(&mut self, _context: &InvocationContext) {}
    fn on_item(&mut self, _context: &InvocationContext, _item: &T) -> bool {
        false
    }
    fn on_finish(&mut self, _context: &InvocationContext, _completion: StreamCompletion) {}
    fn on_panic(&mut self, _context: &InvocationContext, _payload: &(dyn std::any::Any + Send)) {}
}

pub struct ObservedStream<S: Stream, O: StreamObserver<S::Item>> {
    source: Option<Pin<Box<S>>>,
    observer: Box<O>,
    context: InvocationContext,
    started: bool,
    finished: bool,
}

impl<S: Stream, O: StreamObserver<S::Item>> ObservedStream<S, O> {
    pub fn new(context: InvocationContext, source: S, observer: O) -> Self {
        Self {
            source: Some(Box::pin(source)),
            observer: Box::new(observer),
            context,
            started: false,
            finished: false,
        }
    }
    fn finish(&mut self, completion: StreamCompletion) {
        self.finished = true;
        // 观察器主动停止时内层可能尚未耗尽，也必须先完成内层的释放回调。
        drop(self.source.take());
        self.observer.on_finish(&self.context, completion);
    }
}

impl<S: Stream, O: StreamObserver<S::Item>> Stream for ObservedStream<S, O> {
    type Item = S::Item;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.finished {
            return Poll::Ready(None);
        }
        if !this.started {
            this.started = true;
            this.observer.on_start(&this.context);
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            this.source
                .as_mut()
                .expect("未结束的观察器必须持有源流")
                .as_mut()
                .poll_next(cx)
        }));
        match result {
            Ok(Poll::Ready(None)) => {
                this.finish(StreamCompletion::Exhausted);
                Poll::Ready(None)
            }
            Ok(Poll::Ready(Some(item))) => {
                if this.observer.on_item(&this.context, &item) {
                    this.finish(StreamCompletion::Stopped);
                }
                Poll::Ready(Some(item))
            }
            Ok(Poll::Pending) => Poll::Pending,
            Err(payload) => {
                this.finished = true;
                this.observer.on_panic(&this.context, payload.as_ref());
                std::panic::resume_unwind(payload)
            }
        }
    }
}

impl<S: Stream, O: StreamObserver<S::Item>> Drop for ObservedStream<S, O> {
    fn drop(&mut self) {
        // 先释放内层，再结束外层，提前取消也保持与自然结束一致的逆序退出。
        drop(self.source.take());
        if self.started && !self.finished {
            self.finish(StreamCompletion::Dropped);
        }
    }
}
