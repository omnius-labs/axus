use super::*;
use crate::{base::runtime::Shutdown, prelude::*};
use async_trait::async_trait;
use chrono::Utc;
use omnius_core_base::{clock::Clock, sleeper::Sleeper, tsid::TsidProvider};
use omnius_core_omnikit::generated::omni_hash::OmniHash;
use parking_lot::Mutex;
use std::{path::Path, sync::Arc};
use tokio_util::bytes::Bytes;
mod output_publication;
mod repo;
mod store;
mod task_decoder;
mod util;
use self::{store::FileSubscriberStore, task_decoder::TaskDecoder};

pub struct FileSubscriber {
    store: Arc<FileSubscriberStore>,
    task_decoder: Arc<TaskDecoder>,
}

#[async_trait]
impl Shutdown for FileSubscriber {
    async fn shutdown(&self) {
        self.task_decoder.shutdown().await;
    }
}

#[allow(unused)]
impl FileSubscriber {
    pub async fn new(
        state_dir: &Path,
        tsid_provider: Arc<Mutex<dyn TsidProvider + Send + Sync>>,
        clock: Arc<dyn Clock<Utc> + Send + Sync>,
        sleeper: Arc<dyn Sleeper + Send + Sync>,
    ) -> Result<Arc<Self>> {
        let store = FileSubscriberStore::open(state_dir, tsid_provider, clock.clone()).await?;
        let task_decoder = TaskDecoder::new(store.clone(), clock, sleeper).await?;
        Ok(Arc::new(Self { store, task_decoder }))
    }
    pub async fn subscribe(&self, root_hash: &OmniHash, file_path: &str, attrs: Option<&str>, priority: i64) -> Result<String> {
        let id = self.store.subscribe(root_hash, file_path, attrs, priority).await?;
        self.task_decoder.wake();
        Ok(id)
    }
    pub async fn cancel(&self, id: &str) -> Result<()> {
        self.store.cancel(id).await?;
        self.task_decoder.cancel(id).await;
        self.store.discard_output(id).await
    }
    pub async fn remove(&self, id: &str) -> Result<()> {
        self.store.cancel(id).await?;
        self.task_decoder.cancel(id).await;
        self.store.remove(id).await
    }
    pub async fn write_block(&self, root_hash: &OmniHash, block_hash: &OmniHash, bytes: Bytes) -> Result<()> {
        if self.store.write_block(root_hash, block_hash, bytes).await? {
            self.task_decoder.wake();
        }
        Ok(())
    }
    pub async fn get_subscribed_root_hashes(&self) -> Result<Vec<OmniHash>> {
        self.store.get_subscribed_root_hashes().await
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

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn duplicate_subscriptions_round_trip() -> TestResult {
        let content: Vec<u8> = (0..16 * 1024).map(|i| (i % 251) as u8).collect();
        round_trip_with_subscriptions(&content, true).await?;
        Ok(())
    }

    /// publisher で公開した content を subscriber で復元し、復号の途中で観測した最大の rank を返す
    async fn round_trip(content: &[u8]) -> TestResult<u32> {
        round_trip_with_subscriptions(content, false).await
    }

    async fn round_trip_with_subscriptions(content: &[u8], duplicate: bool) -> TestResult<u32> {
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

        let second_output = dir.path().join("second.bin");
        let second_id = if duplicate {
            Some(subscriber.subscribe(&root_hash, second_output.to_str().unwrap(), None, 0).await?)
        } else {
            None
        };

        // FileExchanger の代わりに、subscriber が待っている block を publisher から読んで渡す
        let mut max_rank = 0;
        tokio::time::timeout(TEST_TIMEOUT, async {
            loop {
                let file = subscriber.store.find_file(&id).await?.unwrap();
                if file.rank != SubscribedFile::UNKNOWN_ROOT_RANK {
                    max_rank = max_rank.max(file.rank);
                }
                match file.status {
                    SubscribedFileStatus::Completed => return Result::Ok(()),
                    SubscribedFileStatus::Downloading => {
                        for block in subscriber.store.blocks(&root_hash, file.rank).await? {
                            if block.downloaded {
                                continue;
                            }
                            let value = publisher.read_block(&root_hash, &block.block_hash).await?.unwrap();
                            subscriber.write_block(&root_hash, &block.block_hash, value).await?;
                        }
                    }
                    SubscribedFileStatus::Decoding | SubscribedFileStatus::Finalizing => {}
                    _ => return Err(Error::new(ErrorKind::UnexpectedError).with_message(format!("decode failed: {:?}", file.failed_reason))),
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await??;

        assert_eq!(tokio::fs::read(&output_path).await?, content);

        if let Some(second_id) = second_id {
            wait_completed(&subscriber, &second_id).await?;
            assert_eq!(tokio::fs::read(&second_output).await?, content);
            // All ranks are already cached: a later subscription needs no received blocks.
            let third_output = dir.path().join("third.bin");
            let third_id = subscriber.subscribe(&root_hash, third_output.to_str().unwrap(), None, 0).await?;
            wait_completed(&subscriber, &third_id).await?;
            assert_eq!(tokio::fs::read(&third_output).await?, content);
            subscriber.remove(&id).await?;
            subscriber.remove(&second_id).await?;
            subscriber.remove(&third_id).await?;
            assert_eq!(tokio::fs::read(&output_path).await?, content);
            assert_eq!(tokio::fs::read(&second_output).await?, content);
            assert_eq!(tokio::fs::read(&third_output).await?, content);
        }

        publisher.shutdown().await;
        subscriber.shutdown().await;

        Ok(max_rank)
    }

    async fn wait_completed(subscriber: &FileSubscriber, id: &str) -> Result<()> {
        tokio::time::timeout(TEST_TIMEOUT, async {
            loop {
                let file = subscriber.store.find_file(id).await?.unwrap();
                match file.status {
                    SubscribedFileStatus::Completed => return Result::Ok(()),
                    SubscribedFileStatus::Failed | SubscribedFileStatus::Canceled => {
                        return Err(Error::new(ErrorKind::UnexpectedError).with_message(format!("decode failed: {:?}", file.failed_reason)));
                    }
                    _ => tokio::time::sleep(Duration::from_millis(5)).await,
                }
            }
        })
        .await
        .map_err(|error| Error::new(ErrorKind::UnexpectedError).with_message(error.to_string()))?
    }

    fn create_dir(base: &Path, name: &str) -> Result<std::path::PathBuf> {
        let path = base.join(name);
        std::fs::create_dir_all(&path)?;
        Ok(path)
    }
}
