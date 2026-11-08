use reqwest::Client;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

mod named;
mod redirect;
pub use named::{HttpRequestState, NamedHttpClient, NamedHttpClientRegistration, NamedHttpHandler};

const DEFAULT_CLIENT_LIFETIME_SECONDS: u64 = 120;
const DEFAULT_POOL_IDLE_TIMEOUT_SECONDS: u64 = 60;
const DEFAULT_REQUEST_TIMEOUT_SECONDS: u64 = 100;
const DEFAULT_POOL_MAX_IDLE_PER_HOST: usize = usize::MAX;

/// HTTP 客户端工厂选项
#[derive(Clone, Copy)]
pub struct HttpClientFactoryOptions {
    /// Client 生命周期，超过后轮换底层连接池
    pub client_lifetime: Duration,

    /// 连接池空闲连接保留时长
    pub pool_idle_timeout: Duration,

    /// 独立建连超时；默认不设置，由请求期限控制。
    pub connect_timeout: Option<Duration>,

    /// 单次请求整体超时时间
    pub request_timeout: Duration,

    /// 每个 host 最大空闲连接数
    pub pool_max_idle_per_host: usize,
}

impl Default for HttpClientFactoryOptions {
    fn default() -> Self {
        Self {
            client_lifetime: Duration::from_secs(DEFAULT_CLIENT_LIFETIME_SECONDS),
            pool_idle_timeout: Duration::from_secs(DEFAULT_POOL_IDLE_TIMEOUT_SECONDS),
            connect_timeout: None,
            request_timeout: Duration::from_secs(DEFAULT_REQUEST_TIMEOUT_SECONDS),
            pool_max_idle_per_host: DEFAULT_POOL_MAX_IDLE_PER_HOST,
        }
    }
}

struct HttpClientFactoryInner {
    client: Client,
    streaming_client: Client,
    raw_streaming_client: Client,
    created_at: Instant,
}

/// 类似 IHttpClientFactory 的宿主级 HTTP 客户端工厂
pub struct HttpClientFactory {
    options: HttpClientFactoryOptions,
    inner: RwLock<HttpClientFactoryInner>,
    named: RwLock<HashMap<String, Arc<NamedHttpClientRegistration>>>,
}

impl HttpClientFactory {
    /// 使用默认选项创建 HTTP 客户端工厂
    pub fn new() -> Result<Self, reqwest::Error> {
        Self::with_options(HttpClientFactoryOptions::default())
    }

    /// 使用指定选项创建 HTTP 客户端工厂
    pub fn with_options(options: HttpClientFactoryOptions) -> Result<Self, reqwest::Error> {
        Ok(Self {
            options,
            named: RwLock::new(HashMap::new()),
            inner: RwLock::new(HttpClientFactoryInner {
                client: build_http_client_builder(options)
                    .timeout(options.request_timeout)
                    .build()?,
                streaming_client: build_http_client_builder(options).build()?,
                raw_streaming_client: build_raw_streaming_client_builder(options).build()?,
                created_at: Instant::now(),
            }),
        })
    }

    /// 获取带整包请求超时的普通客户端。
    pub fn client(&self) -> Client {
        self.clients().0
    }

    /// 获取复用连接池的流式客户端，调用方仅为响应头施加请求期限。
    pub fn streaming_client(&self) -> Client {
        self.clients().1
    }

    /// 原始对象传输不自动解压，保持 Range 偏移及对象长度；连接池仍由宿主轮换。
    pub fn raw_streaming_client(&self) -> Client {
        self.clients().2
    }

    pub fn request_timeout(&self) -> Duration {
        self.options.request_timeout
    }

    /// 在宿主装配阶段注册命名客户端；连接和处理器按名称隔离并惰性创建。
    pub fn register_named(
        &mut self,
        name: impl Into<String>,
        registration: NamedHttpClientRegistration,
    ) {
        self.named
            .get_mut()
            .unwrap_or_else(|error| error.into_inner())
            .insert(name.into(), Arc::new(registration));
    }

    pub fn named_client(
        &self,
        name: &str,
    ) -> Result<NamedHttpClient, common::error_helper::CustomError> {
        let named = self.named.read().unwrap_or_else(|error| error.into_inner());
        if let Some(registration) = named.get(name).cloned() {
            drop(named);
            return registration.client(self);
        }
        drop(named);
        if name.is_empty() {
            return Ok(NamedHttpClient::new(
                self.streaming_client(),
                None,
                self.options.request_timeout,
            ));
        }
        // IHttpClientFactory 为未注册名称创建独立默认处理器，不套用空名称的配置。
        let registration = self
            .named
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .entry(name.to_owned())
            .or_insert_with(|| Arc::new(NamedHttpClientRegistration::unconfigured()))
            .clone();
        registration.client(self)
    }

    fn clients(&self) -> (Client, Client, Client) {
        let inner = self.inner.read().unwrap_or_else(|error| error.into_inner());
        if inner.created_at.elapsed() < self.options.client_lifetime {
            return (
                inner.client.clone(),
                inner.streaming_client.clone(),
                inner.raw_streaming_client.clone(),
            );
        }
        drop(inner);
        let mut inner = self
            .inner
            .write()
            .unwrap_or_else(|error| error.into_inner());
        if inner.created_at.elapsed() >= self.options.client_lifetime {
            let ordinary = build_http_client_builder(self.options)
                .timeout(self.options.request_timeout)
                .build();
            let streaming = build_http_client_builder(self.options).build();
            let raw_streaming = build_raw_streaming_client_builder(self.options).build();
            if let (Ok(client), Ok(streaming_client), Ok(raw_streaming_client)) =
                (ordinary, streaming, raw_streaming)
            {
                inner.client = client;
                inner.streaming_client = streaming_client;
                inner.raw_streaming_client = raw_streaming_client;
                inner.created_at = Instant::now();
            }
        }
        (
            inner.client.clone(),
            inner.streaming_client.clone(),
            inner.raw_streaming_client.clone(),
        )
    }

    /// 使用工厂统一选项创建一次性定制客户端
    pub fn build_client_with(
        &self,
        configure: impl FnOnce(reqwest::ClientBuilder) -> reqwest::ClientBuilder,
    ) -> Result<Client, reqwest::Error> {
        configure(build_http_client_builder(self.options).timeout(self.options.request_timeout))
            .build()
    }

    /// 创建由调用方控制超时的流式客户端；响应头与正文可以采用不同期限。
    /// 调用方必须负责请求取消与响应头期限，不沿用普通请求的整包超时。
    pub fn build_streaming_client_with(
        &self,
        configure: impl FnOnce(reqwest::ClientBuilder) -> reqwest::ClientBuilder,
    ) -> Result<Client, reqwest::Error> {
        configure(
            Client::builder()
                .pool_idle_timeout(self.options.pool_idle_timeout)
                .pool_max_idle_per_host(self.options.pool_max_idle_per_host),
        )
        .build()
    }
}

/// 三个宿主的默认客户端不跟随重定向、不保存 Cookie，并解压 gzip/deflate/br。
fn build_http_client_builder(options: HttpClientFactoryOptions) -> reqwest::ClientBuilder {
    let builder = Client::builder()
        .pool_idle_timeout(options.pool_idle_timeout)
        .pool_max_idle_per_host(options.pool_max_idle_per_host)
        .redirect(reqwest::redirect::Policy::none())
        .gzip(true)
        .deflate(true)
        .brotli(true)
        .no_zstd();
    match options.connect_timeout {
        Some(timeout) => builder.connect_timeout(timeout),
        None => builder,
    }
}

fn build_raw_streaming_client_builder(options: HttpClientFactoryOptions) -> reqwest::ClientBuilder {
    build_http_client_builder(options)
        .no_gzip()
        .no_deflate()
        .no_brotli()
}
