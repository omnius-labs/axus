use std::{path::Path, sync::Arc};

use async_trait::async_trait;
use chrono::Utc;
use parking_lot::Mutex;
use tokio::sync::Mutex as TokioMutex;

use omnius_core_base::{clock::Clock, sleeper::Sleeper, tsid::TsidProvider};
use omnius_core_omnikit::model::OmniHash;

use crate::{
    base::{Shutdown, storage::KeyValueRocksdbStorage},
    prelude::*,
};

use super::*;

#[allow(unused)]
pub struct FilePublisher {
    file_publisher_repo: Arc<FilePublisherRepo>,
    blocks_storage: Arc<KeyValueRocksdbStorage>,

    task_encoder: Arc<TokioMutex<Option<Arc<TaskEncoder>>>>,

    tsid_provider: Arc<Mutex<dyn TsidProvider + Send + Sync>>,
    clock: Arc<dyn Clock<Utc> + Send + Sync>,
    sleeper: Arc<dyn Sleeper + Send + Sync>,
}

#[async_trait]
impl Shutdown for FilePublisher {
    async fn shutdown(&self) {
        {
            let mut task_encoder = self.task_encoder.lock().await;
            if let Some(task_encoder) = task_encoder.take() {
                task_encoder.shutdown().await;
            }
        }
    }
}

#[allow(unused)]
impl FilePublisher {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        state_dir: &Path,
        tsid_provider: Arc<Mutex<dyn TsidProvider + Send + Sync>>,
        clock: Arc<dyn Clock<Utc> + Send + Sync>,
        sleeper: Arc<dyn Sleeper + Send + Sync>,
    ) -> Result<Arc<Self>> {
        let file_publisher_repo = Arc::new(FilePublisherRepo::new(state_dir.join("repo"), clock.clone()).await?);
        let blocks_storage = Arc::new(KeyValueRocksdbStorage::new(state_dir.join("blocks"), tsid_provider.clone()).await?);

        let v = Arc::new(Self {
            file_publisher_repo,
            blocks_storage,

            task_encoder: Arc::new(TokioMutex::new(None)),

            tsid_provider,
            clock,
            sleeper,
        });
        v.start().await?;

        Ok(v)
    }

    async fn start(&self) -> Result<()> {
        let task = TaskEncoder::new(
            self.file_publisher_repo.clone(),
            self.blocks_storage.clone(),
            self.tsid_provider.clone(),
            self.clock.clone(),
            self.sleeper.clone(),
        )
        .await?;
        self.task_encoder.lock().await.replace(task);

        Ok(())
    }

    pub async fn get_published_root_hashes(&self) -> Result<Vec<OmniHash>> {
        let files = self.file_publisher_repo.get_committed_files().await?;
        let root_hashes = files.iter().map(|n| n.root_hash.clone()).collect();
        Ok(root_hashes)
    }
}

#[cfg(test)]
mod tests {
    use testresult::TestResult;

    #[tokio::test]
    pub async fn simple_test() -> TestResult {
        Ok(())
    }
}
