use std::{path::Path, sync::Arc};

use async_trait::async_trait;
use chrono::Utc;
use parking_lot::Mutex;
use tokio::sync::Mutex as TokioMutex;
use tokio_util::bytes::Bytes;

use omnius_core_base::{clock::Clock, sleeper::Sleeper, tsid::TsidProvider};
use omnius_core_omnikit::generated::omni_hash::OmniHash;

use crate::{
    base::{runtime::Shutdown, storage::KeyValueRocksdbStorage},
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
        let repo_dir = state_dir.join("repo");
        tokio::fs::create_dir_all(&repo_dir).await?;
        let file_publisher_repo = Arc::new(FilePublisherRepo::new(&repo_dir, clock.clone()).await?);
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

    pub async fn import(&self, file_path: &str, file_name: &str, block_size: u32, attrs: Option<&str>, priority: i64) -> Result<()> {
        let Some(task_encoder) = self.task_encoder.lock().await.clone() else {
            return Err(Error::new(ErrorKind::UnexpectedError).with_message("task encoder is not started"));
        };
        task_encoder.import(file_path, file_name, block_size, attrs, priority).await
    }

    pub async fn read_block(&self, root_hash: &OmniHash, block_hash: &OmniHash) -> Result<Option<Bytes>> {
        let key = gen_committed_block_path(root_hash, block_hash);
        Ok(self.blocks_storage.get_value(&key).await?.map(Bytes::from))
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
