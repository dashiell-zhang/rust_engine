use std::{
    any::Any,
    collections::HashMap,
    future::Future,
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};

use async_trait::async_trait;
use common::{
    error_helper::CustomError,
    http_client_helper::{self, PostFormDataItem, RequestOptions},
};
use reqwest::{Client, ClientBuilder, Request, Response};

use super::{HttpClientFactory, build_http_client_builder, redirect::UnconfiguredTransport};

/// 宿主出站处理器。处理器随命名连接池轮换，不依赖某次入站请求的作用域。
#[async_trait]
pub trait NamedHttpHandler: Send + Sync {
    async fn before_send(
        &self,
        request: &mut Request,
        multipart_state: Option<&HttpRequestState>,
        options: &mut RequestOptions,
    ) -> Result<(), CustomError>;

    async fn after_response(
        &self,
        _response: &Response,
        _options: &mut RequestOptions,
    ) -> Result<(), CustomError> {
        Ok(())
    }

    async fn prepare_multipart(
        &self,
        _items: &mut [PostFormDataItem],
        _options: &mut RequestOptions,
    ) -> Result<Option<Box<HttpRequestState>>, CustomError> {
        Ok(None)
    }
}

pub type HttpRequestState = dyn Any + Send + Sync;

type ConfigureClient = dyn Fn(ClientBuilder) -> Result<ClientBuilder, CustomError> + Send + Sync;
type BuildHandler = dyn Fn(Client) -> Arc<dyn NamedHttpHandler> + Send + Sync;

pub struct NamedHttpClientRegistration {
    configure: Arc<ConfigureClient>,
    handler: Option<Arc<BuildHandler>>,
    current: RwLock<Option<(NamedHttpClient, Instant)>>,
    unconfigured: bool,
}

impl NamedHttpClientRegistration {
    pub fn new(
        configure: impl Fn(ClientBuilder) -> Result<ClientBuilder, CustomError> + Send + Sync + 'static,
    ) -> Self {
        Self {
            configure: Arc::new(configure),
            handler: None,
            current: RwLock::new(None),
            unconfigured: false,
        }
    }

    pub(super) fn unconfigured() -> Self {
        let mut registration = Self::new(|builder| {
            Ok(builder
                .no_gzip()
                .no_deflate()
                .no_brotli()
                .no_zstd()
                .http1_only())
        });
        registration.unconfigured = true;
        registration
    }

    pub fn with_handler(
        mut self,
        build: impl Fn(Client) -> Arc<dyn NamedHttpHandler> + Send + Sync + 'static,
    ) -> Self {
        self.handler = Some(Arc::new(build));
        self
    }

    pub(super) fn client(
        &self,
        factory: &HttpClientFactory,
    ) -> Result<NamedHttpClient, CustomError> {
        let current = self
            .current
            .read()
            .unwrap_or_else(|error| error.into_inner());
        if let Some((client, created_at)) = current.as_ref()
            && created_at.elapsed() < factory.options.client_lifetime
        {
            return Ok(client.clone());
        }
        drop(current);
        let mut current = self
            .current
            .write()
            .unwrap_or_else(|error| error.into_inner());
        if let Some((client, created_at)) = current.as_ref()
            && created_at.elapsed() < factory.options.client_lifetime
        {
            return Ok(client.clone());
        }
        let client = (self.configure)(build_http_client_builder(factory.options))?
            .build()
            .map_err(|error| CustomError::with_source("创建命名 HTTP 客户端失败", error))?;
        let handler = self.handler.as_ref().map(|build| build(factory.client()));
        let mut client = NamedHttpClient::new(client, handler, factory.options.request_timeout);
        if self.unconfigured {
            client.unconfigured = Some(Arc::new(UnconfiguredTransport::new()));
        }
        *current = Some((client.clone(), Instant::now()));
        Ok(client)
    }
}

/// 发送入口始终经过宿主处理器；不暴露可绕过签名的底层客户端。
#[derive(Clone)]
pub struct NamedHttpClient {
    client: Client,
    handler: Option<Arc<dyn NamedHttpHandler>>,
    request_timeout: Duration,
    request_options: RequestOptions,
    unconfigured: Option<Arc<UnconfiguredTransport>>,
}

impl NamedHttpClient {
    pub(super) fn new(
        client: Client,
        handler: Option<Arc<dyn NamedHttpHandler>>,
        request_timeout: Duration,
    ) -> Self {
        Self {
            client,
            handler,
            request_timeout,
            request_options: RequestOptions::new(),
            unconfigured: None,
        }
    }

    /// 每次发送复制选项容器，处理器增删或替换选项不会污染下一次请求。
    pub fn with_request_options(mut self, options: RequestOptions) -> Self {
        self.request_options.extend_missing(&options);
        self
    }

    /// 普通请求包含处理器、响应头及正文读取，共用同一个期限。
    pub async fn execute(&self, request: Request) -> Result<Response, CustomError> {
        self.with_timeout(async {
            let mut options = self.request_options.clone();
            let response = self.send_inner(request, None, &mut options).await?;
            http_client_helper::buffer_response(response).await
        })
        .await
    }

    /// 只等响应头；返回后的正文流和取消由调用方管理。
    pub async fn execute_streaming(&self, request: Request) -> Result<Response, CustomError> {
        let mut options = self.request_options.clone();
        self.with_timeout(self.send_inner(request, None, &mut options))
            .await
    }

    pub async fn get(
        &self,
        url: &str,
        headers: Option<&HashMap<String, String>>,
    ) -> Result<Response, CustomError> {
        self.execute(build(http_client_helper::apply_headers(
            self.client.get(url),
            headers,
        )?)?)
        .await
    }

    pub async fn delete(
        &self,
        url: &str,
        headers: Option<&HashMap<String, String>>,
    ) -> Result<Response, CustomError> {
        self.execute(build(http_client_helper::apply_headers(
            self.client.delete(url),
            headers,
        )?)?)
        .await
    }

    pub async fn post_string(
        &self,
        url: &str,
        data: String,
        data_type: &str,
        headers: Option<&HashMap<String, String>>,
    ) -> Result<Response, CustomError> {
        self.execute(build(http_client_helper::build_post_string(
            &self.client,
            url,
            data,
            data_type,
            headers,
        )?)?)
        .await
    }

    pub async fn post_form(
        &self,
        url: &str,
        items: &HashMap<String, String>,
        headers: Option<&HashMap<String, String>>,
    ) -> Result<Response, CustomError> {
        self.execute(build(http_client_helper::build_post_form(
            &self.client,
            url,
            items,
            headers,
        )?)?)
        .await
    }

    pub async fn post_form_data(
        &self,
        url: &str,
        mut items: Vec<PostFormDataItem>,
        headers: Option<&HashMap<String, String>>,
    ) -> Result<Response, CustomError> {
        self.with_timeout(async {
            for item in &items {
                http_client_helper::validate_form_item(item)?;
            }
            // 自定义 Content-Type 可以覆盖 multipart；签名处理器只处理最终媒体类型。
            let header_request = build(http_client_helper::apply_headers(
                self.client
                    .post(url)
                    .header(reqwest::header::CONTENT_TYPE, "multipart/form-data")
                    .body(""),
                headers,
            )?)?;
            let is_multipart = header_request
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.split(';').next())
                .is_some_and(|value| value.trim() == "multipart/form-data");
            let mut options = self.request_options.clone();
            let request_state = match (&self.handler, is_multipart) {
                (Some(handler), true) => {
                    handler.prepare_multipart(&mut items, &mut options).await?
                }
                _ => None,
            };
            let request = build(http_client_helper::build_post_form_data(
                &self.client,
                url,
                items,
                headers,
            )?)?;
            let response = self
                .send_inner(request, request_state.as_deref(), &mut options)
                .await?;
            http_client_helper::buffer_response(response).await
        })
        .await
    }

    async fn send_inner(
        &self,
        mut request: Request,
        multipart_state: Option<&HttpRequestState>,
        options: &mut RequestOptions,
    ) -> Result<Response, CustomError> {
        if let Some(handler) = &self.handler {
            handler
                .before_send(&mut request, multipart_state, options)
                .await?;
        }
        let mut response = match &self.unconfigured {
            Some(transport) => transport.send(&self.client, request).await?,
            None => self
                .client
                .execute(request)
                .await
                .map_err(|error| CustomError::with_source("远程请求失败", error))?,
        };
        if let Some(handler) = &self.handler {
            handler.after_response(&response, options).await?;
        }
        response.extensions_mut().insert(options.clone());
        Ok(response)
    }

    async fn with_timeout<T>(
        &self,
        work: impl Future<Output = Result<T, CustomError>>,
    ) -> Result<T, CustomError> {
        tokio::time::timeout(self.request_timeout, work)
            .await
            .map_err(|error| CustomError::with_source("远程请求超时", error))?
    }
}

fn build(request: reqwest::RequestBuilder) -> Result<Request, CustomError> {
    request
        .build()
        .map_err(|error| CustomError::with_source("构造远程请求失败", error))
}
