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
        let repo_dir = state_dir.join("repo");
        tokio::fs::create_dir_all(&repo_dir).await?;
        let file_subscriber_repo = Arc::new(FileSubscriberRepo::new(&repo_dir, clock.clone()).await?);
        let blocks_storage = Arc::new(KeyValueRocksdbStorage::new(state_dir.join("blocks"), tsid_provider.clone()).await?);

        let v = Arc::new(Self {
            file_subscriber_repo,
            blocks_storage,

            task_decoder: Arc::new(TokioMutex::new(None)),

            tsid_provider,
            clock,
            sleeper,
        });
        v.start().await?;

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

    pub async fn subscribe(&self, root_hash: &OmniHash, file_path: &str, attrs: Option<&str>, priority: i64) -> Result<String> {
        let id = self.tsid_provider.lock().create().to_string();
        let now = self.clock.now();

        let file = SubscribedFile {
            id: id.clone(),
            root_hash: root_hash.clone(),
            file_path: file_path.to_string(),
            rank: SubscribedFile::UNKNOWN_ROOT_RANK,
            block_count_downloaded: 0,
            block_count_total: 1,
            attrs: attrs.map(|n| n.to_string()),
            priority,
            status: SubscribedFileStatus::Downloading,
            failed_reason: None,
            created_at: now,
            updated_at: now,
        };
        let root_block = SubscribedBlock {
            root_hash: root_hash.clone(),
            block_hash: root_hash.clone(),
            rank: SubscribedFile::UNKNOWN_ROOT_RANK,
            index: 0,
            downloaded: false,
        };
        self.file_subscriber_repo.upsert_file_and_blocks(&file, &[root_block]).await?;

        Ok(id)
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
        if file.status != SubscribedFileStatus::Downloading {
            return Ok(());
        }

        // 同じ hash の block が複数の位置にあり得るため、受け取った回数ではなく現在の rank の block の状態から数える
        let current_blocks = self.file_subscriber_repo.find_blocks_by_root_hash_and_rank(root_hash, file.rank).await?;
        let block_count_downloaded = current_blocks.iter().filter(|n| n.downloaded).count() as u32;
        let status = if block_count_downloaded < file.block_count_total {
            SubscribedFileStatus::Downloading
        } else {
            SubscribedFileStatus::Decoding
        };

        let new_file = SubscribedFile {
            block_count_downloaded,
            status: status.clone(),
            updated_at: self.clock.now(),
            ..file
        };
        self.file_subscriber_repo.upsert_file_and_blocks(&new_file, &[]).await?;

        if status == SubscribedFileStatus::Decoding
            && let Some(task_decoder) = self.task_decoder.lock().await.as_ref()
        {
            task_decoder.export(&new_file.id).await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{path::Path, sync::Arc, time::Duration};

    use chrono::Utc;
    use parking_lot::Mutex;
    use rand::{
        SeedableRng as _,
        rngs::{ChaCha20Rng, SysRng},
    };
    use rand_core::UnwrapErr;
    use testresult::TestResult;

    use omnius_core_base::{
        clock::{Clock, ClockUtc},
        sleeper::{Sleeper, SleeperImpl},
        tsid::{TsidProvider, TsidProviderImpl},
    };

    use crate::{base::runtime::Shutdown as _, core::negotiator::file::FilePublisher, prelude::*};

    use super::{FileSubscriber, SubscribedFile, SubscribedFileStatus};

    const BLOCK_SIZE: u32 = 256;
    const TEST_TIMEOUT: Duration = Duration::from_secs(30);

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn multi_level_file_round_trip() -> TestResult {
        // 256 byte の block に hash が数個しか入らないため、64 KiB の file は複数段の Merkle layer になる
        let content: Vec<u8> = (0..64 * 1024).map(|i| (i % 251) as u8).collect();
        let max_rank = round_trip(&content).await?;
        assert!(max_rank >= 2);

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn file_with_duplicate_blocks_round_trip() -> TestResult {
        let content = vec![0u8; 16 * 1024];
        round_trip(&content).await?;

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn empty_file_round_trip() -> TestResult {
        round_trip(&[]).await?;

        Ok(())
    }

    /// publisher で公開した content を subscriber で復元し、復号の途中で観測した最大の rank を返す
    async fn round_trip(content: &[u8]) -> TestResult<u32> {
        let dir = tempfile::tempdir()?;
        let source_path = dir.path().join("source.bin");
        let output_path = dir.path().join("output.bin");
        tokio::fs::write(&source_path, content).await?;

        let clock: Arc<dyn Clock<Utc> + Send + Sync> = Arc::new(ClockUtc);
        let sleeper: Arc<dyn Sleeper + Send + Sync> = Arc::new(SleeperImpl);
        let tsid_provider: Arc<Mutex<dyn TsidProvider + Send + Sync>> = Arc::new(Mutex::new(TsidProviderImpl::new(ClockUtc, ChaCha20Rng::from_rng(&mut UnwrapErr(SysRng)), 8)));

        let publisher = FilePublisher::new(&create_dir(dir.path(), "publisher")?, tsid_provider.clone(), clock.clone(), sleeper.clone()).await?;
        let subscriber = FileSubscriber::new(&create_dir(dir.path(), "subscriber")?, tsid_provider, clock, sleeper).await?;

        publisher.import(source_path.to_str().unwrap(), "source.bin", BLOCK_SIZE, None, 0).await?;
        let root_hash = tokio::time::timeout(TEST_TIMEOUT, async {
            loop {
                if let Some(root_hash) = publisher.get_published_root_hashes().await?.pop() {
                    return Result::Ok(root_hash);
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await??;

        let id = subscriber.subscribe(&root_hash, output_path.to_str().unwrap(), None, 0).await?;

        // FileExchanger の代わりに、subscriber が待っている block を publisher から読んで渡す
        let mut max_rank = 0;
        tokio::time::timeout(TEST_TIMEOUT, async {
            loop {
                let file = subscriber.file_subscriber_repo.find_file_by_id(&id).await?.unwrap();
                if file.rank != SubscribedFile::UNKNOWN_ROOT_RANK {
                    max_rank = max_rank.max(file.rank);
                }
                match file.status {
                    SubscribedFileStatus::Completed => return Result::Ok(()),
                    SubscribedFileStatus::Downloading => {
                        for block in subscriber.file_subscriber_repo.find_blocks_by_root_hash_and_rank(&root_hash, file.rank).await? {
                            if block.downloaded {
                                continue;
                            }
                            let value = publisher.read_block(&root_hash, &block.block_hash).await?.unwrap();
                            subscriber.write_block(&root_hash, &block.block_hash, value).await?;
                        }
                    }
                    SubscribedFileStatus::Decoding => {}
                    _ => return Err(Error::new(ErrorKind::UnexpectedError).with_message(format!("decode failed: {:?}", file.failed_reason))),
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await??;

        assert_eq!(tokio::fs::read(&output_path).await?, content);

        publisher.shutdown().await;
        subscriber.shutdown().await;

        Ok(max_rank)
    }

    fn create_dir(base: &Path, name: &str) -> Result<std::path::PathBuf> {
        let path = base.join(name);
        std::fs::create_dir_all(&path)?;
        Ok(path)
    }
}
