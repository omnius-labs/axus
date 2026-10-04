use super::*;
use crate::{base::runtime::Shutdown, prelude::*};
use async_trait::async_trait;
use chrono::Utc;
use omnius_core_base::{clock::Clock, sleeper::Sleeper, tsid::TsidProvider};
use omnius_core_omnikit::generated::omni_hash::OmniHash;
use parking_lot::Mutex;
use std::{path::Path, sync::Arc};
use tokio_util::bytes::Bytes;
mod repo;
mod store;
mod task_encoder;
mod util;
use self::{store::FilePublisherStore, task_encoder::TaskEncoder};

pub struct FilePublisher {
    store: Arc<FilePublisherStore>,
    task_encoder: Arc<TaskEncoder>,
}

#[async_trait]
impl Shutdown for FilePublisher {
    async fn shutdown(&self) {
        self.task_encoder.shutdown().await;
    }
}

#[allow(unused)]
impl FilePublisher {
    pub async fn new(
        state_dir: &Path,
        tsid_provider: Arc<Mutex<dyn TsidProvider + Send + Sync>>,
        clock: Arc<dyn Clock<Utc> + Send + Sync>,
        sleeper: Arc<dyn Sleeper + Send + Sync>,
    ) -> Result<Arc<Self>> {
        let store = FilePublisherStore::open(state_dir, tsid_provider, clock.clone()).await?;
        let task_encoder = TaskEncoder::new(store.clone(), clock, sleeper).await?;
        Ok(Arc::new(Self { store, task_encoder }))
    }
    pub async fn import(&self, file_path: &str, file_name: &str, block_size: u32, attrs: Option<&str>, priority: i64) -> Result<String> {
        let id = self.store.import(file_path, file_name, block_size, attrs, priority).await?;
        self.task_encoder.wake();
        Ok(id)
    }
    pub async fn cancel(&self, id: &str) -> Result<()> {
        self.store.cancel(id).await?;
        self.task_encoder.cancel(id);
        self.store.cleanup_blocks(id).await
    }
    pub async fn remove(&self, root_hash: &OmniHash, file_name: &str) -> Result<()> {
        self.store.remove(root_hash, file_name).await
    }
    pub async fn read_block(&self, root_hash: &OmniHash, block_hash: &OmniHash) -> Result<Option<Bytes>> {
        self.store.read_block(root_hash, block_hash).await
    }
    pub async fn get_published_root_hashes(&self) -> Result<Vec<OmniHash>> {
        self.store.get_published_root_hashes().await
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
