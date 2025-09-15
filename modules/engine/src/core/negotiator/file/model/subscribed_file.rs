use chrono::{DateTime, Utc};

use omnius_core_omnikit::model::OmniHash;

#[derive(Clone)]
pub struct SubscribedFile {
    pub id: String,
    pub root_hash: OmniHash,
    pub file_path: String,
    pub rank: u32,
    pub block_count_downloaded: u32,
    pub block_count_total: u32,
    pub attrs: Option<String>,
    pub priority: i64,
    pub status: SubscribedFileStatus,
    pub failed_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, PartialEq, Eq)]
pub enum SubscribedFileStatus {
    Unknown,
    Downloading,
    Decoding,
    Completed,
    Failed,
    Canceled,
}

impl sqlx::Type<sqlx::Sqlite> for SubscribedFileStatus {
    fn type_info() -> <sqlx::Sqlite as sqlx::Database>::TypeInfo {
        <str as sqlx::Type<sqlx::Sqlite>>::type_info()
    }
}

impl sqlx::Encode<'_, sqlx::Sqlite> for SubscribedFileStatus {
    fn encode_by_ref(&self, buf: &mut <sqlx::Sqlite as sqlx::Database>::ArgumentBuffer<'_>) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        let s = match self {
            SubscribedFileStatus::Unknown => "Unknown",
            SubscribedFileStatus::Downloading => "Downloading",
            SubscribedFileStatus::Decoding => "Decoding",
            SubscribedFileStatus::Completed => "Completed",
            SubscribedFileStatus::Failed => "Failed",
            SubscribedFileStatus::Canceled => "Canceled",
        };
        <&str as sqlx::Encode<sqlx::Sqlite>>::encode_by_ref(&s, buf)
    }
}

impl sqlx::Decode<'_, sqlx::Sqlite> for SubscribedFileStatus {
    fn decode(value: <sqlx::Sqlite as sqlx::Database>::ValueRef<'_>) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <String as sqlx::Decode<sqlx::Sqlite>>::decode(value)?;
        match s.as_str() {
            "Downloading" => Ok(SubscribedFileStatus::Downloading),
            "Decoding" => Ok(SubscribedFileStatus::Decoding),
            "Completed" => Ok(SubscribedFileStatus::Completed),
            "Failed" => Ok(SubscribedFileStatus::Failed),
            "Canceled" => Ok(SubscribedFileStatus::Canceled),
            _ => Ok(SubscribedFileStatus::Unknown),
        }
    }
}

#[derive(Clone)]
pub struct SubscribedBlock {
    pub root_hash: OmniHash,
    pub block_hash: OmniHash,
    pub rank: u32,
    pub index: u32,
    pub downloaded: bool,
}
