use std::{io::Cursor, sync::Arc};

use async_trait::async_trait;
use chrono::Utc;
use futures::FutureExt as _;
use parking_lot::Mutex;
use tokio::{
    fs::File,
    io::{AsyncWrite, AsyncWriteExt, BufWriter},
    sync::Mutex as TokioMutex,
    task::JoinHandle,
};
use tokio_util::bytes::Bytes;

use omnius_core_base::{clock::Clock, sleeper::Sleeper, tsid::TsidProvider};
use omnius_core_omnikit::model::OmniHash;

use crate::{
    base::{Shutdown, storage::KeyValueRocksdbStorage, sync::EventListener},
    core::negotiator::file::model::SubscribedFile,
    prelude::*,
};

use super::*;

#[allow(unused)]
pub struct TaskDecoder {
    file_subscriber_repo: Arc<FileSubscriberRepo>,
    blocks_storage: Arc<KeyValueRocksdbStorage>,

    tsid_provider: Arc<Mutex<dyn TsidProvider + Send + Sync>>,
    clock: Arc<dyn Clock<Utc> + Send + Sync>,
    sleeper: Arc<dyn Sleeper + Send + Sync>,

    current_decoding_file_id: Arc<Mutex<Option<String>>>,
    enqueue_event_listener: Arc<EventListener>,
    cancel_event_listener: Arc<EventListener>,

    join_handle: Arc<TokioMutex<Option<JoinHandle<()>>>>,
}

#[async_trait]
impl Shutdown for TaskDecoder {
    async fn shutdown(&self) {
        if let Some(join_handle) = self.join_handle.lock().await.take() {
            join_handle.abort();
            let _ = join_handle.fuse().await;
        }
    }
}

#[allow(unused)]
impl TaskDecoder {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        file_subscriber_repo: Arc<FileSubscriberRepo>,
        blocks_storage: Arc<KeyValueRocksdbStorage>,

        tsid_provider: Arc<Mutex<dyn TsidProvider + Send + Sync>>,
        clock: Arc<dyn Clock<Utc> + Send + Sync>,
        sleeper: Arc<dyn Sleeper + Send + Sync>,
    ) -> Result<Arc<Self>> {
        let v = Arc::new(Self {
            file_subscriber_repo,
            blocks_storage,

            tsid_provider,
            clock,
            sleeper,

            current_decoding_file_id: Arc::new(Mutex::new(None)),
            enqueue_event_listener: Arc::new(EventListener::new()),
            cancel_event_listener: Arc::new(EventListener::new()),

            join_handle: Arc::new(TokioMutex::new(None)),
        });
        v.clone().start().await?;

        Ok(v)
    }

    pub async fn export(&self, _file_id: &str) -> Result<()> {
        self.enqueue_event_listener.notify();

        Ok(())
    }

    pub async fn cancel(&self, file_id: &str) -> Result<()> {
        self.file_subscriber_repo.update_file_status(file_id, &SubscribedFileStatus::Canceled).await?;

        let guard = self.current_decoding_file_id.lock();

        let Some(current_decoding_file_id) = guard.as_ref() else {
            return Ok(());
        };

        if current_decoding_file_id != file_id {
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
                    next = this.decode() => {
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

    async fn decode(&self) -> bool {
        let Some(file) = self.pickup().await else {
            return false;
        };

        if let Err(e) = self.decode_file(file).await {
            warn!(error = ?e, "encode file error");
        }

        *self.current_decoding_file_id.lock() = None;

        true
    }

    async fn pickup(&self) -> Option<SubscribedFile> {
        let file = match self.file_subscriber_repo.find_file_by_decoding_next().await {
            Ok(file) => file,
            Err(e) => {
                info!(error_message = e.to_string(), "file not found",);
                return None;
            }
        };
        let Some(file) = file else {
            *self.current_decoding_file_id.lock() = None;
            return None;
        };

        *self.current_decoding_file_id.lock() = Some(file.id.clone());

        // current_import_file_idがセットされる前に状態が変わる可能性があるため、再度取得する
        let file = match self.file_subscriber_repo.find_file_by_id(&file.id).await {
            Ok(file) => file,
            Err(e) => {
                warn!(error_message = e.to_string(), "file did lost",);
                return None;
            }
        };
        let Some(file) = file else {
            *self.current_decoding_file_id.lock() = None;
            return None;
        };

        Some(file)
    }

    async fn decode_file(&self, file: SubscribedFile) -> Result<()> {
        if file.status == SubscribedFileStatus::Canceled {
            self.file_subscriber_repo.delete_file(&file.id).await?;
            return Ok(());
        }

        if file.rank == 0 {
            let blocks = self.file_subscriber_repo.find_blocks_by_root_hash_and_rank(&file.root_hash, file.rank).await?;

            let block_hashes: Vec<OmniHash> = blocks.iter().map(|n| n.block_hash.clone()).collect();

            let mut f = File::open(file.file_path).await?;
            self.decode_bytes(&mut f, &file.root_hash, &block_hashes).await?;

            self.file_subscriber_repo.update_file_status(&file.id, &SubscribedFileStatus::Completed).await?;
        } else {
            let blocks = self.file_subscriber_repo.find_blocks_by_root_hash_and_rank(&file.root_hash, file.rank).await?;

            let block_hashes: Vec<OmniHash> = blocks.iter().map(|n| n.block_hash.clone()).collect();

            let bytes: Vec<u8> = Vec::new();
            let cursor = Cursor::new(bytes);
            let mut writer = BufWriter::new(cursor);
            self.decode_bytes(&mut writer, &file.root_hash, &block_hashes).await?;

            let cursor = writer.into_inner();
            let bytes = cursor.into_inner();
            let bytes = Bytes::from(bytes);
            let merkle_layer = MerkleLayer::import(&bytes)?;

            if merkle_layer.rank != (file.rank - 1) {
                return Err(Error::new(ErrorKind::InvalidFormat));
            }

            let now = self.clock.now();

            let new_file = SubscribedFile {
                rank: merkle_layer.rank,
                block_count_downloaded: 0,
                block_count_total: merkle_layer.hashes.len() as u32,
                status: SubscribedFileStatus::Downloading,
                updated_at: now,
                ..file
            };
            let new_blocks: Vec<SubscribedBlock> = merkle_layer
                .hashes
                .into_iter()
                .enumerate()
                .map(|(i, n)| SubscribedBlock {
                    root_hash: new_file.root_hash.clone(),
                    block_hash: n,
                    rank: merkle_layer.rank,
                    index: i as u32,
                    downloaded: false,
                })
                .collect();

            self.file_subscriber_repo.upsert_file_and_blocks(&new_file, &new_blocks).await?;
        }

        Ok(())
    }

    async fn decode_bytes<W>(&self, writer: &mut W, root_hash: &OmniHash, block_hashes: &[OmniHash]) -> Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        for block_hash in block_hashes {
            let key = gen_block_path(root_hash, block_hash);
            let Some(block) = self.blocks_storage.get_value(&key).await? else {
                return Err(Error::new(ErrorKind::IoError).with_message("decoding error: block is not found"));
            };
            writer.write_all(&block).await?;
        }

        writer.flush().await?;

        Ok(())
    }
}
