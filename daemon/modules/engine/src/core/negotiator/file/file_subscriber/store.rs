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
use tokio::sync::{Mutex as TokioMutex, Notify, OwnedMutexGuard, RwLock};
use tokio_util::bytes::Bytes;

use omnius_core_base::{clock::Clock, tsid::TsidProvider};
use omnius_core_omnikit::generated::omni_hash::OmniHash;

use crate::base::storage::KeyValueRocksdbStorage;

use super::{output_publication::OutputPublication, repo::FileSubscriberRepo, *};

pub struct FileSubscriberStore {
    repo: FileSubscriberRepo,
    blocks_storage: KeyValueRocksdbStorage,
    lock: RwLock<()>,
    output_lock: Arc<TokioMutex<()>>,
    finalization_lock: TokioMutex<()>,
    finalization_notify: Notify,
    sweep_needed: AtomicBool,
    sweep_notify: Notify,
    #[cfg(test)]
    output_failures: Mutex<std::collections::HashMap<OutputStep, usize>>,
    #[cfg(test)]
    rename_pause: Mutex<Option<(tokio::sync::oneshot::Sender<()>, tokio::sync::oneshot::Receiver<()>)>>,
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
            output_lock: Arc::new(TokioMutex::new(())),
            finalization_lock: TokioMutex::new(()),
            finalization_notify: Notify::new(),
            sweep_needed: AtomicBool::new(false),
            sweep_notify: Notify::new(),
            #[cfg(test)]
            output_failures: Mutex::new(std::collections::HashMap::new()),
            #[cfg(test)]
            rename_pause: Mutex::new(None),
            tsid_provider,
            clock,
        });
        for file in store.repo.find_finalizing_files().await? {
            if let Err(error) = store.finalize(&file.id).await {
                if *error.kind() != ErrorKind::IoError {
                    return Err(error);
                }
                warn!(?error, id = %file.id, "subscriber output recovery deferred");
            }
        }
        {
            let _guard = store.lock.write().await;
            let files = store.repo.get_committed_files().await?;
            store.sweep_blocks_locked(&files).await?;
            if let Err(error) = store.sweep_outputs_locked(&files).await {
                store.request_sweep();
                warn!(?error, "subscriber temporary output sweep deferred");
            }
        }
        Ok(store)
    }

    pub async fn subscribe(&self, root_hash: &OmniHash, file_path: &str, attrs: Option<&str>, priority: i64) -> Result<String> {
        let _guard = self.lock.read().await;
        let path = Path::new(file_path);
        let output_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| Error::new(ErrorKind::InvalidFormat).with_message("invalid output entry name"))?;
        if SubscribedFile::is_reserved_output_name(output_name) {
            return Err(Error::new(ErrorKind::InvalidFormat).with_message("output entry name is reserved for temporary subscriber output"));
        }
        let parent = path.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or(Path::new("."));
        let directory = tokio::fs::canonicalize(parent).await?;
        if !tokio::fs::metadata(&directory).await?.is_dir() {
            return Err(Error::new(ErrorKind::InvalidFormat).with_message("output parent is not a directory"));
        }
        match tokio::fs::symlink_metadata(directory.join(output_name)).await {
            Ok(_) => return Err(Error::new(ErrorKind::AlreadyExists).with_message("output entry already exists")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let output_directory = directory
            .to_str()
            .ok_or_else(|| Error::new(ErrorKind::InvalidFormat).with_message("invalid output directory"))?;
        let id = self.tsid_provider.lock().create().to_string();
        let now = self.clock.now();
        let file = SubscribedFile {
            id: id.clone(),
            root_hash: root_hash.clone(),
            output_directory: output_directory.to_string(),
            output_name: output_name.to_string(),
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
        self.cancel(id).await?;
        self.discard_output(id).await?;
        let _guard = self.lock.write().await;
        let Some(file) = self.repo.find_file_by_id(id).await? else {
            return Ok(());
        };
        self.repo.delete_file(id).await?;
        if self.repo.find_file_by_root_hash(&file.root_hash).await?.is_none() {
            let prefix = format!("{}/", file.root_hash);
            let keys: Vec<_> = self.blocks_storage.get_keys()?.filter(|key| key.starts_with(prefix.as_bytes())).collect();
            for key in keys {
                if let Err(error) = self.blocks_storage.delete(key).await {
                    self.request_sweep();
                    return Err(error);
                }
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

    pub async fn create_output(&self, file: &SubscribedFile) -> Result<Option<OutputWriter>> {
        let guard = self.output_lock.clone().lock_owned().await;
        if !self.repo.find_file_by_id(&file.id).await?.is_some_and(|file| file.status == SubscribedFileStatus::Decoding) {
            return Ok(None);
        }
        let result: Result<tokio::fs::File> = async {
            self.delete_temporary_output(file).await?;
            Ok(tokio::fs::OpenOptions::new().write(true).create_new(true).open(file.temporary_output_path()).await?)
        }
        .await;
        match result {
            Ok(output) => Ok(Some(OutputWriter { file: output, _guard: guard })),
            Err(error) => {
                drop(guard);
                self.fail(&file.id, &error.to_string()).await?;
                Err(error)
            }
        }
    }

    pub async fn sync_output(&self, output: &mut OutputWriter) -> Result<()> {
        use tokio::io::AsyncWriteExt as _;
        output.file.flush().await?;
        #[cfg(test)]
        self.inject_failure(OutputStep::SyncFile)?;
        output.file.sync_all().await?;
        Ok(())
    }

    pub async fn close_output(&self, mut output: OutputWriter) -> Result<()> {
        use tokio::io::AsyncWriteExt as _;
        // cancel で write の future を抜けても、すでに始まった I/O の終了を待つ。
        output.file.flush().await?;
        Ok(())
    }

    pub async fn begin_finalizing(&self, id: &str) -> Result<bool> {
        let result = self.repo.transition(id, &SubscribedFileStatus::Decoding, &SubscribedFileStatus::Finalizing, None).await;
        // commit の成否が不明でも、再読の後にだけ配置または回収を行う。
        self.finalization_notify.notify_one();
        #[cfg(test)]
        if matches!(result, Ok(true)) {
            self.inject_failure(OutputStep::FinalizingCommit)?;
        }
        result
    }

    pub async fn finalizing_files(&self) -> Result<Vec<SubscribedFile>> {
        self.repo.find_finalizing_files().await
    }

    pub async fn finalization_requested(&self) {
        self.finalization_notify.notified().await;
    }

    pub async fn finalize(&self, id: &str) -> Result<()> {
        let _guard = self.finalization_lock.lock().await;
        let _output_guard = self.output_lock.lock().await;
        let Some(file) = self.repo.find_file_by_id(id).await? else {
            return Ok(());
        };
        if file.status != SubscribedFileStatus::Finalizing {
            return Ok(());
        }
        let directory = Path::new(&file.output_directory);
        if !tokio::fs::metadata(directory).await?.is_dir() {
            return Err(Error::new(ErrorKind::IoError).with_message("output parent is not a directory"));
        }
        let temporary = Self::entry_exists(&file.temporary_output_path()).await?;
        let destination = Self::entry_exists(&file.output_path()).await?;
        match (temporary, destination) {
            (true, true) => self.fail_finalizing(&file, "output entry already exists").await,
            (false, false) => self.fail_finalizing(&file, "temporary output is missing").await,
            (false, true) => self.complete_finalizing(&file).await,
            (true, false) => {
                #[cfg(test)]
                self.inject_failure(OutputStep::Rename)?;
                let result = self.rename_output(&file).await?;
                if let Err(error) = result {
                    // ENOENT には親 directory の消失も含まれるため、先に親を確かめる。
                    if error.kind() == std::io::ErrorKind::NotFound && tokio::fs::metadata(directory).await?.is_dir() && !Self::entry_exists(&file.temporary_output_path()).await? {
                        if Self::entry_exists(&file.output_path()).await? {
                            return self.complete_finalizing(&file).await;
                        }
                        return self.fail_finalizing(&file, "temporary output is missing").await;
                    }
                    if OutputPublication::is_existing_destination(&error)
                        || OutputPublication::is_unsupported(&error)
                        || (Self::entry_exists(&file.output_path()).await? && Self::entry_exists(&file.temporary_output_path()).await?)
                    {
                        return self.fail_finalizing(&file, &error.to_string()).await;
                    }
                    return Err(error.into());
                }
                #[cfg(test)]
                self.inject_failure(OutputStep::AfterRename)?;
                self.complete_finalizing(&file).await
            }
        }
    }

    async fn rename_output(&self, file: &SubscribedFile) -> Result<std::io::Result<()>> {
        #[cfg(test)]
        {
            let pause = self.rename_pause.lock().take();
            if let Some((entered, resume)) = pause {
                let _ = entered.send(());
                let _ = resume.await;
            }
            if self.inject_failure(OutputStep::ExistingDestination).is_err() {
                return Ok(Err(std::io::ErrorKind::AlreadyExists.into()));
            }
            if self.inject_failure(OutputStep::UnsupportedRename).is_err() {
                return Ok(Err(std::io::ErrorKind::Unsupported.into()));
            }
        }
        let source = file.temporary_output_path();
        let destination = file.output_path();
        Ok(tokio::task::spawn_blocking(move || OutputPublication::rename(&source, &destination)).await?)
    }

    async fn complete_finalizing(&self, file: &SubscribedFile) -> Result<()> {
        let directory = std::path::PathBuf::from(&file.output_directory);
        #[cfg(test)]
        self.inject_failure(OutputStep::SyncDirectory)?;
        tokio::task::spawn_blocking(move || OutputPublication::sync_directory(&directory)).await??;
        #[cfg(test)]
        self.inject_failure(OutputStep::Complete)?;
        if !self
            .repo
            .transition(&file.id, &SubscribedFileStatus::Finalizing, &SubscribedFileStatus::Completed, None)
            .await?
        {
            return Err(Error::new(ErrorKind::Reject).with_message("output completion state changed"));
        }
        Ok(())
    }

    async fn fail_finalizing(&self, file: &SubscribedFile, reason: &str) -> Result<()> {
        if !self
            .repo
            .transition(&file.id, &SubscribedFileStatus::Finalizing, &SubscribedFileStatus::Failed, Some(reason))
            .await?
        {
            return Err(Error::new(ErrorKind::Reject).with_message("output finalization state changed"));
        }
        // finalize の writer lock 内で、Failed の永続化後にだけ消す。
        if let Err(error) = self.delete_temporary_output(file).await {
            self.request_sweep();
            return Err(error);
        }
        Ok(())
    }

    pub async fn discard_output(&self, id: &str) -> Result<()> {
        let _guard = self.output_lock.lock().await;
        let Some(file) = self.repo.find_file_by_id(id).await? else {
            return Ok(());
        };
        if matches!(file.status, SubscribedFileStatus::Finalizing | SubscribedFileStatus::Completed) {
            return Ok(());
        }
        if let Err(error) = self.delete_temporary_output(&file).await {
            self.request_sweep();
            return Err(error);
        }
        Ok(())
    }

    async fn delete_temporary_output(&self, file: &SubscribedFile) -> Result<()> {
        #[cfg(test)]
        self.inject_failure(OutputStep::DeleteTemporary)?;
        match tokio::fs::remove_file(file.temporary_output_path()).await {
            Ok(()) => Ok(()),
            Err(error) if matches!(error.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::InvalidFilename) => {
                // 長すぎる名前は存在し得ないため、一時出力がない場合と同じに扱う。
                // 出力先の disk が外れている場合は、削除済みと判定しない。
                if !tokio::fs::metadata(&file.output_directory).await?.is_dir() {
                    return Err(Error::new(ErrorKind::IoError).with_message("output parent is not a directory"));
                }
                Ok(())
            }
            Err(error) => Err(error.into()),
        }
    }

    async fn entry_exists(path: &Path) -> Result<bool> {
        match tokio::fs::symlink_metadata(path).await {
            Ok(_) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    pub fn needs_sweep(&self) -> bool {
        self.sweep_needed.load(Ordering::Relaxed)
    }

    fn request_sweep(&self) {
        self.sweep_needed.store(true, Ordering::Relaxed);
        self.sweep_notify.notify_one();
    }

    pub async fn sweep_requested(&self) {
        self.sweep_notify.notified().await;
    }

    pub async fn sweep(&self) -> Result<()> {
        let _guard = self.lock.write().await;
        self.sweep_needed.store(false, Ordering::Relaxed);
        let result = async {
            let files = self.repo.get_committed_files().await?;
            self.sweep_blocks_locked(&files).await?;
            self.sweep_outputs_locked(&files).await
        }
        .await;
        if result.is_err() {
            self.request_sweep();
        }
        result
    }

    async fn sweep_blocks_locked(&self, files: &[SubscribedFile]) -> Result<()> {
        let roots: HashSet<_> = files.iter().map(|file| file.root_hash.to_string()).collect();
        self.blocks_storage
            .shrink(move |key| std::str::from_utf8(key).ok().and_then(|key| key.split('/').next()).is_some_and(|root| roots.contains(root)))
            .await
    }

    async fn sweep_outputs_locked(&self, files: &[SubscribedFile]) -> Result<()> {
        let mut failure = None;
        for file in files {
            if matches!(file.status, SubscribedFileStatus::Canceled | SubscribedFileStatus::Failed) {
                let _guard = self.output_lock.lock().await;
                if let Err(error) = self.delete_temporary_output(file).await {
                    failure = Some(error);
                }
            }
        }
        if let Some(error) = failure {
            self.request_sweep();
            return Err(error);
        }
        Ok(())
    }

    #[cfg(test)]
    fn inject_failure(&self, step: OutputStep) -> Result<()> {
        let mut failures = self.output_failures.lock();
        if let Some(count) = failures.get_mut(&step)
            && *count > 0
        {
            *count -= 1;
            return Err(Error::new(ErrorKind::IoError).with_message(format!("injected output failure: {step:?}")));
        }
        Ok(())
    }
    pub async fn fail(&self, id: &str, reason: &str) -> Result<bool> {
        let changed = self
            .repo
            .transition(id, &SubscribedFileStatus::Decoding, &SubscribedFileStatus::Failed, Some(reason))
            .await?;
        self.discard_output(id).await?;
        Ok(changed)
    }
}

pub struct OutputWriter {
    pub file: tokio::fs::File,
    _guard: OwnedMutexGuard<()>,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum OutputStep {
    SyncFile,
    FinalizingCommit,
    Rename,
    UnsupportedRename,
    ExistingDestination,
    AfterRename,
    SyncDirectory,
    Complete,
    DeleteTemporary,
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

    async fn rank_zero_subscription(store: &FileSubscriberStore, output: &Path, content: &'static [u8]) -> TestResult<String> {
        let root = OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, output.to_str().unwrap().as_bytes());
        let hash = OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, content);
        let id = store.subscribe(&root, output.to_str().unwrap(), None, 0).await?;
        store.write_block(&root, &root, Bytes::from_static(b"root")).await?;
        let file = store.find_file(&id).await?.unwrap();
        store
            .advance_layer(
                &file,
                MerkleLayer {
                    rank: 0,
                    hashes: vec![hash.clone()],
                },
            )
            .await?;
        store.write_block(&root, &hash, Bytes::from_static(content)).await?;
        Ok(id)
    }

    async fn write_temporary(store: &FileSubscriberStore, id: &str, bytes: &[u8]) -> TestResult {
        use tokio::io::AsyncWriteExt as _;
        let file = store.find_file(id).await?.unwrap();
        let mut writer = store.create_output(&file).await?.unwrap();
        writer.file.write_all(bytes).await?;
        store.sync_output(&mut writer).await?;
        Ok(())
    }

    async fn wait_status(store: &FileSubscriberStore, id: &str, status: SubscribedFileStatus) -> TestResult {
        tokio::time::timeout(Duration::from_secs(30), async {
            while store.find_file(id).await?.unwrap().status != status {
                tokio::task::yield_now().await;
            }
            Result::Ok(())
        })
        .await??;
        Ok(())
    }

    #[tokio::test]
    async fn uncertain_finalizing_commit_preserves_temporary_until_status_is_read() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (ids, clock) = dependencies();
        let store = FileSubscriberStore::open(&dir.path().join("state"), ids, clock).await?;
        let output = dir.path().join("output");
        let id = rank_zero_subscription(&store, &output, b"content").await?;
        write_temporary(&store, &id, b"content").await?;
        store.output_failures.lock().insert(OutputStep::FinalizingCommit, 1);
        assert!(store.begin_finalizing(&id).await.is_err());
        assert!(!store.fail(&id, "unknown commit outcome").await?);
        store.discard_output(&id).await?;
        let file = store.find_file(&id).await?.unwrap();
        assert!(file.status == SubscribedFileStatus::Finalizing);
        assert!(file.temporary_output_path().exists());
        assert!(!output.exists());
        store.finalize(&id).await?;
        assert_eq!(tokio::fs::read(&output).await?, b"content");
        Ok(())
    }

    #[tokio::test]
    async fn temporary_output_is_recreated_and_sync_precedes_finalizing() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (ids, clock) = dependencies();
        let store = FileSubscriberStore::open(&dir.path().join("state"), ids, clock).await?;
        let output = dir.path().join("output");
        let id = rank_zero_subscription(&store, &output, b"content").await?;
        let file = store.find_file(&id).await?.unwrap();
        assert_eq!(file.temporary_output_path().file_name().unwrap().to_str().unwrap(), format!(".output.axus-{id}.part"));
        tokio::fs::write(file.temporary_output_path(), b"stale partial").await?;
        let mut writer = store.create_output(&file).await?.unwrap();
        assert_eq!(writer.file.metadata().await?.len(), 0);
        use tokio::io::AsyncWriteExt as _;
        writer.file.write_all(b"content").await?;
        store.output_failures.lock().insert(OutputStep::SyncFile, 1);
        assert!(store.sync_output(&mut writer).await.is_err());
        assert!(store.find_file(&id).await?.unwrap().status == SubscribedFileStatus::Decoding);
        assert!(!output.exists());
        store.sync_output(&mut writer).await?;
        drop(writer);
        assert!(store.begin_finalizing(&id).await?);
        store.finalize(&id).await?;
        assert!(store.find_file(&id).await?.unwrap().status == SubscribedFileStatus::Completed);
        assert_eq!(tokio::fs::read(&output).await?, b"content");
        assert!(!file.temporary_output_path().exists());
        assert!(store.subscribe(&file.root_hash, output.to_str().unwrap(), None, 0).await.is_err());
        tokio::fs::remove_file(&output).await?;
        store.subscribe(&file.root_hash, output.to_str().unwrap(), None, 0).await?;
        Ok(())
    }

    #[tokio::test]
    async fn external_entry_created_immediately_before_rename_is_preserved() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (ids, clock) = dependencies();
        let store = FileSubscriberStore::open(&dir.path().join("state"), ids, clock).await?;
        let output = dir.path().join("output");
        let id = rank_zero_subscription(&store, &output, b"content").await?;
        write_temporary(&store, &id, b"content").await?;
        store.begin_finalizing(&id).await?;
        let (entered, arrived) = tokio::sync::oneshot::channel();
        let (resume, resumed) = tokio::sync::oneshot::channel();
        *store.rename_pause.lock() = Some((entered, resumed));
        let job_store = store.clone();
        let job_id = id.clone();
        let job = tokio::spawn(async move { job_store.finalize(&job_id).await });
        arrived.await?;
        tokio::fs::write(&output, b"external").await?;
        let _ = resume.send(());
        job.await??;
        let file = store.find_file(&id).await?.unwrap();
        assert!(file.status == SubscribedFileStatus::Failed);
        assert!(!file.temporary_output_path().exists());
        assert_eq!(tokio::fs::read(&output).await?, b"external");
        tokio::fs::remove_file(&output).await?;
        store.subscribe(&file.root_hash, output.to_str().unwrap(), None, 0).await?;
        Ok(())
    }

    #[tokio::test]
    async fn existing_entry_rename_error_is_final_even_if_entry_disappears() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (ids, clock) = dependencies();
        let store = FileSubscriberStore::open(&dir.path().join("state"), ids, clock).await?;
        let output = dir.path().join("output");
        let id = rank_zero_subscription(&store, &output, b"content").await?;
        write_temporary(&store, &id, b"content").await?;
        let file = store.find_file(&id).await?.unwrap();
        store.begin_finalizing(&id).await?;
        // rename が既存 entry を検出した直後に、外部 process が entry を消した状態を作る。
        store.output_failures.lock().insert(OutputStep::ExistingDestination, 1);
        store.finalize(&id).await?;
        assert!(store.find_file(&id).await?.unwrap().status == SubscribedFileStatus::Failed);
        assert!(!file.temporary_output_path().exists());
        assert!(!output.exists());
        store.finalize(&id).await?;
        assert!(!output.exists());
        Ok(())
    }

    #[tokio::test]
    async fn missing_temporary_output_during_rename_fails_without_deleting_destination() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (ids, clock) = dependencies();
        let store = FileSubscriberStore::open(&dir.path().join("state"), ids, clock).await?;
        let output = dir.path().join("output");
        let id = rank_zero_subscription(&store, &output, b"content").await?;
        write_temporary(&store, &id, b"content").await?;
        let file = store.find_file(&id).await?.unwrap();
        store.begin_finalizing(&id).await?;
        let (entered, arrived) = tokio::sync::oneshot::channel();
        let (resume, resumed) = tokio::sync::oneshot::channel();
        *store.rename_pause.lock() = Some((entered, resumed));
        let job_store = store.clone();
        let job_id = id.clone();
        let job = tokio::spawn(async move { job_store.finalize(&job_id).await });
        arrived.await?;
        tokio::fs::remove_file(file.temporary_output_path()).await?;
        let _ = resume.send(());
        job.await??;
        assert!(store.find_file(&id).await?.unwrap().status == SubscribedFileStatus::Failed);
        assert!(!output.exists());
        Ok(())
    }

    #[tokio::test]
    async fn decode_failure_preserves_external_output_and_discards_only_temporary() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (ids, clock) = dependencies();
        let store = FileSubscriberStore::open(&dir.path().join("state"), ids, clock.clone()).await?;
        let output = dir.path().join("output");
        let id = rank_zero_subscription(&store, &output, b"content").await?;
        let file = store.find_file(&id).await?.unwrap();
        let hash = store.blocks(&file.root_hash, 0).await?[0].block_hash.clone();
        store.blocks_storage.delete(util::gen_block_path(&file.root_hash, &hash)).await?;
        tokio::fs::write(&output, b"external").await?;
        let task = TaskDecoder::new(store.clone(), clock, Arc::new(SleeperImpl)).await?;
        wait_status(&store, &id, SubscribedFileStatus::Failed).await?;
        task.shutdown().await;
        assert_eq!(tokio::fs::read(&output).await?, b"external");
        assert!(!file.temporary_output_path().exists());
        Ok(())
    }

    #[tokio::test]
    async fn conditional_finalization_and_completion_mismatches_preserve_external_output() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (ids, clock) = dependencies();
        let store = FileSubscriberStore::open(&dir.path().join("state"), ids, clock).await?;
        let output = dir.path().join("output");
        let id = rank_zero_subscription(&store, &output, b"content").await?;
        write_temporary(&store, &id, b"content").await?;
        tokio::fs::write(&output, b"external").await?;
        store.cancel(&id).await?;
        assert!(!store.begin_finalizing(&id).await?);
        store.discard_output(&id).await?;
        assert_eq!(tokio::fs::read(&output).await?, b"external");
        let file = store.find_file(&id).await?.unwrap();
        // Completed の条件に合わない場合も、利用者の出力は回収しない。
        assert!(store.complete_finalizing(&file).await.is_err());
        assert_eq!(tokio::fs::read(&output).await?, b"external");
        assert!(!file.temporary_output_path().exists());
        Ok(())
    }

    #[tokio::test]
    async fn too_long_temporary_name_records_failed_and_releases_reservation() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (ids, clock) = dependencies();
        let store = FileSubscriberStore::open(&dir.path().join("state"), ids, clock).await?;
        let output = dir.path().join("x".repeat(250));
        let id = rank_zero_subscription(&store, &output, b"content").await?;
        let file = store.find_file(&id).await?.unwrap();
        let error = tokio::fs::remove_file(file.temporary_output_path()).await.unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidFilename);
        assert!(store.create_output(&file).await.is_err());
        assert!(store.find_file(&id).await?.unwrap().status == SubscribedFileStatus::Failed);
        assert!(!output.exists());
        store.subscribe(&file.root_hash, output.to_str().unwrap(), None, 0).await?;
        store.sweep().await?;
        assert!(!store.needs_sweep());
        store.remove(&id).await?;
        assert!(store.find_file(&id).await?.is_none());
        assert!(!store.needs_sweep());
        Ok(())
    }

    #[tokio::test]
    async fn completed_cancel_and_remove_succeed_after_output_directory_is_deleted() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (ids, clock) = dependencies();
        let store = FileSubscriberStore::open(&dir.path().join("state"), ids, clock.clone()).await?;
        let directory = dir.path().join("output-directory");
        tokio::fs::create_dir(&directory).await?;
        let output = directory.join("output");
        let id = rank_zero_subscription(&store, &output, b"content").await?;
        write_temporary(&store, &id, b"content").await?;
        assert!(store.begin_finalizing(&id).await?);
        store.finalize(&id).await?;
        assert!(store.find_file(&id).await?.unwrap().status == SubscribedFileStatus::Completed);
        tokio::fs::remove_dir_all(&directory).await?;
        let task_decoder = TaskDecoder::new(store.clone(), clock, Arc::new(SleeperImpl)).await?;
        let subscriber = FileSubscriber {
            store: store.clone(),
            task_decoder,
        };
        let canceled = subscriber.cancel(&id).await;
        assert!(store.find_file(&id).await?.unwrap().status == SubscribedFileStatus::Completed);
        let removed = subscriber.remove(&id).await;
        subscriber.shutdown().await;
        canceled?;
        removed?;
        assert!(store.find_file(&id).await?.is_none());
        assert!(store.blocks_storage.get_keys()?.next().is_none());
        assert!(!store.needs_sweep());
        Ok(())
    }

    #[tokio::test]
    async fn unsupported_rename_fails_and_discards_temporary_output() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (ids, clock) = dependencies();
        let store = FileSubscriberStore::open(&dir.path().join("state"), ids, clock).await?;
        let output = dir.path().join("output");
        let id = rank_zero_subscription(&store, &output, b"content").await?;
        write_temporary(&store, &id, b"content").await?;
        store.begin_finalizing(&id).await?;
        store.output_failures.lock().insert(OutputStep::UnsupportedRename, 1);
        store.finalize(&id).await?;
        let file = store.find_file(&id).await?.unwrap();
        assert!(file.status == SubscribedFileStatus::Failed);
        assert!(!file.temporary_output_path().exists());
        assert!(!output.exists());
        Ok(())
    }

    #[tokio::test]
    async fn finalizing_rejects_cancel_remove_and_keeps_reservation() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (ids, clock) = dependencies();
        let store = FileSubscriberStore::open(&dir.path().join("state"), ids, clock).await?;
        let output = dir.path().join("output");
        let id = rank_zero_subscription(&store, &output, b"content").await?;
        write_temporary(&store, &id, b"content").await?;
        store.begin_finalizing(&id).await?;
        assert!(store.cancel(&id).await.is_err());
        assert!(store.remove(&id).await.is_err());
        let file = store.find_file(&id).await?.unwrap();
        assert!(file.status == SubscribedFileStatus::Finalizing);
        assert!(file.temporary_output_path().exists());
        assert!(store.subscribe(&file.root_hash, output.to_str().unwrap(), None, 0).await.is_err());
        store.finalize(&id).await?;
        store.cancel(&id).await?;
        store.remove(&id).await?;
        assert_eq!(tokio::fs::read(&output).await?, b"content");
        Ok(())
    }

    #[tokio::test]
    async fn cancel_cleanup_failure_keeps_canceled_and_sweep_recovers_temporary() -> TestResult {
        cleanup_failure(false).await
    }

    #[tokio::test]
    async fn remove_cleanup_failure_keeps_canceled_row_until_sweep() -> TestResult {
        cleanup_failure(true).await
    }

    async fn cleanup_failure(remove: bool) -> TestResult {
        let dir = tempfile::tempdir()?;
        let (ids, clock) = dependencies();
        let store = FileSubscriberStore::open(&dir.path().join("state"), ids, clock.clone()).await?;
        let root = OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, b"root");
        let output = dir.path().join("output");
        let id = store.subscribe(&root, output.to_str().unwrap(), None, 0).await?;
        let file = store.find_file(&id).await?.unwrap();
        tokio::fs::write(file.temporary_output_path(), b"partial").await?;
        tokio::fs::write(&output, b"external").await?;
        store.output_failures.lock().insert(OutputStep::DeleteTemporary, 1);
        let task_decoder = TaskDecoder::new(store.clone(), clock, Arc::new(SleeperImpl)).await?;
        let subscriber = FileSubscriber {
            store: store.clone(),
            task_decoder,
        };
        let result = if remove { subscriber.remove(&id).await } else { subscriber.cancel(&id).await };
        assert!(result.is_err());
        assert!(store.find_file(&id).await?.unwrap().status == SubscribedFileStatus::Canceled);
        subscriber.shutdown().await;
        store.sweep().await?;
        assert!(!file.temporary_output_path().exists());
        assert_eq!(tokio::fs::read(&output).await?, b"external");
        if remove {
            store.remove(&id).await?;
            assert!(store.find_file(&id).await?.is_none());
        }
        Ok(())
    }

    #[tokio::test]
    async fn sweep_preserves_unowned_temporary_and_active_output() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (ids, clock) = dependencies();
        let store = FileSubscriberStore::open(&dir.path().join("state"), ids, clock).await?;
        let id = rank_zero_subscription(&store, &dir.path().join("output"), b"content").await?;
        write_temporary(&store, &id, b"content").await?;
        let file = store.find_file(&id).await?.unwrap();
        let unowned = dir.path().join(".output.axus-missing.part");
        tokio::fs::write(&unowned, b"unowned").await?;
        store.sweep().await?;
        assert!(file.temporary_output_path().exists());
        assert!(unowned.exists());
        store.fail(&id, "decode failed").await?;
        tokio::fs::write(file.temporary_output_path(), b"late partial").await?;
        store.sweep().await?;
        assert!(!file.temporary_output_path().exists());
        assert!(unowned.exists());
        Ok(())
    }

    #[tokio::test]
    async fn reopen_after_temporary_creation_restarts_decoding() -> TestResult {
        reopen_boundary(0).await
    }
    #[tokio::test]
    async fn reopen_after_file_sync_restarts_decoding() -> TestResult {
        reopen_boundary(1).await
    }
    #[tokio::test]
    async fn reopen_after_finalizing_retries_placement() -> TestResult {
        reopen_boundary(2).await
    }
    #[tokio::test]
    async fn reopen_after_rename_completes_publication() -> TestResult {
        reopen_boundary(3).await
    }
    #[tokio::test]
    async fn reopen_before_completed_records_completed() -> TestResult {
        reopen_boundary(4).await
    }

    async fn reopen_boundary(boundary: u8) -> TestResult {
        use tokio::io::AsyncWriteExt as _;
        let dir = tempfile::tempdir()?;
        let state = dir.path().join("state");
        let output = dir.path().join("output");
        let (ids, clock) = dependencies();
        let store = FileSubscriberStore::open(&state, ids.clone(), clock.clone()).await?;
        let id = rank_zero_subscription(&store, &output, b"content").await?;
        let file = store.find_file(&id).await?.unwrap();
        let mut writer = store.create_output(&file).await?.unwrap();
        writer.file.write_all(if boundary < 2 { b"partial" } else { b"content" }).await?;
        if boundary >= 1 {
            store.sync_output(&mut writer).await?;
        }
        drop(writer);
        if boundary >= 2 {
            assert!(store.begin_finalizing(&id).await?);
        }
        if boundary >= 3 {
            store
                .output_failures
                .lock()
                .insert(if boundary == 3 { OutputStep::AfterRename } else { OutputStep::Complete }, 1);
            assert!(store.finalize(&id).await.is_err());
            assert!(store.find_file(&id).await?.unwrap().status == SubscribedFileStatus::Finalizing);
        }
        drop(store);
        let store = FileSubscriberStore::open(&state, ids, clock.clone()).await?;
        if boundary < 2 {
            assert!(store.find_file(&id).await?.unwrap().status == SubscribedFileStatus::Decoding);
            assert!(!output.exists());
            let task = TaskDecoder::new(store.clone(), clock, Arc::new(SleeperImpl)).await?;
            wait_status(&store, &id, SubscribedFileStatus::Completed).await?;
            task.shutdown().await;
        } else {
            assert!(store.find_file(&id).await?.unwrap().status == SubscribedFileStatus::Completed);
        }
        assert_eq!(tokio::fs::read(&output).await?, b"content");
        assert!(!file.temporary_output_path().exists());
        Ok(())
    }

    #[tokio::test]
    async fn reopen_recovers_all_finalizing_entry_combinations() -> TestResult {
        for (temporary, destination) in [(true, false), (true, true), (false, true), (false, false)] {
            let dir = tempfile::tempdir()?;
            let (ids, clock) = dependencies();
            let state = dir.path().join("state");
            let output = dir.path().join("output");
            let store = FileSubscriberStore::open(&state, ids.clone(), clock.clone()).await?;
            let id = rank_zero_subscription(&store, &output, b"content").await?;
            let file = store.find_file(&id).await?.unwrap();
            if temporary {
                write_temporary(&store, &id, b"content").await?;
            }
            if destination {
                tokio::fs::write(&output, b"external").await?;
            }
            store.begin_finalizing(&id).await?;
            drop(store);
            let store = FileSubscriberStore::open(&state, ids, clock).await?;
            let expected = if temporary != destination {
                SubscribedFileStatus::Completed
            } else {
                SubscribedFileStatus::Failed
            };
            assert!(store.find_file(&id).await?.unwrap().status == expected);
            assert!(!file.temporary_output_path().exists());
            if destination {
                assert_eq!(tokio::fs::read(&output).await?, b"external");
            } else if temporary {
                assert_eq!(tokio::fs::read(&output).await?, b"content");
            } else {
                assert!(!output.exists());
            }
        }
        Ok(())
    }

    #[tokio::test]
    async fn unavailable_parent_defers_only_its_subscription_on_open() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (ids, clock) = dependencies();
        let state = dir.path().join("state");
        let parent = dir.path().join("parent");
        tokio::fs::create_dir(&parent).await?;
        let output = parent.join("output");
        let store = FileSubscriberStore::open(&state, ids.clone(), clock.clone()).await?;
        let id = rank_zero_subscription(&store, &output, b"content").await?;
        write_temporary(&store, &id, b"content").await?;
        store.begin_finalizing(&id).await?;
        tokio::fs::rename(&parent, dir.path().join("unavailable")).await?;
        drop(store);
        let store = FileSubscriberStore::open(&state, ids, clock).await?;
        assert!(store.find_file(&id).await?.unwrap().status == SubscribedFileStatus::Finalizing);
        let other_output = dir.path().join("other");
        let other_id = rank_zero_subscription(&store, &other_output, b"other content").await?;
        write_temporary(&store, &other_id, b"other content").await?;
        store.begin_finalizing(&other_id).await?;
        store.finalize(&other_id).await?;
        assert_eq!(tokio::fs::read(other_output).await?, b"other content");
        tokio::fs::rename(dir.path().join("unavailable"), &parent).await?;
        store.finalize(&id).await?;
        assert_eq!(tokio::fs::read(output).await?, b"content");
        Ok(())
    }

    #[tokio::test]
    async fn io_failures_keep_finalizing_until_placement_and_directory_sync_succeed() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (ids, clock) = dependencies();
        let store = FileSubscriberStore::open(&dir.path().join("state"), ids, clock).await?;
        let output = dir.path().join("output");
        let id = rank_zero_subscription(&store, &output, b"content").await?;
        write_temporary(&store, &id, b"content").await?;
        store.begin_finalizing(&id).await?;
        store.output_failures.lock().insert(OutputStep::Rename, 1);
        assert!(store.finalize(&id).await.is_err());
        assert!(!output.exists());
        assert!(store.find_file(&id).await?.unwrap().status == SubscribedFileStatus::Finalizing);
        store.output_failures.lock().insert(OutputStep::SyncDirectory, 1);
        assert!(store.finalize(&id).await.is_err());
        assert_eq!(tokio::fs::read(&output).await?, b"content");
        assert!(store.find_file(&id).await?.unwrap().status == SubscribedFileStatus::Finalizing);
        store.finalize(&id).await?;
        assert!(store.find_file(&id).await?.unwrap().status == SubscribedFileStatus::Completed);
        Ok(())
    }

    struct RecordingSleeper {
        durations: Mutex<Vec<Duration>>,
        permits: tokio::sync::Semaphore,
    }

    #[async_trait::async_trait]
    impl omnius_core_base::sleeper::Sleeper for RecordingSleeper {
        async fn sleep(&self, duration: Duration) {
            self.durations.lock().push(duration);
            if let Ok(permit) = self.permits.acquire().await {
                permit.forget();
            }
        }
    }

    impl RecordingSleeper {
        fn new() -> Self {
            Self {
                durations: Mutex::new(Vec::new()),
                permits: tokio::sync::Semaphore::new(0),
            }
        }
        async fn wait_count(&self, count: usize) -> TestResult {
            tokio::time::timeout(Duration::from_secs(30), async {
                while self.durations.lock().len() < count {
                    tokio::task::yield_now().await;
                }
            })
            .await?;
            Ok(())
        }
    }

    #[tokio::test]
    async fn task_decoder_retries_finalizing_with_backoff_and_responds_to_shutdown() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (ids, clock) = dependencies();
        let store = FileSubscriberStore::open(&dir.path().join("state"), ids, clock.clone()).await?;
        let output = dir.path().join("output");
        let id = rank_zero_subscription(&store, &output, b"content").await?;
        write_temporary(&store, &id, b"content").await?;
        store.begin_finalizing(&id).await?;
        store.output_failures.lock().insert(OutputStep::Rename, 3);
        let sleeper = Arc::new(RecordingSleeper::new());
        let task = TaskDecoder::new(store.clone(), clock.clone(), sleeper.clone()).await?;
        for count in 1..=3 {
            sleeper.wait_count(count).await?;
            assert!(store.find_file(&id).await?.unwrap().status == SubscribedFileStatus::Finalizing);
            if count != 3 {
                sleeper.permits.add_permits(1);
            }
        }
        assert_eq!(*sleeper.durations.lock(), [Duration::from_secs(1), Duration::from_secs(2), Duration::from_secs(4)]);
        let subscriber = FileSubscriber {
            store: store.clone(),
            task_decoder: task.clone(),
        };
        assert!(tokio::time::timeout(Duration::from_secs(1), subscriber.cancel(&id)).await?.is_err());
        assert!(tokio::time::timeout(Duration::from_secs(1), subscriber.remove(&id)).await?.is_err());
        tokio::time::timeout(Duration::from_secs(1), subscriber.shutdown()).await?;
        assert!(store.find_file(&id).await?.unwrap().status == SubscribedFileStatus::Finalizing);
        let task = TaskDecoder::new(store.clone(), clock, Arc::new(SleeperImpl)).await?;
        wait_status(&store, &id, SubscribedFileStatus::Completed).await?;
        task.shutdown().await;
        assert_eq!(tokio::fs::read(&output).await?, b"content");
        Ok(())
    }

    #[tokio::test]
    async fn startup_sweep_failure_is_retried_after_parent_returns() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (ids, clock) = dependencies();
        let state = dir.path().join("state");
        let parent = dir.path().join("parent");
        let unavailable = dir.path().join("unavailable");
        tokio::fs::create_dir(&parent).await?;
        let store = FileSubscriberStore::open(&state, ids.clone(), clock.clone()).await?;
        let id = rank_zero_subscription(&store, &parent.join("output"), b"content").await?;
        write_temporary(&store, &id, b"content").await?;
        let file = store.find_file(&id).await?.unwrap();
        store.cancel(&id).await?;
        tokio::fs::rename(&parent, &unavailable).await?;
        drop(store);
        let store = FileSubscriberStore::open(&state, ids, clock.clone()).await?;
        assert!(store.needs_sweep());
        let sleeper = Arc::new(RecordingSleeper::new());
        let task = TaskDecoder::new(store.clone(), clock, sleeper.clone()).await?;
        sleeper.wait_count(1).await?;
        tokio::fs::rename(&unavailable, &parent).await?;
        sleeper.permits.add_permits(1);
        tokio::time::timeout(Duration::from_secs(30), async {
            while store.needs_sweep() || file.temporary_output_path().exists() {
                tokio::task::yield_now().await;
            }
        })
        .await?;
        task.shutdown().await;
        assert!(store.find_file(&id).await?.unwrap().status == SubscribedFileStatus::Canceled);
        assert!(!file.temporary_output_path().exists());
        assert!(!file.output_path().exists());
        Ok(())
    }

    #[tokio::test]
    async fn task_decoder_retries_temporary_sweep_with_backoff() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (ids, clock) = dependencies();
        let store = FileSubscriberStore::open(&dir.path().join("state"), ids, clock.clone()).await?;
        let output = dir.path().join("output");
        let id = rank_zero_subscription(&store, &output, b"content").await?;
        write_temporary(&store, &id, b"content").await?;
        let file = store.find_file(&id).await?.unwrap();
        store.cancel(&id).await?;
        store.output_failures.lock().insert(OutputStep::DeleteTemporary, 4);
        assert!(store.discard_output(&id).await.is_err());
        let sleeper = Arc::new(RecordingSleeper::new());
        let task = TaskDecoder::new(store.clone(), clock, sleeper.clone()).await?;
        for count in 1..=3 {
            sleeper.wait_count(count).await?;
            sleeper.permits.add_permits(1);
        }
        tokio::time::timeout(Duration::from_secs(30), async {
            while file.temporary_output_path().exists() {
                tokio::task::yield_now().await;
            }
        })
        .await?;
        task.shutdown().await;
        assert_eq!(*sleeper.durations.lock(), [Duration::from_secs(1), Duration::from_secs(2), Duration::from_secs(4)]);
        assert!(!store.needs_sweep());
        Ok(())
    }

    #[tokio::test]
    async fn subscribe_rejects_reserved_names_but_accepts_similar_names() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (ids, clock) = dependencies();
        let store = FileSubscriberStore::open(&dir.path().join("state"), ids, clock).await?;
        let root = OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, b"root");
        for name in [".output.axus-0.000000000.aa.part", ".output.AXUS-0.000000000.AA.PART", ".output.AxUs-0.000000000.aA.Part"] {
            let reserved = dir.path().join(name);
            let error = store.subscribe(&root, reserved.to_str().unwrap(), None, 0).await.unwrap_err();
            assert_eq!(*error.kind(), ErrorKind::InvalidFormat);
            assert!(error.to_string().contains("reserved"));
            assert!(!reserved.exists());
        }
        for name in [
            "output.axus-0.000000000.00.part",
            ".output.axus-0.000000000.00.parts",
            ".output.axus-0.00000000.00.part",
            ".output.axus-0.000000000.gg.part",
            ".output.axus-name.part",
        ] {
            store.subscribe(&root, dir.path().join(name).to_str().unwrap(), None, 0).await?;
        }
        Ok(())
    }

    #[tokio::test]
    async fn subscribe_rejects_another_subscriptions_temporary_output() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (ids, clock) = dependencies();
        let store = FileSubscriberStore::open(&dir.path().join("state"), ids, clock).await?;
        let root = OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, b"root");
        let id = store.subscribe(&root, dir.path().join("foo").to_str().unwrap(), None, 0).await?;
        let file = store.find_file(&id).await?.unwrap();
        let error = store.subscribe(&root, file.temporary_output_path().to_str().unwrap(), None, 0).await.unwrap_err();
        assert_eq!(*error.kind(), ErrorKind::InvalidFormat);
        assert!(error.to_string().contains("reserved"));
        assert!(store.find_file(&id).await?.unwrap().status == SubscribedFileStatus::Downloading);
        assert!(!file.temporary_output_path().exists());
        Ok(())
    }

    #[tokio::test]
    async fn subscribe_rejects_existing_file_and_directory() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (ids, clock) = dependencies();
        let store = FileSubscriberStore::open(&dir.path().join("state"), ids, clock).await?;
        let root = OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, b"root");
        let output = dir.path().join("file");
        tokio::fs::write(&output, b"original").await?;
        assert!(store.subscribe(&root, output.to_str().unwrap(), None, 0).await.is_err());
        assert_eq!(tokio::fs::read(&output).await?, b"original");
        let output = dir.path().join("directory");
        tokio::fs::create_dir(&output).await?;
        tokio::fs::write(output.join("child"), b"original").await?;
        assert!(store.subscribe(&root, output.to_str().unwrap(), None, 0).await.is_err());
        assert_eq!(tokio::fs::read(output.join("child")).await?, b"original");
        assert!(store.repo.get_committed_files().await?.is_empty());
        Ok(())
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn subscribe_rejects_symlink_and_dangling_symlink() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (ids, clock) = dependencies();
        let store = FileSubscriberStore::open(&dir.path().join("state"), ids, clock).await?;
        let root = OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, b"root");
        let target = dir.path().join("target");
        tokio::fs::write(&target, b"original").await?;
        for name in ["link", "dangling"] {
            let output = dir.path().join(name);
            let destination = if name == "link" { target.clone() } else { dir.path().join("missing") };
            std::os::unix::fs::symlink(&destination, &output)?;
            assert!(store.subscribe(&root, output.to_str().unwrap(), None, 0).await.is_err());
            assert_eq!(tokio::fs::read_link(&output).await?, destination);
        }
        assert_eq!(tokio::fs::read(&target).await?, b"original");
        Ok(())
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn subscribe_resolves_parent_before_reserving_output() -> TestResult {
        let dir = tempfile::tempdir()?;
        let parent = dir.path().join("parent");
        tokio::fs::create_dir(&parent).await?;
        let alias = dir.path().join("alias");
        std::os::unix::fs::symlink(&parent, &alias)?;
        let (ids, clock) = dependencies();
        let store = FileSubscriberStore::open(&dir.path().join("state"), ids, clock).await?;
        let root = OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, b"root");
        let id = store.subscribe(&root, alias.join("output").to_str().unwrap(), None, 0).await?;
        let file = store.find_file(&id).await?.unwrap();
        assert_eq!(file.output_directory, tokio::fs::canonicalize(&parent).await?.to_str().unwrap());
        assert!(store.subscribe(&root, parent.join("output").to_str().unwrap(), None, 0).await.is_err());
        store.cancel(&id).await?;
        store.subscribe(&root, parent.join("output").to_str().unwrap(), None, 0).await?;
        Ok(())
    }

    #[tokio::test]
    async fn downloaded_progress_survives_reopen_with_block() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (ids, clock) = dependencies();
        let store = FileSubscriberStore::open(dir.path(), ids.clone(), clock.clone()).await?;
        let root = OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, b"root");
        let id = store.subscribe(&root, "output", None, 0).await?;
        assert!(store.write_block(&root, &root, Bytes::from_static(b"root")).await?);
        drop(store);
        let store = FileSubscriberStore::open(dir.path(), ids, clock).await?;
        let file = store.find_file(&id).await?.unwrap();
        assert_eq!(file.block_count_downloaded, 1);
        assert!(file.status == SubscribedFileStatus::Decoding);
        assert!(store.blocks(&root, SubscribedFile::UNKNOWN_ROOT_RANK).await?[0].downloaded);
        assert_eq!(store.read_block(&root, &root).await?, Some(b"root".to_vec()));
        Ok(())
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
        assert!(!store.begin_finalizing(&first).await?);
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
        let temporary = store.find_file(&id).await?.unwrap().temporary_output_path();
        let subscriber = FileSubscriber { store, task_decoder };
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                if tokio::fs::metadata(&temporary).await.is_ok_and(|meta| meta.len() > 0) {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await?;
        assert!(subscriber.store.find_file(&id).await?.unwrap().status == SubscribedFileStatus::Decoding);
        tokio::fs::write(&output, b"external").await?;
        if remove {
            subscriber.remove(&id).await?;
        } else {
            subscriber.cancel(&id).await?;
        }
        subscriber.shutdown().await;
        assert_eq!(tokio::fs::read(&output).await?, b"external");
        assert!(!temporary.exists());
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
