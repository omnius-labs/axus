use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::prelude::*;

const CONFIG_FILE_NAME: &str = "axus.toml";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct DaemonConfigToml {
    pub core: CoreConfigToml,
    pub api: ApiConfigToml,
    pub p2p: P2pConfigToml,
    pub logging: LoggingConfigToml,
    #[serde(default)]
    pub storage: StorageConfigToml,
    #[serde(default)]
    pub web: WebConfigToml,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct CoreConfigToml {
    pub state_dir: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct ApiConfigToml {
    pub listen_addr: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct P2pConfigToml {
    pub listen_addr: String,
    #[serde(default)]
    pub advertise_addrs: Vec<String>,
    #[serde(default)]
    pub use_upnp: bool,
    #[serde(default)]
    pub bootstrap_nodes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct LoggingConfigToml {
    pub level: String,
    pub json: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
struct StorageConfigToml {
    pub download_dir: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
struct WebConfigToml {
    pub dir: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonConfig {
    pub core: CoreConfig,
    pub api: ApiConfig,
    pub p2p: P2pConfig,
    pub logging: LoggingConfig,
    pub storage: StorageConfig,
    pub web: WebConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreConfig {
    pub state_dir: PathBuf,
}

/// Listen address of the REST API. Separate from [`P2pConfig`] so that the
/// HTTP surface and the P2P transport never share a port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiConfig {
    pub listen_addr: String,
}

impl ApiConfig {
    /// The API has no authentication, so listening elsewhere exposes it to whatever reaches the address.
    pub fn listens_only_on_loopback(&self) -> bool {
        let host = self.listen_addr.rsplit_once(':').map_or(self.listen_addr.as_str(), |(host, _)| host);
        let host = host.trim_start_matches('[').trim_end_matches(']');
        host.eq_ignore_ascii_case("localhost") || host.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageConfig {
    /// Where the outputs of subscriptions are placed.
    pub download_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebConfig {
    /// The build output of the browser client, served on the API's port when set.
    pub dir: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct P2pConfig {
    pub listen_addr: String,
    /// 他の node に広告する `host:port` の一覧。空なら待ち受けアドレスから決める
    pub advertise_addrs: Vec<String>,
    /// 不特定のアドレスで待ち受けるとき、UPnP でルーターのポートを開放する
    pub use_upnp: bool,
    /// 起動時に接続を試みる node の NodeProfile の URI（`axus:node/...`）の一覧
    pub bootstrap_nodes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoggingConfig {
    pub level: String,
    pub json: bool,
}

impl DaemonConfig {
    pub async fn load<P: AsRef<Path>>(dir: P) -> Result<Self> {
        let dir = dir.as_ref();
        tokio::fs::create_dir_all(dir).await?;

        let toml = Self::load_toml(dir).await?;
        let state_dir = dir.join(toml.core.state_dir);

        Ok(DaemonConfig {
            storage: StorageConfig {
                download_dir: toml.storage.download_dir.map_or_else(|| state_dir.join("downloads"), |path| dir.join(path)),
            },
            web: WebConfig {
                dir: toml.web.dir.map(|path| dir.join(path)),
            },
            core: CoreConfig { state_dir },
            api: ApiConfig {
                listen_addr: toml.api.listen_addr,
            },
            p2p: P2pConfig {
                listen_addr: toml.p2p.listen_addr,
                advertise_addrs: toml.p2p.advertise_addrs,
                use_upnp: toml.p2p.use_upnp,
                bootstrap_nodes: toml.p2p.bootstrap_nodes,
            },
            logging: LoggingConfig {
                level: toml.logging.level,
                json: toml.logging.json,
            },
        })
    }

    async fn load_toml(dir: &Path) -> Result<DaemonConfigToml> {
        let toml_path = dir.join(CONFIG_FILE_NAME);
        let toml = tokio::fs::read_to_string(toml_path).await?;
        Ok(toml::from_str(&toml)?)
    }
}

#[cfg(test)]
mod tests {
    use testresult::TestResult;

    use super::*;

    #[tokio::test]
    async fn load_config_test() -> TestResult {
        let toml = r#"
            [core]
            state_dir = "./state"

            [api]
            listen_addr = "0.0.0.0:6051"

            [p2p]
            listen_addr = "0.0.0.0:6052"

            [logging]
            level = "info"
            json = false
        "#;

        let tempdir = tempfile::tempdir()?;
        std::fs::write(tempdir.path().join("axus.toml"), toml)?;

        let conf = DaemonConfig::load(tempdir.path()).await?;

        assert_eq!(conf.core.state_dir, tempdir.path().join("./state"));
        assert_eq!(conf.api.listen_addr, "0.0.0.0:6051");
        assert_eq!(conf.p2p.listen_addr, "0.0.0.0:6052");
        assert!(conf.p2p.advertise_addrs.is_empty());
        assert!(!conf.p2p.use_upnp);
        assert!(conf.p2p.bootstrap_nodes.is_empty());
        assert_eq!(conf.logging.level, "info");
        assert!(!conf.logging.json);

        Ok(())
    }

    #[tokio::test]
    async fn load_config_defaults_storage_and_web_test() -> TestResult {
        let toml = r#"
            [core]
            state_dir = "./state"

            [api]
            listen_addr = "127.0.0.1:6051"

            [p2p]
            listen_addr = "127.0.0.1:6052"

            [logging]
            level = "info"
            json = false
        "#;

        let tempdir = tempfile::tempdir()?;
        std::fs::write(tempdir.path().join("axus.toml"), toml)?;

        let conf = DaemonConfig::load(tempdir.path()).await?;

        assert_eq!(conf.storage.download_dir, tempdir.path().join("./state").join("downloads"));
        assert_eq!(conf.web.dir, None);

        Ok(())
    }

    #[tokio::test]
    async fn load_config_reads_storage_and_web_test() -> TestResult {
        let toml = r#"
            [core]
            state_dir = "./state"

            [api]
            listen_addr = "127.0.0.1:6051"

            [p2p]
            listen_addr = "127.0.0.1:6052"

            [logging]
            level = "info"
            json = false

            [storage]
            download_dir = "/downloads"

            [web]
            dir = "./web"
        "#;

        let tempdir = tempfile::tempdir()?;
        std::fs::write(tempdir.path().join("axus.toml"), toml)?;

        let conf = DaemonConfig::load(tempdir.path()).await?;

        assert_eq!(conf.storage.download_dir, Path::new("/downloads"));
        assert_eq!(conf.web.dir, Some(tempdir.path().join("./web")));

        Ok(())
    }

    #[test]
    fn listens_only_on_loopback_test() {
        for (listen_addr, expected) in [
            ("127.0.0.1:5050", true),
            ("[::1]:5050", true),
            ("localhost:5050", true),
            ("0.0.0.0:5050", false),
            ("[::]:5050", false),
            ("192.168.0.2:5050", false),
            ("example.com:5050", false),
        ] {
            let conf = ApiConfig {
                listen_addr: listen_addr.to_string(),
            };
            assert_eq!(conf.listens_only_on_loopback(), expected, "{listen_addr}");
        }
    }

    #[tokio::test]
    async fn load_config_reads_advertise_settings_test() -> TestResult {
        let toml = r#"
            [core]
            state_dir = "./state"

            [api]
            listen_addr = "127.0.0.1:6051"

            [p2p]
            listen_addr = "0.0.0.0:6052"
            advertise_addrs = ["203.0.113.1:6052", "example.com:6052"]
            use_upnp = true
            bootstrap_nodes = ["axus:node/a", "axus:node/b"]

            [logging]
            level = "info"
            json = false
        "#;

        let tempdir = tempfile::tempdir()?;
        std::fs::write(tempdir.path().join("axus.toml"), toml)?;

        let conf = DaemonConfig::load(tempdir.path()).await?;

        assert_eq!(conf.p2p.advertise_addrs, vec!["203.0.113.1:6052", "example.com:6052"]);
        assert!(conf.p2p.use_upnp);
        assert_eq!(conf.p2p.bootstrap_nodes, vec!["axus:node/a", "axus:node/b"]);

        Ok(())
    }

    #[tokio::test]
    async fn load_repository_sample_test() -> TestResult {
        let config_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config");

        let conf = DaemonConfig::load(&config_dir).await?;

        assert_eq!(conf.core.state_dir, config_dir.join("./state"));
        assert_eq!(conf.api.listen_addr, "127.0.0.1:5050");
        assert_eq!(conf.p2p.listen_addr, "127.0.0.1:5051");
        assert!(conf.p2p.advertise_addrs.is_empty());
        assert!(!conf.p2p.use_upnp);
        assert!(conf.p2p.bootstrap_nodes.is_empty());
        assert_eq!(conf.logging.level, "info");
        assert!(!conf.logging.json);

        Ok(())
    }

    #[tokio::test]
    async fn load_config_rejects_missing_p2p_section_test() -> TestResult {
        let toml = r#"
            [core]
            state_dir = "./state"

            [api]
            listen_addr = "0.0.0.0:6051"

            [logging]
            level = "info"
            json = false
        "#;

        let tempdir = tempfile::tempdir()?;
        std::fs::write(tempdir.path().join("axus.toml"), toml)?;

        assert!(DaemonConfig::load(tempdir.path()).await.is_err());

        Ok(())
    }
}
