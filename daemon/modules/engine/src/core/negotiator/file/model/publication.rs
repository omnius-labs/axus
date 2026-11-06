use chrono::{DateTime, Utc};

use omnius_core_omnikit::generated::omni_hash::OmniHash;

use super::{PublishedUncommittedFile, PublishedUncommittedFileStatus};

/// One import as the API exposes it, from the declaration of its size to the committed file.
#[derive(Clone)]
pub struct Publication {
    pub id: String,
    pub file_name: String,
    pub size: i64,
    pub received_bytes: i64,
    pub status: PublicationStatus,
    pub root_hash: Option<OmniHash>,
    pub failed_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicationStatus {
    Receiving,
    Pending,
    Processing,
    Committed,
    Failed,
    Canceled,
}

impl Publication {
    pub fn from_uncommitted(file: PublishedUncommittedFile) -> Self {
        let status = match file.status {
            PublishedUncommittedFileStatus::Receiving => PublicationStatus::Receiving,
            PublishedUncommittedFileStatus::Pending => PublicationStatus::Pending,
            PublishedUncommittedFileStatus::Processing => PublicationStatus::Processing,
            PublishedUncommittedFileStatus::Canceled => PublicationStatus::Canceled,
            PublishedUncommittedFileStatus::Failed | PublishedUncommittedFileStatus::Unknown => PublicationStatus::Failed,
        };
        Self {
            id: file.id,
            file_name: file.file_name,
            size: file.size.unwrap_or(file.received_bytes),
            received_bytes: file.received_bytes,
            status,
            root_hash: None,
            failed_reason: file.failed_reason,
            created_at: file.created_at,
            updated_at: file.updated_at,
        }
    }

    pub fn from_committed(id: String, file_name: String, size: i64, root_hash: OmniHash, created_at: DateTime<Utc>, updated_at: DateTime<Utc>) -> Self {
        Self {
            id,
            file_name,
            size,
            received_bytes: size,
            status: PublicationStatus::Committed,
            root_hash: Some(root_hash),
            failed_reason: None,
            created_at,
            updated_at,
        }
    }
}
