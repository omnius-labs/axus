use std::sync::Arc;

use chrono::Utc;
use tempfile::TempDir;

use std::str::FromStr as _;

use omnius_axus_engine::{
    model::NodeProfile,
    service::{AxusService, AxusServiceOption},
};
use omnius_core_base::{
    clock::{Clock, ClockUtc},
    tsid::{TsidProvider, TsidProviderImpl},
};
use omnius_core_omnikit::model::omni_addr::OmniAddr;
use rand::{
    SeedableRng as _,
    rngs::{ChaCha20Rng, SysRng},
};
use rand_core::UnwrapErr;

use crate::{
    config::DaemonConfig,
    prelude::*,
    service::{publish::PublishService, subscribe::SubscribeService, upload::UploadService},
};

pub struct DaemonState {
    pub conf: DaemonConfig,
    pub publish: Arc<PublishService>,
    pub subscribe: Arc<SubscribeService>,
    uploads: Arc<UploadService>,
    axus_service: AxusService,
    #[allow(unused)]
    temp_dir: TempDir,
}

impl DaemonState {
    pub async fn new(conf: DaemonConfig) -> Result<Self> {
        Self::new_with_option(conf, AxusServiceOption::default()).await
    }

    async fn new_with_option(conf: DaemonConfig, mut option: AxusServiceOption) -> Result<Self> {
        if !Self::is_loopback_listen_addr(&conf.api.listen_addr) {
            warn!(
                addr = conf.api.listen_addr,
                "the REST API listens on a non-loopback address without authentication; bind a loopback address unless the network is trusted"
            );
        }

        let state_dir = conf.core.state_dir.clone();
        tokio::fs::create_dir_all(&state_dir).await?;

        let temp_dir = TempDir::new()?;

        option.advertise_addrs = conf
            .p2p
            .advertise_addrs
            .iter()
            .map(OmniAddr::from_host_and_port_str)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        option.bootstrap_node_profiles = conf
            .p2p
            .bootstrap_nodes
            .iter()
            .map(|uri| NodeProfile::from_str(uri))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        option.use_upnp = conf.p2p.use_upnp;

        let axus_service = AxusService::new(&state_dir, &conf.p2p.listen_addr, temp_dir.path(), option).await?;
        let file_service = axus_service.file_service();

        let clock: Arc<dyn Clock<Utc> + Send + Sync> = Arc::new(ClockUtc);
        let uploads = match UploadService::new(&state_dir, conf.api.max_upload_size, clock).await {
            Ok(uploads) => uploads,
            Err(error) => {
                axus_service.shutdown().await;
                return Err(error);
            }
        };
        let ids: Arc<parking_lot::Mutex<dyn TsidProvider + Send + Sync>> =
            Arc::new(parking_lot::Mutex::new(TsidProviderImpl::new(ClockUtc, ChaCha20Rng::from_rng(&mut UnwrapErr(SysRng)), 8)));
        let publish = Arc::new(PublishService::new(file_service.clone(), uploads.clone(), ids));
        let subscribe = Arc::new(SubscribeService::new(file_service));

        Ok(Self {
            conf,
            publish,
            subscribe,
            uploads,
            axus_service,
            temp_dir,
        })
    }

    fn is_loopback_listen_addr(addr: &str) -> bool {
        if let Ok(addr) = addr.parse::<std::net::SocketAddr>() {
            return addr.ip().is_loopback();
        }
        use std::net::ToSocketAddrs as _;
        addr.to_socket_addrs().map(|mut addrs| addrs.all(|addr| addr.ip().is_loopback())).unwrap_or(false)
    }

    #[cfg(test)]
    pub async fn new_for_test(conf: DaemonConfig, option: AxusServiceOption) -> Result<Self> {
        Self::new_with_option(conf, option).await
    }

    #[cfg(test)]
    pub fn node_profile(&self) -> NodeProfile {
        self.axus_service.node_profile()
    }

    pub async fn shutdown(&self) {
        self.uploads.shutdown().await;
        self.axus_service.shutdown().await;
    }
}

#[cfg(test)]
mod tests {
    use testresult::TestResult;

    use crate::config::{ApiConfig, CoreConfig, LoggingConfig, P2pConfig};

    use super::*;

    #[tokio::test]
    async fn new_builds_axus_service_and_shuts_down_test() -> TestResult {
        let state_dir = tempfile::tempdir()?;

        let conf = DaemonConfig {
            core: CoreConfig {
                state_dir: state_dir.path().to_path_buf(),
            },
            api: ApiConfig {
                listen_addr: "127.0.0.1:0".to_string(),
                max_upload_size: 0,
            },
            p2p: P2pConfig {
                listen_addr: "127.0.0.1:0".to_string(),
                advertise_addrs: vec![],
                use_upnp: false,
                bootstrap_nodes: vec![],
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                json: false,
            },
        };

        let state = DaemonState::new(conf).await?;
        state.shutdown().await;

        Ok(())
    }

    #[tokio::test]
    async fn new_fails_when_p2p_listen_addr_is_invalid_test() -> TestResult {
        let state_dir = tempfile::tempdir()?;

        let conf = DaemonConfig {
            core: CoreConfig {
                state_dir: state_dir.path().to_path_buf(),
            },
            api: ApiConfig {
                listen_addr: "127.0.0.1:0".to_string(),
                max_upload_size: 0,
            },
            p2p: P2pConfig {
                listen_addr: "not-an-addr".to_string(),
                advertise_addrs: vec![],
                use_upnp: false,
                bootstrap_nodes: vec![],
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                json: false,
            },
        };

        assert!(DaemonState::new(conf).await.is_err());

        Ok(())
    }

    #[tokio::test]
    async fn new_fails_when_bootstrap_node_is_invalid_test() -> TestResult {
        let state_dir = tempfile::tempdir()?;

        let conf = DaemonConfig {
            core: CoreConfig {
                state_dir: state_dir.path().to_path_buf(),
            },
            api: ApiConfig {
                listen_addr: "127.0.0.1:0".to_string(),
                max_upload_size: 0,
            },
            p2p: P2pConfig {
                listen_addr: "127.0.0.1:0".to_string(),
                advertise_addrs: vec![],
                use_upnp: false,
                bootstrap_nodes: vec!["axus:node/not-a-node-profile".to_string()],
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                json: false,
            },
        };

        assert!(DaemonState::new(conf).await.is_err());

        Ok(())
    }

    #[test]
    fn loopback_listen_addr_detection_test() {
        assert!(DaemonState::is_loopback_listen_addr("127.0.0.1:5050"));
        assert!(DaemonState::is_loopback_listen_addr("[::1]:5050"));
        assert!(!DaemonState::is_loopback_listen_addr("0.0.0.0:5050"));
        assert!(!DaemonState::is_loopback_listen_addr("[::]:5050"));
    }
}
