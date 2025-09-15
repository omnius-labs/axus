use async_trait::async_trait;
use fast_socks5::client::Socks5Stream;
use omnius_core_omnikit::model::OmniAddr;
use tokio::net::TcpStream;

use crate::{base::connection::FramedStream, prelude::*};

pub struct TcpProxyOption {
    pub typ: TcpProxyType,
    pub addr: Option<String>,
}

pub enum TcpProxyType {
    None,
    #[allow(unused)]
    Socks5,
}

#[async_trait]
pub trait ConnectionTcpConnector {
    async fn connect(&self, addr: &OmniAddr) -> Result<FramedStream>;
}

pub struct ConnectionTcpConnectorImpl {
    proxy_option: TcpProxyOption,
}

impl ConnectionTcpConnectorImpl {
    pub async fn new(proxy_option: TcpProxyOption) -> Result<Self> {
        Ok(Self { proxy_option })
    }
}

#[async_trait]
impl ConnectionTcpConnector for ConnectionTcpConnectorImpl {
    async fn connect(&self, addr: &OmniAddr) -> Result<FramedStream> {
        match self.proxy_option.typ {
            TcpProxyType::None => {
                let socket_addr = addr.parse_tcp_ip()?;
                let stream = TcpStream::connect(socket_addr).await?;
                let (reader, writer) = tokio::io::split(stream);
                let stream = FramedStream::new(reader, writer);
                Ok(stream)
            }
            TcpProxyType::Socks5 => {
                let (host, port) = addr.parse_tcp_host()?;
                if let Some(proxy_addr) = &self.proxy_option.addr {
                    let config = fast_socks5::client::Config::default();
                    let stream = Socks5Stream::connect(proxy_addr.as_str(), host, port, config).await?;
                    let stream = stream.get_socket();
                    let (reader, writer) = tokio::io::split(stream);
                    let stream = FramedStream::new(reader, writer);
                    return Ok(stream);
                }
                return Err(Error::builder()
                    .kind(ErrorKind::NetworkError)
                    .message(format!("failed to connect by socks5: {addr:?}"))
                    .build());
            }
        }
    }
}
