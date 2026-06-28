use std::path::PathBuf;

use tempfile::TempDir;

use crate::{config::DaemonConfig, prelude::*};

pub struct DaemonState {
    pub conf: DaemonConfig,
    #[allow(unused)]
    temp_dir: TempDir,
}

impl DaemonState {
    pub async fn new(conf: DaemonConfig) -> Result<Self> {
        let state_dir = PathBuf::from(&conf.core.state_dir);
        tokio::fs::create_dir_all(&state_dir).await?;

        let temp_dir = TempDir::new()?;

        // let clock: Arc<dyn Clock<Utc> + Send + Sync> = Arc::new(ClockUtc);
        // let rng = Arc::new(Mutex::new(ChaCha20Rng::from_rng(&mut UnwrapErr(SysRng))));
        // let db = db::connect(&state_dir).await?;

        Ok(Self { conf, temp_dir })
    }
}
