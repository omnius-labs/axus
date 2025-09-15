use std::{path::Path, sync::Arc};

use async_trait::async_trait;
use chrono::Utc;
use parking_lot::Mutex;
use tokio::sync::Mutex as TokioMutex;
use tokio_util::bytes::Bytes;

use omnius_core_base::{clock::Clock, sleeper::Sleeper, tsid::TsidProvider};
use omnius_core_omnikit::model::OmniHash;

use crate::{
    base::{Shutdown, storage::KeyValueRocksdbStorage},
    prelude::*,
};

use super::*;

#[allow(unused)]
pub struct FileSubscriber {
    file_subscriber_repo: Arc<FileSubscriberRepo>,
    blocks_storage: Arc<KeyValueRocksdbStorage>,

    task_decoder: Arc<TokioMutex<Option<Arc<TaskDecoder>>>>,

    tsid_provider: Arc<Mutex<dyn TsidProvider + Send + Sync>>,
    clock: Arc<dyn Clock<Utc> + Send + Sync>,
    sleeper: Arc<dyn Sleeper + Send + Sync>,
}

#[async_trait]
impl Shutdown for FileSubscriber {
    async fn shutdown(&self) {
        {
            let mut task_decoder = self.task_decoder.lock().await;
            if let Some(task_decoder) = task_decoder.take() {
                task_decoder.shutdown().await;
            }
        }
    }
}

#[allow(unused)]
impl FileSubscriber {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        state_dir: &Path,

        tsid_provider: Arc<Mutex<dyn TsidProvider + Send + Sync>>,
        clock: Arc<dyn Clock<Utc> + Send + Sync>,
        sleeper: Arc<dyn Sleeper + Send + Sync>,
    ) -> Result<Arc<Self>> {
        let file_subscriber_repo = Arc::new(FileSubscriberRepo::new(state_dir.join("repo"), clock.clone()).await?);
        let blocks_storage = Arc::new(KeyValueRocksdbStorage::new(state_dir.join("blocks"), tsid_provider.clone()).await?);

        let v = Arc::new(Self {
            file_subscriber_repo,
            blocks_storage,

            task_decoder: Arc::new(TokioMutex::new(None)),

            tsid_provider,
            clock,
            sleeper,
        });

        Ok(v)
    }

    async fn start(&self) -> Result<()> {
        let task = TaskDecoder::new(
            self.file_subscriber_repo.clone(),
            self.blocks_storage.clone(),
            self.tsid_provider.clone(),
            self.clock.clone(),
            self.sleeper.clone(),
        )
        .await?;
        self.task_decoder.lock().await.replace(task);

        Ok(())
    }

    pub async fn get_subscribed_root_hashes(&self) -> Result<Vec<OmniHash>> {
        let files = self.file_subscriber_repo.get_committed_files().await?;
        let root_hashes = files.iter().map(|n| n.root_hash.clone()).collect();
        Ok(root_hashes)
    }

    pub async fn write_block(&self, root_hash: &OmniHash, block_hash: &OmniHash, value: Bytes) -> Result<()> {
        let blocks = self.file_subscriber_repo.find_blocks_by_root_hash_and_block_hash(root_hash, block_hash).await?;
        if blocks.is_empty() {
            return Ok(());
        }

        let key = gen_block_path(root_hash, block_hash);
        self.blocks_storage.put_value(&key, value, true).await?;

        let new_blocks: Vec<SubscribedBlock> = blocks.into_iter().map(|n| SubscribedBlock { downloaded: true, ..n }).collect();
        self.file_subscriber_repo.upsert_blocks(&new_blocks).await?;

        let Some(file) = self.file_subscriber_repo.find_file_by_root_hash(root_hash).await? else {
            return Ok(());
        };

        let block_count_downloaded = file.block_count_downloaded + 1;
        let status = if block_count_downloaded < file.block_count_total {
            SubscribedFileStatus::Downloading
        } else {
            SubscribedFileStatus::Decoding
        };

        let new_file = SubscribedFile {
            block_count_downloaded,
            status,
            ..file
        };
        self.file_subscriber_repo.insert_file(&new_file).await?;

        Ok(())
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
