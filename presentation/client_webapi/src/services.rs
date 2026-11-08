use std::{convert::Infallible, time::Duration};

use crate::state::AppState;
use application_model::article::models::ArticleDto;
use common::app_error::AppError;
use futures_util::{Stream, stream};
use source_generator_runtime::{generate_direct_service_factory, invocation};
use tokio::time::sleep;

/// 宿主演示服务工厂，每次请求创建服务，只借用宿主基础设施。
pub struct DemoServices<'a> {
    state: &'a AppState,
}

impl<'a> DemoServices<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }
}

generate_direct_service_factory! {
    impl_generics = <'a>;
    factory = DemoServices<'a>;
    (self.state) {
        demo: DemoService<'a> => dyn Demo + 'a,
        demo2: Demo2Service<'a>
    }
}

/// 与接口代理示例对应的宿主局部契约。
#[async_trait::async_trait]
pub trait Demo: Send + Sync {
    fn echo(&self, name: &str) -> String;
    async fn add(&self, a: i32, b: i32) -> Result<i32, AppError>;
}

/// 客户端宿主的接口服务演示。
pub struct DemoService<'a> {
    state: &'a AppState,
}

impl<'a> DemoService<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    pub fn echo(&self, name: &str) -> String {
        format!("hello, {name}")
    }

    #[invocation(key_scope = "client.demo.add", key_include_self = false)]
    #[cacheable(cache = self.state.cache, lock = self.state.distributed_lock, ttl_seconds = 120)]
    pub async fn add(&self, a: i32, b: i32) -> Result<i32, AppError> {
        sleep(Duration::from_millis(10)).await;
        Ok(a.wrapping_add(b))
    }
}

#[async_trait::async_trait]
impl Demo for DemoService<'_> {
    fn echo(&self, name: &str) -> String {
        DemoService::echo(self, name)
    }
    async fn add(&self, a: i32, b: i32) -> Result<i32, AppError> {
        DemoService::add(self, a, b).await
    }
}

/// 客户端宿主的引用参数及异步序列演示。
pub struct Demo2Service<'a> {
    state: &'a AppState,
}

impl<'a> Demo2Service<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    pub fn echo(&self, name: &str) -> String {
        format!("hello, {name}")
    }

    #[invocation(key_scope = "client.demo2.add", key_include_self = false)]
    #[cacheable(cache = self.state.cache, lock = self.state.distributed_lock, ttl_seconds = 120,
        key = (a, b, source_generator_runtime::trusted_serde_key(article)))]
    pub async fn add(&self, a: i32, b: i32, article: &ArticleDto) -> Result<i32, AppError> {
        let _ = article;
        sleep(Duration::from_millis(10)).await;
        Ok(a.wrapping_add(b))
    }

    #[invocation(key_scope = "client.demo2.add-ref", key_include_self = false)]
    #[logging(result = false)]
    pub fn add_ref(&self, a: &mut i32, b: i32) -> i32 {
        let sum = a.wrapping_add(b);
        *a = 2019;
        sum
    }

    pub fn stream_numbers(
        &self,
        count: i32,
    ) -> impl Stream<Item = Result<i32, Infallible>> + use<> {
        number_stream(count, Duration::from_millis(50))
    }

    pub async fn stream_numbers_async(
        &self,
        count: i32,
    ) -> impl Stream<Item = Result<i32, Infallible>> + use<> {
        sleep(Duration::from_millis(100)).await;
        number_stream(count, Duration::from_millis(30))
    }
}

/// 每次消费只生产一个数字；释放序列会同时释放当前等待。
fn number_stream(count: i32, delay: Duration) -> impl Stream<Item = Result<i32, Infallible>> {
    stream::unfold(0..count, move |mut numbers| async move {
        let value = numbers.next()?;
        sleep(delay).await;
        Some((Ok(value), numbers))
    })
}
