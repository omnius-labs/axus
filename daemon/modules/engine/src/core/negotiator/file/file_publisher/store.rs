use std::{
    collections::HashSet,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use chrono::Utc;
use parking_lot::Mutex;
use tokio::sync::{Mutex as TokioMutex, Notify};
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
    sweep_needed: AtomicBool,
    sweep_notify: Notify,
    #[cfg(test)]
    sweep_failures: std::sync::atomic::AtomicUsize,
    #[cfg(test)]
    delete_failures: std::sync::atomic::AtomicUsize,
    #[cfg(test)]
    commit_pause: Mutex<Option<(bool, Arc<Notify>)>>,
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
            sweep_needed: AtomicBool::new(false),
            sweep_notify: Notify::new(),
            #[cfg(test)]
            sweep_failures: std::sync::atomic::AtomicUsize::new(0),
            #[cfg(test)]
            delete_failures: std::sync::atomic::AtomicUsize::new(0),
            #[cfg(test)]
            commit_pause: Mutex::new(None),
        });
        {
            let _guard = store.lock.lock().await;
            store.blocks_storage.shrink(|key| !key.starts_with(b"U/")).await?;
            store.repo.recover().await?;
            store.sweep_locked().await?;
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
        if existing.as_ref().is_some_and(|file| file.status != PublishedUncommittedFileStatus::Failed) {
            return Err(Error::new(ErrorKind::AlreadyExists));
        }
        let id = self.tsid_provider.lock().create().to_string();
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
        if let Some(previous) = existing {
            if !self.repo.replace_failed_file(&previous.id, &file).await? {
                return Err(Error::new(ErrorKind::AlreadyExists));
            }
            if let Err(error) = self.cleanup_blocks_locked(&previous.id).await {
                warn!(?error, "failed import block cleanup failed");
                self.sweep_after_failure_locked().await;
            }
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
        self.blocks_storage
            .put_value(util::gen_uncommitted_block_path(&block.file_id, &block.block_hash), bytes, true)
            .await?;
        self.repo.insert_or_ignore_uncommitted_block(block).await?;
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
        let result = self.cleanup_blocks_locked(id).await;
        if result.is_err() {
            self.sweep_after_failure_locked().await;
        }
        result
    }

    async fn cleanup_blocks_locked(&self, id: &str) -> Result<()> {
        self.delete_prefix(&format!("U/{id}/")).await?;
        let blocks = self.repo.find_uncommitted_blocks_by_file_id(id).await?;
        self.repo.delete_uncommitted_blocks(&blocks).await
    }

    fn keys_with_prefix(&self, prefix: &str) -> Result<Vec<Vec<u8>>> {
        let mut iter = self.blocks_storage.get_keys()?;
        let keys = iter.by_ref().filter(|key| key.starts_with(prefix.as_bytes())).map(|key| key.into_vec()).collect();
        iter.status()?;
        Ok(keys)
    }

    async fn delete_prefix(&self, prefix: &str) -> Result<()> {
        #[cfg(test)]
        if self
            .delete_failures
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |count| count.checked_sub(1))
            .is_ok()
        {
            return Err(Error::new(ErrorKind::IoError).with_message("injected block deletion failure"));
        }
        self.blocks_storage.delete_bulk(&self.keys_with_prefix(prefix)?).await
    }

    pub fn needs_sweep(&self) -> bool {
        self.sweep_needed.load(Ordering::Relaxed)
    }

    pub async fn sweep_requested(&self) {
        self.sweep_notify.notified().await;
    }

    pub async fn sweep(&self) -> Result<()> {
        let _guard = self.lock.lock().await;
        self.sweep_needed.store(true, Ordering::Relaxed);
        self.sweep_locked().await
    }

    async fn sweep_locked(&self) -> Result<()> {
        // 参照をすべて読めた後にだけ、同じ mutex 内で key を削除する。
        let (roots, ids) = self.repo.get_block_references().await?;
        #[cfg(test)]
        if self.sweep_failures.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |count| count.checked_sub(1)).is_ok() {
            return Err(Error::new(ErrorKind::IoError).with_message("injected sweep deletion failure"));
        }
        self.blocks_storage
            .shrink(move |key| {
                let Ok(key) = std::str::from_utf8(key) else {
                    return false;
                };
                let Some((kind, key)) = key.split_once('/') else {
                    return false;
                };
                let Some((reference, _)) = key.split_once('/') else {
                    return false;
                };
                match kind {
                    "U" => ids.contains(reference),
                    "C" => roots.contains(reference),
                    _ => false,
                }
            })
            .await?;
        self.sweep_needed.store(false, Ordering::Relaxed);
        Ok(())
    }

    async fn sweep_after_failure_locked(&self) {
        self.sweep_needed.store(true, Ordering::Relaxed);
        self.sweep_notify.notify_one();
        if let Err(error) = self.sweep_locked().await {
            warn!(?error, "publisher block sweep failed");
        }
    }

    pub async fn commit(&self, file: &PublishedUncommittedFile, root_hash: &OmniHash, blocks: &[PublishedUncommittedBlock]) -> Result<bool> {
        let _guard = self.lock.lock().await;
        let result = self.commit_locked(file, root_hash, blocks).await;
        if result.is_err() {
            self.sweep_after_failure_locked().await;
        }
        result
    }

    async fn commit_locked(&self, file: &PublishedUncommittedFile, root_hash: &OmniHash, blocks: &[PublishedUncommittedBlock]) -> Result<bool> {
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
            let deletions = self.keys_with_prefix(&format!("C/{root_hash}/"))?;
            let mut hashes = HashSet::new();
            let renames: Vec<_> = blocks
                .iter()
                .filter(|block| hashes.insert(&block.block_hash))
                .map(|block| {
                    (
                        util::gen_uncommitted_block_path(&file.id, &block.block_hash).into_bytes(),
                        util::gen_committed_block_path(root_hash, &block.block_hash).into_bytes(),
                    )
                })
                .collect();
            #[cfg(test)]
            self.interrupt_block_commit(false).await;
            self.blocks_storage.rename_keys(&renames, &deletions).await?;
            #[cfg(test)]
            self.interrupt_block_commit(true).await;
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
            let result = self.delete_prefix(&format!("C/{root_hash}/")).await;
            if result.is_err() {
                self.sweep_after_failure_locked().await;
            }
            result?;
        }
        Ok(())
    }

    #[cfg(test)]
    fn pause_block_commit(&self, after_commit: bool) -> Arc<Notify> {
        let reached = Arc::new(Notify::new());
        *self.commit_pause.lock() = Some((after_commit, reached.clone()));
        reached
    }

    #[cfg(test)]
    async fn interrupt_block_commit(&self, after_commit: bool) {
        let reached = {
            let mut pause = self.commit_pause.lock();
            if pause.as_ref().is_some_and(|(after, _)| *after == after_commit) {
                pause.take().map(|(_, reached)| reached)
            } else {
                None
            }
        };
        if let Some(reached) = reached {
            reached.notify_one();
            std::future::pending::<()>().await;
        }
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

    async fn stage_blocks(store: &FilePublisherStore, id: &str) -> TestResult<(PublishedUncommittedFile, OmniHash, Vec<PublishedUncommittedBlock>)> {
        let file = store.claim(id).await?.unwrap();
        let mut blocks = Vec::new();
        let mut root = None;
        for (rank, index, bytes) in [
            (0, 0, b"leaf".as_slice()),
            (0, 1, b"leaf".as_slice()),
            (0, 2, b"other".as_slice()),
            (1, 0, b"root".as_slice()),
        ] {
            let hash = OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, bytes);
            let block = PublishedUncommittedBlock {
                file_id: id.to_string(),
                block_hash: hash.clone(),
                rank,
                index,
            };
            assert!(store.write_block(&block, Bytes::copy_from_slice(bytes)).await?);
            blocks.push(block);
            root = Some(hash);
        }
        Ok((file, root.unwrap(), blocks))
    }

    fn stored_keys(store: &FilePublisherStore) -> TestResult<HashSet<String>> {
        let mut iter = store.blocks_storage.get_keys()?;
        let keys = iter.by_ref().map(|key| String::from_utf8(key.into_vec())).collect::<std::result::Result<_, _>>()?;
        iter.status()?;
        Ok(keys)
    }

    async fn reject_metadata_commit(store: &FilePublisherStore) -> TestResult {
        // 遅延外部 key 制約で INSERT を通し、SQLite の COMMIT 自体を失敗させる。
        sqlx::raw_sql("CREATE TABLE commit_parent (id INTEGER PRIMARY KEY); CREATE TABLE commit_failure (id INTEGER REFERENCES commit_parent(id) DEFERRABLE INITIALLY DEFERRED); CREATE TRIGGER reject_commit AFTER INSERT ON committed_files BEGIN INSERT INTO commit_failure VALUES (1); END;")
            .execute(store.repo.test_db()).await?;
        Ok(())
    }

    #[tokio::test]
    async fn rename_failure_keeps_active_blocks_and_sweeps_orphans() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (ids, clock) = dependencies();
        let store = FilePublisherStore::open(dir.path(), ids, clock).await?;
        let shared = store.import("shared", "shared", 256, None, 0).await?;
        let shared_root = commit_dummy(&store, &shared).await?;
        let id = store.import("source", "a", 256, None, 0).await?;
        let (file, root, blocks) = stage_blocks(&store, &id).await?;
        let orphan = format!("C/{root}/orphan");
        store.blocks_storage.put_value(&orphan, Bytes::from_static(b"orphan"), true).await?;
        store.blocks_storage.delete(util::gen_uncommitted_block_path(&id, &blocks[2].block_hash)).await?;
        assert_eq!(store.commit(&file, &root, &blocks).await.unwrap_err().kind(), &ErrorKind::NotFound);
        let keys = stored_keys(&store)?;
        assert!(!keys.contains(&orphan));
        assert!(keys.contains(&util::gen_uncommitted_block_path(&id, &blocks[0].block_hash)));
        assert!(keys.contains(&util::gen_uncommitted_block_path(&id, &root)));
        assert!(!keys.iter().any(|key| key.starts_with(&format!("C/{root}/"))));
        assert_eq!(store.read_block(&shared_root, &shared_root).await?, Some(Bytes::from_static(b"block")));
        store.fail(&id, "rename failed").await?;
        store.sweep().await?;
        assert!(!stored_keys(&store)?.iter().any(|key| key.starts_with(&format!("U/{id}/"))));
        Ok(())
    }

    #[tokio::test]
    async fn metadata_commit_failure_sweeps_moved_blocks() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (ids, clock) = dependencies();
        let store = FilePublisherStore::open(dir.path(), ids, clock).await?;
        let id = store.import("source", "a", 256, None, 0).await?;
        let (file, root, blocks) = stage_blocks(&store, &id).await?;
        reject_metadata_commit(&store).await?;
        let error = store.commit(&file, &root, &blocks).await.unwrap_err();
        assert!(std::error::Error::source(&error).unwrap().to_string().contains("FOREIGN KEY constraint failed"));
        assert!(stored_keys(&store)?.is_empty());
        assert!(!store.repo.contains_committed_file(&root).await?);
        assert!(!store.repo.contains_committed_block(&root, &root).await?);
        assert!(store.repo.find_uncommitted_file_by_id(&id).await?.is_some());
        assert!(!store.needs_sweep());
        sqlx::query("DROP TRIGGER reject_commit").execute(store.repo.test_db()).await?;
        store.fail(&id, "commit failed").await?;
        let retry = store.import("source", "a", 256, None, 0).await?;
        assert_ne!(retry, id);
        let (file, next_root, blocks) = stage_blocks(&store, &retry).await?;
        assert_eq!(next_root, root);
        assert!(store.commit(&file, &root, &blocks).await?);
        Ok(())
    }

    #[tokio::test]
    async fn uncertain_metadata_commit_preserves_shared_root() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (ids, clock) = dependencies();
        let store = FilePublisherStore::open(dir.path(), ids, clock).await?;
        let id = store.import("source", "a", 256, None, 0).await?;
        let (file, root, blocks) = stage_blocks(&store, &id).await?;
        store.repo.fail_after_commit();
        assert!(store.commit(&file, &root, &blocks).await.is_err());
        assert!(store.repo.contains_committed_file(&root).await?);
        let committed_keys = stored_keys(&store)?;
        assert_eq!(committed_keys.len(), 3);
        store.fail(&id, "late failure").await?;
        store.cleanup_blocks(&id).await?;
        let second = store.import("source", "b", 256, None, 0).await?;
        let (file, _, blocks) = stage_blocks(&store, &second).await?;
        store.repo.fail_after_commit();
        assert!(store.commit(&file, &root, &blocks).await.is_err());
        assert_eq!(stored_keys(&store)?, committed_keys);
        assert_eq!(store.repo.get_committed_files().await?.len(), 2);
        store.sweep().await?;
        assert_eq!(store.read_block(&root, &root).await?, Some(Bytes::from_static(b"root")));
        store.remove(&root, "a").await?;
        assert_eq!(stored_keys(&store)?, committed_keys);
        store.remove(&root, "b").await?;
        assert!(stored_keys(&store)?.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn duplicate_hashes_share_blocks_and_preserve_each_index() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (ids, clock) = dependencies();
        let store = FilePublisherStore::open(dir.path(), ids, clock).await?;
        let first = store.import("source", "a", 256, None, 0).await?;
        let (file, root, blocks) = stage_blocks(&store, &first).await?;
        store.blocks_storage.put_value(format!("C/{root}/stale"), Bytes::from_static(b"stale"), true).await?;
        assert!(store.commit(&file, &root, &blocks).await?);
        assert_eq!(stored_keys(&store)?.len(), 3);
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM committed_blocks").fetch_one(store.repo.test_db()).await?;
        assert_eq!(count, 4);
        let second = store.import("source", "a", 512, Some("ignored"), 1).await?;
        let file = store.claim(&second).await?.unwrap();
        // 共有する root の block を上書きしないことも確認する。
        let block = PublishedUncommittedBlock {
            file_id: second.clone(),
            block_hash: root.clone(),
            rank: 0,
            index: 0,
        };
        store.write_block(&block, Bytes::from_static(b"unused")).await?;
        assert!(store.commit(&file, &root, &[block]).await?);
        assert_eq!(store.read_block(&root, &root).await?, Some(Bytes::from_static(b"root")));
        assert_eq!(store.repo.get_committed_files().await?.len(), 1);
        assert_eq!(store.repo.get_committed_files().await?[0].block_size, 256);
        assert_eq!(stored_keys(&store)?.len(), 3);
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM committed_blocks").fetch_one(store.repo.test_db()).await?;
        assert_eq!(count, 4);
        assert!(store.repo.find_uncommitted_file_by_id(&second).await?.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn sweep_uses_current_references_and_keeps_pending_and_processing() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (ids, clock) = dependencies();
        let store = FilePublisherStore::open(dir.path(), ids, clock).await?;
        let shared = store.import("shared", "shared", 256, None, 0).await?;
        let root = commit_dummy(&store, &shared).await?;
        let pending = store.import("pending", "pending", 256, None, 0).await?;
        let processing = store.import("processing", "processing", 256, None, 0).await?;
        store.claim(&processing).await?;
        let failed = store.import("failed", "failed", 256, None, 0).await?;
        store.claim(&failed).await?;
        store.fail(&failed, "failed").await?;
        for id in [&pending, &processing, &failed, "missing"] {
            store.blocks_storage.put_value(format!("U/{id}/{root}"), Bytes::from_static(b"value"), true).await?;
        }
        store.blocks_storage.put_value("C/missing/block", Bytes::from_static(b"value"), true).await?;
        store.sweep().await?;
        let keys = stored_keys(&store)?;
        assert_eq!(keys.len(), 3);
        for id in [&pending, &processing] {
            assert!(keys.contains(&format!("U/{id}/{root}")));
        }
        assert!(keys.contains(&util::gen_committed_block_path(&root, &root)));
        Ok(())
    }

    #[tokio::test]
    async fn unreadable_sqlite_sweep_deletes_nothing() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (ids, clock) = dependencies();
        let store = FilePublisherStore::open(dir.path(), ids, clock).await?;
        store.blocks_storage.put_value("U/missing/block", Bytes::from_static(b"value"), true).await?;
        store.blocks_storage.put_value("C/missing/block", Bytes::from_static(b"value"), true).await?;
        let keys = stored_keys(&store)?;
        store.repo.test_db().close().await;
        assert!(store.sweep().await.is_err());
        assert!(store.needs_sweep());
        assert_eq!(stored_keys(&store)?, keys);
        Ok(())
    }

    #[tokio::test]
    async fn later_commit_replaces_orphans_before_delayed_sweep() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (ids, clock) = dependencies();
        let store = FilePublisherStore::open(dir.path(), ids, clock).await?;
        let id = store.import("source", "a", 256, None, 0).await?;
        let (file, root, blocks) = stage_blocks(&store, &id).await?;
        reject_metadata_commit(&store).await?;
        store.sweep_failures.store(1, Ordering::Relaxed);
        assert!(store.commit(&file, &root, &blocks).await.is_err());
        assert!(store.needs_sweep());
        assert_eq!(stored_keys(&store)?.len(), 3);
        sqlx::query("DROP TRIGGER reject_commit").execute(store.repo.test_db()).await?;
        let next = store.import("source", "b", 256, None, 0).await?;
        let (file, _, blocks) = stage_blocks(&store, &next).await?;
        assert!(store.commit(&file, &root, &blocks).await?);
        store.sweep().await?;
        assert_eq!(stored_keys(&store)?.len(), 3);
        assert_eq!(store.read_block(&root, &root).await?, Some(Bytes::from_static(b"root")));
        Ok(())
    }

    #[tokio::test]
    async fn cancel_and_remove_retry_failed_block_deletions() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (ids, clock) = dependencies();
        let store = FilePublisherStore::open(dir.path(), ids, clock).await?;
        let id = store.import("source", "a", 256, None, 0).await?;
        stage_blocks(&store, &id).await?;
        store.cancel(&id).await?;
        store.delete_failures.store(1, Ordering::Relaxed);
        store.sweep_failures.store(2, Ordering::Relaxed);
        assert!(store.cleanup_blocks(&id).await.is_err());
        assert!(store.needs_sweep());
        assert!(store.sweep().await.is_err());
        assert_eq!(stored_keys(&store)?.len(), 3);
        store.sweep().await?;
        assert!(stored_keys(&store)?.is_empty());
        let next = store.import("source", "a", 256, None, 0).await?;
        let root = commit_dummy(&store, &next).await?;
        store.delete_failures.store(1, Ordering::Relaxed);
        store.sweep_failures.store(1, Ordering::Relaxed);
        assert!(store.remove(&root, "a").await.is_err());
        assert!(store.needs_sweep());
        assert_eq!(stored_keys(&store)?.len(), 1);
        let next = store.import("source", "a", 256, None, 0).await?;
        assert_eq!(commit_dummy(&store, &next).await?, root);
        store.sweep().await?;
        assert_eq!(store.read_block(&root, &root).await?, Some(Bytes::from_static(b"block")));
        Ok(())
    }

    #[tokio::test]
    async fn failed_reimport_new_id_survives_late_cleanup() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (ids, clock) = dependencies();
        let store = FilePublisherStore::open(dir.path(), ids, clock).await?;
        let id = store.import("source", "a", 256, None, 0).await?;
        stage_blocks(&store, &id).await?;
        store.fail(&id, "failed").await?;
        store.delete_failures.store(1, Ordering::Relaxed);
        store.sweep_failures.store(1, Ordering::Relaxed);
        let retry = store.import("source", "a", 256, None, 0).await?;
        assert_ne!(retry, id);
        assert!(store.needs_sweep());
        let (file, root, blocks) = stage_blocks(&store, &retry).await?;
        store.cleanup_blocks(&id).await?;
        assert_eq!(stored_keys(&store)?.len(), 3);
        store.sweep().await?;
        assert_eq!(stored_keys(&store)?.len(), 3);
        assert!(store.commit(&file, &root, &blocks).await?);
        assert!(stored_keys(&store)?.iter().all(|key| key.starts_with("C/")));
        assert!(store.repo.find_uncommitted_file_by_id(&id).await?.is_none());
        assert_eq!(store.read_block(&root, &root).await?, Some(Bytes::from_static(b"root")));
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn reopen_at_each_commit_boundary_recovers_and_republishes() -> TestResult {
        for boundary in 0..4 {
            let dir = tempfile::tempdir()?;
            let (ids, clock) = dependencies();
            let store = FilePublisherStore::open(dir.path(), ids.clone(), clock.clone()).await?;
            let id = store.import("source", "a", 256, None, 0).await?;
            let (file, root, blocks) = stage_blocks(&store, &id).await?;
            let reached = match boundary {
                0 => store.pause_block_commit(false),
                1 => store.pause_block_commit(true),
                2 => store.repo.pause_commit(false),
                3 => store.repo.pause_commit(true),
                _ => unreachable!(),
            };
            let running_store = store.clone();
            let committing_root = root.clone();
            let handle = tokio::spawn(async move { running_store.commit(&file, &committing_root, &blocks).await });
            tokio::time::timeout(Duration::from_secs(10), reached.notified()).await?;
            handle.abort();
            assert!(handle.await.unwrap_err().is_cancelled());
            drop(store);
            let store = FilePublisherStore::open(dir.path(), ids, clock).await?;
            let next = if boundary == 3 {
                assert!(store.repo.find_uncommitted_file_by_id(&id).await?.is_none());
                assert_eq!(stored_keys(&store)?.len(), 3);
                assert_eq!(store.read_block(&root, &root).await?, Some(Bytes::from_static(b"root")));
                store.import("source", "a", 256, None, 0).await?
            } else {
                assert!(stored_keys(&store)?.is_empty());
                assert!(store.repo.find_uncommitted_file_by_id(&id).await?.unwrap().status == PublishedUncommittedFileStatus::Pending);
                assert!(store.repo.find_uncommitted_blocks_by_file_id(&id).await?.is_empty());
                assert!(store.get_published_root_hashes().await?.is_empty());
                id
            };
            let (file, next_root, blocks) = stage_blocks(&store, &next).await?;
            assert_eq!(next_root, root);
            assert!(store.commit(&file, &root, &blocks).await?);
            assert_eq!(stored_keys(&store)?.len(), 3);
            assert_eq!(store.read_block(&root, &root).await?, Some(Bytes::from_static(b"root")));
        }
        Ok(())
    }

    struct RecordingSleeper {
        delays: Mutex<Vec<Duration>>,
    }

    #[async_trait::async_trait]
    impl omnius_core_base::sleeper::Sleeper for RecordingSleeper {
        async fn sleep(&self, delay: Duration) {
            self.delays.lock().push(delay);
            tokio::task::yield_now().await;
        }
    }

    #[tokio::test]
    async fn encoder_retries_sweep_with_increasing_delays() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (ids, clock) = dependencies();
        let store = FilePublisherStore::open(dir.path(), ids, clock.clone()).await?;
        let id = store.import("source", "a", 256, None, 0).await?;
        stage_blocks(&store, &id).await?;
        store.cancel(&id).await?;
        store.delete_failures.store(1, Ordering::Relaxed);
        store.sweep_failures.store(4, Ordering::Relaxed);
        assert!(store.cleanup_blocks(&id).await.is_err());
        let sleeper = Arc::new(RecordingSleeper { delays: Mutex::new(Vec::new()) });
        let task = TaskEncoder::new(store.clone(), clock, sleeper.clone()).await?;
        tokio::time::timeout(Duration::from_secs(10), async {
            while store.needs_sweep() {
                tokio::task::yield_now().await;
            }
        })
        .await?;
        task.shutdown().await;
        assert_eq!(*sleeper.delays.lock(), vec![Duration::from_secs(1), Duration::from_secs(2), Duration::from_secs(4)]);
        assert!(stored_keys(&store)?.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn reopen_recovers_after_repeated_sweep_failures() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (ids, clock) = dependencies();
        let store = FilePublisherStore::open(dir.path(), ids.clone(), clock.clone()).await?;
        let id = store.import("source", "a", 256, None, 0).await?;
        let (file, root, blocks) = stage_blocks(&store, &id).await?;
        reject_metadata_commit(&store).await?;
        store.sweep_failures.store(3, Ordering::Relaxed);
        assert!(store.commit(&file, &root, &blocks).await.is_err());
        assert!(store.sweep().await.is_err());
        assert!(store.sweep().await.is_err());
        assert_eq!(stored_keys(&store)?.len(), 3);
        sqlx::query("DROP TRIGGER reject_commit").execute(store.repo.test_db()).await?;
        drop(store);
        let store = FilePublisherStore::open(dir.path(), ids, clock).await?;
        assert!(stored_keys(&store)?.is_empty());
        assert!(!store.needs_sweep());
        let (file, _, blocks) = stage_blocks(&store, &id).await?;
        assert!(store.commit(&file, &root, &blocks).await?);
        Ok(())
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
        assert_ne!(retry, fourth);
        assert!(store.repo.find_uncommitted_file_by_id(&fourth).await?.is_none());
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
    async fn failed_import_uses_new_id_and_wakes_encoder() -> TestResult {
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
        let retry = publisher.import(source.to_str().unwrap(), "source", 256, None, 0).await?;
        assert_ne!(retry, id);
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
