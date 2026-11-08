use std::{future::pending, io, net::SocketAddr, time::Duration};

use axum::{
    extract::connect_info::Connected,
    serve::{IncomingStream, Listener},
};
use futures_util::{StreamExt, future::BoxFuture, stream::FuturesUnordered};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::{TcpListener, TcpStream},
};
use tokio_rustls::TlsAcceptor;

use crate::{
    startup::{HostSettings, WebApiStartup},
    tls::load_acceptor,
};

/// 与每个连接绑定的传输信息，不从请求行判断是否使用 TLS。
#[derive(Clone, Copy, Debug)]
pub(crate) struct ConnectionInfo {
    pub(crate) address: SocketAddr,
    pub(crate) tls: bool,
}

pub(crate) trait HttpIo: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> HttpIo for T {}
type Accepted = (Box<dyn HttpIo>, ConnectionInfo);

/// 一个 Axum 服务同时处理 HTTP/HTTPS，共享停机与连接排空期限。
pub(crate) struct HostListener {
    http: Option<TcpListener>,
    https: Option<TcpListener>,
    tls_acceptor: Option<TlsAcceptor>,
    handshakes: FuturesUnordered<BoxFuture<'static, Option<Accepted>>>,
}

impl HostListener {
    pub(crate) async fn bind(settings: &HostSettings) -> Result<Self, String> {
        let tls_acceptor = settings.tls.as_ref().map(load_acceptor).transpose()?;
        let http = if settings.listen_address.trim().is_empty() {
            None
        } else {
            Some(WebApiStartup::bind_listener(&settings.listen_address).await?)
        };
        let https = match &settings.tls {
            Some(settings) => Some(WebApiStartup::bind_listener(&settings.listen_address).await?),
            None => None,
        };
        if http.is_none() && https.is_none() {
            return Err("至少需要配置一个 HTTP 或 HTTPS 监听地址".to_string());
        }
        Ok(Self {
            http,
            https,
            tls_acceptor,
            handshakes: FuturesUnordered::new(),
        })
    }

    pub(crate) fn addresses(&self) -> io::Result<Vec<ConnectionInfo>> {
        let mut addresses = Vec::new();
        for (listener, tls) in [(&self.http, false), (&self.https, true)] {
            if let Some(listener) = listener {
                addresses.push(ConnectionInfo {
                    address: listener.local_addr()?,
                    tls,
                });
            }
        }
        Ok(addresses)
    }
}

impl Listener for HostListener {
    type Io = Box<dyn HttpIo>;
    type Addr = ConnectionInfo;

    async fn accept(&mut self) -> Accepted {
        loop {
            tokio::select! {
                (stream, address) = accept_tcp(&mut self.http) => {
                    return (Box::new(stream), ConnectionInfo { address, tls: false });
                }
                (stream, address) = accept_tcp(&mut self.https) => {
                    if let Some(acceptor) = self.tls_acceptor.clone() {
                        self.handshakes.push(Box::pin(async move {
                            match tokio::time::timeout(Duration::from_secs(10), acceptor.accept(stream)).await {
                                Ok(Ok(stream)) => Some((Box::new(stream) as Box<dyn HttpIo>, ConnectionInfo { address, tls: true })),
                                _ => {
                                    tracing::debug!(target: "app.tls", %address, "TLS 握手失败或超时");
                                    None
                                }
                            }
                        }));
                    }
                }
                result = self.handshakes.next(), if !self.handshakes.is_empty() => {
                    if let Some(Some(connection)) = result { return connection; }
                }
            }
        }
    }

    fn local_addr(&self) -> io::Result<ConnectionInfo> {
        self.addresses()?
            .into_iter()
            .next()
            .ok_or_else(|| io::Error::new(io::ErrorKind::AddrNotAvailable, "没有监听地址"))
    }
}

async fn accept_tcp(listener: &mut Option<TcpListener>) -> (TcpStream, SocketAddr) {
    match listener {
        Some(listener) => Listener::accept(listener).await,
        None => pending().await,
    }
}

impl Connected<IncomingStream<'_, HostListener>> for ConnectionInfo {
    fn connect_info(stream: IncomingStream<'_, HostListener>) -> Self {
        *stream.remote_addr()
    }
}
