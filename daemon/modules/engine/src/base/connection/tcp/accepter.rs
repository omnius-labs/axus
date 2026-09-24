use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    str::FromStr,
};

use async_trait::async_trait;
use tokio::net::TcpListener;

use omnius_core_base::net::Reachable;
use omnius_core_omnikit::model::omni_addr::OmniAddr;

use crate::{
    base::{connection::FramedStream, runtime::Shutdown},
    prelude::*,
};

use super::UpnpClient;

#[async_trait]
pub trait ConnectionTcpAccepter: Shutdown {
    async fn accept(&self) -> Result<(FramedStream, SocketAddr)>;
    #[allow(unused)]
    async fn get_global_ip_addresses(&self) -> Result<Vec<IpAddr>>;
}

pub struct ConnectionTcpAccepterImpl {
    listener: TcpListener,
    #[allow(unused)]
    upnp_port_mapping: Option<UpnpPortMapping>,
}

impl ConnectionTcpAccepterImpl {
    pub async fn new(addr: &OmniAddr, use_upnp: bool) -> Result<Self> {
        let socket_addr = addr.parse_tcp_ip()?;
        if socket_addr.is_ipv4() {
            let listener = TcpListener::bind(socket_addr).await?;

            if use_upnp && socket_addr.ip().is_unspecified() {
                // port に 0 を指定した場合も、OS が割り当てた port を開放する
                let upnp_port_mapping = UpnpPortMapping::new(listener.local_addr()?.port()).await;
                if let Ok(upnp_port_mapping) = upnp_port_mapping {
                    return Ok(Self {
                        listener,
                        upnp_port_mapping: Some(upnp_port_mapping),
                    });
                }
            }

            return Ok(Self {
                listener,
                upnp_port_mapping: None,
            });
        } else if socket_addr.is_ipv6() {
            let listener = TcpListener::bind(socket_addr).await?;
            return Ok(Self {
                listener,
                upnp_port_mapping: None,
            });
        }

        Err(Error::new(ErrorKind::InvalidFormat).with_message("invalid address"))
    }

    /// 他の node に広告する自 node のアドレスを返す。
    /// 特定のアドレスで待ち受けていればそれを返し、不特定のアドレスで待ち受けていれば、到達できる IP に待ち受けポートを付けて返す。
    pub async fn get_advertised_addrs(&self) -> Result<Vec<OmniAddr>> {
        let local_addr = self.listener.local_addr()?;
        if !local_addr.ip().is_unspecified() {
            return Ok(vec![OmniAddr::create_tcp(local_addr.ip(), local_addr.port())]);
        }

        let ips = self.get_global_ip_addresses().await?;
        Ok(ips.into_iter().map(|ip| OmniAddr::create_tcp(ip, local_addr.port())).collect())
    }
}

#[async_trait]
impl Shutdown for ConnectionTcpAccepterImpl {
    async fn shutdown(&self) {
        if let Some(upnp_port_mapping) = &self.upnp_port_mapping {
            upnp_port_mapping.shutdown().await;
        }
    }
}

#[async_trait]
impl ConnectionTcpAccepter for ConnectionTcpAccepterImpl {
    async fn accept(&self) -> Result<(FramedStream, SocketAddr)> {
        let (stream, addr) = self.listener.accept().await?;
        let (reader, writer) = tokio::io::split(stream);
        let stream = FramedStream::new(reader, writer);
        Ok((stream, addr))
    }

    async fn get_global_ip_addresses(&self) -> Result<Vec<IpAddr>> {
        let mut res: Vec<IpAddr> = Vec::new();
        if let Ok(IpAddr::V4(ip4)) = local_ip_address::local_ip()
            && ip4.is_reachable()
        {
            res.push(IpAddr::V4(ip4));
        }
        if let Ok(IpAddr::V6(ip6)) = local_ip_address::local_ipv6()
            && ip6.is_reachable()
        {
            res.push(IpAddr::V6(ip6));
        }
        if let Some(upnp) = &self.upnp_port_mapping
            && upnp.external_ip.is_reachable()
        {
            res.push(IpAddr::V4(upnp.external_ip));
        }

        Ok(res)
    }
}

#[allow(unused)]
struct UpnpPortMapping {
    port: u16,
    external_ip: Ipv4Addr,
}

impl UpnpPortMapping {
    pub async fn new(port: u16) -> Result<Self> {
        UpnpClient::delete_port_mapping("TCP", port).await?;
        UpnpClient::add_port_mapping("TCP", port, port, "axus").await?;
        let res = UpnpClient::get_external_ip_address().await?;
        let external_ip = res
            .get("NewExternalIPAddress")
            .ok_or_else(|| Error::new(ErrorKind::NotFound).with_message("not found external ip"))?;
        let external_ip = Ipv4Addr::from_str(external_ip.as_str())?;
        Ok(Self { port, external_ip })
    }
}

#[async_trait]
impl Shutdown for UpnpPortMapping {
    async fn shutdown(&self) {
        let _ = UpnpClient::delete_port_mapping("TCP", self.port).await;
    }
}

#[cfg(test)]
mod tests {
    use testresult::TestResult;

    use omnius_core_omnikit::model::omni_addr::OmniAddr;

    use super::ConnectionTcpAccepterImpl;

    #[tokio::test]
    async fn specific_listen_address_is_advertised_as_is() -> TestResult {
        let accepter = ConnectionTcpAccepterImpl::new(&OmniAddr::create_tcp("127.0.0.1".parse()?, 0), false).await?;
        let port = accepter.listener.local_addr()?.port();

        assert_eq!(accepter.get_advertised_addrs().await?, vec![OmniAddr::create_tcp("127.0.0.1".parse()?, port)]);

        Ok(())
    }

    #[tokio::test]
    async fn unspecified_listen_address_advertises_reachable_ips_with_the_listen_port() -> TestResult {
        let accepter = ConnectionTcpAccepterImpl::new(&OmniAddr::create_tcp("0.0.0.0".parse()?, 0), false).await?;
        let port = accepter.listener.local_addr()?.port();

        // 到達できる IP は実行環境に依存するため、件数ではなく各アドレスの形だけを確かめる
        for addr in accepter.get_advertised_addrs().await? {
            let socket_addr = addr.parse_tcp_ip()?;
            assert!(!socket_addr.ip().is_unspecified());
            assert_eq!(socket_addr.port(), port);
        }

        Ok(())
    }
}
