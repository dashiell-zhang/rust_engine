mod sign;

use std::sync::Arc;

use application_interface::http_client::{HttpClientFactory, NamedHttpClientRegistration};
use common::error_helper::CustomError;
use distributed_cache::DistributedCache;
use openssl::pkcs12::Pkcs12;
use serde::Deserialize;
use webapi_core::settings::Settings;

use sign::{HttpSignHandler, HttpSignSettings};

#[derive(Clone, Deserialize)]
#[serde(default)]
struct CarryCertSettings {
    certificate_path: String,
    password: String,
}

impl Default for CarryCertSettings {
    fn default() -> Self {
        Self {
            certificate_path: "ssl/xxxx.p12".into(),
            password: String::new(),
        }
    }
}

/// 命名客户端只由客户端宿主注册；证书在实际获取客户端时读取。
pub fn configure(
    settings: &Settings,
    cache: Arc<dyn DistributedCache>,
    factory: &mut HttpClientFactory,
) -> Result<(), String> {
    let sign_settings: HttpSignSettings = read_settings(settings, "http_sign")?;
    let cert_settings: CarryCertSettings = read_settings(settings, "carry_cert")?;
    factory.register_named(
        "SkipSsl",
        NamedHttpClientRegistration::new(|builder| Ok(builder.danger_accept_invalid_certs(true))),
    );
    factory.register_named(
        "HttpSign",
        NamedHttpClientRegistration::new(Ok).with_handler(move |client| {
            Arc::new(HttpSignHandler::new(
                cache.clone(),
                client,
                sign_settings.clone(),
            ))
        }),
    );
    factory.register_named(
        "CarryCert",
        NamedHttpClientRegistration::new(move |builder| {
            Ok(builder.identity(load_identity(&cert_settings)?))
        }),
    );
    Ok(())
}

fn read_settings<T: for<'de> Deserialize<'de> + Default>(
    settings: &Settings,
    name: &str,
) -> Result<T, String> {
    match settings.http_clients.get(name) {
        Some(value) => serde_json::from_value(value.clone())
            .map_err(|_| format!("http_clients.{name} 配置格式无效")),
        None => Ok(T::default()),
    }
}

fn load_identity(settings: &CarryCertSettings) -> Result<reqwest::Identity, CustomError> {
    let bytes = std::fs::read(&settings.certificate_path)
        .map_err(|error| CustomError::with_source("读取 CarryCert 证书失败", error))?;
    let identity = Pkcs12::from_der(&bytes)
        .and_then(|value| value.parse2(&settings.password))
        .map_err(|_| CustomError::new("CarryCert PKCS#12 证书或密码无效"))?;
    let certificate = identity
        .cert
        .ok_or_else(|| CustomError::new("CarryCert 证书缺少公钥证书"))?;
    let key = identity
        .pkey
        .ok_or_else(|| CustomError::new("CarryCert 证书缺少私钥"))?;
    let public = certificate
        .public_key()
        .map_err(|_| CustomError::new("CarryCert 无法读取证书公钥"))?;
    if !public.public_eq(&key) {
        return Err(CustomError::new("CarryCert 证书与私钥不匹配"));
    }
    let mut pem = key
        .private_key_to_pem_pkcs8()
        .map_err(|_| CustomError::new("CarryCert 无法读取私钥"))?;
    pem.extend(
        certificate
            .to_pem()
            .map_err(|_| CustomError::new("CarryCert 无法读取证书"))?,
    );
    if let Some(chain) = identity.ca {
        for certificate in chain {
            pem.extend(
                certificate
                    .to_pem()
                    .map_err(|_| CustomError::new("CarryCert 无法读取证书链"))?,
            );
        }
    }
    reqwest::Identity::from_pem(&pem).map_err(|_| CustomError::new("CarryCert 无法创建 TLS 身份"))
}
