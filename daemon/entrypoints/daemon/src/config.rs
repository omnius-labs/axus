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
    #[serde(default)]
    pub max_upload_size: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
struct StorageConfigToml {
    pub download_dir: Option<String>,
    #[serde(default)]
    pub library_roots: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
struct WebConfigToml {
    pub dir: Option<String>,
    #[serde(default)]
    pub insecure_cookie: bool,
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

/// Listen address of the local REST API. Separate from [`P2pConfig`] so that the
/// HTTP surface and the P2P transport never share a port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiConfig {
    pub listen_addr: String,
    /// Largest declared publication size in bytes. Zero means no limit.
    pub max_upload_size: i64,
}

impl ApiConfig {
    pub fn listens_only_on_loopback(&self) -> bool {
        self.listen_addr.parse::<std::net::SocketAddr>().is_ok_and(|addr| addr.ip().is_loopback())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageConfig {
    pub download_dir: PathBuf,
    pub library_roots: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebConfig {
    pub dir: Option<PathBuf>,
    /// Drops `Secure` and the `__Host-` prefix so a browser on the LAN can log in over plain HTTP.
    pub insecure_cookie: bool,
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
        if toml.api.max_upload_size < 0 {
            return Err(Error::new(ErrorKind::InvalidFormat).with_message("api.max_upload_size must not be negative"));
        }
        let state_dir = dir.join(toml.core.state_dir);
        let download_dir = toml.storage.download_dir.map(|path| dir.join(path)).unwrap_or_else(|| state_dir.join("downloads"));

        Ok(DaemonConfig {
            core: CoreConfig { state_dir },
            api: ApiConfig {
                listen_addr: toml.api.listen_addr,
                max_upload_size: toml.api.max_upload_size,
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
            storage: StorageConfig {
                download_dir,
                library_roots: toml.storage.library_roots.iter().map(|path| dir.join(path)).collect(),
            },
            web: WebConfig {
                dir: toml.web.dir.map(|path| dir.join(path)),
                insecure_cookie: toml.web.insecure_cookie,
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

    #[test]
    fn only_loopback_addresses_are_accepted() {
        let config = |addr: &str| ApiConfig {
            listen_addr: addr.to_string(),
            max_upload_size: 0,
        };
        assert!(config("127.0.0.1:5050").listens_only_on_loopback());
        assert!(config("[::1]:5050").listens_only_on_loopback());
        assert!(!config("0.0.0.0:5050").listens_only_on_loopback());
        assert!(!config("192.168.1.10:5050").listens_only_on_loopback());
        assert!(!config("localhost:5050").listens_only_on_loopback());
    }

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
