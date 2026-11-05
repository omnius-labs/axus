use tempfile::TempDir;

use std::{path::Path, str::FromStr as _};

use omnius_axus_engine::{
    model::NodeProfile,
    service::{AxusService, AxusServiceOption},
};
use omnius_core_omnikit::model::omni_addr::OmniAddr;

use crate::{config::DaemonConfig, prelude::*, server::upload_session::UploadSessions};

pub struct DaemonState {
    pub conf: DaemonConfig,
    pub uploads: UploadSessions,
    axus_service: AxusService,
    #[allow(unused)]
    temp_dir: TempDir,
}

impl DaemonState {
    pub async fn new(conf: DaemonConfig) -> Result<Self> {
        Self::with_option(conf, AxusServiceOption::default()).await
    }

    /// `base_option` carries what the configuration does not set.
    async fn with_option(conf: DaemonConfig, base_option: AxusServiceOption) -> Result<Self> {
        let state_dir = conf.core.state_dir.clone();
        tokio::fs::create_dir_all(&state_dir).await?;
        tokio::fs::create_dir_all(&conf.storage.download_dir).await?;

        let temp_dir = TempDir::new()?;

        let advertise_addrs = conf
            .p2p
            .advertise_addrs
            .iter()
            .map(OmniAddr::from_host_and_port_str)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let bootstrap_node_profiles = conf
            .p2p
            .bootstrap_nodes
            .iter()
            .map(|uri| NodeProfile::from_str(uri))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let option = AxusServiceOption {
            bootstrap_node_profiles,
            advertise_addrs,
            use_upnp: conf.p2p.use_upnp,
            ..base_option
        };

        let uploads = UploadSessions::open(&Self::uploads_dir(&state_dir)).await?;
        let axus_service = AxusService::new(&state_dir, &conf.p2p.listen_addr, temp_dir.path(), option).await?;

        Ok(Self {
            conf,
            uploads,
            axus_service,
            temp_dir,
        })
    }

    fn uploads_dir(state_dir: &Path) -> std::path::PathBuf {
        state_dir.join("uploads")
    }

    pub fn service(&self) -> &AxusService {
        &self.axus_service
    }

    #[cfg(test)]
    pub(crate) async fn for_test(conf: DaemonConfig, option: AxusServiceOption) -> Result<Self> {
        Self::with_option(conf, option).await
    }

    pub async fn shutdown(&self) {
        self.axus_service.shutdown().await;
    }
}

#[cfg(test)]
mod tests {
    use testresult::TestResult;

    use crate::config::{ApiConfig, CoreConfig, LoggingConfig, P2pConfig, StorageConfig, WebConfig};

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
            storage: StorageConfig {
                download_dir: state_dir.path().join("downloads"),
            },
            web: WebConfig { dir: None },
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
            storage: StorageConfig {
                download_dir: state_dir.path().join("downloads"),
            },
            web: WebConfig { dir: None },
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
            storage: StorageConfig {
                download_dir: state_dir.path().join("downloads"),
            },
            web: WebConfig { dir: None },
        };

        assert!(DaemonState::new(conf).await.is_err());

        Ok(())
    }
}
