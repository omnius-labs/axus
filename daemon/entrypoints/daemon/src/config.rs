use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct DaemonConfigToml {
    pub core: CoreConfigToml,
    pub logging: LoggingConfigToml,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct CoreConfigToml {
    pub state_dir: String,
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
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreConfig {
    pub state_dir: PathBuf,
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
                listen_addr: toml.core.listen_addr,
            },
            logging: LoggingConfig {
                level: toml.logging.level,
                json: toml.logging.json,
            },
        })
    }

    async fn load_toml(dir: &Path) -> Result<DaemonConfigToml> {
        let toml_path = dir.join("pxna.toml");
        let toml = tokio::fs::read_to_string(toml_path).await?;
        Ok(toml::from_str(&toml)?)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use testresult::TestResult;

    use super::*;

    #[ignore]
    #[tokio::test]
    async fn secret_reader_test() -> TestResult {
        let toml = r#"
            [core]
            state_dir = "./state"
            listen_addr = "0.0.0.0:6051"

            [logging]
            level = "info"
            json = false
        "#;

        let tempdir = tempfile::tempdir()?;
        let config_path = tempdir.path().join("pxna.toml");
        std::fs::File::create(&config_path)?.write_all(toml.as_bytes())?;

        let conf = DaemonConfig::load(tempdir.path()).await?;

        assert_eq!(conf.core.listen_addr, "0.0.0.0:6050");
        assert_eq!(conf.core.state_dir, tempdir.path().join("./state"));
        assert_eq!(conf.logging.level, "info");
        assert!(!conf.logging.json);

        Ok(())
    }
}
