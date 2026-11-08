use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use application_runtime::http_client::{HttpRequestState, NamedHttpHandler};
use async_trait::async_trait;
use common::{
    error_helper::CustomError,
    http_client_helper::{self, PostFormDataItem, RequestOptions},
};
use distributed_cache::{DistributedCache, DistributedCacheExt};
use reqwest::{
    Client, Request, Response,
    header::{AUTHORIZATION, CONTENT_TYPE, HeaderValue},
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncSeekExt};

#[derive(Clone, Deserialize)]
#[serde(default)]
pub(super) struct HttpSignSettings {
    pub token_url: String,
    pub name: String,
    pub password: String,
    pub cache_key: String,
}

impl Default for HttpSignSettings {
    fn default() -> Self {
        Self {
            token_url: "https://localhost:9833/api/Authorize/GetToken".into(),
            name: "admin".into(),
            password: String::new(),
            cache_key: "token".into(),
        }
    }
}

pub(super) struct HttpSignHandler {
    cache: Arc<dyn DistributedCache>,
    login_client: Client,
    settings: HttpSignSettings,
}

struct SigningState {
    authorization: String,
    time: String,
    body: String,
}

impl HttpSignHandler {
    pub(super) fn new(
        cache: Arc<dyn DistributedCache>,
        login_client: Client,
        settings: HttpSignSettings,
    ) -> Self {
        Self {
            cache,
            login_client,
            settings,
        }
    }

    async fn prepare(&self) -> Result<SigningState, CustomError> {
        let mut token = self
            .cache
            .get_string(&self.settings.cache_key)
            .await
            .map_err(|error| CustomError::with_source("读取出站令牌失败", error))?
            .unwrap_or_default();
        if token.is_empty() {
            if self.settings.name.is_empty() || self.settings.password.is_empty() {
                return Err(CustomError::new("HttpSign 未配置登录账号或密码"));
            }
            let data =
                serde_json::json!({"name": self.settings.name, "passWord": self.settings.password})
                    .to_string();
            let response = http_client_helper::post_string(
                &self.login_client,
                &self.settings.token_url,
                data,
                "json",
                None,
            )
            .await?;
            if response.status().is_success() {
                token = response
                    .text()
                    .await
                    .map_err(|error| CustomError::with_source("读取出站登录响应失败", error))?;
                self.cache
                    .set_string(&self.settings.cache_key, &token)
                    .await
                    .map_err(|error| CustomError::with_source("保存出站令牌失败", error))?;
            }
        }
        if token.trim().is_empty() {
            return Err(CustomError::new("token获取失败"));
        }
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| CustomError::with_source("获取签名时间失败", error))?
            .as_millis()
            .to_string();
        Ok(SigningState {
            authorization: token,
            time,
            body: String::new(),
        })
    }
}

#[async_trait]
impl NamedHttpHandler for HttpSignHandler {
    async fn before_send(
        &self,
        request: &mut Request,
        multipart_state: Option<&HttpRequestState>,
        _options: &mut RequestOptions,
    ) -> Result<(), CustomError> {
        let media_type = request
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(';').next())
            .unwrap_or_default()
            .trim()
            .to_owned();
        let multipart = if request.body().is_some() && media_type == "multipart/form-data" {
            Some(
                multipart_state
                    .and_then(|state| state.downcast_ref::<SigningState>())
                    .ok_or_else(|| {
                        CustomError::new("HttpSign multipart 请求必须通过 post_form_data 构建")
                    })?,
            )
        } else {
            None
        };
        let mut ordinary;
        let state = match multipart {
            Some(state) => state,
            None => {
                ordinary = self.prepare().await?;
                if request.body().is_some() && media_type == "application/json" {
                    ordinary.body = read_request_text(request).await?;
                }
                &ordinary
            }
        };
        let mut digest = Sha256::new();
        digest.update(state.authorization.rsplit('.').next().unwrap_or_default());
        digest.update(&state.time);
        digest.update(request.url().path());
        if let Some(query) = request.url().query() {
            digest.update("?");
            digest.update(query);
        }
        digest.update(&state.body);
        let signature = digest_hex(&digest.finalize());
        let mut authorization =
            HeaderValue::from_str(&format!("Bearer {}", state.authorization))
                .map_err(|_| CustomError::new("出站令牌不能用作 Authorization 请求头"))?;
        authorization.set_sensitive(true);
        request.headers_mut().insert(AUTHORIZATION, authorization);
        request.headers_mut().append(
            "token",
            HeaderValue::from_str(&signature).map_err(|_| CustomError::new("签名摘要无效"))?,
        );
        request.headers_mut().append(
            "time",
            HeaderValue::from_str(&state.time).map_err(|_| CustomError::new("签名时间无效"))?,
        );
        Ok(())
    }

    async fn after_response(
        &self,
        response: &Response,
        _options: &mut RequestOptions,
    ) -> Result<(), CustomError> {
        if response.status() == reqwest::StatusCode::OK
            && let Some(token) = response.headers().get("newtoken")
        {
            let token = token
                .to_str()
                .map_err(|_| CustomError::new("续签令牌请求头无效"))?;
            if !token.is_empty() {
                self.cache
                    .set_string(&self.settings.cache_key, token)
                    .await
                    .map_err(|error| CustomError::with_source("保存续签令牌失败", error))?;
            }
        }
        Ok(())
    }

    async fn prepare_multipart(
        &self,
        items: &mut [PostFormDataItem],
        _options: &mut RequestOptions,
    ) -> Result<Option<Box<HttpRequestState>>, CustomError> {
        let mut state = self.prepare().await?;
        let mut fields = Vec::new();
        let mut files = Vec::new();
        for item in items {
            match item {
                PostFormDataItem::Text { key, value } => fields.push((key.clone(), value.clone())),
                PostFormDataItem::File { key, bytes, .. } => {
                    files.push((key.clone(), digest_hex(&Sha256::digest(bytes))))
                }
                PostFormDataItem::FileStream { key, body, .. } => {
                    let bytes = body.as_bytes().ok_or_else(|| CustomError::new("multipart/form-data 文件流不支持 Seek，HttpSignHandler 无法在读取签名后复位流位置，请改为使用可 Seek 的流（如 FileHandle）"))?;
                    files.push((key.clone(), digest_hex(&Sha256::digest(bytes))));
                }
                PostFormDataItem::FileHandle {
                    key, file, length, ..
                } => {
                    let mut hash = Sha256::new();
                    let mut buffer = [0u8; 8192];
                    loop {
                        let count = file.read(&mut buffer).await.map_err(|error| {
                            CustomError::with_source("读取待签名文件失败", error)
                        })?;
                        if count == 0 {
                            break;
                        }
                        hash.update(&buffer[..count]);
                    }
                    file.rewind()
                        .await
                        .map_err(|error| CustomError::with_source("复位待发送文件失败", error))?;
                    *length = Some(
                        file.metadata()
                            .await
                            .map_err(|error| CustomError::with_source("读取文件长度失败", error))?
                            .len(),
                    );
                    files.push((key.clone(), digest_hex(&hash.finalize())));
                }
            }
        }
        fields.sort_by(|a, b| a.0.cmp(&b.0));
        files.sort_by(|a, b| a.0.cmp(&b.0));
        for (key, value) in fields.into_iter().chain(files) {
            state.body.push_str(&key);
            state.body.push_str(&value);
        }
        Ok(Some(Box::new(state)))
    }
}

async fn read_request_text(request: &mut Request) -> Result<String, CustomError> {
    let body = request
        .body_mut()
        .take()
        .ok_or_else(|| CustomError::new("请求正文不存在"))?;
    let response: Response = http::Response::new(body).into();
    let bytes = http_client_helper::buffer_response(response)
        .await?
        .bytes()
        .await
        .map_err(|error| CustomError::with_source("读取签名正文失败", error))?;
    *request.body_mut() = Some(bytes.clone().into());
    let mut response = http::Response::new(bytes);
    if let Some(content_type) = request.headers().get(CONTENT_TYPE) {
        response
            .headers_mut()
            .insert(CONTENT_TYPE, content_type.clone());
    }
    Response::from(response)
        .text()
        .await
        .map_err(|error| CustomError::with_source("解码签名正文失败", error))
}

fn digest_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02X}")).collect()
}
