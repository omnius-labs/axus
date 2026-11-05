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

    #[allow(clippy::too_many_arguments)]
    pub async fn import_owned(&self, id: &str, source_path: &str, file_name: &str, size: i64, block_size: u32, attrs: Option<&str>, priority: i64) -> Result<()> {
        self.store.import_owned(id, source_path, file_name, size, block_size, attrs, priority).await?;
        self.task_encoder.wake();
        Ok(())
    }

    pub async fn get_publish(&self, id: &str) -> Result<Option<PublishRecord>> {
        self.store.get_publish(id).await
    }

    pub async fn list_publishes(&self) -> Result<Vec<PublishRecord>> {
        self.store.list_publishes().await
    }

    pub async fn encoded_block_count(&self, id: &str) -> Result<i64> {
        self.store.encoded_block_count(id).await
    }

    pub async fn remove_publish(&self, id: &str) -> Result<()> {
        self.task_encoder.cancel(id);
        self.store.remove_publish(id).await
    }

    pub async fn cancel(&self, id: &str) -> Result<bool> {
        self.task_encoder.cancel(id);
        let removed = self.store.cancel(id).await?;
        if removed {
            self.store.cleanup_blocks(id).await?;
        }
        Ok(removed)
    }
    pub async fn remove(&self, root_hash: &OmniHash, file_name: &str) -> Result<()> {
        self.store.remove(root_hash, file_name).await
    }
    pub async fn read_block(&self, root_hash: &OmniHash, block_hash: &OmniHash) -> Result<Option<Bytes>> {
        self.store.read_block(root_hash, block_hash).await
    }
    pub async fn contains_blocks(&self, root_hash: &OmniHash, block_hashes: &[OmniHash]) -> Result<Vec<bool>> {
        self.store.contains_blocks(root_hash, block_hashes).await
    }
    pub async fn get_published_root_hashes(&self) -> Result<Vec<OmniHash>> {
        self.store.get_published_root_hashes().await
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use chrono::Utc;
    use omnius_core_base::{
        clock::ClockUtc,
        sleeper::SleeperImpl,
        tsid::{TsidProvider, TsidProviderImpl},
    };
    use parking_lot::Mutex;
    use rand::{
        SeedableRng as _,
        rngs::{ChaCha20Rng, SysRng},
    };
    use rand_core::UnwrapErr;
    use testresult::TestResult;

    use super::*;

    const BLOCK_SIZE: u32 = 256;

    fn publisher_parts() -> (
        Arc<Mutex<dyn TsidProvider + Send + Sync>>,
        Arc<dyn Clock<Utc> + Send + Sync>,
        Arc<dyn Sleeper + Send + Sync>,
    ) {
        let rng = ChaCha20Rng::from_rng(&mut UnwrapErr(SysRng));
        let ids: Arc<Mutex<dyn TsidProvider + Send + Sync>> = Arc::new(Mutex::new(TsidProviderImpl::new(ClockUtc, rng, 8)));
        (ids, Arc::new(ClockUtc), Arc::new(SleeperImpl))
    }

    #[tokio::test]
    pub async fn imported_source_is_moved_and_removed_after_commit_test() -> TestResult {
        let dir = tempfile::tempdir()?;
        let source = dir.path().join("upload.bin");
        let content: Vec<u8> = (0..1024).map(|index| (index % 251) as u8).collect();
        std::fs::write(&source, &content)?;
        let state_dir = dir.path().join("publisher");

        let (ids, clock, sleeper) = publisher_parts();
        let publisher = FilePublisher::new(&state_dir, ids, clock, sleeper).await?;
        publisher
            .import_owned("publish-1", source.to_str().unwrap(), "file.bin", content.len() as i64, BLOCK_SIZE, None, 0)
            .await?;
        assert!(!source.exists());

        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        loop {
            match publisher.get_publish("publish-1").await? {
                Some(PublishRecord::Committed(file)) => {
                    assert_eq!(file.source_id.as_deref(), Some("publish-1"));
                    assert_eq!(file.size, Some(content.len() as i64));
                    assert!(publisher.encoded_block_count("publish-1").await? > 0);
                    break;
                }
                Some(PublishRecord::Uncommitted(_)) => {}
                None => panic!("the publish disappeared"),
            }
            if tokio::time::Instant::now() >= deadline {
                panic!("the publish did not commit");
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(std::fs::read_dir(state_dir.join("sources"))?.next().is_none());
        publisher.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    pub async fn cancel_removes_the_owned_source_test() -> TestResult {
        let dir = tempfile::tempdir()?;
        let source = dir.path().join("upload.bin");
        std::fs::write(&source, [0u8; 8])?;
        let state_dir = dir.path().join("publisher");

        let (ids, clock, sleeper) = publisher_parts();
        let publisher = FilePublisher::new(&state_dir, ids, clock, sleeper).await?;
        publisher.import_owned("publish-2", source.to_str().unwrap(), "file.bin", 8, BLOCK_SIZE, None, 0).await?;
        assert!(state_dir.join("sources").join("publish-2").exists());

        publisher.cancel("publish-2").await?;
        assert!(!state_dir.join("sources").join("publish-2").exists());
        assert!(publisher.get_publish("publish-2").await?.is_none());
        publisher.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    pub async fn orphan_source_is_removed_on_open_test() -> TestResult {
        let dir = tempfile::tempdir()?;
        let state_dir = dir.path().join("publisher");
        std::fs::create_dir_all(state_dir.join("sources"))?;
        std::fs::write(state_dir.join("sources").join("orphan"), [0u8; 4])?;

        let (ids, clock, sleeper) = publisher_parts();
        let publisher = FilePublisher::new(&state_dir, ids, clock, sleeper).await?;
        assert!(!state_dir.join("sources").join("orphan").exists());
        publisher.shutdown().await;
        Ok(())
    }
}
