use std::sync::Arc;

use omnius_axus_engine::{FileService, PublishRecord};
use omnius_axus_interface::{models, types::Nullable};
use omnius_core_base::tsid::TsidProvider;
use parking_lot::Mutex;

use crate::{
    error::{ApiError, ApiStatus},
    service::upload::{UploadService, UploadState},
};

const DEFAULT_BLOCK_SIZE: u32 = 1024 * 1024;
const MAX_BLOCK_SIZE: u32 = 1024 * 1024;

pub struct PublishService {
    file_service: Arc<FileService>,
    uploads: Arc<UploadService>,
    ids: Arc<Mutex<dyn TsidProvider + Send + Sync>>,
}

impl PublishService {
    pub fn new(file_service: Arc<FileService>, uploads: Arc<UploadService>, ids: Arc<Mutex<dyn TsidProvider + Send + Sync>>) -> Self {
        Self { file_service, uploads, ids }
    }

    pub async fn create(&self, body: &models::PublishCreateRequest) -> std::result::Result<models::Publish, ApiError> {
        if body.file_name.is_empty() {
            return Err(ApiError::invalid("fileName must not be empty"));
        }
        if body.size > i64::MAX as u64 {
            return Err(ApiError::invalid("size is too large"));
        }
        let block_size = body.block_size.unwrap_or(DEFAULT_BLOCK_SIZE);
        if block_size == 0 || block_size > MAX_BLOCK_SIZE {
            return Err(ApiError::invalid(format!("blockSize must be between 1 and {MAX_BLOCK_SIZE}")));
        }
        let id = self.ids.lock().create().to_string();
        let state = self
            .uploads
            .create(
                id,
                body.file_name.clone(),
                body.size,
                block_size,
                Self::from_nullable(&body.attrs),
                body.priority.unwrap_or(0),
            )
            .await?;
        Ok(Self::upload_to_model(&state))
    }

    pub async fn write(&self, id: &str, offset: u64, body: axum::body::Body) -> std::result::Result<(), ApiError> {
        match self.uploads.write_stream(id, offset, body).await {
            Ok(()) => Ok(()),
            Err(error) if error.status == ApiStatus::NotFound => {
                if self.file_service.get_publish(id).await.map_err(ApiError::from)?.is_some() {
                    Err(ApiError::conflict("publish content is already committed"))
                } else {
                    Err(error)
                }
            }
            Err(error) => Err(error),
        }
    }

    pub async fn commit(&self, id: &str) -> std::result::Result<models::Publish, ApiError> {
        let Some(staged) = self.uploads.commit(id).await? else {
            if self.file_service.get_publish(id).await.map_err(ApiError::from)?.is_some() {
                return Err(ApiError::conflict("publish is already committed"));
            }
            return Err(ApiError::not_found("publish upload not found"));
        };

        let size = i64::try_from(staged.size).map_err(|_| ApiError::invalid("size is too large"))?;
        let source_path = staged.path.to_str().ok_or_else(|| ApiError::internal("upload staging path is not valid UTF-8"))?;
        if let Err(error) = self
            .file_service
            .import_owned(
                &staged.id,
                source_path,
                &staged.file_name,
                size,
                staged.block_size,
                staged.attrs.as_deref(),
                staged.priority,
            )
            .await
        {
            let _ = tokio::fs::remove_file(&staged.path).await;
            return Err(error.into());
        }

        self.get(id).await?.ok_or_else(|| ApiError::internal("publish was not recorded"))
    }

    pub async fn get(&self, id: &str) -> std::result::Result<Option<models::Publish>, ApiError> {
        if let Some(state) = self.uploads.get(id).await {
            return Ok(Some(Self::upload_to_model(&state)));
        }
        match self.file_service.get_publish(id).await.map_err(ApiError::from)? {
            Some(record) => Ok(Some(self.record_to_model(&record).await?)),
            None => Ok(None),
        }
    }

    pub async fn list(&self, limit: Option<u64>, offset: Option<u64>) -> std::result::Result<models::PublishList, ApiError> {
        let mut publishes = Vec::new();
        for state in self.uploads.list().await {
            publishes.push(Self::upload_to_model(&state));
        }
        for record in self.file_service.list_publishes().await.map_err(ApiError::from)? {
            publishes.push(self.record_to_model(&record).await?);
        }
        publishes.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| a.id.cmp(&b.id)));

        let total = publishes.len() as i64;
        let offset = usize::try_from(offset.unwrap_or(0)).unwrap_or(usize::MAX);
        let limit = usize::try_from(limit.unwrap_or(100).min(1000)).unwrap_or(1000);
        let items = publishes.into_iter().skip(offset).take(limit).collect::<Vec<_>>();
        Ok(models::PublishList::new(items, total))
    }

    pub async fn cancel(&self, id: &str) -> std::result::Result<(), ApiError> {
        if self.uploads.cancel(id).await {
            return Ok(());
        }
        match self.file_service.get_publish(id).await.map_err(ApiError::from)? {
            Some(PublishRecord::Committed(_)) => Err(ApiError::conflict("a succeeded publish is removed, not cancelled")),
            Some(PublishRecord::Uncommitted(_)) => {
                self.file_service.cancel_publish(id).await.map_err(ApiError::from)?;
                Ok(())
            }
            None => Err(ApiError::not_found("publish not found")),
        }
    }

    pub async fn remove(&self, id: &str) -> std::result::Result<(), ApiError> {
        if self.uploads.cancel(id).await {
            return Ok(());
        }
        if self.file_service.get_publish(id).await.map_err(ApiError::from)?.is_none() {
            return Err(ApiError::not_found("publish not found"));
        }
        self.file_service.remove_publish(id).await.map_err(ApiError::from)
    }

    async fn record_to_model(&self, record: &PublishRecord) -> std::result::Result<models::Publish, ApiError> {
        match record {
            PublishRecord::Uncommitted(file) => {
                let encoded_block_count = self.file_service.encoded_block_count(&file.id).await.map_err(ApiError::from)?;
                let status = match file.status {
                    omnius_axus_engine::PublishedUncommittedFileStatus::Pending => models::PublishStatus::Pending,
                    omnius_axus_engine::PublishedUncommittedFileStatus::Processing => models::PublishStatus::Processing,
                    omnius_axus_engine::PublishedUncommittedFileStatus::Failed => models::PublishStatus::Failed,
                    omnius_axus_engine::PublishedUncommittedFileStatus::Unknown => models::PublishStatus::Failed,
                };
                let size = file.size.unwrap_or(0);
                let mut publish = models::Publish::new(
                    file.id.clone(),
                    file.file_name.clone(),
                    size,
                    file.block_size as i32,
                    file.priority,
                    status,
                    size,
                    encoded_block_count,
                    file.created_at,
                    file.updated_at,
                );
                publish.attrs = Self::to_nullable(file.attrs.clone());
                publish.failed_reason = Self::to_nullable(file.failed_reason.clone());
                Ok(publish)
            }
            PublishRecord::Committed(file) => {
                let id = file.source_id.clone().unwrap_or_default();
                let encoded_block_count = self.file_service.encoded_block_count(&id).await.map_err(ApiError::from)?;
                let size = file.size.unwrap_or(0);
                let mut publish = models::Publish::new(
                    id,
                    file.file_name.clone(),
                    size,
                    file.block_size as i32,
                    file.priority,
                    models::PublishStatus::Succeeded,
                    size,
                    encoded_block_count,
                    file.created_at,
                    file.updated_at,
                );
                publish.attrs = Self::to_nullable(file.attrs.clone());
                publish.root_hash = Some(Nullable::Present(file.root_hash.to_string()));
                Ok(publish)
            }
        }
    }
}

impl PublishService {
    fn upload_to_model(state: &UploadState) -> models::Publish {
        let mut publish = models::Publish::new(
            state.id.clone(),
            state.file_name.clone(),
            state.size as i64,
            state.block_size as i32,
            state.priority,
            models::PublishStatus::Uploading,
            state.uploaded_size as i64,
            0,
            state.created_at,
            state.updated_at,
        );
        publish.attrs = Self::to_nullable(state.attrs.clone());
        publish
    }

    fn to_nullable(value: Option<String>) -> Option<Nullable<String>> {
        value.map(Nullable::Present)
    }

    fn from_nullable(value: &Option<Nullable<String>>) -> Option<String> {
        match value {
            Some(Nullable::Present(value)) => Some(value.clone()),
            _ => None,
        }
    }
}
