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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct P2pConfig {
    pub listen_addr: String,
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

        Ok(DaemonConfig {
            core: CoreConfig {
                state_dir: dir.join(toml.core.state_dir),
            },
            api: ApiConfig {
                listen_addr: toml.api.listen_addr,
            },
            p2p: P2pConfig {
                listen_addr: toml.p2p.listen_addr,
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
        assert_eq!(conf.logging.level, "info");
        assert!(!conf.logging.json);

        Ok(())
    }

    #[tokio::test]
    async fn load_repository_sample_test() -> TestResult {
        let config_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config");

        let conf = DaemonConfig::load(&config_dir).await?;

        assert_eq!(conf.core.state_dir, config_dir.join("./state"));
        assert_eq!(conf.api.listen_addr, "127.0.0.1:5050");
        assert_eq!(conf.p2p.listen_addr, "127.0.0.1:5051");
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
