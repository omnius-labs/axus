use std::{
    collections::HashMap,
    io::SeekFrom,
    path::{Path, PathBuf},
    sync::Arc,
};

use chrono::{DateTime, Duration, Utc};
use futures_util::StreamExt as _;
use omnius_core_base::clock::Clock;
use tokio::{
    io::{AsyncSeekExt as _, AsyncWriteExt as _},
    sync::Mutex,
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;

use crate::{error::ApiError, prelude::*};

const UPLOAD_EXPIRY_HOURS: i64 = 24;
const REAP_INTERVAL_SECONDS: u64 = 60;

#[derive(Clone)]
pub struct UploadState {
    pub id: String,
    pub staging_path: PathBuf,
    pub file_name: String,
    pub size: u64,
    pub block_size: u32,
    pub attrs: Option<String>,
    pub priority: i64,
    pub uploaded_size: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

struct UploadEntry {
    /// Serializes the chunk writes, commit, cancel, and the reaper's delete
    /// for one upload. The state lock is held only for a short update so a
    /// slow request body does not block other uploads or the upload list.
    io_lock: Mutex<()>,
    state: Mutex<UploadState>,
}

pub struct StagedUpload {
    pub id: String,
    pub path: PathBuf,
    pub file_name: String,
    pub size: u64,
    pub block_size: u32,
    pub attrs: Option<String>,
    pub priority: i64,
}

/// The in-memory state of publishes that have not reached the engine yet.
///
/// The staging directory is removed at startup, so an upload in progress
/// does not survive a daemon restart.
pub struct UploadService {
    dir: PathBuf,
    max_upload_size: u64,
    clock: Arc<dyn Clock<Utc> + Send + Sync>,
    uploads: Mutex<HashMap<String, Arc<UploadEntry>>>,
    token: CancellationToken,
    handle: Mutex<Option<JoinHandle<()>>>,
}

impl UploadService {
    pub async fn new(state_dir: &Path, max_upload_size: u64, clock: Arc<dyn Clock<Utc> + Send + Sync>) -> Result<Arc<Self>> {
        let dir = state_dir.join("uploads");
        let _ = tokio::fs::remove_dir_all(&dir).await;
        tokio::fs::create_dir_all(&dir).await?;

        let service = Arc::new(Self {
            dir,
            max_upload_size,
            clock,
            uploads: Mutex::new(HashMap::new()),
            token: CancellationToken::new(),
            handle: Mutex::new(None),
        });

        let weak = Arc::downgrade(&service);
        let token = service.token.clone();
        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(REAP_INTERVAL_SECONDS));
            loop {
                tokio::select! {
                    _ = token.cancelled() => break,
                    _ = interval.tick() => {
                        if let Some(service) = weak.upgrade() {
                            service.reap().await;
                        }
                    }
                }
            }
        });
        *service.handle.lock().await = Some(handle);

        Ok(service)
    }

    pub async fn shutdown(&self) {
        self.token.cancel();
        if let Some(handle) = self.handle.lock().await.take() {
            let _ = handle.await;
        }
    }

    pub async fn create(&self, id: String, file_name: String, size: u64, block_size: u32, attrs: Option<String>, priority: i64) -> std::result::Result<UploadState, ApiError> {
        if self.max_upload_size != 0 && size > self.max_upload_size {
            return Err(ApiError::payload_too_large(format!(
                "declared size {} exceeds the maximum upload size {}",
                size, self.max_upload_size
            )));
        }
        let available = available_space(&self.dir).map_err(|error| ApiError::internal(format!("cannot read available space: {error}")))?;
        if available < size {
            return Err(ApiError::insufficient_storage(format!("declared size {size} exceeds the available space {available}")));
        }

        let staging_path = self.dir.join(&id);
        let file = tokio::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&staging_path)
            .await
            .map_err(|error| ApiError::internal(format!("cannot create the upload staging file: {error}")))?;
        file.sync_all()
            .await
            .map_err(|error| ApiError::internal(format!("cannot sync the upload staging file: {error}")))?;
        drop(file);

        let now = self.clock.now();
        let state = UploadState {
            id: id.clone(),
            staging_path,
            file_name,
            size,
            block_size,
            attrs,
            priority,
            uploaded_size: 0,
            created_at: now,
            updated_at: now,
            expires_at: now + Duration::hours(UPLOAD_EXPIRY_HOURS),
        };
        let entry = Arc::new(UploadEntry {
            io_lock: Mutex::new(()),
            state: Mutex::new(state.clone()),
        });
        self.uploads.lock().await.insert(id, entry);
        Ok(state)
    }

    pub async fn get(&self, id: &str) -> Option<UploadState> {
        let entry = self.uploads.lock().await.get(id).cloned()?;
        Some(entry.state.lock().await.clone())
    }

    pub async fn list(&self) -> Vec<UploadState> {
        let entries = self.uploads.lock().await.values().cloned().collect::<Vec<_>>();
        {
            let mut states = Vec::with_capacity(entries.len());
            for entry in entries {
                states.push(entry.state.lock().await.clone());
            }
            states
        }
    }

    pub async fn write_stream(&self, id: &str, offset: u64, body: axum::body::Body) -> std::result::Result<(), ApiError> {
        let Some(entry) = self.uploads.lock().await.get(id).cloned() else {
            return Err(ApiError::not_found("publish upload not found or already committed"));
        };
        let _io = entry.io_lock.lock().await;

        let staging_path = {
            let state = entry.state.lock().await;
            if state.expires_at <= self.clock.now() {
                let staging_path = state.staging_path.clone();
                drop(state);
                let _ = self.uploads.lock().await.remove(id);
                let _ = tokio::fs::remove_file(staging_path).await;
                return Err(ApiError::not_found("publish upload expired"));
            }
            if offset != state.uploaded_size {
                return Err(ApiError::conflict(format!("offset must equal the current uploaded size {}", state.uploaded_size)));
            }
            if state.uploaded_size == state.size {
                return Err(ApiError::payload_too_large("the upload already reached the declared size"));
            }
            state.staging_path.clone()
        };

        let available = available_space(&self.dir).map_err(|error| ApiError::internal(format!("cannot read available space: {error}")))?;
        if available == 0 {
            return Err(ApiError::insufficient_storage("no space is available for the upload"));
        }

        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .open(&staging_path)
            .await
            .map_err(|error| ApiError::internal(format!("cannot open the upload staging file: {error}")))?;
        file.seek(SeekFrom::Start(offset))
            .await
            .map_err(|error| ApiError::internal(format!("cannot seek the upload staging file: {error}")))?;

        let mut stream = body.into_data_stream();
        let mut written = 0u64;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| ApiError::invalid(format!("cannot read the request body: {error}")))?;
            let chunk_len = chunk.len() as u64;

            let start = {
                let mut state = entry.state.lock().await;
                if chunk_len > state.size - state.uploaded_size {
                    return Err(ApiError::payload_too_large("the chunk exceeds the declared upload size"));
                }
                let start = state.uploaded_size;
                state.uploaded_size += chunk_len;
                state.updated_at = self.clock.now();
                state.expires_at = self.clock.now() + Duration::hours(UPLOAD_EXPIRY_HOURS);
                start
            };

            if available < written + chunk_len {
                let mut state = entry.state.lock().await;
                state.uploaded_size = start;
                return Err(ApiError::insufficient_storage("not enough space for the chunk"));
            }
            file.write_all(&chunk)
                .await
                .map_err(|error| ApiError::internal(format!("cannot write the upload staging file: {error}")))?;
            file.set_len(start + chunk_len)
                .await
                .map_err(|error| ApiError::internal(format!("cannot truncate the upload staging file: {error}")))?;
            written += chunk_len;
        }
        file.sync_all()
            .await
            .map_err(|error| ApiError::internal(format!("cannot sync the upload staging file: {error}")))?;
        Ok(())
    }

    pub async fn commit(&self, id: &str) -> std::result::Result<Option<StagedUpload>, ApiError> {
        let Some(entry) = self.uploads.lock().await.get(id).cloned() else {
            return Ok(None);
        };
        let _io = entry.io_lock.lock().await;

        let state = entry.state.lock().await.clone();
        if state.uploaded_size != state.size {
            return Err(ApiError::invalid(format!(
                "uploaded size {} does not equal the declared size {}",
                state.uploaded_size, state.size
            )));
        }
        let file = tokio::fs::OpenOptions::new()
            .write(true)
            .open(&state.staging_path)
            .await
            .map_err(|error| ApiError::internal(format!("cannot open the upload staging file: {error}")))?;
        file.sync_all()
            .await
            .map_err(|error| ApiError::internal(format!("cannot sync the upload staging file: {error}")))?;
        drop(file);

        if self.uploads.lock().await.remove(id).is_none() {
            return Ok(None);
        }

        Ok(Some(StagedUpload {
            id: state.id,
            path: state.staging_path,
            file_name: state.file_name,
            size: state.size,
            block_size: state.block_size,
            attrs: state.attrs,
            priority: state.priority,
        }))
    }

    pub async fn cancel(&self, id: &str) -> bool {
        let Some(entry) = self.uploads.lock().await.remove(id) else {
            return false;
        };
        let _io = entry.io_lock.lock().await;
        let path = entry.state.lock().await.staging_path.clone();
        let _ = tokio::fs::remove_file(path).await;
        true
    }

    async fn reap(&self) {
        let entries = self.uploads.lock().await.iter().map(|(id, entry)| (id.clone(), entry.clone())).collect::<Vec<_>>();
        for (id, entry) in entries {
            let _io = entry.io_lock.lock().await;
            let expired = {
                let state = entry.state.lock().await;
                state.expires_at <= self.clock.now()
            };
            if !expired {
                continue;
            }
            if self.uploads.lock().await.remove(&id).is_none() {
                continue;
            }
            let path = entry.state.lock().await.staging_path.clone();
            let _ = tokio::fs::remove_file(path).await;
        }
    }
}

#[cfg(unix)]
// omnius-lint:allow(free-fn) wraps the OS statvfs call as a platform boundary
fn available_space(path: &Path) -> std::io::Result<u64> {
    use std::os::unix::ffi::OsStrExt as _;

    let path = std::ffi::CString::new(path.as_os_str().as_bytes()).map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "path contains a NUL byte"))?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    let result = unsafe { libc::statvfs(path.as_ptr(), &mut stat) };
    if result != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok((stat.f_bavail as u64).saturating_mul(stat.f_frsize as u64))
}

#[cfg(not(unix))]
// omnius-lint:allow(free-fn) wraps the OS statvfs call as a platform boundary
fn available_space(_: &Path) -> std::io::Result<u64> {
    Ok(u64::MAX)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::Utc;
    use omnius_core_base::clock::FakeClockUtc;
    use testresult::TestResult;

    use super::*;

    #[tokio::test]
    async fn expired_upload_is_removed_test() -> TestResult {
        let dir = tempfile::tempdir()?;
        let clock = Arc::new(FakeClockUtc::new(Utc::now()));
        let service = UploadService::new(dir.path(), 0, clock.clone()).await?;
        let state = service.create("upload-1".to_string(), "file.bin".to_string(), 4, 256, None, 0).await?;
        assert!(state.staging_path.exists());

        clock.advance(std::time::Duration::from_secs(25 * 60 * 60));
        service.reap().await;

        assert!(service.get("upload-1").await.is_none());
        assert!(!state.staging_path.exists());
        service.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn write_extends_the_upload_expiry_test() -> TestResult {
        let dir = tempfile::tempdir()?;
        let clock = Arc::new(FakeClockUtc::new(Utc::now()));
        let service = UploadService::new(dir.path(), 0, clock.clone()).await?;
        service.create("upload-2".to_string(), "file.bin".to_string(), 4, 256, None, 0).await?;

        clock.advance(std::time::Duration::from_secs(23 * 60 * 60));
        service.write_stream("upload-2", 0, axum::body::Body::from(vec![0u8; 2])).await?;

        clock.advance(std::time::Duration::from_secs(23 * 60 * 60));
        service.reap().await;
        assert!(service.get("upload-2").await.is_some());

        clock.advance(std::time::Duration::from_secs(2 * 60 * 60));
        service.reap().await;
        assert!(service.get("upload-2").await.is_none());
        service.shutdown().await;
        Ok(())
    }
}
