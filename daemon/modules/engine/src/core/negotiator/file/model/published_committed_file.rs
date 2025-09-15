use chrono::{DateTime, Utc};

use omnius_core_omnikit::model::OmniHash;

pub struct PublishedCommittedFile {
    pub root_hash: OmniHash,
    pub file_name: String,
    pub block_size: u32,
    pub attrs: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct PublishedCommittedBlock {
    pub root_hash: OmniHash,
    pub block_hash: OmniHash,
    pub rank: u32,
    pub index: u32,
}
