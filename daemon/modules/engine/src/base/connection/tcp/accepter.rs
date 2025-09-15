use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    str::FromStr,
};

use async_trait::async_trait;
use tokio::net::TcpListener;

use omnius_core_base::net::Reachable;
use omnius_core_omnikit::model::OmniAddr;

use crate::{
    base::{Shutdown, connection::FramedStream},
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
                let upnp_port_mapping = UpnpPortMapping::new(socket_addr.port()).await;
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

        Err(Error::builder().kind(ErrorKind::InvalidFormat).message("invalid address").build())
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
            .ok_or_else(|| Error::builder().kind(ErrorKind::NotFound).message("not found external ip").build())?;
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
