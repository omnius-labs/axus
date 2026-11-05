use std::sync::Arc;

use omnius_core_omnikit::generated::omni_hash::OmniHash;

use super::{
    file_publisher::FilePublisher,
    file_subscriber::FileSubscriber,
    model::{PublishRecord, SubscribedFile},
};
use crate::prelude::*;

/// The engine-facing entry point for publish and subscribe operations.
///
/// The daemon layer uses this type instead of reaching into the
/// FileExchanger context.
pub struct FileService {
    publisher: Arc<FilePublisher>,
    subscriber: Arc<FileSubscriber>,
}

impl FileService {
    pub fn new(publisher: Arc<FilePublisher>, subscriber: Arc<FileSubscriber>) -> Self {
        Self { publisher, subscriber }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn import_owned(&self, id: &str, source_path: &str, file_name: &str, size: i64, block_size: u32, attrs: Option<&str>, priority: i64) -> Result<()> {
        self.publisher.import_owned(id, source_path, file_name, size, block_size, attrs, priority).await
    }

    pub async fn get_publish(&self, id: &str) -> Result<Option<PublishRecord>> {
        self.publisher.get_publish(id).await
    }

    pub async fn list_publishes(&self) -> Result<Vec<PublishRecord>> {
        self.publisher.list_publishes().await
    }

    pub async fn encoded_block_count(&self, id: &str) -> Result<i64> {
        self.publisher.encoded_block_count(id).await
    }

    pub async fn cancel_publish(&self, id: &str) -> Result<()> {
        self.publisher.cancel(id).await
    }

    pub async fn remove_publish(&self, id: &str) -> Result<()> {
        self.publisher.remove_publish(id).await
    }

    pub async fn create_subscribe(&self, root_hash: &OmniHash, output_path: &str, attrs: Option<&str>, priority: i64) -> Result<String> {
        self.subscriber.subscribe(root_hash, output_path, attrs, priority).await
    }

    pub async fn get_subscribe(&self, id: &str) -> Result<Option<SubscribedFile>> {
        self.subscriber.find_file(id).await
    }

    pub async fn list_subscribes(&self) -> Result<Vec<SubscribedFile>> {
        self.subscriber.list_files().await
    }

    pub async fn cancel_subscribe(&self, id: &str) -> Result<()> {
        self.subscriber.cancel(id).await
    }

    pub async fn remove_subscribe(&self, id: &str) -> Result<()> {
        self.subscriber.remove(id).await
    }
}
