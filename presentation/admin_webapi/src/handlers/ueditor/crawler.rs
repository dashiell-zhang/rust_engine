//! UEditor 抓图协议；网络接入归属宿主，记录保存复用文件应用契约。
use std::{
    net::{IpAddr, SocketAddr},
    path::Path,
    sync::Arc,
    time::Duration,
};

use application_model::file::models::UploadFileDto;
use application_runtime::{context::CurrentUser, http_client::HttpClientFactory};
use common::{io_helper::TemporaryFileGuard, string_helper::is_public_ip_address};
use reqwest::{Response, Url, header, redirect::Policy};
use tokio::{fs, io::AsyncWriteExt};
use webapi_support::WebApiApplicationExt;

use super::{
    ARTICLE_BUSINESS, IMAGE_ALLOW_FILES, IMAGE_MAX_SIZE, UeditorCrawlerItem, normalize_ueditor_url,
};
use crate::state::AppState;

/// 单个来源失败只影响本项，原始来源与空字段始终保留。
pub(super) async fn fetch(
    state: &Arc<AppState>,
    user: &CurrentUser,
    source: &str,
    key: i64,
) -> UeditorCrawlerItem {
    let mut item = UeditorCrawlerItem {
        state: String::new(),
        source: source.into(),
        url: None,
        file_id: None,
    };
    match fetch_and_save(state, user, source, key).await {
        Ok((file_id, url)) => {
            item.state = "SUCCESS".into();
            item.file_id = Some(file_id);
            item.url = Some(url);
        }
        Err(error) => item.state = error,
    }
    item
}

async fn fetch_and_save(
    state: &AppState,
    user: &CurrentUser,
    source: &str,
    key: i64,
) -> Result<(String, String), String> {
    let url = Url::parse(source.trim()).map_err(|_| "INVALID_Url".to_string())?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err("INVALID_Url".into());
    }
    // 源处理器先验证整个 DNS 结果，再检查初始 URL 的文件名。
    let addresses = resolve_addresses(&url).await.map_err(crawl_error)?;
    if !all_public(&addresses) {
        return Err("INVALID_Url".into());
    }
    let local_path = urlencoding::decode(url.path()).map_err(crawl_error)?;
    let file_name = local_path.rsplit('/').next().unwrap_or_default();
    let extension = common::io_helper::file_extension(file_name).to_ascii_lowercase();
    if file_name.trim().is_empty() || !IMAGE_ALLOW_FILES.contains(&extension.as_str()) {
        return Err("不允许的文件格式".into());
    }
    // HttpClient 默认的 100 秒覆盖取得最终响应头；正文流没有独立超时。
    let mut response = tokio::time::timeout(
        Duration::from_secs(100),
        get_response(&state.http_client_factory, url.clone()),
    )
    .await
    .map_err(|_| crawl_error("请求在 100 秒内未返回响应头"))??;
    if response.status().as_u16() != 200 {
        return Err(format!(
            "Url returns {}",
            status_name(response.status().as_u16())
        ));
    }
    let image = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<mime_guess::mime::Mime>().ok())
        .is_some_and(|value| value.type_() == mime_guess::mime::IMAGE);
    if !image {
        return Err("Url is not an image".into());
    }
    if response
        .content_length()
        .is_some_and(|size| size > IMAGE_MAX_SIZE as u64)
    {
        return Err("文件大小超出服务器限制".into());
    }
    let directory = Path::new(&state.settings.file.root_path).join("temps");
    fs::create_dir_all(&directory).await.map_err(crawl_error)?;
    let path = directory.join(format!("{}{extension}", uuid::Uuid::new_v4()));
    // 创建文件和守卫在同一个阻塞任务中完成；等待期间取消也会丢弃并清理任务结果。
    let (guard, output) = tokio::task::spawn_blocking(move || {
        let output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        Ok::<_, std::io::Error>((TemporaryFileGuard::new(path), output))
    })
    .await
    .map_err(crawl_error)?
    .map_err(crawl_error)?;
    let mut output = fs::File::from_std(output);
    let mut length = 0_u64;
    while let Some(chunk) = response.chunk().await.map_err(crawl_error)? {
        length += chunk.len() as u64;
        if length > IMAGE_MAX_SIZE as u64 {
            return Err("文件大小超出服务器限制".into());
        }
        output.write_all(&chunk).await.map_err(crawl_error)?;
    }
    output.flush().await.map_err(crawl_error)?;
    drop(output);
    let service = state.services().file();
    let file_id = service
        .upload_file(
            UploadFileDto {
                business: ARTICLE_BUSINESS.into(),
                key: Some(key),
                sign: "content-remote-image".into(),
                is_public_read: true,
                file_name: file_name.into(),
                temp_file_path: guard.path().to_string_lossy().into_owned(),
            },
            user,
        )
        .await
        .map_err(|error| crawl_error(error.message()))?;
    let id = file_id.parse::<i64>().map_err(crawl_error)?;
    let url = service
        .get_file_url(id, false)
        .await
        .map_err(|error| crawl_error(error.message()))?;
    Ok((
        file_id,
        normalize_ueditor_url(url, &state.settings.file.public_base_url),
    ))
}

/// 每次连接重新解析并固定全部已验证的地址，禁止代理和未经检查的重定向。
async fn get_response(factory: &HttpClientFactory, mut url: Url) -> Result<Response, String> {
    let cookies = Arc::new(reqwest::cookie::Jar::default());
    for redirects in 0..=5 {
        let addresses = resolve_addresses(&url).await.map_err(crawl_error)?;
        if !all_public(&addresses) {
            return Err(crawl_error("远程主机解析到了非公网地址"));
        }
        let client = factory
            .build_streaming_client_with(|builder| {
                builder
                    .no_proxy()
                    .cookie_provider(cookies.clone())
                    .redirect(Policy::none())
                    .no_gzip()
                    .no_brotli()
                    .no_deflate()
                    .no_zstd()
                    .resolve_to_addrs(url.host_str().unwrap_or_default(), &addresses)
            })
            .map_err(crawl_error)?;
        let response = client.get(url.clone()).send().await.map_err(crawl_error)?;
        // .NET 超过最大重定向次数或缺少 Location 时返回最后一份响应。
        if redirects == 5
            || !matches!(
                response.status().as_u16(),
                300 | 301 | 302 | 303 | 307 | 308
            )
        {
            return Ok(response);
        }
        let Some(location) = response
            .headers()
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok())
        else {
            return Ok(response);
        };
        let Ok(mut next) = url.join(location) else {
            return Ok(response);
        };
        if url.scheme() == "https" && next.scheme() != "https" {
            return Ok(response);
        }
        if !matches!(next.scheme(), "http" | "https") {
            return Err(crawl_error("重定向地址不支持 HTTP 或 HTTPS"));
        }
        if next.fragment().is_none_or(str::is_empty) {
            next.set_fragment(url.fragment());
        }
        url = next;
    }
    unreachable!("有限重定向循环在最后一次返回响应")
}

/// 数值 IP 不经过 DNS；域名的全部结果都必须为公网地址。
async fn resolve_addresses(url: &Url) -> Result<Vec<SocketAddr>, String> {
    let host = url
        .host_str()
        .ok_or_else(|| "远程地址缺少主机名".to_string())?;
    let host = host.trim_start_matches('[').trim_end_matches(']');
    let port = url
        .port_or_known_default()
        .ok_or_else(|| "远程地址端口无效".to_string())?;
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Ok(vec![SocketAddr::new(ip, port)]);
    }
    tokio::net::lookup_host((host, port))
        .await
        .map(|values| values.collect())
        .map_err(|error| error.to_string())
}

fn all_public(addresses: &[SocketAddr]) -> bool {
    !addresses.is_empty()
        && addresses
            .iter()
            .all(|address| is_public_ip_address(address.ip()))
}

fn crawl_error(error: impl std::fmt::Display) -> String {
    format!("抓取错误：{error}")
}

/// 源协议输出 HttpStatusCode 枚举名称，未知状态码保留数字。
fn status_name(status: u16) -> String {
    let name = match status {
        100 => "Continue",
        101 => "SwitchingProtocols",
        102 => "Processing",
        103 => "EarlyHints",
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        203 => "NonAuthoritativeInformation",
        204 => "NoContent",
        205 => "ResetContent",
        206 => "PartialContent",
        207 => "MultiStatus",
        208 => "AlreadyReported",
        226 => "IMUsed",
        300 => "MultipleChoices",
        301 => "MovedPermanently",
        302 => "Found",
        303 => "SeeOther",
        304 => "NotModified",
        305 => "UseProxy",
        306 => "Unused",
        307 => "RedirectKeepVerb",
        308 => "PermanentRedirect",
        400 => "BadRequest",
        401 => "Unauthorized",
        402 => "PaymentRequired",
        403 => "Forbidden",
        404 => "NotFound",
        405 => "MethodNotAllowed",
        406 => "NotAcceptable",
        407 => "ProxyAuthenticationRequired",
        408 => "RequestTimeout",
        409 => "Conflict",
        410 => "Gone",
        411 => "LengthRequired",
        412 => "PreconditionFailed",
        413 => "RequestEntityTooLarge",
        414 => "RequestUriTooLong",
        415 => "UnsupportedMediaType",
        416 => "RequestedRangeNotSatisfiable",
        417 => "ExpectationFailed",
        421 => "MisdirectedRequest",
        422 => "UnprocessableEntity",
        423 => "Locked",
        424 => "FailedDependency",
        426 => "UpgradeRequired",
        428 => "PreconditionRequired",
        429 => "TooManyRequests",
        431 => "RequestHeaderFieldsTooLarge",
        451 => "UnavailableForLegalReasons",
        500 => "InternalServerError",
        501 => "NotImplemented",
        502 => "BadGateway",
        503 => "ServiceUnavailable",
        504 => "GatewayTimeout",
        505 => "HttpVersionNotSupported",
        506 => "VariantAlsoNegotiates",
        507 => "InsufficientStorage",
        508 => "LoopDetected",
        510 => "NotExtended",
        511 => "NetworkAuthenticationRequired",
        _ => return status.to_string(),
    };
    name.into()
}
