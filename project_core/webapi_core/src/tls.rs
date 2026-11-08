use std::{path::Path, sync::Arc};

use openssl::{pkcs12::Pkcs12, pkey::PKey, x509::X509};
use serde::Deserialize;
use tokio_rustls::{
    TlsAcceptor,
    rustls::{
        self,
        pki_types::{CertificateDer, PrivatePkcs8KeyDer},
    },
};

/// 宿主 HTTPS 监听与证书配置。
#[derive(Clone, Deserialize)]
pub struct TlsSettings {
    /// HTTPS 监听地址。
    pub listen_address: String,
    /// PEM/CRT 证书链或 PFX/PKCS#12 文件。
    pub certificate_path: String,
    /// PEM 私钥路径；未配置时从 certificate_path 中读取。
    #[serde(default)]
    pub key_path: Option<String>,
    /// PFX 或加密 PEM 私钥密码。
    #[serde(default)]
    pub password: Option<String>,
}

/// 证书文件仅在启动时读取；错误不包含密码或私钥内容。
pub(crate) fn load_acceptor(settings: &TlsSettings) -> Result<TlsAcceptor, String> {
    let certificate = std::fs::read(&settings.certificate_path)
        .map_err(|error| format!("读取 TLS 证书失败: {error}"))?;
    let extension = Path::new(&settings.certificate_path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let (certificates, key) = if extension.eq_ignore_ascii_case("pfx")
        || extension.eq_ignore_ascii_case("p12")
    {
        let identity = Pkcs12::from_der(&certificate)
            .and_then(|identity| identity.parse2(settings.password.as_deref().unwrap_or_default()))
            .map_err(|_| "无法解析 TLS PKCS#12 证书或密码不正确".to_string())?;
        let leaf = identity.cert.ok_or("TLS PKCS#12 缺少证书")?;
        let key = identity.pkey.ok_or("TLS PKCS#12 缺少私钥")?;
        let mut certificates = vec![leaf];
        if let Some(chain) = identity.ca {
            certificates.extend(chain);
        }
        (certificates, key)
    } else {
        let certificates = X509::stack_from_pem(&certificate)
            .map_err(|_| "无法解析 TLS PEM 证书链".to_string())?;
        let key_bytes = match &settings.key_path {
            Some(path) => {
                std::fs::read(path).map_err(|error| format!("读取 TLS 私钥失败: {error}"))?
            }
            None => certificate,
        };
        // 始终提供密码回调，避免缺少密码时 OpenSSL 尝试从终端交互读取。
        let key = PKey::private_key_from_pem_passphrase(
            &key_bytes,
            settings.password.as_deref().unwrap_or_default().as_bytes(),
        )
        .map_err(|_| "无法解析 TLS PEM 私钥或密码不正确".to_string())?;
        (certificates, key)
    };
    if certificates.is_empty() {
        return Err("TLS 证书链为空".to_string());
    }
    let chain = certificates
        .into_iter()
        .map(|certificate| {
            certificate
                .to_der()
                .map(CertificateDer::from)
                .map_err(|_| "无法读取 TLS 证书".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let private_key = key
        .private_key_to_pkcs8()
        .map_err(|_| "无法读取 TLS 私钥".to_string())?;
    let mut config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|error| format!("TLS 协议配置失败: {error}"))?
    .with_no_client_auth()
    .with_single_cert(chain, PrivatePkcs8KeyDer::from(private_key).into())
    .map_err(|_| "TLS 证书与私钥不匹配或算法不受支持".to_string())?;
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(TlsAcceptor::from(Arc::new(config)))
}
