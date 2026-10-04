use std::{collections::HashSet, path::Path, sync::Arc};

use chrono::Utc;
use parking_lot::Mutex;
use tokio::sync::RwLock;
use tokio_util::bytes::Bytes;

use omnius_core_base::{clock::Clock, tsid::TsidProvider};
use omnius_core_omnikit::generated::omni_hash::OmniHash;

use crate::base::storage::KeyValueRocksdbStorage;

use super::{repo::FileSubscriberRepo, *};

pub struct FileSubscriberStore {
    repo: FileSubscriberRepo,
    blocks_storage: KeyValueRocksdbStorage,
    lock: RwLock<()>,
    tsid_provider: Arc<Mutex<dyn TsidProvider + Send + Sync>>,
    clock: Arc<dyn Clock<Utc> + Send + Sync>,
}

impl FileSubscriberStore {
    pub async fn open(state_dir: &Path, tsid_provider: Arc<Mutex<dyn TsidProvider + Send + Sync>>, clock: Arc<dyn Clock<Utc> + Send + Sync>) -> Result<Arc<Self>> {
        let repo_dir = state_dir.join("repo");
        tokio::fs::create_dir_all(&repo_dir).await?;
        let store = Arc::new(Self {
            repo: FileSubscriberRepo::new(&repo_dir, clock.clone()).await?,
            blocks_storage: KeyValueRocksdbStorage::new(state_dir.join("blocks"), tsid_provider.clone()).await?,
            lock: RwLock::new(()),
            tsid_provider,
            clock,
        });
        {
            let _guard = store.lock.write().await;
            let roots: HashSet<_> = store.repo.get_committed_files().await?.into_iter().map(|file| file.root_hash.to_string()).collect();
            store
                .blocks_storage
                .shrink(move |key| std::str::from_utf8(key).ok().and_then(|key| key.split('/').next()).is_some_and(|root| roots.contains(root)))
                .await?;
        }
        Ok(store)
    }

    pub async fn subscribe(&self, root_hash: &OmniHash, file_path: &str, attrs: Option<&str>, priority: i64) -> Result<String> {
        let _guard = self.lock.read().await;
        let id = self.tsid_provider.lock().create().to_string();
        let now = self.clock.now();
        let file = SubscribedFile {
            id: id.clone(),
            root_hash: root_hash.clone(),
            file_path: file_path.to_string(),
            rank: SubscribedFile::UNKNOWN_ROOT_RANK,
            block_count_downloaded: 0,
            block_count_total: 1,
            attrs: attrs.map(str::to_string),
            priority,
            status: SubscribedFileStatus::Downloading,
            failed_reason: None,
            created_at: now,
            updated_at: now,
        };
        let block = SubscribedBlock {
            root_hash: root_hash.clone(),
            block_hash: root_hash.clone(),
            rank: SubscribedFile::UNKNOWN_ROOT_RANK,
            index: 0,
            downloaded: false,
        };
        self.repo.insert_file_and_blocks(&file, &[block]).await?;
        Ok(id)
    }

    pub async fn write_block(&self, root_hash: &OmniHash, block_hash: &OmniHash, bytes: Bytes) -> Result<bool> {
        let _guard = self.lock.read().await;
        if self.repo.find_blocks_by_root_hash_and_block_hash(root_hash, block_hash).await?.is_empty() {
            return Ok(false);
        }
        self.blocks_storage.put_value(util::gen_block_path(root_hash, block_hash), bytes, true).await?;
        self.repo.mark_downloaded(root_hash, block_hash).await
    }

    pub async fn cancel(&self, id: &str) -> Result<()> {
        self.repo.cancel(id).await
    }

    pub async fn remove(&self, id: &str) -> Result<()> {
        let _guard = self.lock.write().await;
        let Some(file) = self.repo.find_file_by_id(id).await? else {
            return Ok(());
        };
        self.repo.delete_file(id).await?;
        if self.repo.find_file_by_root_hash(&file.root_hash).await?.is_none() {
            let prefix = format!("{}/", file.root_hash);
            let keys: Vec<_> = self.blocks_storage.get_keys()?.filter(|key| key.starts_with(prefix.as_bytes())).collect();
            for key in keys {
                self.blocks_storage.delete(key).await?;
            }
        }
        Ok(())
    }

    pub async fn find_file(&self, id: &str) -> Result<Option<SubscribedFile>> {
        self.repo.find_file_by_id(id).await
    }
    pub async fn next_file(&self) -> Result<Option<SubscribedFile>> {
        self.repo.find_file_by_decoding_next().await
    }
    pub async fn blocks(&self, root_hash: &OmniHash, rank: u32) -> Result<Vec<SubscribedBlock>> {
        self.repo.find_blocks_by_root_hash_and_rank(root_hash, rank).await
    }
    pub async fn get_subscribed_root_hashes(&self) -> Result<Vec<OmniHash>> {
        self.repo.get_downloading_root_hashes().await
    }

    pub async fn read_block(&self, root_hash: &OmniHash, block_hash: &OmniHash) -> Result<Option<Vec<u8>>> {
        self.blocks_storage.get_value(util::gen_block_path(root_hash, block_hash)).await
    }

    pub async fn advance_layer(&self, file: &SubscribedFile, layer: MerkleLayer) -> Result<bool> {
        let _guard = self.lock.read().await;
        let mut blocks = Vec::new();
        for (index, hash) in layer.hashes.into_iter().enumerate() {
            let downloaded = self.blocks_storage.contains_key(util::gen_block_path(&file.root_hash, &hash)).await?;
            blocks.push(SubscribedBlock {
                root_hash: file.root_hash.clone(),
                block_hash: hash,
                rank: layer.rank,
                index: index as u32,
                downloaded,
            });
        }
        let next = SubscribedFile {
            rank: layer.rank,
            block_count_total: blocks.len() as u32,
            ..file.clone()
        };
        self.repo.advance_layer(&next, &blocks).await
    }

    pub async fn complete(&self, id: &str) -> Result<bool> {
        self.repo.transition(id, &SubscribedFileStatus::Decoding, &SubscribedFileStatus::Completed, None).await
    }
    pub async fn fail(&self, id: &str, reason: &str) -> Result<bool> {
        self.repo.transition(id, &SubscribedFileStatus::Decoding, &SubscribedFileStatus::Failed, Some(reason)).await
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

    #[tokio::test]
    async fn duplicate_subscriptions_share_progress_and_remove() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (ids, clock) = dependencies();
        let store = FileSubscriberStore::open(dir.path(), ids, clock).await?;
        let root = OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, b"root");
        let first = store.subscribe(&root, "a", None, 0).await?;
        let second = store.subscribe(&root, "b", None, 0).await?;
        assert_eq!(store.get_subscribed_root_hashes().await?, vec![root.clone()]);
        assert!(store.write_block(&root, &root, Bytes::from_static(b"root")).await?);
        for id in [&first, &second] {
            let file = store.find_file(id).await?.unwrap();
            assert!(file.status == SubscribedFileStatus::Decoding);
            assert_eq!(file.block_count_downloaded, 1);
        }
        assert!(store.get_subscribed_root_hashes().await?.is_empty());
        let third = store.subscribe(&root, "c", None, 0).await?;
        assert!(store.find_file(&third).await?.unwrap().status == SubscribedFileStatus::Decoding);
        assert!(store.blocks(&root, SubscribedFile::UNKNOWN_ROOT_RANK).await?[0].downloaded);
        store.cancel(&first).await?;
        assert!(!store.fail(&first, "late failure").await?);
        assert!(!store.complete(&first).await?);
        assert!(store.find_file(&first).await?.unwrap().status == SubscribedFileStatus::Canceled);
        assert!(
            !store
                .advance_layer(&store.find_file(&first).await?.unwrap(), MerkleLayer { rank: 0, hashes: vec![] })
                .await?
        );
        let stale = store.find_file(&first).await?.unwrap();
        store.remove(&first).await?;
        assert!(store.read_block(&root, &root).await?.is_some());
        assert!(!store.fail(&first, "late failure").await?);
        store.remove(&second).await?;
        store.remove(&third).await?;
        assert!(store.blocks(&root, SubscribedFile::UNKNOWN_ROOT_RANK).await?.is_empty());
        assert!(store.read_block(&root, &root).await?.is_none());
        assert!(!store.advance_layer(&stale, MerkleLayer { rank: 0, hashes: vec![] }).await?);
        Ok(())
    }

    #[tokio::test]
    async fn reopen_removes_orphan_keys_and_preserves_referenced_blocks() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (ids, clock) = dependencies();
        let store = FileSubscriberStore::open(dir.path(), ids.clone(), clock.clone()).await?;
        let root = OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, b"root");
        let orphan = OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, b"orphan");
        store.subscribe(&root, "a", None, 0).await?;
        store.write_block(&root, &root, Bytes::from_static(b"root")).await?;
        store
            .blocks_storage
            .put_value(util::gen_block_path(&orphan, &orphan), Bytes::from_static(b"orphan"), true)
            .await?;
        drop(store);
        let store = FileSubscriberStore::open(dir.path(), ids, clock).await?;
        assert!(store.read_block(&root, &root).await?.is_some());
        assert!(store.read_block(&orphan, &orphan).await?.is_none());
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn cancel_during_decode_removes_partial_output() -> TestResult {
        interrupt_decode(false).await
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn remove_during_decode_removes_partial_output() -> TestResult {
        interrupt_decode(true).await
    }

    async fn interrupt_decode(remove: bool) -> TestResult {
        let dir = tempfile::tempdir()?;
        let output = dir.path().join("output");
        let (ids, clock) = dependencies();
        let store = FileSubscriberStore::open(&dir.path().join("subscriber"), ids, clock.clone()).await?;
        let bytes = Bytes::from(vec![7; 8192]);
        let hash = OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, &bytes);
        let root = OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, b"root");
        let id = store.subscribe(&root, output.to_str().unwrap(), None, 0).await?;
        store.write_block(&root, &root, Bytes::from_static(b"root")).await?;
        let file = store.find_file(&id).await?.unwrap();
        store
            .advance_layer(
                &file,
                MerkleLayer {
                    rank: 0,
                    hashes: vec![hash.clone(); 8192],
                },
            )
            .await?;
        store.write_block(&root, &hash, bytes).await?;
        let task_decoder = TaskDecoder::new(store.clone(), clock, Arc::new(SleeperImpl)).await?;
        let subscriber = FileSubscriber { store, task_decoder };
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                if tokio::fs::metadata(&output).await.is_ok_and(|meta| meta.len() > 0) {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await?;
        assert!(subscriber.store.find_file(&id).await?.unwrap().status == SubscribedFileStatus::Decoding);
        if remove {
            subscriber.remove(&id).await?;
        } else {
            subscriber.cancel(&id).await?;
        }
        subscriber.shutdown().await;
        assert!(!output.exists());
        if remove {
            assert!(subscriber.store.find_file(&id).await?.is_none());
            assert!(subscriber.store.blocks_storage.get_keys()?.next().is_none());
        } else {
            assert!(subscriber.store.find_file(&id).await?.unwrap().status == SubscribedFileStatus::Canceled);
            assert!(subscriber.store.read_block(&root, &hash).await?.is_some());
        }
        Ok(())
    }
}
