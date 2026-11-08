//! 将请求 future 和响应体的中断传播给业务调用，不把正常响应结束视为取消。
use std::{
    convert::Infallible,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use axum::{
    body::{Body, Bytes, HttpBody},
    extract::{FromRequestParts, Request, State},
    http::{Method, StatusCode, request::Parts},
    middleware::Next,
    response::{IntoResponse, Response},
};
use common::cancellation::CancellationToken;
use http_body::{Frame, SizeHint};
use tokio_util::task::TaskTracker;
use tracing::Instrument;

/// 宿主持有运行时守卫；连接断开后的协作清理仍计入宿主停机期限。
pub(crate) struct RequestRuntime {
    tasks: RequestTasks,
}

#[derive(Clone)]
pub(crate) struct RequestTasks {
    tracker: TaskTracker,
    abort: CancellationToken,
}

impl RequestRuntime {
    pub(crate) fn new() -> Self {
        Self {
            tasks: RequestTasks {
                tracker: TaskTracker::new(),
                abort: CancellationToken::new(),
            },
        }
    }

    pub(crate) fn tasks(&self) -> RequestTasks {
        self.tasks.clone()
    }

    /// HTTP 服务停止接收并排空后调用，避免关闭跟踪器后仍进入新的请求。
    pub(crate) async fn shutdown(&self, deadline: tokio::time::Instant) -> bool {
        self.tasks.tracker.close();
        if tokio::time::timeout_at(deadline, self.tasks.tracker.wait())
            .await
            .is_ok()
        {
            return true;
        }
        self.tasks.abort.cancel();
        false
    }
}

impl Drop for RequestRuntime {
    fn drop(&mut self) {
        // 异常退出也必须结束已脱离连接 future 的处理任务。
        self.tasks.abort.cancel();
    }
}

impl RequestTasks {
    async fn run(&self, request: impl Future<Output = Response> + Send + 'static) -> Response {
        let abort = self.abort.clone();
        let task = self.tracker.spawn(
            async move {
                tokio::select! {
                    biased;
                    _ = abort.cancelled() => None,
                    response = request => Some(response),
                }
            }
            .instrument(tracing::Span::current()),
        );
        match task.await {
            Ok(Some(response)) => response,
            Ok(None) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
            Err(_) => crate::error::internal_error_response("请求处理任务异常退出"),
        }
    }
}

/// 入口按需提取并显式传给应用服务；与 HTTP 上下文无关的业务契约只接收令牌。
#[derive(Clone, Default)]
pub struct RequestCancellation(pub CancellationToken);

impl<S: Send + Sync> FromRequestParts<S> for RequestCancellation {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        Ok(parts.extensions.get::<Self>().cloned().unwrap_or_default())
    }
}

struct CancelOnAbort {
    token: CancellationToken,
    completed: bool,
}

impl Drop for CancelOnAbort {
    fn drop(&mut self) {
        if !self.completed {
            self.token.cancel();
        }
    }
}

pub(crate) async fn request_cancellation(
    State(tasks): State<Option<RequestTasks>>,
    mut request: Request,
    next: Next,
) -> Response {
    // 在请求任务调度和任何内层鉴权等待之前固定正文存在状态。
    let body_presence = crate::json_body::BodyPresence::from_request(&request);
    request.extensions_mut().insert(body_presence);
    let token = CancellationToken::new();
    let mut guard = CancelOnAbort {
        token: token.clone(),
        completed: false,
    };
    let head = request.method() == Method::HEAD;
    request.extensions_mut().insert(RequestCancellation(token));
    let response = if let Some(tasks) = tasks {
        tasks.run(next.run(request)).await
    } else {
        // 单独使用路由构建器时没有宿主负责排空，保持请求作用域内执行。
        next.run(request).await
    };
    let status = response.status();
    let (parts, body) = response.into_parts();
    // 这些响应由 HTTP 层省略正文，不能把其正常释放误判成断连。
    guard.completed = head
        || status == StatusCode::NO_CONTENT
        || status == StatusCode::NOT_MODIFIED
        || status.is_informational()
        || body.is_end_stream();
    Response::from_parts(parts, Body::new(CancellationBody { body, guard }))
}

struct CancellationBody {
    body: Body,
    guard: CancelOnAbort,
}

impl HttpBody for CancellationBody {
    type Data = Bytes;
    type Error = axum::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
        let frame = Pin::new(&mut self.body).poll_frame(cx);
        match &frame {
            Poll::Ready(Some(Err(_))) => self.guard.token.cancel(),
            Poll::Ready(None) => self.guard.completed = true,
            Poll::Ready(Some(Ok(_))) if self.body.is_end_stream() => self.guard.completed = true,
            _ => {}
        }
        frame
    }

    fn is_end_stream(&self) -> bool {
        self.body.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.body.size_hint()
    }
}
