use chrono::{DateTime, Utc};

use omnius_core_omnikit::generated::omni_hash::OmniHash;

#[derive(Clone)]
pub struct SubscribedFile {
    pub id: String,
    pub root_hash: OmniHash,
    pub output_directory: String,
    pub output_name: String,
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

impl SubscribedFile {
    /// root block を復号する前は root の rank が分からないため、file と root block の rank にこの値を使う
    pub const UNKNOWN_ROOT_RANK: u32 = u32::MAX;

    const TEMPORARY_SEPARATOR: &str = ".axus-";
    const TEMPORARY_SUFFIX: &str = ".part";

    #[cfg(test)]
    pub fn temporary_output_name(output_name: &str, id: &str) -> String {
        format!(".{output_name}{}{id}{}", Self::TEMPORARY_SEPARATOR, Self::TEMPORARY_SUFFIX)
    }

    #[cfg(test)]
    pub fn temporary_output_path(&self) -> std::path::PathBuf {
        std::path::Path::new(&self.output_directory).join(Self::temporary_output_name(&self.output_name, &self.id))
    }

    pub fn is_reserved_output_name(name: &str) -> bool {
        let name = name.to_ascii_lowercase();
        let Some(name) = name.strip_prefix('.').and_then(|name| name.strip_suffix(Self::TEMPORARY_SUFFIX)) else {
            return false;
        };
        let Some((entry, id)) = name.rsplit_once(Self::TEMPORARY_SEPARATOR) else {
            return false;
        };
        let mut parts = id.split('.');
        let (Some(seconds), Some(nanos), Some(random), None) = (parts.next(), parts.next(), parts.next(), parts.next()) else {
            return false;
        };
        let (Ok(seconds_value), Ok(nanos_value)) = (seconds.parse::<i64>(), nanos.parse::<u32>()) else {
            return false;
        };
        !entry.is_empty()
            && seconds_value.to_string() == seconds
            && format!("{nanos_value:09}") == nanos
            && (nanos_value < 1_000_000_000 || (nanos_value < 2_000_000_000 && seconds_value.rem_euclid(60) == 59))
            && random.len().is_multiple_of(2)
            && random.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    }

    pub fn output_path(&self) -> std::path::PathBuf {
        std::path::Path::new(&self.output_directory).join(&self.output_name)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum SubscribedFileStatus {
    Unknown,
    Downloading,
    Decoding,
    Finalizing,
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
    fn encode_by_ref(&self, buf: &mut <sqlx::Sqlite as sqlx::Database>::ArgumentBuffer) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        let s = match self {
            SubscribedFileStatus::Unknown => "Unknown",
            SubscribedFileStatus::Downloading => "Downloading",
            SubscribedFileStatus::Decoding => "Decoding",
            SubscribedFileStatus::Finalizing => "Finalizing",
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
            "Finalizing" => Ok(SubscribedFileStatus::Finalizing),
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

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;
    use omnius_core_base::tsid::Tsid;

    use super::*;

    #[test]
    fn generated_temporary_names_are_reserved() {
        for seconds in [-1, 0, 1, i32::MAX as i64] {
            for nanos in [0, 1, 999_999_999] {
                for random_bytes in [vec![], vec![0], vec![255; 16]] {
                    let id = Tsid {
                        timestamp: Utc.timestamp_opt(seconds, nanos).unwrap(),
                        random_bytes,
                    }
                    .to_string();
                    for entry in ["output", ".hidden", "日本語", "nested.axus-name.part"] {
                        let name = SubscribedFile::temporary_output_name(entry, &id);
                        assert!(SubscribedFile::is_reserved_output_name(&name), "{name}");
                        assert!(SubscribedFile::is_reserved_output_name(&name.to_ascii_uppercase()));
                    }
                }
            }
        }
    }

    #[test]
    fn generated_leap_second_temporary_names_are_reserved() {
        for nanos in [1_000_000_000, 1_999_999_999] {
            let id = Tsid {
                timestamp: Utc.timestamp_opt(59, nanos).unwrap(),
                random_bytes: vec![255],
            }
            .to_string();
            let name = SubscribedFile::temporary_output_name("foo", &id);
            assert!(SubscribedFile::is_reserved_output_name(&name));
            assert!(SubscribedFile::is_reserved_output_name(&name.to_ascii_uppercase()));
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn ascii_case_aliases_share_temporary_entry_on_case_insensitive_filesystems() -> testresult::TestResult {
        use std::os::unix::fs::MetadataExt as _;
        let dir = tempfile::tempdir()?;
        let name = SubscribedFile::temporary_output_name("foo", "0.000000000.aa");
        let temporary = dir.path().join(&name);
        let alias = dir.path().join(name.to_ascii_uppercase());
        std::fs::write(&alias, b"completed output")?;
        if !temporary.exists() {
            eprintln!("skip: filesystem distinguishes ASCII case");
            return Ok(());
        }
        let a = std::fs::metadata(&temporary)?;
        let b = std::fs::metadata(&alias)?;
        assert_eq!((a.dev(), a.ino()), (b.dev(), b.ino()));
        assert!(SubscribedFile::is_reserved_output_name(alias.file_name().unwrap().to_str().unwrap()));
        Ok(())
    }
}
