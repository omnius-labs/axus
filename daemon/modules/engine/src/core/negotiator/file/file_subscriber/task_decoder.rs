use std::{
    collections::{HashMap, hash_map::Entry},
    io::Cursor,
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use chrono::Utc;
use parking_lot::Mutex;
use tokio::{
    io::{AsyncWrite, AsyncWriteExt},
    sync::{Mutex as TokioMutex, Notify},
    task::JoinHandle,
};
use tokio_util::{bytes::Bytes, sync::CancellationToken};

use omnius_core_base::{clock::Clock, sleeper::Sleeper};
use omnius_core_omnikit::generated::omni_hash::OmniHash;

use super::{store::FileSubscriberStore, *};
use crate::base::runtime::Shutdown;

pub struct TaskDecoder {
    store: Arc<FileSubscriberStore>,
    clock: Arc<dyn Clock<Utc> + Send + Sync>,
    sleeper: Arc<dyn Sleeper + Send + Sync>,
    enqueue_notify: Notify,
    active_jobs: Mutex<HashMap<String, CancellationToken>>,
    jobs_finished: Notify,
    join_handle: TokioMutex<Option<JoinHandle<()>>>,
    token: CancellationToken,
}

#[async_trait]
impl Shutdown for TaskDecoder {
    async fn shutdown(&self) {
        self.token.cancel();
        if let Some(handle) = self.join_handle.lock().await.take() {
            let _ = handle.await;
        }
    }
}

impl TaskDecoder {
    pub async fn new(store: Arc<FileSubscriberStore>, clock: Arc<dyn Clock<Utc> + Send + Sync>, sleeper: Arc<dyn Sleeper + Send + Sync>) -> Result<Arc<Self>> {
        let task = Arc::new(Self {
            store,
            clock,
            sleeper,
            enqueue_notify: Notify::new(),
            active_jobs: Mutex::new(HashMap::new()),
            jobs_finished: Notify::new(),
            join_handle: TokioMutex::new(None),
            token: CancellationToken::new(),
        });
        let this = task.clone();
        *task.join_handle.lock().await = Some(tokio::spawn(async move {
            tokio::join!(this.run_decoding(), this.run_finalizations(), this.run_sweeps());
        }));
        Ok(task)
    }

    async fn run_decoding(&self) {
        while !self.token.is_cancelled() {
            if self.decode().await {
                continue;
            }
            tokio::select! {
                _ = self.enqueue_notify.notified() => {},
                _ = self.token.cancelled() => break,
            }
        }
    }

    async fn run_finalizations(&self) {
        let mut delay = Duration::from_secs(1);
        while !self.token.is_cancelled() {
            let mut retry = false;
            match self.store.finalizing_files().await {
                Ok(files) if files.is_empty() => {
                    delay = Duration::from_secs(1);
                    tokio::select! {
                        _ = self.store.finalization_requested() => {},
                        _ = self.token.cancelled() => break,
                    }
                    continue;
                }
                Ok(files) => {
                    for file in files {
                        if self.token.is_cancelled() {
                            break;
                        }
                        if let Err(error) = self.store.finalize(&file.id).await {
                            warn!(?error, id = %file.id, "subscriber output finalization retry failed");
                            retry = true;
                        }
                    }
                }
                Err(error) => {
                    warn!(?error, "subscriber finalizing queue fetch failed");
                    retry = true;
                }
            }
            if retry {
                tokio::select! {
                    _ = self.sleeper.sleep(delay) => {},
                    _ = self.token.cancelled() => break,
                }
                delay = (delay * 2).min(Duration::from_secs(60));
            } else {
                delay = Duration::from_secs(1);
            }
        }
    }

    async fn run_sweeps(&self) {
        let mut delay = Duration::from_secs(1);
        while !self.token.is_cancelled() {
            if !self.store.needs_sweep() {
                tokio::select! {
                    _ = self.store.sweep_requested() => {},
                    _ = self.token.cancelled() => break,
                }
                continue;
            }
            match self.store.sweep().await {
                Ok(()) => delay = Duration::from_secs(1),
                Err(error) => {
                    warn!(?error, "subscriber sweep retry failed");
                    tokio::select! {
                        _ = self.sleeper.sleep(delay) => {},
                        _ = self.token.cancelled() => break,
                    }
                    delay = (delay * 2).min(Duration::from_secs(60));
                }
            }
        }
    }

    pub fn wake(&self) {
        self.enqueue_notify.notify_one();
    }
    pub async fn cancel(&self, id: &str) {
        loop {
            let finished = self.jobs_finished.notified();
            tokio::pin!(finished);
            finished.as_mut().enable();
            let token = self.active_jobs.lock().get(id).cloned();
            let Some(token) = token else {
                return;
            };
            token.cancel();
            finished.await;
        }
    }

    async fn decode(&self) -> bool {
        let file = match self.store.next_file().await {
            Ok(Some(file)) => file,
            Ok(None) => return false,
            Err(error) => {
                warn!(?error, at = %self.clock.now(), "decoding queue fetch failed");
                tokio::select! { _ = self.sleeper.sleep(std::time::Duration::from_secs(1)) => {}, _ = self.token.cancelled() => {} }
                self.wake();
                return false;
            }
        };
        let token = match self.active_jobs.lock().entry(file.id.clone()) {
            Entry::Occupied(_) => return false,
            Entry::Vacant(entry) => {
                let token = self.token.child_token();
                entry.insert(token.clone());
                token
            }
        };
        if let Err(error) = self.decode_file(&file.id, &token).await {
            warn!(?error, "decode file error");
            if !token.is_cancelled()
                && let Err(error) = self.store.fail(&file.id, &error.to_string()).await
            {
                warn!(?error, "decode failure recording failed");
            }
        }
        self.active_jobs.lock().remove(&file.id);
        self.jobs_finished.notify_waiters();
        true
    }

    async fn decode_file(&self, id: &str, token: &CancellationToken) -> Result<Option<()>> {
        if token.is_cancelled() {
            return Ok(None);
        }
        let Some(file) = self.store.find_file(id).await? else {
            return Ok(None);
        };
        if file.status != SubscribedFileStatus::Decoding {
            return Ok(None);
        }
        let blocks = self.store.blocks(&file.root_hash, file.rank).await?;
        let hashes: Vec<_> = blocks.into_iter().map(|block| block.block_hash).collect();
        if token.is_cancelled() {
            return Ok(None);
        }
        if file.rank == 0 {
            let Some(mut output) = self.store.create_output(&file).await? else {
                return Ok(None);
            };
            let result = async {
                if self.decode_bytes(&mut output.file, &file.root_hash, &hashes, token).await?.is_none() {
                    return Ok(None);
                }
                if token.is_cancelled() {
                    return Ok(None);
                }
                if let Err(error) = self.store.sync_output(&mut output).await {
                    warn!(?error, "subscriber temporary output sync failed");
                    tokio::select! { _ = self.sleeper.sleep(Duration::from_secs(1)) => {}, _ = token.cancelled() => {} }
                    return Ok(None);
                }
                if token.is_cancelled() || !self.store.begin_finalizing(id).await? {
                    return Ok(None);
                }
                Result::Ok(Some(()))
            }
            .await;
            let closing = self.store.close_output(output).await;
            if matches!(result, Ok(Some(()))) {
                if let Err(error) = self.store.finalize(id).await {
                    warn!(?error, "subscriber output finalization deferred");
                }
            } else {
                self.store.discard_output(id).await?;
            }
            closing?;
            result
        } else {
            let mut output = Cursor::new(Vec::new());
            if self.decode_bytes(&mut output, &file.root_hash, &hashes, token).await?.is_none() {
                return Ok(None);
            }
            let layer = MerkleLayer::import(&Bytes::from(output.into_inner()))?;
            if file.rank != SubscribedFile::UNKNOWN_ROOT_RANK && Some(layer.rank) != file.rank.checked_sub(1) {
                return Err(Error::new(ErrorKind::InvalidFormat).with_message("unexpected merkle layer rank"));
            }
            if token.is_cancelled() || !self.store.advance_layer(&file, layer).await? {
                return Ok(None);
            }
            Ok(Some(()))
        }
    }

    async fn decode_bytes<W>(&self, writer: &mut W, root_hash: &OmniHash, hashes: &[OmniHash], token: &CancellationToken) -> Result<Option<()>>
    where
        W: AsyncWrite + Unpin,
    {
        for hash in hashes {
            if token.is_cancelled() {
                return Ok(None);
            }
            let block = self.store.read_block(root_hash, hash).await?;
            if token.is_cancelled() {
                return Ok(None);
            }
            let Some(block) = block else {
                return Err(Error::new(ErrorKind::IoError).with_message("decoding error: block is not found"));
            };
            tokio::select! {
                _ = token.cancelled() => return Ok(None),
                result = writer.write_all(&block) => result?,
            }
        }
        Ok(Some(()))
    }
}
