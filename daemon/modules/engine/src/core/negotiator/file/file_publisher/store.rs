use std::{collections::HashSet, path::Path, sync::Arc};

use chrono::Utc;
use parking_lot::Mutex;
use tokio::sync::Mutex as TokioMutex;
use tokio_util::bytes::Bytes;

use omnius_core_base::{clock::Clock, tsid::TsidProvider};
use omnius_core_omnikit::generated::omni_hash::OmniHash;

use crate::base::storage::KeyValueRocksdbStorage;

use super::{repo::FilePublisherRepo, *};

pub struct FilePublisherStore {
    repo: FilePublisherRepo,
    blocks_storage: KeyValueRocksdbStorage,
    lock: TokioMutex<()>,
    tsid_provider: Arc<Mutex<dyn TsidProvider + Send + Sync>>,
    clock: Arc<dyn Clock<Utc> + Send + Sync>,
}

impl FilePublisherStore {
    pub async fn open(state_dir: &Path, tsid_provider: Arc<Mutex<dyn TsidProvider + Send + Sync>>, clock: Arc<dyn Clock<Utc> + Send + Sync>) -> Result<Arc<Self>> {
        let repo_dir = state_dir.join("repo");
        tokio::fs::create_dir_all(&repo_dir).await?;
        let store = Arc::new(Self {
            repo: FilePublisherRepo::new(&repo_dir, clock.clone()).await?,
            blocks_storage: KeyValueRocksdbStorage::new(state_dir.join("blocks"), tsid_provider.clone()).await?,
            lock: TokioMutex::new(()),
            tsid_provider,
            clock,
        });
        {
            let _guard = store.lock.lock().await;
            let roots: HashSet<String> = store.repo.get_committed_files().await?.into_iter().map(|file| file.root_hash.to_string()).collect();
            store
                .blocks_storage
                .shrink(move |key| {
                    let Ok(key) = std::str::from_utf8(key) else {
                        return false;
                    };
                    key.strip_prefix("C/").and_then(|key| key.split('/').next()).is_some_and(|root| roots.contains(root))
                })
                .await?;
            store.repo.recover().await?;
        }
        Ok(store)
    }

    pub async fn import(&self, file_path: &str, file_name: &str, block_size: u32, attrs: Option<&str>, priority: i64) -> Result<String> {
        if block_size == 0 {
            return Err(Error::new(ErrorKind::InvalidFormat).with_message("block size must be nonzero"));
        }
        let _guard = self.lock.lock().await;
        let existing = self
            .repo
            .get_uncommitted_files()
            .await?
            .into_iter()
            .find(|file| file.file_path == file_path && file.file_name == file_name);
        let id = match &existing {
            Some(file) if file.status == PublishedUncommittedFileStatus::Failed => file.id.clone(),
            Some(_) => return Err(Error::new(ErrorKind::AlreadyExists)),
            None => self.tsid_provider.lock().create().to_string(),
        };
        let now = self.clock.now();
        let file = PublishedUncommittedFile {
            id: id.clone(),
            file_path: file_path.to_string(),
            file_name: file_name.to_string(),
            block_size,
            attrs: attrs.map(str::to_string),
            priority,
            status: PublishedUncommittedFileStatus::Pending,
            failed_reason: None,
            created_at: now,
            updated_at: now,
        };
        if existing.is_some() {
            self.repo.retry_file(&file).await?;
        } else {
            self.repo.insert_uncommitted_file(&file).await?;
        }
        Ok(id)
    }

    pub async fn next_file(&self) -> Result<Option<PublishedUncommittedFile>> {
        self.repo.find_uncommitted_file_by_encoding_next().await
    }

    pub async fn claim(&self, id: &str) -> Result<Option<PublishedUncommittedFile>> {
        let _guard = self.lock.lock().await;
        let Some(mut file) = self.repo.find_uncommitted_file_by_id(id).await? else {
            return Ok(None);
        };
        if file.status != PublishedUncommittedFileStatus::Pending {
            return Ok(None);
        }
        if !self
            .repo
            .update_uncommitted_file_status(id, &PublishedUncommittedFileStatus::Pending, &PublishedUncommittedFileStatus::Processing)
            .await?
        {
            return Ok(None);
        }
        file.status = PublishedUncommittedFileStatus::Processing;
        Ok(Some(file))
    }

    pub async fn write_block(&self, block: &PublishedUncommittedBlock, bytes: Bytes) -> Result<bool> {
        let _guard = self.lock.lock().await;
        if !self
            .repo
            .find_uncommitted_file_by_id(&block.file_id)
            .await?
            .is_some_and(|file| file.status == PublishedUncommittedFileStatus::Processing)
        {
            return Ok(false);
        }
        self.repo.insert_or_ignore_uncommitted_block(block).await?;
        self.blocks_storage
            .put_value(util::gen_uncommitted_block_path(&block.file_id, &block.block_hash), bytes, true)
            .await?;
        Ok(true)
    }

    pub async fn fail(&self, id: &str, reason: &str) -> Result<()> {
        let _guard = self.lock.lock().await;
        self.repo.update_uncommitted_file_as_failed(id, reason).await
    }

    pub async fn cancel(&self, id: &str) -> Result<()> {
        let _guard = self.lock.lock().await;
        self.repo.delete_uncommitted_file(id).await
    }

    pub async fn cleanup_blocks(&self, id: &str) -> Result<()> {
        let _guard = self.lock.lock().await;
        self.cleanup_blocks_locked(id).await
    }

    async fn cleanup_blocks_locked(&self, id: &str) -> Result<()> {
        self.delete_prefix(&format!("U/{id}/")).await?;
        let blocks = self.repo.find_uncommitted_blocks_by_file_id(id).await?;
        self.repo.delete_uncommitted_blocks(&blocks).await
    }

    async fn delete_prefix(&self, prefix: &str) -> Result<()> {
        let keys: Vec<_> = self.blocks_storage.get_keys()?.filter(|key| key.starts_with(prefix.as_bytes())).collect();
        for key in keys {
            self.blocks_storage.delete(key).await?;
        }
        Ok(())
    }

    pub async fn commit(&self, file: &PublishedUncommittedFile, root_hash: &OmniHash, blocks: &[PublishedUncommittedBlock]) -> Result<bool> {
        let _guard = self.lock.lock().await;
        if !self
            .repo
            .find_uncommitted_file_by_id(&file.id)
            .await?
            .is_some_and(|file| file.status == PublishedUncommittedFileStatus::Processing)
        {
            return Ok(false);
        }
        let now = self.clock.now();
        let committed = PublishedCommittedFile {
            root_hash: root_hash.clone(),
            file_name: file.file_name.clone(),
            block_size: file.block_size,
            attrs: file.attrs.clone(),
            created_at: now,
            updated_at: now,
        };
        if self.repo.contains_committed_file(root_hash).await? {
            self.cleanup_blocks_locked(&file.id).await?;
            self.repo.commit_file_without_blocks(&committed, &file.id).await?;
        } else {
            // A previous failed rename may have left keys without a committed row.
            self.delete_prefix(&format!("C/{root_hash}/")).await?;
            let hashes: HashSet<_> = blocks.iter().map(|block| &block.block_hash).collect();
            for hash in hashes {
                self.blocks_storage
                    .rename_key(util::gen_uncommitted_block_path(&file.id, hash), util::gen_committed_block_path(root_hash, hash), false)
                    .await?;
            }
            let committed_blocks: Vec<_> = blocks
                .iter()
                .map(|block| PublishedCommittedBlock {
                    root_hash: root_hash.clone(),
                    block_hash: block.block_hash.clone(),
                    rank: block.rank,
                    index: block.index,
                })
                .collect();
            self.repo.commit_file_with_blocks(&committed, &committed_blocks, &file.id).await?;
        }
        Ok(true)
    }

    pub async fn remove(&self, root_hash: &OmniHash, file_name: &str) -> Result<()> {
        let _guard = self.lock.lock().await;
        self.repo.remove_committed_file(root_hash, file_name).await?;
        if !self.repo.contains_committed_file(root_hash).await? {
            self.delete_prefix(&format!("C/{root_hash}/")).await?;
        }
        Ok(())
    }

    pub async fn read_block(&self, root_hash: &OmniHash, block_hash: &OmniHash) -> Result<Option<Bytes>> {
        Ok(self.blocks_storage.get_value(util::gen_committed_block_path(root_hash, block_hash)).await?.map(Bytes::from))
    }

    pub async fn get_published_root_hashes(&self) -> Result<Vec<OmniHash>> {
        Ok(self
            .repo
            .get_committed_files()
            .await?
            .into_iter()
            .map(|file| file.root_hash)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use omnius_core_base::{
        clock::{Clock, ClockUtc},
        sleeper::SleeperImpl,
        tsid::{TsidProvider, TsidProviderImpl},
    };
    use omnius_core_omnikit::generated::omni_hash::OmniHashAlgorithmType;
    use parking_lot::Mutex;
    use rand::{
        SeedableRng as _,
        rngs::{ChaCha20Rng, SysRng},
    };
    use rand_core::UnwrapErr;
    use std::{sync::Arc, time::Duration};
    use testresult::TestResult;

    fn dependencies() -> (Arc<Mutex<dyn TsidProvider + Send + Sync>>, Arc<dyn Clock<Utc> + Send + Sync>) {
        (
            Arc::new(Mutex::new(TsidProviderImpl::new(ClockUtc, ChaCha20Rng::from_rng(&mut UnwrapErr(SysRng)), 8))),
            Arc::new(ClockUtc),
        )
    }

    async fn commit_dummy(store: &FilePublisherStore, id: &str) -> TestResult<OmniHash> {
        let file = store.claim(id).await?.unwrap();
        let hash = OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, b"block");
        let block = PublishedUncommittedBlock {
            file_id: id.to_string(),
            block_hash: hash.clone(),
            rank: 0,
            index: 0,
        };
        assert!(store.write_block(&block, Bytes::from_static(b"block")).await?);
        assert!(store.commit(&file, &hash, &[block]).await?);
        Ok(hash)
    }

    #[tokio::test]
    async fn reimport_after_commit_cancel_and_failure() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (ids, clock) = dependencies();
        let store = FilePublisherStore::open(dir.path(), ids, clock).await?;
        let first = store.import("source", "a", 256, None, 0).await?;
        assert_eq!(store.import("source", "a", 256, None, 0).await.unwrap_err().kind(), &ErrorKind::AlreadyExists);
        let root = commit_dummy(&store, &first).await?;
        assert!(store.repo.find_uncommitted_file_by_id(&first).await?.is_none());
        // The same name already committed must also remove its uncommitted row and keys.
        let second = store.import("source", "a", 256, None, 0).await?;
        assert_eq!(commit_dummy(&store, &second).await?, root);
        assert!(store.repo.find_uncommitted_file_by_id(&second).await?.is_none());
        assert!(store.blocks_storage.get_keys()?.all(|key| !key.starts_with(b"U/")));
        let third = store.import("source", "a", 256, None, 0).await?;
        store.cancel(&third).await?;
        let fourth = store.import("source", "a", 256, None, 0).await?;
        store.claim(&fourth).await?;
        assert_eq!(store.import("source", "a", 256, None, 0).await.unwrap_err().kind(), &ErrorKind::AlreadyExists);
        store.fail(&fourth, "failure").await?;
        let retry = store.import("source", "a", 512, Some("retry"), 5).await?;
        assert_eq!(retry, fourth);
        let file = store.repo.find_uncommitted_file_by_id(&retry).await?.unwrap();
        assert!(file.status == PublishedUncommittedFileStatus::Pending);
        assert!(file.failed_reason.is_none());
        assert_eq!(file.block_size, 512);
        assert_eq!(file.attrs.as_deref(), Some("retry"));
        store.cancel(&retry).await?;
        store.fail(&retry, "late failure").await?;
        assert!(store.repo.find_uncommitted_file_by_id(&retry).await?.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn remove_preserves_shared_root_until_last_name() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (ids, clock) = dependencies();
        let store = FilePublisherStore::open(dir.path(), ids, clock).await?;
        let first = store.import("source", "a", 256, None, 0).await?;
        let root = commit_dummy(&store, &first).await?;
        let second = store.import("source", "b", 256, None, 0).await?;
        assert_eq!(commit_dummy(&store, &second).await?, root);
        store.remove(&root, "a").await?;
        assert_eq!(store.read_block(&root, &root).await?, Some(Bytes::from_static(b"block")));
        assert_eq!(store.repo.get_committed_files().await?.len(), 1);
        store.remove(&root, "b").await?;
        assert!(store.read_block(&root, &root).await?.is_none());
        assert!(!store.repo.contains_committed_block(&root, &root).await?);
        assert!(store.get_published_root_hashes().await?.is_empty());
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn interrupted_commit_recovers_and_republishes() -> TestResult {
        let dir = tempfile::tempdir()?;
        let state = dir.path().join("publisher");
        let source = dir.path().join("source");
        tokio::fs::write(&source, b"content").await?;
        let (ids, clock) = dependencies();
        let store = FilePublisherStore::open(&state, ids.clone(), clock.clone()).await?;
        let id = store.import(source.to_str().unwrap(), "source", 256, None, 0).await?;
        store.claim(&id).await?;
        let leaf = OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, b"content");
        let layer = MerkleLayer {
            rank: 0,
            hashes: vec![leaf.clone()],
        }
        .export()?;
        let root = OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, &layer);
        for (hash, rank, bytes) in [(leaf.clone(), 0, Bytes::from_static(b"content")), (root.clone(), 1, Bytes::from(layer))] {
            store
                .write_block(
                    &PublishedUncommittedBlock {
                        file_id: id.clone(),
                        block_hash: hash,
                        rank,
                        index: 0,
                    },
                    bytes,
                )
                .await?;
        }
        // Stop after one rename, before SQLite records the committed file.
        store
            .blocks_storage
            .rename_key(util::gen_uncommitted_block_path(&id, &root), util::gen_committed_block_path(&root, &root), false)
            .await?;
        drop(store);
        let store = FilePublisherStore::open(&state, ids.clone(), clock.clone()).await?;
        assert!(store.blocks_storage.get_keys()?.next().is_none());
        assert!(store.repo.find_uncommitted_blocks_by_file_id(&id).await?.is_empty());
        assert!(store.repo.find_uncommitted_file_by_id(&id).await?.unwrap().status == PublishedUncommittedFileStatus::Pending);
        drop(store);
        let publisher = FilePublisher::new(&state, ids, clock, Arc::new(SleeperImpl)).await?;
        tokio::time::timeout(Duration::from_secs(30), async {
            while publisher.get_published_root_hashes().await?.is_empty() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            Result::Ok(())
        })
        .await??;
        assert_eq!(publisher.read_block(&root, &leaf).await?, Some(Bytes::from_static(b"content")));
        publisher.shutdown().await;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn cancel_during_encode_cleans_own_blocks() -> TestResult {
        let dir = tempfile::tempdir()?;
        let source = dir.path().join("source");
        tokio::fs::write(&source, vec![42; 1024 * 1024]).await?;
        let (ids, clock) = dependencies();
        let publisher = FilePublisher::new(&dir.path().join("publisher"), ids, clock, Arc::new(SleeperImpl)).await?;
        let id = publisher.import(source.to_str().unwrap(), "source", 256, None, 0).await?;
        tokio::time::timeout(Duration::from_secs(30), async {
            while publisher.store.repo.find_uncommitted_blocks_by_file_id(&id).await?.is_empty() {
                tokio::task::yield_now().await;
            }
            Result::Ok(())
        })
        .await??;
        publisher.cancel(&id).await?;
        assert!(publisher.store.repo.find_uncommitted_file_by_id(&id).await?.is_none());
        assert!(publisher.get_published_root_hashes().await?.is_empty());
        tokio::fs::write(&source, b"retry").await?;
        let retry = publisher.import(source.to_str().unwrap(), "source", 256, None, 0).await?;
        assert_ne!(id, retry);
        wait_published(&publisher).await?;
        publisher.shutdown().await;
        assert!(publisher.store.repo.find_uncommitted_file_by_id(&retry).await?.is_none());
        assert!(publisher.store.blocks_storage.get_keys()?.all(|key| !key.starts_with(b"U/")));
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn failed_import_reuses_id_and_wakes_encoder() -> TestResult {
        let dir = tempfile::tempdir()?;
        let source = dir.path().join("source");
        let (ids, clock) = dependencies();
        let publisher = FilePublisher::new(&dir.path().join("publisher"), ids, clock, Arc::new(SleeperImpl)).await?;
        let id = publisher.import(source.to_str().unwrap(), "source", 256, None, 0).await?;
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                if publisher
                    .store
                    .repo
                    .find_uncommitted_file_by_id(&id)
                    .await?
                    .is_some_and(|file| file.status == PublishedUncommittedFileStatus::Failed)
                {
                    return Result::Ok(());
                }
                tokio::task::yield_now().await;
            }
        })
        .await??;
        tokio::fs::write(&source, b"retry").await?;
        assert_eq!(publisher.import(source.to_str().unwrap(), "source", 256, None, 0).await?, id);
        wait_published(&publisher).await?;
        publisher.shutdown().await;
        assert!(publisher.store.repo.find_uncommitted_file_by_id(&id).await?.is_none());
        Ok(())
    }

    async fn wait_published(publisher: &FilePublisher) -> Result<()> {
        tokio::time::timeout(Duration::from_secs(30), async {
            while publisher.get_published_root_hashes().await?.is_empty() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            Result::Ok(())
        })
        .await
        .map_err(|error| Error::new(ErrorKind::UnexpectedError).with_message(error.to_string()))?
    }
}
