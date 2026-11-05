use std::{str::FromStr as _, sync::Arc};

use omnius_axus_engine::{FileService, SubscribedFile, SubscribedFileStatus};
use omnius_axus_interface::{models, types::Nullable};
use omnius_core_omnikit::generated::omni_hash::OmniHash;

use crate::{error::ApiError, service::download::DownloadInfo};

pub struct SubscribeService {
    file_service: Arc<FileService>,
}

impl SubscribeService {
    pub fn new(file_service: Arc<FileService>) -> Self {
        Self { file_service }
    }

    pub async fn create(&self, body: &models::SubscribeCreateRequest) -> std::result::Result<models::Subscribe, ApiError> {
        if body.output_path.is_empty() {
            return Err(ApiError::invalid("outputPath must not be empty"));
        }
        let root_hash = OmniHash::from_str(&body.root_hash).map_err(|error| ApiError::invalid(format!("invalid rootHash: {error}")))?;
        let id = self
            .file_service
            .create_subscribe(&root_hash, &body.output_path, Self::from_nullable(&body.attrs).as_deref(), body.priority.unwrap_or(0))
            .await
            .map_err(ApiError::from)?;
        self.get(&id).await?.ok_or_else(|| ApiError::internal("subscription was not recorded"))
    }

    pub async fn get(&self, id: &str) -> std::result::Result<Option<models::Subscribe>, ApiError> {
        Ok(self.file_service.get_subscribe(id).await.map_err(ApiError::from)?.as_ref().map(Self::subscribe_to_model))
    }

    pub async fn list(&self, limit: Option<u64>, offset: Option<u64>) -> std::result::Result<models::SubscribeList, ApiError> {
        let mut subscribes = self
            .file_service
            .list_subscribes()
            .await
            .map_err(ApiError::from)?
            .iter()
            .map(Self::subscribe_to_model)
            .collect::<Vec<_>>();
        subscribes.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| a.id.cmp(&b.id)));

        let total = subscribes.len() as i64;
        let offset = usize::try_from(offset.unwrap_or(0)).unwrap_or(usize::MAX);
        let limit = usize::try_from(limit.unwrap_or(100).min(1000)).unwrap_or(1000);
        let items = subscribes.into_iter().skip(offset).take(limit).collect::<Vec<_>>();
        Ok(models::SubscribeList::new(items, total))
    }

    pub async fn cancel(&self, id: &str) -> std::result::Result<models::Subscribe, ApiError> {
        let file = self
            .file_service
            .get_subscribe(id)
            .await
            .map_err(ApiError::from)?
            .ok_or_else(|| ApiError::not_found("subscription not found"))?;
        if file.status == SubscribedFileStatus::Finalizing {
            return Err(ApiError::conflict("a finalizing subscription cannot be cancelled"));
        }
        self.file_service.cancel_subscribe(id).await.map_err(ApiError::from)?;
        self.get(id).await?.ok_or_else(|| ApiError::internal("subscription was not found after cancel"))
    }

    pub async fn remove(&self, id: &str) -> std::result::Result<(), ApiError> {
        let file = self
            .file_service
            .get_subscribe(id)
            .await
            .map_err(ApiError::from)?
            .ok_or_else(|| ApiError::not_found("subscription not found"))?;
        if file.status == SubscribedFileStatus::Finalizing {
            return Err(ApiError::conflict("a finalizing subscription cannot be removed"));
        }
        self.file_service.remove_subscribe(id).await.map_err(ApiError::from)
    }

    pub async fn download(&self, id: &str) -> std::result::Result<DownloadInfo, ApiError> {
        let file = self
            .file_service
            .get_subscribe(id)
            .await
            .map_err(ApiError::from)?
            .ok_or_else(|| ApiError::not_found("subscription not found"))?;
        if file.status != SubscribedFileStatus::Completed {
            return Err(ApiError::conflict("the subscription has not completed"));
        }
        let path = file.output_path();
        let metadata = tokio::fs::metadata(&path)
            .await
            .map_err(|_| ApiError::gone("the subscription output is no longer available"))?;
        if !metadata.is_file() {
            return Err(ApiError::gone("the subscription output is no longer available"));
        }
        Ok(DownloadInfo {
            path,
            file_name: file.output_name.clone(),
            size: metadata.len(),
            etag: format!("\"{}\"", file.root_hash),
        })
    }
}

impl SubscribeService {
    fn subscribe_to_model(file: &SubscribedFile) -> models::Subscribe {
        let status = match file.status {
            SubscribedFileStatus::Downloading => models::SubscribeStatus::Downloading,
            SubscribedFileStatus::Decoding => models::SubscribeStatus::Decoding,
            SubscribedFileStatus::Finalizing => models::SubscribeStatus::Finalizing,
            SubscribedFileStatus::Completed => models::SubscribeStatus::Completed,
            SubscribedFileStatus::Failed => models::SubscribeStatus::Failed,
            SubscribedFileStatus::Canceled => models::SubscribeStatus::Canceled,
            SubscribedFileStatus::Unknown => models::SubscribeStatus::Downloading,
        };
        let rank = if file.rank == SubscribedFile::UNKNOWN_ROOT_RANK { -1 } else { file.rank as i32 };
        let mut subscribe = models::Subscribe::new(
            file.id.clone(),
            file.root_hash.to_string(),
            file.output_path().to_string_lossy().to_string(),
            rank,
            file.block_count_downloaded as i32,
            file.block_count_total as i32,
            file.priority,
            status,
            file.created_at,
            file.updated_at,
        );
        subscribe.attrs = file.attrs.clone().map(Nullable::Present);
        subscribe.failed_reason = file.failed_reason.clone().map(Nullable::Present);
        subscribe
    }

    fn from_nullable(value: &Option<Nullable<String>>) -> Option<String> {
        match value {
            Some(Nullable::Present(value)) => Some(value.clone()),
            _ => None,
        }
    }
}
