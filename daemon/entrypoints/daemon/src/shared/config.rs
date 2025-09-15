use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub listen_addr: String,
    pub state_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct AppConfigToml {
    pub listen_addr: String,
    pub state_dir: String,
}

impl AppConfig {
    pub async fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path
            .as_ref()
            .to_str()
            .ok_or_else(|| Error::builder().kind(ErrorKind::UnexpectedError).message("Invalid path").build())?;

        let toml = std::fs::read_to_string(path)?;
        let toml: AppConfigToml = toml::from_str(&toml)?;

        Ok(AppConfig {
            listen_addr: toml.listen_addr,
            state_dir: PathBuf::from(toml.state_dir),
        })
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
            listen_addr = "localhost:8080"
        "#;

        let tempfile = tempfile::NamedTempFile::new()?;
        tempfile.as_file().write_all(toml.as_bytes())?;

        let conf = AppConfig::load(tempfile.path()).await?;

        println!("{conf:?}");

        Ok(())
    }
}
