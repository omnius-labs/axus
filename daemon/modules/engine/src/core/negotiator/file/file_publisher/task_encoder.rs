use std::{io::Cursor, sync::Arc};

use async_trait::async_trait;
use chrono::Utc;
use futures::FutureExt as _;
use parking_lot::Mutex;
use tokio::{
    fs::File,
    io::{AsyncRead, AsyncReadExt, BufReader},
    sync::Mutex as TokioMutex,
    task::JoinHandle,
};
use tokio_util::bytes::Bytes;
use tracing::warn;

use omnius_core_base::{clock::Clock, sleeper::Sleeper, tsid::TsidProvider};
use omnius_core_omnikit::model::{OmniHash, OmniHashAlgorithmType};
use omnius_core_rocketpack::RocketMessage;

use crate::{
    base::{Shutdown, storage::KeyValueRocksdbStorage, sync::EventListener},
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

    current_encoding_file_id: Arc<Mutex<Option<String>>>,
    enqueue_event_listener: Arc<EventListener>,
    cancel_event_listener: Arc<EventListener>,

    join_handle: Arc<TokioMutex<Option<JoinHandle<()>>>>,
}

#[async_trait]
impl Shutdown for TaskEncoder {
    async fn shutdown(&self) {
        if let Some(join_handle) = self.join_handle.lock().await.take() {
            join_handle.abort();
            let _ = join_handle.fuse().await;
        }
    }
}

// TODO: encode処理中断後のごみ処理が未実装

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

            current_encoding_file_id: Arc::new(Mutex::new(None)),
            enqueue_event_listener: Arc::new(EventListener::new()),
            cancel_event_listener: Arc::new(EventListener::new()),

            join_handle: Arc::new(TokioMutex::new(None)),
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

        let guard = self.current_encoding_file_id.lock();

        let Some(current_encoding_file_id) = guard.as_ref() else {
            return Ok(());
        };

        if current_encoding_file_id != file_id {
            return Ok(());
        }

        self.cancel_event_listener.notify();

        Ok(())
    }

    async fn start(self: Arc<Self>) -> Result<()> {
        let this = self.clone();
        *self.join_handle.lock().await = Some(tokio::spawn(async move {
            loop {
                tokio::select! {
                    next = this.encode() => {
                        if next {
                            continue;
                        }
                    }
                    _ = this.cancel_event_listener.wait() => {
                        info!("import task canceled");
                    }
                };

                this.enqueue_event_listener.wait().await;
            }
        }));

        Ok(())
    }

    async fn encode(&self) -> bool {
        let Some(uncommitted_file) = self.pickup().await else {
            return false;
        };

        if let Err(e) = self.encode_file(uncommitted_file).await {
            warn!(error = ?e, "encode file error");
        }

        *self.current_encoding_file_id.lock() = None;

        true
    }

    async fn pickup(&self) -> Option<PublishedUncommittedFile> {
        let uncommitted_file = match self.file_publisher_repo.find_uncommitted_file_by_encoding_next().await {
            Ok(uncommitted_file) => uncommitted_file,
            Err(e) => {
                info!(error_message = e.to_string(), "uncommitted file not found",);
                return None;
            }
        };
        let Some(uncommitted_file) = uncommitted_file else {
            *self.current_encoding_file_id.lock() = None;
            return None;
        };

        *self.current_encoding_file_id.lock() = Some(uncommitted_file.id.clone());

        // current_import_file_idがセットされる前に状態が変わる可能性があるため、再度取得する
        let uncommitted_file = match self.file_publisher_repo.find_uncommitted_file_by_id(&uncommitted_file.id).await {
            Ok(uncommitted_file) => uncommitted_file,
            Err(e) => {
                warn!(error_message = e.to_string(), "uncommitted file did lost",);
                return None;
            }
        };
        let Some(uncommitted_file) = uncommitted_file else {
            *self.current_encoding_file_id.lock() = None;
            return None;
        };

        Some(uncommitted_file)
    }

    async fn encode_file(&self, uncommitted_file: PublishedUncommittedFile) -> Result<()> {
        if uncommitted_file.status == PublishedUncommittedFileStatus::Canceled {
            self.file_publisher_repo.delete_uncommitted_file(&uncommitted_file.id).await?;
            return Ok(());
        }

        let mut all_uncommitted_blocks: Vec<PublishedUncommittedBlock> = Vec::new();
        let mut current_block_hashes: Vec<OmniHash> = Vec::new();

        let mut f = File::open(uncommitted_file.file_path.as_str()).await?;
        let mut uncommitted_blocks = self.encode_bytes(&mut f, &uncommitted_file.id, uncommitted_file.block_size, 0).await?;
        all_uncommitted_blocks.extend(uncommitted_blocks.iter().cloned());
        current_block_hashes.extend(uncommitted_blocks.iter().map(|block| block.block_hash.clone()));

        let mut rank = 1;
        loop {
            let merkle_layer = MerkleLayer {
                rank,
                hashes: std::mem::take(&mut current_block_hashes),
            };

            let bytes = merkle_layer.export()?;
            let bytes_slice = bytes.as_ref();
            let cursor = Cursor::new(bytes_slice);
            let mut reader = BufReader::new(cursor);

            uncommitted_blocks = self.encode_bytes(&mut reader, &uncommitted_file.id, uncommitted_file.block_size, rank).await?;
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
                self.file_publisher_repo.delete_uncommitted_file(&uncommitted_file.id).await?;

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

    async fn encode_bytes<R>(&self, reader: &mut R, file_id: &str, max_block_size: u32, rank: u32) -> Result<Vec<PublishedUncommittedBlock>>
    where
        R: AsyncRead + Unpin,
    {
        let mut uncommitted_blocks: Vec<PublishedUncommittedBlock> = Vec::new();
        let mut index = 0;

        loop {
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

        Ok(uncommitted_blocks)
    }
}
