use chrono::{DateTime, Utc};

use omnius_core_omnikit::model::OmniHash;

#[derive(Clone)]
pub struct PublishedUncommittedFile {
    pub id: String,
    pub file_path: String,
    pub file_name: String,
    pub block_size: u32,
    pub attrs: Option<String>,
    pub priority: i64,
    pub status: PublishedUncommittedFileStatus,
    pub failed_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, PartialEq, Eq)]
pub enum PublishedUncommittedFileStatus {
    Unknown,
    Pending,
    Processing,
    Completed,
    Failed,
    Canceled,
}

impl sqlx::Type<sqlx::Sqlite> for PublishedUncommittedFileStatus {
    fn type_info() -> <sqlx::Sqlite as sqlx::Database>::TypeInfo {
        <str as sqlx::Type<sqlx::Sqlite>>::type_info()
    }
}

impl sqlx::Encode<'_, sqlx::Sqlite> for PublishedUncommittedFileStatus {
    fn encode_by_ref(&self, buf: &mut <sqlx::Sqlite as sqlx::Database>::ArgumentBuffer<'_>) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        let s = match self {
            PublishedUncommittedFileStatus::Unknown => "Unknown",
            PublishedUncommittedFileStatus::Pending => "Pending",
            PublishedUncommittedFileStatus::Processing => "Processing",
            PublishedUncommittedFileStatus::Completed => "Completed",
            PublishedUncommittedFileStatus::Failed => "Failed",
            PublishedUncommittedFileStatus::Canceled => "Canceled",
        };
        <&str as sqlx::Encode<sqlx::Sqlite>>::encode_by_ref(&s, buf)
    }
}

impl sqlx::Decode<'_, sqlx::Sqlite> for PublishedUncommittedFileStatus {
    fn decode(value: <sqlx::Sqlite as sqlx::Database>::ValueRef<'_>) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <String as sqlx::Decode<sqlx::Sqlite>>::decode(value)?;
        match s.as_str() {
            "Pending" => Ok(PublishedUncommittedFileStatus::Pending),
            "Processing" => Ok(PublishedUncommittedFileStatus::Processing),
            "Completed" => Ok(PublishedUncommittedFileStatus::Completed),
            "Failed" => Ok(PublishedUncommittedFileStatus::Failed),
            "Canceled" => Ok(PublishedUncommittedFileStatus::Canceled),
            _ => Ok(PublishedUncommittedFileStatus::Unknown),
        }
    }
}

#[derive(Clone)]
pub struct PublishedUncommittedBlock {
    pub file_id: String,
    pub block_hash: OmniHash,
    pub rank: u32,
    pub index: u32,
}
