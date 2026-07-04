use std::{
    collections::{HashMap, hash_map::Entry},
    io::Cursor,
    sync::Arc,
};

use async_trait::async_trait;
use chrono::Utc;
use parking_lot::Mutex;
use tokio::{
    fs::File,
    io::{AsyncRead, AsyncReadExt, BufReader},
    sync::Mutex as TokioMutex,
    task::JoinHandle,
};
use tokio_util::{bytes::Bytes, sync::CancellationToken};
use tracing::warn;

use omnius_core_base::{clock::Clock, sleeper::Sleeper, tsid::TsidProvider};
use omnius_core_omnikit::model::{OmniHash, OmniHashAlgorithmType};

use crate::{
    base::{runtime::Shutdown, storage::KeyValueRocksdbStorage, sync::EventListener},
    core::negotiator::file::model::PublishedUncommittedFile,
    prelude::*,
};

use super::*;

#[allow(unused)]
pub struct TaskEncoder {
    file_publisher_repo: Arc<FilePublisherRepo>,
    blocks_storage: Arc<KeyValueRocksdbStorage>,

    tsid_provider: Arc<Mutex<dyn TsidProvider + Send + Sync>>,
    clock: Arc<dyn Clock<Utc> + Send + Sync>,
    sleeper: Arc<dyn Sleeper + Send + Sync>,

    enqueue_event_listener: Arc<EventListener>,
    active_jobs: Arc<Mutex<HashMap<String, CancellationToken>>>,

    join_handle: Arc<TokioMutex<Option<JoinHandle<()>>>>,
    token: CancellationToken,
}

#[async_trait]
impl Shutdown for TaskEncoder {
    async fn shutdown(&self) {
        self.token.cancel();
        if let Some(join_handle) = self.join_handle.lock().await.take() {
            let _ = join_handle.await;
        }
    }
}

#[allow(unused)]
impl TaskEncoder {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        file_publisher_repo: Arc<FilePublisherRepo>,
        blocks_storage: Arc<KeyValueRocksdbStorage>,

        tsid_provider: Arc<Mutex<dyn TsidProvider + Send + Sync>>,
        clock: Arc<dyn Clock<Utc> + Send + Sync>,
        sleeper: Arc<dyn Sleeper + Send + Sync>,
    ) -> Result<Arc<Self>> {
        let v = Arc::new(Self {
            file_publisher_repo,
            blocks_storage,

            tsid_provider,
            clock,
            sleeper,

            enqueue_event_listener: Arc::new(EventListener::new()),
            active_jobs: Arc::new(Mutex::new(HashMap::new())),

            join_handle: Arc::new(TokioMutex::new(None)),
            token: CancellationToken::new(),
        });
        v.clone().start().await?;

        Ok(v)
    }

    pub async fn import(&self, file_path: &str, file_name: &str, block_size: u32, attrs: Option<&str>, priority: i64) -> Result<()> {
        let id = self.tsid_provider.lock().create().to_string();
        let now = self.clock.now();

        let file = PublishedUncommittedFile {
            id,
            file_path: file_path.to_string(),
            file_name: file_name.to_string(),
            block_size,
            attrs: attrs.map(|n| n.to_string()),
            priority,
            status: PublishedUncommittedFileStatus::Pending,
            failed_reason: None,
            created_at: now,
            updated_at: now,
        };

        self.file_publisher_repo.insert_uncommitted_file(&file).await?;

        self.enqueue_event_listener.notify();

        Ok(())
    }

    pub async fn cancel(&self, file_id: &str) -> Result<()> {
        self.file_publisher_repo
            .update_uncommitted_file_status(file_id, &PublishedUncommittedFileStatus::Canceled)
            .await?;

        if let Some(token) = self.active_jobs.lock().get(file_id) {
            token.cancel();
        }

        Ok(())
    }

    async fn start(self: Arc<Self>) -> Result<()> {
        let this = self.clone();
        *self.join_handle.lock().await = Some(tokio::spawn(async move {
            this.cleanup().await;
            while !this.token.is_cancelled() {
                if this.encode().await {
                    continue;
                }
                tokio::select! {
                    _ = this.enqueue_event_listener.wait() => {}
                    _ = this.token.cancelled() => break,
                }
            }
        }));

        Ok(())
    }

    async fn cleanup(&self) {
        const CHUNK_SIZE: i64 = 5000;

        loop {
            let blocks = match self.file_publisher_repo.get_uncommitted_blocks(CHUNK_SIZE).await {
                Ok(vs) => vs,
                Err(e) => {
                    error!(error_message = e.to_string(), "uncommitted blocks fetch failed");
                    break;
                }
            };
            if blocks.is_empty() {
                break;
            }
            for b in &blocks {
                let path = gen_uncommitted_block_path(&b.file_id, &b.block_hash);
                if let Err(e) = self.blocks_storage.delete(path).await {
                    warn!(error = ?e);
                }
            }
            if let Err(e) = self.file_publisher_repo.delete_uncommitted_blocks(&blocks).await {
                error!(error_message = e.to_string(), "uncommitted blocks delete failed");
            }
        }
    }

    async fn encode(&self) -> bool {
        let file_id = match self.file_publisher_repo.find_uncommitted_file_by_encoding_next().await {
            Ok(Some(uncommitted_file)) => uncommitted_file.id,
            Ok(None) => {
                info!("uncommitted file not found");
                return false;
            }
            Err(e) => {
                error!(error_message = e.to_string(), "uncommitted fetch failed");
                return false;
            }
        };

        let token = match self.active_jobs.lock().entry(file_id.clone()) {
            Entry::Occupied(_) => return false,
            Entry::Vacant(e) => {
                let token = self.token.child_token();
                e.insert(token.clone());
                token
            }
        };

        if let Err(e) = self.encode_file(&file_id, &token).await {
            warn!(error = ?e, "encode file error");
            if let Err(e) = self.file_publisher_repo.update_uncommitted_file_as_failed(&file_id, &e.to_string()).await {
                warn!(error = ?e, "update uncommitted file as failed error");
            }
        }

        self.active_jobs.lock().remove(&file_id);

        true
    }

    async fn encode_file(&self, file_id: &str, token: &CancellationToken) -> Result<()> {
        let uncommitted_file = match self.file_publisher_repo.find_uncommitted_file_by_id(file_id).await {
            Ok(Some(uncommitted_file)) => uncommitted_file,
            Ok(None) => {
                info!("uncommitted file not found");
                return Ok(());
            }
            Err(e) => {
                error!(error_message = e.to_string(), "uncommitted fetch failed");
                return Ok(());
            }
        };

        if uncommitted_file.status == PublishedUncommittedFileStatus::Canceled {
            return Ok(());
        }

        let mut all_uncommitted_blocks: Vec<PublishedUncommittedBlock> = Vec::new();
        let mut current_block_hashes: Vec<OmniHash> = Vec::new();

        let mut f = File::open(uncommitted_file.file_path.as_str()).await?;
        let Some(uncommitted_blocks) = self.encode_bytes(&mut f, &uncommitted_file.id, uncommitted_file.block_size, 0, token).await? else {
            self.cleanup().await;
            return Ok(());
        };
        all_uncommitted_blocks.extend(uncommitted_blocks.iter().cloned());
        current_block_hashes.extend(uncommitted_blocks.iter().map(|block| block.block_hash.clone()));

        let mut rank = 1;
        loop {
            let merkle_layer = MerkleLayer {
                rank,
                hashes: std::mem::take(&mut current_block_hashes),
            };

            let bytes = merkle_layer.export()?;
            let cursor = Cursor::new(bytes);
            let mut reader = BufReader::new(cursor);

            let Some(uncommitted_blocks) = self.encode_bytes(&mut reader, &uncommitted_file.id, uncommitted_file.block_size, rank, token).await? else {
                self.cleanup().await;
                return Ok(());
            };
            all_uncommitted_blocks.extend(uncommitted_blocks.iter().cloned());
            current_block_hashes = uncommitted_blocks.iter().map(|block| block.block_hash.clone()).collect();

            if uncommitted_blocks.len() == 1 {
                break;
            }

            rank += 1;
        }

        let root_hash = current_block_hashes.pop().unwrap();

        if let Some(committed_file) = self.file_publisher_repo.find_committed_file_by_root_hash(&root_hash).await? {
            if committed_file.file_name == uncommitted_file.file_name {
                self.file_publisher_repo
                    .update_uncommitted_file_status(&uncommitted_file.id, &PublishedUncommittedFileStatus::Completed)
                    .await?;
                return Ok(());
            }

            let new_committed_file = PublishedCommittedFile {
                file_name: uncommitted_file.file_name.clone(),
                ..committed_file
            };

            for uncommitted_block in self.file_publisher_repo.find_uncommitted_blocks_by_file_id(&uncommitted_file.id).await? {
                let path = gen_uncommitted_block_path(&uncommitted_file.id, &uncommitted_block.block_hash);
                self.blocks_storage.delete(path.as_str()).await?;
            }

            self.file_publisher_repo.commit_file_without_blocks(&new_committed_file, &uncommitted_file.id).await?;
            return Ok(());
        }

        let now = self.clock.now();

        let committed_file = PublishedCommittedFile {
            root_hash: root_hash.clone(),
            file_name: uncommitted_file.file_name.clone(),
            block_size: uncommitted_file.block_size,
            attrs: uncommitted_file.attrs.clone(),
            created_at: now,
            updated_at: now,
        };
        let committed_blocks = all_uncommitted_blocks
            .iter()
            .map(|block| PublishedCommittedBlock {
                root_hash: root_hash.clone(),
                block_hash: block.block_hash.clone(),
                rank: block.rank,
                index: block.index,
            })
            .collect::<Vec<_>>();

        for uncommitted_block in all_uncommitted_blocks {
            let old_key = gen_uncommitted_block_path(&uncommitted_file.id, &uncommitted_block.block_hash);
            let new_key = gen_committed_block_path(&root_hash, &uncommitted_block.block_hash);
            self.blocks_storage.rename_key(old_key.as_str(), new_key.as_str(), false).await?;
        }

        self.file_publisher_repo
            .commit_file_with_blocks(&committed_file, &committed_blocks, &uncommitted_file.id)
            .await?;

        Ok(())
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
            let n = take.read_to_end(&mut block).await?;
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
            self.file_publisher_repo.insert_or_ignore_uncommitted_block(&uncommitted_block).await?;
            uncommitted_blocks.push(uncommitted_block);

            let path = gen_uncommitted_block_path(file_id, &block_hash);
            self.blocks_storage.put_value(path.as_str(), Bytes::from(block), true).await?;

            index += 1;
        }

        Ok(Some(uncommitted_blocks))
    }
}
