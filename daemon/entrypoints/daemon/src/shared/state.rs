use std::sync::Arc;

use tempfile::TempDir;

use omnius_axus_engine::engine::AxusEngine;

use super::{AppConfig, info::AppInfo};

use crate::prelude::*;

pub struct AppState {
    pub info: AppInfo,
    pub conf: AppConfig,
    #[allow(unused)]
    pub engine: Arc<AxusEngine>,

    #[allow(unused)]
    temp_dir: TempDir,
}

impl AppState {
    pub async fn new(info: AppInfo, conf: AppConfig) -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let engine = Arc::new(AxusEngine::new(&conf.state_dir, &conf.listen_addr, &temp_dir.path()).await?);

        Ok(Self { info, conf, engine, temp_dir })
    }
}
