use super::{store::FilePublisherStore, *};
use crate::base::runtime::Shutdown;
use async_trait::async_trait;
use chrono::Utc;
use omnius_core_base::{clock::Clock, sleeper::Sleeper};
use omnius_core_omnikit::generated::omni_hash::{OmniHash, OmniHashAlgorithmType};
use parking_lot::Mutex;
use std::{
    collections::{HashMap, hash_map::Entry},
    io::Cursor,
    sync::Arc,
    time::Duration,
};
use tokio::{
    fs::File,
    io::{AsyncRead, AsyncReadExt, BufReader},
    sync::{Mutex as TokioMutex, Notify},
    task::JoinHandle,
};
use tokio_util::{bytes::Bytes, sync::CancellationToken};

pub struct TaskEncoder {
    store: Arc<FilePublisherStore>,
    clock: Arc<dyn Clock<Utc> + Send + Sync>,
    sleeper: Arc<dyn Sleeper + Send + Sync>,
    enqueue_notify: Notify,
    active_jobs: Mutex<HashMap<String, CancellationToken>>,
    join_handle: TokioMutex<Option<JoinHandle<()>>>,
    token: CancellationToken,
}

#[async_trait]
impl Shutdown for TaskEncoder {
    async fn shutdown(&self) {
        self.token.cancel();
        if let Some(handle) = self.join_handle.lock().await.take() {
            let _ = handle.await;
        }
    }
}

impl TaskEncoder {
    pub async fn new(store: Arc<FilePublisherStore>, clock: Arc<dyn Clock<Utc> + Send + Sync>, sleeper: Arc<dyn Sleeper + Send + Sync>) -> Result<Arc<Self>> {
        let task = Arc::new(Self {
            store,
            clock,
            sleeper,
            enqueue_notify: Notify::new(),
            active_jobs: Mutex::new(HashMap::new()),
            join_handle: TokioMutex::new(None),
            token: CancellationToken::new(),
        });
        let this = task.clone();
        *task.join_handle.lock().await = Some(tokio::spawn(async move {
            tokio::join!(this.run_encoding(), this.run_sweeps());
        }));
        Ok(task)
    }

    async fn run_encoding(&self) {
        while !self.token.is_cancelled() {
            if self.encode().await {
                continue;
            }
            tokio::select! {
                _ = self.enqueue_notify.notified() => {},
                _ = self.token.cancelled() => break,
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
                    warn!(?error, "publisher block sweep retry failed");
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
    pub fn cancel(&self, id: &str) {
        if let Some(token) = self.active_jobs.lock().get(id) {
            token.cancel();
        }
    }

    async fn encode(&self) -> bool {
        let file = match self.store.next_file().await {
            Ok(Some(file)) => file,
            Ok(None) => return false,
            Err(error) => {
                warn!(?error, at = %self.clock.now(), "encoding queue fetch failed");
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
        let result = self.encode_file(&file.id, &token).await;
        if let Err(error) = result {
            warn!(?error, "encode file error");
            if !token.is_cancelled()
                && let Err(error) = self.store.fail(&file.id, &error.to_string()).await
            {
                warn!(?error, "encode failure recording failed");
            }
        }
        if let Err(error) = self.store.cleanup_blocks(&file.id).await {
            warn!(?error, "uncommitted block cleanup failed");
        }
        self.active_jobs.lock().remove(&file.id);
        true
    }

    async fn encode_file(&self, file_id: &str, token: &CancellationToken) -> Result<Option<()>> {
        if token.is_cancelled() {
            return Ok(None);
        }
        let Some(file) = self.store.claim(file_id).await? else {
            return Ok(None);
        };
        let mut input = File::open(&file.file_path).await?;
        let Some(mut all_blocks) = self.encode_bytes(&mut input, &file.id, file.block_size, 0, token).await? else {
            return Ok(None);
        };
        let mut hashes: Vec<OmniHash> = all_blocks.iter().map(|block| block.block_hash.clone()).collect();
        let mut rank = 1;
        loop {
            let hash_count = hashes.len();
            let layer = MerkleLayer { rank: rank - 1, hashes };
            let mut reader = BufReader::new(Cursor::new(layer.export()?));
            let Some(blocks) = self.encode_bytes(&mut reader, &file.id, file.block_size, rank, token).await? else {
                return Ok(None);
            };
            if blocks.len() > 1 && blocks.len() >= hash_count {
                return Err(Error::new(ErrorKind::InvalidFormat).with_message("block size is too small for a Merkle layer"));
            }
            hashes = blocks.iter().map(|block| block.block_hash.clone()).collect();
            let is_root = blocks.len() == 1;
            all_blocks.extend(blocks);
            if is_root {
                break;
            }
            rank += 1;
        }
        if token.is_cancelled() {
            return Ok(None);
        }
        let root_hash = hashes.pop().unwrap();
        if !self.store.commit(&file, &root_hash, &all_blocks).await? {
            return Ok(None);
        }
        Ok(Some(()))
    }

    async fn encode_bytes<R>(&self, reader: &mut R, file_id: &str, max_block_size: u32, rank: u32, token: &CancellationToken) -> Result<Option<Vec<PublishedUncommittedBlock>>>
    where
        R: AsyncRead + Unpin,
    {
        let mut uncommitted_blocks: Vec<PublishedUncommittedBlock> = Vec::new();
        let mut index = 0;

        loop {
            if token.is_cancelled() {
                return Ok(None);
            }

            let mut block: Vec<u8> = Vec::new();
            let mut take = reader.take(max_block_size as u64);
            let n = tokio::select! {
                _ = token.cancelled() => return Ok(None),
                result = take.read_to_end(&mut block) => result?,
            };
            if n == 0 {
                break;
            }

            let block_hash = OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, &block);

            let uncommitted_block = PublishedUncommittedBlock {
                file_id: file_id.to_string(),
                block_hash: block_hash.clone(),
                rank,
                index,
            };
            if !self.store.write_block(&uncommitted_block, Bytes::from(block)).await? {
                return Ok(None);
            }
            uncommitted_blocks.push(uncommitted_block);

            index += 1;
        }

        Ok(Some(uncommitted_blocks))
    }
}
