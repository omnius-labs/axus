use super::{PublishedCommittedFile, PublishedUncommittedFile};

/// One publish as the API sees it: an uncommitted file while it uploads or
/// encodes, and the committed file after the root hash is final.
pub enum PublishRecord {
    Uncommitted(PublishedUncommittedFile),
    Committed(PublishedCommittedFile),
}

impl PublishRecord {
    pub fn id(&self) -> &str {
        match self {
            Self::Uncommitted(file) => &file.id,
            Self::Committed(file) => file.source_id.as_deref().unwrap_or_default(),
        }
    }
}
