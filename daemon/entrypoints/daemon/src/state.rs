use tempfile::TempDir;

use omnius_axus_engine::service::{AxusService, AxusServiceOption};
use omnius_core_omnikit::model::omni_addr::OmniAddr;

use crate::{config::DaemonConfig, prelude::*};

pub struct DaemonState {
    pub conf: DaemonConfig,
    axus_service: AxusService,
    #[allow(unused)]
    temp_dir: TempDir,
}

impl DaemonState {
    pub async fn new(conf: DaemonConfig) -> Result<Self> {
        let state_dir = conf.core.state_dir.clone();
        tokio::fs::create_dir_all(&state_dir).await?;

        let temp_dir = TempDir::new()?;

        let advertise_addrs = conf
            .p2p
            .advertise_addrs
            .iter()
            .map(OmniAddr::from_host_and_port_str)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let option = AxusServiceOption {
            advertise_addrs,
            use_upnp: conf.p2p.use_upnp,
            ..Default::default()
        };

        let axus_service = AxusService::new(&state_dir, &conf.p2p.listen_addr, temp_dir.path(), option).await?;

        Ok(Self { conf, axus_service, temp_dir })
    }

    pub async fn shutdown(&self) {
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
            },
            p2p: P2pConfig {
                listen_addr: "127.0.0.1:0".to_string(),
                advertise_addrs: vec![],
                use_upnp: false,
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
            },
            p2p: P2pConfig {
                listen_addr: "not-an-addr".to_string(),
                advertise_addrs: vec![],
                use_upnp: false,
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                json: false,
            },
        };

        assert!(DaemonState::new(conf).await.is_err());

        Ok(())
    }
}
