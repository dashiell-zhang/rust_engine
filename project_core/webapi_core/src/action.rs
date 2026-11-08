//! 在 Action 过滤器之前完成参数提取，业务调用只在过滤器放行后发生。

use std::{
    future::Future,
    sync::{Arc, Mutex},
};

use axum::{
    body::Body,
    extract::{FromRequest, FromRequestParts, Request},
    handler::Handler,
    response::{IntoResponse, Response},
    routing::MethodRouter,
};
use futures_util::future::BoxFuture;
use tower::ServiceExt;

use crate::{error::internal_error_response, replay_body::ReplayBody};

type Invocation = Box<dyn FnOnce() -> BoxFuture<'static, Response> + Send>;

#[derive(Clone)]
struct PreparedAction(Arc<Mutex<Option<Invocation>>>);

pub(crate) async fn invoke(mut request: Request) -> Response {
    let invocation = request
        .extensions_mut()
        .remove::<PreparedAction>()
        .and_then(|action| action.0.lock().ok()?.take());
    match invocation {
        Some(invoke) => invoke().await,
        None => internal_error_response("缺少已绑定的接口调用"),
    }
}

/// 控制器函数的参数绑定适配器；由路由构建器创建。
#[derive(Clone)]
pub struct BoundHandler<H, S> {
    pub(crate) handler: H,
    pub(crate) action: MethodRouter<S>,
    pub(crate) replay: bool,
}

async fn bodies(body: Body, replay: bool) -> Result<(Body, Body, Option<ReplayBody>), String> {
    if !replay {
        return Ok((body, Body::empty(), None));
    }
    let buffered = ReplayBody::new(body);
    let binding = buffered.clone().into_body().await?;
    let action = buffered.clone().into_body().await?;
    Ok((binding, action, Some(buffered)))
}

impl<H, F, R, S> Handler<((),), S> for BoundHandler<H, S>
where
    H: FnOnce() -> F + Clone + Send + Sync + 'static,
    F: Future<Output = R> + Send + 'static,
    R: IntoResponse,
    S: Clone + Send + Sync + 'static,
{
    type Future = BoxFuture<'static, Response>;
    fn call(self, mut request: Request, state: S) -> Self::Future {
        Box::pin(async move {
            request
                .extensions_mut()
                .insert(PreparedAction(Arc::new(Mutex::new(Some(Box::new(
                    move || Box::pin(async move { (self.handler)().await.into_response() }),
                ))))));
            match self.action.with_state(state).oneshot(request).await {
                Ok(response) => response,
                Err(error) => match error {},
            }
        })
    }
}

macro_rules! bound_handler {
    ([$($part:ident),*], $last:ident) => {
        #[allow(non_snake_case, unused_mut)]
        impl<H, F, R, S, M, $($part,)* $last> Handler<(M, $($part,)* $last,), S>
            for BoundHandler<H, S>
        where
            H: FnOnce($($part,)* $last) -> F + Clone + Send + Sync + 'static,
            F: Future<Output = R> + Send + 'static,
            R: IntoResponse,
            S: Clone + Send + Sync + 'static,
            $($part: FromRequestParts<S> + Send + 'static,)*
            $last: FromRequest<S, M> + Send + 'static,
        {
            type Future = BoxFuture<'static, Response>;
            fn call(self, request: Request, state: S) -> Self::Future {
                Box::pin(async move {
                    let body_presence = crate::json_body::BodyPresence::from_request(&request);
                    let (mut parts, body) = request.into_parts();
                    parts.extensions.insert(body_presence);
                    let mut model_errors = crate::request_binding::ModelErrors::default();
                    $(let $part = match $part::from_request_parts(&mut parts, &state).await {
                        Ok(value) => Some(value),
                        Err(error) => {
                            let mut response = error.into_response();
                            if !model_errors.append_response(&mut response) {
                                return response;
                            }
                            None
                        }
                    };)*
                    let mut action_parts = parts.clone();
                    let (binding_body, action_body, buffered) = match bodies(body, self.replay).await {
                        Ok(bodies) => bodies,
                        Err(error) => return internal_error_response(&error),
                    };
                    if let Some(buffered) = buffered {
                        parts.extensions.insert(buffered.clone());
                        action_parts.extensions.insert(buffered);
                    }
                    let $last = match $last::from_request(Request::from_parts(parts, binding_body), &state)
                        .await.map_err(IntoResponse::into_response)
                    {
                        Ok(value) => Some(value),
                        Err(mut error) => {
                            // 缓存文件或网络正文读取失败属于宿主异常，不暴露为模型错误。
                            if let Some(buffered) = action_parts.extensions.get::<ReplayBody>()
                                && let Some(failure) = buffered.failure().await
                            {
                                return internal_error_response(&failure);
                            }
                            if !model_errors.append_response(&mut error) {
                                return error;
                            }
                            None
                        }
                    };
                    if !model_errors.is_empty() {
                        return model_errors.into_response();
                    }
                    // 失败的参数没有替代值；仅在全部绑定成功时创建业务调用。
                    $(let Some($part) = $part else {
                        return internal_error_response("缺少已绑定的接口参数");
                    };)*
                    let Some($last) = $last else {
                        return internal_error_response("缺少已绑定的接口参数");
                    };
                    action_parts.extensions.insert(PreparedAction(Arc::new(Mutex::new(Some(Box::new(
                        move || Box::pin(async move { (self.handler)($($part,)* $last).await.into_response() }),
                    ))))));
                    let request = Request::from_parts(action_parts, action_body);
                    match self.action.with_state(state).oneshot(request).await {
                        Ok(response) => response,
                        Err(error) => match error {},
                    }
                })
            }
        }
    };
}

bound_handler!([], A);
bound_handler!([A], B);
bound_handler!([A, B], C);
bound_handler!([A, B, C], D);
bound_handler!([A, B, C, D], E);
bound_handler!([A, B, C, D, E], G);
bound_handler!([A, B, C, D, E, G], I);
bound_handler!([A, B, C, D, E, G, I], J);
bound_handler!([A, B, C, D, E, G, I, J], K);
bound_handler!([A, B, C, D, E, G, I, J, K], L);
bound_handler!([A, B, C, D, E, G, I, J, K, L], N);
bound_handler!([A, B, C, D, E, G, I, J, K, L, N], O);
bound_handler!([A, B, C, D, E, G, I, J, K, L, N, O], P);
bound_handler!([A, B, C, D, E, G, I, J, K, L, N, O, P], Q);
bound_handler!([A, B, C, D, E, G, I, J, K, L, N, O, P, Q], U);
bound_handler!([A, B, C, D, E, G, I, J, K, L, N, O, P, Q, U], V);
