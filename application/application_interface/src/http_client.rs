use reqwest::Client;
use std::sync::RwLock;
use std::time::{Duration, Instant};

const DEFAULT_CLIENT_LIFETIME_SECONDS: u64 = 120;
const DEFAULT_POOL_IDLE_TIMEOUT_SECONDS: u64 = 90;
const DEFAULT_CONNECT_TIMEOUT_SECONDS: u64 = 10;
const DEFAULT_REQUEST_TIMEOUT_SECONDS: u64 = 30;
const DEFAULT_POOL_MAX_IDLE_PER_HOST: usize = 16;

/// HTTP 客户端工厂选项
#[derive(Clone, Copy)]
pub struct HttpClientFactoryOptions {
    /// Client 生命周期，超过后轮换底层连接池
    pub client_lifetime: Duration,

    /// 连接池空闲连接保留时长
    pub pool_idle_timeout: Duration,

    /// 建连超时时间
    pub connect_timeout: Duration,

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
            connect_timeout: Duration::from_secs(DEFAULT_CONNECT_TIMEOUT_SECONDS),
            request_timeout: Duration::from_secs(DEFAULT_REQUEST_TIMEOUT_SECONDS),
            pool_max_idle_per_host: DEFAULT_POOL_MAX_IDLE_PER_HOST,
        }
    }
}

struct HttpClientFactoryInner {
    client: Client,
    created_at: Instant,
}

/// 类似 IHttpClientFactory 的宿主级 HTTP 客户端工厂
pub struct HttpClientFactory {
    options: HttpClientFactoryOptions,
    inner: RwLock<HttpClientFactoryInner>,
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
            inner: RwLock::new(HttpClientFactoryInner {
                client: build_http_client(options)?,
                created_at: Instant::now(),
            }),
        })
    }

    /// 获取当前可用的 HTTP 客户端
    pub fn client(&self) -> Client {
        if let Ok(inner) = self.inner.read()
            && inner.created_at.elapsed() < self.options.client_lifetime
        {
            return inner.client.clone();
        }

        let Ok(mut inner) = self.inner.write() else {
            return build_http_client(self.options).unwrap_or_else(|_| Client::new());
        };

        if inner.created_at.elapsed() >= self.options.client_lifetime
            && let Ok(client) = build_http_client(self.options)
        {
            inner.client = client;
            inner.created_at = Instant::now();
        }

        inner.client.clone()
    }
}

fn build_http_client(options: HttpClientFactoryOptions) -> Result<Client, reqwest::Error> {
    Client::builder()
        .pool_idle_timeout(options.pool_idle_timeout)
        .pool_max_idle_per_host(options.pool_max_idle_per_host)
        .connect_timeout(options.connect_timeout)
        .timeout(options.request_timeout)
        .build()
}
