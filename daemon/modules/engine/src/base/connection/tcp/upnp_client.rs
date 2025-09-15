use std::{collections::HashMap, time::Duration};

use once_cell::sync::Lazy;
use rupnp::ssdp::{SearchTarget, URN};
use tokio_stream::StreamExt;

use crate::prelude::*;

static URNS: Lazy<Vec<URN>> = Lazy::new(|| {
    vec![
        URN::service("schemas-upnp-org", "WANIPConnection", 1),
        URN::service("schemas-upnp-org", "WANPPPConnection", 1),
    ]
});

pub struct UpnpClient;

impl UpnpClient {
    pub async fn add_port_mapping(protocol: &str, external_port: u16, internal_port: u16, description: &str) -> Result<()> {
        let internal_ip = local_ip_address::local_ip()?.to_string();
        for urn in URNS.iter() {
            let name = "AddPortMapping";
            let args = format!(
                r#"\
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
<s:Body>
 <u:{name} xmlns:u="{urn}">
  <NewRemoteHost></NewRemoteHost>
  <NewExternalPort>{external_port}</NewExternalPort>
  <NewProtocol>{protocol}</NewProtocol>
  <NewInternalPort>{internal_port}</NewInternalPort>
  <NewInternalClient>{internal_ip}</NewInternalClient>
  <NewEnabled>1</NewEnabled>
  <NewPortMappingDescription>{description}</NewPortMappingDescription>
  <NewLeaseDuration>0</NewLeaseDuration>
 </u:{name}>
</s:Body>
/s:Envelope>
"#
            );

            let res = Self::action(urn, name, args.as_str()).await;
            if res.is_err() {
                continue;
            }
            return Ok(());
        }

        Err(Error::builder().kind(ErrorKind::UpnpError).message("failed to add port mapping").build())
    }

    pub async fn delete_port_mapping(protocol: &str, external_port: u16) -> Result<()> {
        for urn in URNS.iter() {
            let name = "DeletePortMapping";
            let args = format!(
                r#"\
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
<s:Body>
 <u:{name} xmlns:u="{urn}">
  <NewRemoteHost></NewRemoteHost>
  <NewExternalPort>{external_port}</NewExternalPort>
  <NewProtocol>{protocol}</NewProtocol>
 </u:{name}>
</s:Body>
/s:Envelope>
"#
            );

            let res = Self::action(urn, name, args.as_str()).await;
            if res.is_err() {
                continue;
            }
            return Ok(());
        }

        Err(Error::builder().kind(ErrorKind::UpnpError).message("failed to delete port mapping").build())
    }

    #[allow(unused)]
    pub async fn get_generic_port_mapping_entry(index: i32) -> Result<HashMap<String, String>> {
        for urn in URNS.iter() {
            let name = "GetGenericPortMappingEntry";
            let args = format!(
                r#"\
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
<s:Body>
  <u:{name} xmlns:u="{urn}">
   <NewPortMappingIndex>{index}</NewPortMappingIndex>
  </u:{name}>
</s:Body>
/s:Envelope>
"#
            );

            let res = Self::action(urn, name, args.as_str()).await;
            if res.is_err() {
                continue;
            }
            return Ok(res.unwrap());
        }

        Err(Error::builder().kind(ErrorKind::UpnpError).message("failed to get generic port mapping").build())
    }

    pub async fn get_external_ip_address() -> Result<HashMap<String, String>> {
        for urn in URNS.iter() {
            let name = "GetExternalIPAddress";
            let args = format!(
                r#"\
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
<s:Body>
  <u:{name} xmlns:u="{urn}">
  </u:{name}>
</s:Body>
/s:Envelope>
"#
            );

            let res = Self::action(urn, name, args.as_str()).await;
            if res.is_err() {
                continue;
            }
            return Ok(res.unwrap());
        }

        Err(Error::builder().kind(ErrorKind::UpnpError).message("failed to get external ip address").build())
    }

    async fn action(urn: &URN, name: &str, args: &str) -> Result<HashMap<String, String>> {
        let search_target = SearchTarget::URN(urn.clone());
        let devices = rupnp::discover(&search_target, Duration::from_secs(3), None).await?;
        pin_utils::pin_mut!(devices);

        while let Some(device) = devices.try_next().await? {
            let service = device.find_service(&urn.clone());
            if service.is_none() {
                continue;
            }
            let service = service.unwrap();

            let result = service.action(device.url(), name, args).await;
            if result.is_err() {
                continue;
            }

            return Ok(result.unwrap());
        }

        Err(Error::builder().kind(ErrorKind::UpnpError).message(format!("failed to UPnP action: {name}")).build())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn add_port_mapping_test() {
        let res = UpnpClient::add_port_mapping("TCP", 10000, 10000, "axus-daemon-rs-test").await;
        println!("{res:?}");
    }

    #[tokio::test]
    #[ignore]
    async fn delete_port_mapping_test() {
        let res = UpnpClient::delete_port_mapping("TCP", 10000).await;
        println!("{res:?}");
    }

    #[tokio::test]
    #[ignore]
    async fn get_generic_port_mapping_entry_test() {
        for i in 0..5 {
            let res = UpnpClient::get_generic_port_mapping_entry(i).await;
            if res.is_err() {
                return;
            }
            println!("{res:?}");
        }
    }

    #[tokio::test]
    #[ignore]
    async fn get_external_ip_address_test() {
        let res = UpnpClient::get_external_ip_address().await;
        println!("{res:?}");
    }
}
