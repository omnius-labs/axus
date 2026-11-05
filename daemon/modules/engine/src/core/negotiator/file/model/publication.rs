use chrono::{DateTime, Utc};

use omnius_core_omnikit::generated::omni_hash::OmniHash;

use super::{PublishedCommittedFile, PublishedUncommittedFile, PublishedUncommittedFileStatus};

/// An import as a client sees it, from its uncommitted file until the committed file its import ID maps to.
#[derive(Clone)]
pub struct Publication {
    pub id: String,
    pub file_name: String,
    pub status: PublicationStatus,
    pub root_hash: Option<OmniHash>,
    pub failed_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicationStatus {
    Pending,
    Processing,
    Failed,
    Committed,
}

impl Publication {
    pub fn from_uncommitted(file: PublishedUncommittedFile) -> Self {
        let status = match file.status {
            PublishedUncommittedFileStatus::Pending => PublicationStatus::Pending,
            PublishedUncommittedFileStatus::Processing => PublicationStatus::Processing,
            PublishedUncommittedFileStatus::Failed | PublishedUncommittedFileStatus::Unknown => PublicationStatus::Failed,
        };
        Self {
            id: file.id,
            file_name: file.file_name,
            status,
            root_hash: None,
            failed_reason: file.failed_reason,
            created_at: file.created_at,
            updated_at: file.updated_at,
        }
    }

    pub fn from_committed(id: String, file: PublishedCommittedFile) -> Self {
        Self {
            id,
            file_name: file.file_name,
            status: PublicationStatus::Committed,
            root_hash: Some(file.root_hash),
            failed_reason: None,
            created_at: file.created_at,
            updated_at: file.updated_at,
        }
    }
}
