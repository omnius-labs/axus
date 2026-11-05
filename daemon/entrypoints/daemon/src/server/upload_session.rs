use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use axum::body::Body;
use chrono::{DateTime, Duration, Utc};
use futures_util::StreamExt as _;
use tokio::io::AsyncWriteExt as _;

use omnius_core_base::clock::{Clock, ClockUtc};

use crate::prelude::*;

use super::api_error::ApiError;

/// The largest body of one request, whether a whole file or a chunk of a session.
pub const MAX_BODY_BYTES: u64 = 32 * 1024 * 1024;

const SESSION_LIFETIME_HOURS: i64 = 24;

/// A file arriving in chunks. It lives in memory, and only its bytes are on disk.
pub struct UploadSession {
    pub id: String,
    pub file_name: String,
    pub size: u64,
    pub expires_at: DateTime<Utc>,
    path: PathBuf,
    progress: TokioMutex<UploadProgress>,
}

struct UploadProgress {
    committed_size: u64,
    /// A failed rollback leaves the bytes after `committed_size` unknown, so nothing more is appended.
    aborted: bool,
    /// The session left the list, so a chunk that found it earlier must not touch its file.
    closed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadSnapshot {
    pub id: String,
    pub file_name: String,
    pub size: u64,
    pub committed_size: u64,
    pub expires_at: DateTime<Utc>,
}

/// A finished upload, whose file the caller must hand over or delete.
pub struct FinishedUpload {
    pub path: PathBuf,
    pub file_name: String,
}

pub struct UploadSessions {
    dir: PathBuf,
    sessions: Mutex<HashMap<String, Arc<UploadSession>>>,
    clock: Arc<dyn Clock<Utc> + Send + Sync>,
}

impl UploadSessions {
    /// Empties `dir`, because no session survives a restart.
    pub async fn open(dir: &Path) -> Result<Self> {
        Self::open_with_clock(dir, Arc::new(ClockUtc)).await
    }

    async fn open_with_clock(dir: &Path, clock: Arc<dyn Clock<Utc> + Send + Sync>) -> Result<Self> {
        match tokio::fs::remove_dir_all(dir).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        tokio::fs::create_dir_all(dir).await?;
        Ok(Self {
            dir: dir.to_path_buf(),
            sessions: Mutex::new(HashMap::new()),
            clock,
        })
    }

    pub async fn create(&self, file_name: &str, size: u64) -> std::result::Result<UploadSnapshot, ApiError> {
        self.delete_expired().await;
        let id = uuid::Uuid::new_v4().simple().to_string();
        let path = self.dir.join(&id);
        tokio::fs::File::create(&path).await?;
        let session = Arc::new(UploadSession {
            id: id.clone(),
            file_name: file_name.to_string(),
            size,
            expires_at: self.clock.now() + Duration::hours(SESSION_LIFETIME_HOURS),
            path,
            progress: TokioMutex::new(UploadProgress {
                committed_size: 0,
                aborted: false,
                closed: false,
            }),
        });
        let snapshot = UploadSnapshot {
            id: id.clone(),
            file_name: session.file_name.clone(),
            size,
            committed_size: 0,
            expires_at: session.expires_at,
        };
        self.sessions.lock().insert(id, session);
        Ok(snapshot)
    }

    pub async fn get(&self, id: &str) -> std::result::Result<UploadSnapshot, ApiError> {
        self.delete_expired().await;
        let session = self.find(id)?;
        let progress = session.progress.lock().await;
        Ok(UploadSnapshot {
            id: session.id.clone(),
            file_name: session.file_name.clone(),
            size: session.size,
            committed_size: progress.committed_size,
            expires_at: session.expires_at,
        })
    }

    /// Appends a chunk, or leaves the session as it was so the client can resume from the same offset.
    pub async fn append(&self, id: &str, offset: u64, body: Body) -> std::result::Result<(), ApiError> {
        self.delete_expired().await;
        let session = self.find(id)?;
        let mut progress = session.progress.try_lock().map_err(|_| ApiError::conflict("another chunk is being written"))?;
        if progress.closed {
            return Err(ApiError::not_found("no such upload session"));
        }
        if progress.aborted {
            return Err(ApiError::conflict("the session failed and must be started again"));
        }
        if offset != progress.committed_size {
            return Err(ApiError::conflict(format!("expected offset {}", progress.committed_size)));
        }
        let remaining = session.size - progress.committed_size;
        let mut file = tokio::fs::OpenOptions::new().append(true).open(&session.path).await?;
        let written = BodyWriter::write(body, &mut file, |written| {
            if written > MAX_BODY_BYTES {
                return Some(ApiError::limit_exceeded("a chunk must be at most 32 MiB"));
            }
            (written > remaining).then(|| ApiError::invalid_request("the chunk goes beyond the declared size"))
        })
        .await;
        match written {
            Ok(written) => {
                progress.committed_size += written;
                Ok(())
            }
            Err(error) => {
                if let Err(rollback) = file.set_len(progress.committed_size).await {
                    warn!(?rollback, "upload rollback failed");
                    progress.aborted = true;
                }
                Err(error)
            }
        }
    }

    /// Ends the session once all its bytes have arrived.
    pub async fn finish(&self, id: &str) -> std::result::Result<FinishedUpload, ApiError> {
        self.delete_expired().await;
        let session = self.find(id)?;
        let mut progress = session.progress.try_lock().map_err(|_| ApiError::conflict("a chunk is being written"))?;
        if progress.committed_size != session.size {
            return Err(ApiError::invalid_request(format!("{} of {} bytes have arrived", progress.committed_size, session.size)));
        }
        self.take(id)?;
        progress.closed = true;
        Ok(FinishedUpload {
            path: session.path.clone(),
            file_name: session.file_name.clone(),
        })
    }

    pub async fn delete(&self, id: &str) -> std::result::Result<(), ApiError> {
        self.delete_expired().await;
        let session = self.take(id)?;
        Ok(Self::delete_bytes(&session).await?)
    }

    /// Writes a whole-file body under the upload directory, and deletes it again on failure.
    pub async fn receive_whole(&self, body: Body) -> std::result::Result<PathBuf, ApiError> {
        let path = self.dir.join(uuid::Uuid::new_v4().simple().to_string());
        let mut file = tokio::fs::File::create(&path).await?;
        let written = BodyWriter::write(body, &mut file, |written| {
            (written > MAX_BODY_BYTES).then(|| ApiError::payload_too_large("a file over 32 MiB must use an upload session"))
        })
        .await;
        if let Err(error) = written {
            drop(file);
            Self::log_deletion_failure(Self::delete_file(&path).await);
            return Err(error);
        }
        Ok(path)
    }

    /// Removing from the list decides the one owner when finish, delete, and expiry meet.
    fn take(&self, id: &str) -> std::result::Result<Arc<UploadSession>, ApiError> {
        self.sessions.lock().remove(id).ok_or_else(|| ApiError::not_found("no such upload session"))
    }

    fn find(&self, id: &str) -> std::result::Result<Arc<UploadSession>, ApiError> {
        self.sessions.lock().get(id).cloned().ok_or_else(|| ApiError::not_found("no such upload session"))
    }

    /// Leaves a session with a chunk in flight for the next call.
    async fn delete_expired(&self) {
        let now = self.clock.now();
        let expired: Vec<Arc<UploadSession>> = {
            let mut sessions = self.sessions.lock();
            let ids: Vec<String> = sessions
                .iter()
                .filter(|(_, session)| session.expires_at <= now && session.progress.try_lock().is_ok())
                .map(|(id, _)| id.clone())
                .collect();
            ids.iter().filter_map(|id| sessions.remove(id)).collect()
        };
        for session in expired {
            Self::log_deletion_failure(Self::delete_bytes(&session).await);
        }
    }

    /// Waits for a chunk in flight, so nothing writes to the file after it is deleted.
    async fn delete_bytes(session: &UploadSession) -> std::io::Result<()> {
        let mut progress = session.progress.lock().await;
        progress.closed = true;
        Self::delete_file(&session.path).await
    }

    async fn delete_file(path: &Path) -> std::io::Result<()> {
        match tokio::fs::remove_file(path).await {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        }
    }

    /// With no caller to tell, the bytes wait for the next start, which empties the directory.
    fn log_deletion_failure(result: std::io::Result<()>) {
        if let Err(error) = result {
            warn!(?error, "upload deletion failed");
        }
    }
}

/// Streams a request body to a file without holding it in memory.
struct BodyWriter;

impl BodyWriter {
    /// `check` sees the running total and stops the write by returning an error.
    async fn write(body: Body, file: &mut tokio::fs::File, check: impl Fn(u64) -> Option<ApiError>) -> std::result::Result<u64, ApiError> {
        let mut stream = body.into_data_stream();
        let mut written = 0u64;
        while let Some(frame) = stream.next().await {
            let frame = frame.map_err(|error| ApiError::invalid_request(format!("the request body was cut off: {error}")))?;
            written += frame.len() as u64;
            if let Some(error) = check(written) {
                return Err(error);
            }
            file.write_all(&frame).await?;
        }
        file.flush().await?;
        Ok(written)
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Bytes;
    use http::StatusCode;
    use omnius_core_base::clock::FakeClockUtc;
    use testresult::TestResult;

    use super::*;

    fn entries(dir: &Path) -> usize {
        std::fs::read_dir(dir).unwrap().count()
    }

    #[tokio::test]
    async fn open_empties_the_directory() -> TestResult {
        let dir = tempfile::tempdir()?;
        let uploads = dir.path().join("uploads");
        std::fs::create_dir(&uploads)?;
        std::fs::write(uploads.join("left"), "left")?;
        UploadSessions::open(&uploads).await?;
        assert_eq!(entries(&uploads), 0);
        Ok(())
    }

    #[tokio::test]
    async fn an_expired_session_disappears_with_its_bytes() -> TestResult {
        let dir = tempfile::tempdir()?;
        let uploads = dir.path().join("uploads");
        let clock = Arc::new(FakeClockUtc::new(chrono::Utc::now()));
        let sessions = UploadSessions::open_with_clock(&uploads, clock.clone()).await?;
        let session = sessions.create("name", 3).await?;
        sessions.append(&session.id, 0, Body::from(Bytes::from_static(b"abc"))).await?;
        assert_eq!(entries(&uploads), 1);

        clock.set(session.expires_at - Duration::seconds(1));
        assert_eq!(sessions.get(&session.id).await?.committed_size, 3);

        clock.set(session.expires_at);
        assert_eq!(sessions.get(&session.id).await.unwrap_err().status, StatusCode::NOT_FOUND);
        assert_eq!(entries(&uploads), 0);
        Ok(())
    }

    #[tokio::test]
    async fn a_delete_that_cannot_remove_the_bytes_reports_the_failure() -> TestResult {
        let dir = tempfile::tempdir()?;
        let uploads = dir.path().join("uploads");
        let sessions = UploadSessions::open(&uploads).await?;
        let id = sessions.create("name", 1).await?.id;
        // A directory in place of the file makes the removal fail with something other than `NotFound`.
        tokio::fs::remove_file(uploads.join(&id)).await?;
        tokio::fs::create_dir(uploads.join(&id)).await?;
        assert_eq!(sessions.delete(&id).await.unwrap_err().status, StatusCode::INTERNAL_SERVER_ERROR);
        Ok(())
    }

    #[tokio::test]
    async fn finish_and_delete_have_one_winner() -> TestResult {
        let dir = tempfile::tempdir()?;
        let uploads = dir.path().join("uploads");
        let sessions = Arc::new(UploadSessions::open(&uploads).await?);
        for _ in 0..50 {
            let id = sessions.create("name", 0).await?.id;
            let (finish, delete) = {
                let (a, b) = (sessions.clone(), sessions.clone());
                let (first, second) = (id.clone(), id.clone());
                tokio::join!(tokio::spawn(async move { a.finish(&first).await }), tokio::spawn(async move { b.delete(&second).await }))
            };
            let (finish, delete) = (finish?, delete?);
            match finish {
                Ok(finished) => {
                    assert_eq!(delete.unwrap_err().status, StatusCode::NOT_FOUND);
                    assert!(finished.path.exists(), "the winner of finish keeps its bytes");
                    tokio::fs::remove_file(finished.path).await?;
                }
                Err(error) => {
                    assert!(delete.is_ok());
                    assert!(matches!(error.status, StatusCode::NOT_FOUND | StatusCode::CONFLICT), "{error}");
                }
            }
            assert_eq!(entries(&uploads), 0);
        }
        Ok(())
    }

    #[tokio::test]
    async fn a_chunk_that_found_a_session_before_it_closed_does_not_touch_it() -> TestResult {
        let dir = tempfile::tempdir()?;
        let uploads = dir.path().join("uploads");
        let sessions = UploadSessions::open(&uploads).await?;
        let id = sessions.create("name", 3).await?.id;
        let session = sessions.find(&id)?;
        sessions.delete(&id).await?;
        assert!(session.progress.lock().await.closed);
        assert_eq!(
            sessions.append(&id, 0, Body::from(Bytes::from_static(b"abc"))).await.unwrap_err().status,
            StatusCode::NOT_FOUND
        );
        assert_eq!(entries(&uploads), 0);
        Ok(())
    }

    #[tokio::test]
    async fn a_body_that_is_cut_off_leaves_the_offset_in_place() -> TestResult {
        let dir = tempfile::tempdir()?;
        let uploads = dir.path().join("uploads");
        let sessions = UploadSessions::open(&uploads).await?;
        let session = sessions.create("name", 6).await?;
        sessions.append(&session.id, 0, Body::from(Bytes::from_static(b"abc"))).await?;

        let stream = futures_util::stream::iter([Ok(Bytes::from_static(b"de")), Err(std::io::Error::other("cut off"))]);
        let error = sessions.append(&session.id, 3, Body::from_stream(stream)).await.unwrap_err();
        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert_eq!(sessions.get(&session.id).await?.committed_size, 3);

        sessions.append(&session.id, 3, Body::from(Bytes::from_static(b"def"))).await?;
        let finished = sessions.finish(&session.id).await?;
        assert_eq!(tokio::fs::read(&finished.path).await?, b"abcdef");
        Ok(())
    }
}
