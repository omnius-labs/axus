use std::sync::Arc;

use tempfile::TempDir;

use omnius_axus_engine::service::AxusService;

use super::{AppConfig, info::AppInfo};

use crate::prelude::*;

pub struct AppState {
    pub info: AppInfo,
    pub conf: AppConfig,
    #[allow(unused)]
    pub engine: Arc<AxusService>,

    #[allow(unused)]
    temp_dir: TempDir,
}

impl AppState {
    pub async fn new(info: AppInfo, conf: AppConfig) -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let engine = Arc::new(AxusService::new(&conf.state_dir, &conf.listen_addr, &temp_dir.path()).await?);

        Ok(Self { info, conf, engine, temp_dir })
    }
}
