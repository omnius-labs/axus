use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    body::Bytes,
    extract::{Path, Query, Request, State, rejection::QueryRejection},
    response::{IntoResponse as _, Response},
};
use axum_extra::extract::CookieJar;
use headers::Host;
use http::{Method, StatusCode};

use omnius_axus_interface::{apis::uploads as api, models};

use crate::state::DaemonState;

use super::{
    ApiServer,
    api_error::{ApiError, impl_from_api_error},
    file_name::FileName,
    publications::PublicationRoute,
    upload_session::UploadSnapshot,
};

impl_from_api_error!(api::CreateUploadResponse {
    StatusCode::BAD_REQUEST => Status400_TheRequestIsInvalid,
    StatusCode::INSUFFICIENT_STORAGE => Status507_TheVolumeHasNoSpaceLeft,
});
impl_from_api_error!(api::GetUploadResponse {
    StatusCode::NOT_FOUND => Status404_TheResourceDoesNotExist,
});
impl_from_api_error!(api::DeleteUploadResponse {
    StatusCode::NOT_FOUND => Status404_TheResourceDoesNotExist,
});
impl_from_api_error!(api::CommitUploadResponse {
    StatusCode::BAD_REQUEST => Status400_TheRequestIsInvalid,
    StatusCode::NOT_FOUND => Status404_TheResourceDoesNotExist,
    StatusCode::CONFLICT => Status409_TheResource,
    StatusCode::INSUFFICIENT_STORAGE => Status507_TheVolumeHasNoSpaceLeft,
});

/// Converts a session's state to the contract's.
struct UploadModel;

impl UploadModel {
    fn from_snapshot(snapshot: UploadSnapshot) -> models::UploadSession {
        models::UploadSession::new(snapshot.id, snapshot.file_name, snapshot.size as i64, snapshot.committed_size as i64, snapshot.expires_at)
    }
}

#[allow(unused_variables)]
#[async_trait]
impl api::Uploads for ApiServer {
    async fn commit_upload(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::CommitUploadPathParams,
    ) -> std::result::Result<api::CommitUploadResponse, ()> {
        let finished = match self.state.uploads.finish(&path_params.upload_id).await {
            Ok(finished) => finished,
            Err(error) => return Ok(error.into()),
        };
        Ok(match PublicationRoute::hand_over(&self.state, &finished.path, &finished.file_name).await {
            Ok(publication) => api::CommitUploadResponse::Status202_AllBytesArrivedAndEncodingIsQueued(publication),
            Err(error) => error.into(),
        })
    }

    async fn create_upload(&self, method: &Method, host: &Host, cookies: &CookieJar, body: &models::CreateUploadRequest) -> std::result::Result<api::CreateUploadResponse, ()> {
        if let Err(error) = FileName::validate(&body.file_name) {
            return Ok(error.into());
        }
        let Ok(size) = u64::try_from(body.size) else {
            return Ok(ApiError::invalid_request("the size must not be negative").into());
        };
        Ok(match self.state.uploads.create(&body.file_name, size).await {
            Ok(snapshot) => api::CreateUploadResponse::Status201_SessionCreated(UploadModel::from_snapshot(snapshot)),
            Err(error) => error.into(),
        })
    }

    async fn delete_upload(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::DeleteUploadPathParams,
    ) -> std::result::Result<api::DeleteUploadResponse, ()> {
        Ok(match self.state.uploads.delete(&path_params.upload_id).await {
            Ok(()) => api::DeleteUploadResponse::Status204_TheSessionAndItsBytesAreGone,
            Err(error) => error.into(),
        })
    }

    async fn get_upload(&self, method: &Method, host: &Host, cookies: &CookieJar, path_params: &models::GetUploadPathParams) -> std::result::Result<api::GetUploadResponse, ()> {
        Ok(match self.state.uploads.get(&path_params.upload_id).await {
            Ok(snapshot) => api::GetUploadResponse::Status200_SessionState(UploadModel::from_snapshot(snapshot)),
            Err(error) => error.into(),
        })
    }

    /// [`UploadContentRoute`] answers this operation in front of the generated router.
    async fn write_upload_content(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::WriteUploadContentPathParams,
        query_params: &models::WriteUploadContentQueryParams,
        body: &Bytes,
    ) -> std::result::Result<api::WriteUploadContentResponse, ()> {
        Err(())
    }
}

/// `PUT /uploads/{id}/content`, which streams the chunk to disk instead of reading it into memory.
pub struct UploadContentRoute;

impl UploadContentRoute {
    pub const PATH: &'static str = "/api/v1/uploads/{upload_id}/content";

    pub async fn put(
        State(state): State<Arc<DaemonState>>,
        Path(upload_id): Path<String>,
        query: std::result::Result<Query<models::WriteUploadContentQueryParams>, QueryRejection>,
        request: Request,
    ) -> Response {
        // A closed connection drops this future, and the task still gets to roll the chunk back.
        let task = tokio::spawn(async move {
            let Query(params) = query.map_err(|error| ApiError::invalid_request(error.body_text()))?;
            let offset = u64::try_from(params.offset).map_err(|_| ApiError::invalid_request("the offset must not be negative"))?;
            state.uploads.append(&upload_id, offset, request.into_body()).await
        });
        match task.await {
            Ok(Ok(())) => StatusCode::NO_CONTENT.into_response(),
            Ok(Err(error)) => error.into_response(),
            Err(error) => ApiError::from(std::io::Error::other(error)).into_response(),
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use http::StatusCode;
    use omnius_axus_interface::apis::uploads as api;
    use serde_json::{Value, json};
    use testresult::TestResult;
    use tower::ServiceExt as _;

    use crate::server::{api_error::ApiError, tests::TestDaemon, upload_session::MAX_BODY_BYTES};

    async fn create(daemon: &TestDaemon, size: i64) -> TestResult<String> {
        let (status, value) = daemon.request_json("POST", "/api/v1/uploads", &json!({"fileName": "large.bin", "size": size})).await?;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(value["committedSize"], 0);
        assert!(value["expiresAt"].as_str().is_some());
        Ok(value["id"].as_str().unwrap().to_string())
    }

    async fn put(daemon: &TestDaemon, id: &str, offset: i64, chunk: &[u8]) -> TestResult<StatusCode> {
        let uri = format!("/api/v1/uploads/{id}/content?offset={offset}");
        Ok(daemon.request("PUT", &uri, Body::from(chunk.to_vec())).await?.0)
    }

    #[tokio::test]
    async fn uploads_resume_from_the_reported_offset_and_commit_returns_a_publication() -> TestResult {
        let daemon = TestDaemon::start().await?;
        let content: Vec<u8> = (0..5000u32).map(|i| (i % 251) as u8).collect();
        let id = create(&daemon, content.len() as i64).await?;

        assert_eq!(put(&daemon, &id, 0, &content[..2000]).await?, StatusCode::NO_CONTENT);
        let (status, state) = daemon.get_json(&format!("/api/v1/uploads/{id}")).await?;
        assert_eq!(status, StatusCode::OK);
        let offset = state["committedSize"].as_i64().unwrap();
        assert_eq!(offset, 2000);
        assert_eq!(put(&daemon, &id, offset, &content[2000..]).await?, StatusCode::NO_CONTENT);

        let (status, bytes) = daemon.request("POST", &format!("/api/v1/uploads/{id}/commit"), Body::empty()).await?;
        assert_eq!(status, StatusCode::ACCEPTED);
        let publication: Value = serde_json::from_slice(&bytes)?;
        assert_eq!(publication["fileName"], "large.bin");
        let uri = format!("/api/v1/publications/{}", publication["id"].as_str().unwrap());
        assert!(daemon.wait_status(&uri, "Committed").await?["rootHash"].as_str().is_some());

        let (status, _) = daemon.get_json(&format!("/api/v1/uploads/{id}")).await?;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(std::fs::read_dir(daemon.dir.path().join("state/uploads"))?.count(), 0);
        daemon.stop().await;
        Ok(())
    }

    #[tokio::test]
    async fn uploads_roll_back_a_chunk_whose_connection_closed() -> TestResult {
        let daemon = TestDaemon::start().await?;
        let id = create(&daemon, 6).await?;
        assert_eq!(put(&daemon, &id, 0, b"abc").await?, StatusCode::NO_CONTENT);

        let (sender, receiver) = tokio::sync::mpsc::channel::<std::io::Result<axum::body::Bytes>>(1);
        sender.send(Ok(axum::body::Bytes::from_static(b"xx"))).await?;
        let uri = format!("/api/v1/uploads/{id}/content?offset=3");
        let request = axum::http::Request::builder()
            .method("PUT")
            .uri(uri)
            .header("host", "localhost")
            .body(Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(receiver)))?;
        // The router's future is dropped while the body is still open, as when a client disconnects.
        let in_flight = tokio::time::timeout(std::time::Duration::from_millis(200), daemon.send(request)).await;
        assert!(in_flight.is_err());
        assert_eq!(put(&daemon, &id, 3, b"def").await?, StatusCode::CONFLICT);
        sender.send(Err(std::io::Error::other("connection closed"))).await?;
        drop(sender);

        tokio::time::timeout(std::time::Duration::from_secs(120), async {
            while put(&daemon, &id, 3, b"def").await? != StatusCode::NO_CONTENT {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
            TestResult::Ok(())
        })
        .await??;
        let (status, bytes) = daemon.request("POST", &format!("/api/v1/uploads/{id}/commit"), Body::empty()).await?;
        assert_eq!(status, StatusCode::ACCEPTED);
        let publication: Value = serde_json::from_slice(&bytes)?;
        let uri = format!("/api/v1/publications/{}", publication["id"].as_str().unwrap());
        let committed = daemon.wait_status(&uri, "Committed").await?;

        let whole = daemon.publish("whole.bin", b"abcdef").await?;
        let whole_uri = format!("/api/v1/publications/{}", whole["id"].as_str().unwrap());
        assert_eq!(daemon.wait_status(&whole_uri, "Committed").await?["rootHash"], committed["rootHash"]);
        daemon.stop().await;
        Ok(())
    }

    #[tokio::test]
    async fn uploads_reject_a_commit_while_a_chunk_is_in_flight_with_409() -> TestResult {
        let daemon = TestDaemon::start().await?;
        let id = create(&daemon, 1).await?;
        let (sender, receiver) = tokio::sync::mpsc::channel::<std::io::Result<axum::body::Bytes>>(1);
        let request = axum::http::Request::builder()
            .method("PUT")
            .uri(format!("/api/v1/uploads/{id}/content?offset=0"))
            .header("host", "localhost")
            .body(Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(receiver)))?;
        let in_flight = tokio::spawn(daemon.router.clone().oneshot(request));
        // Until the chunk takes the session's lock the commit answers 400 for the missing byte, and then 409.
        tokio::time::timeout(std::time::Duration::from_secs(120), async {
            loop {
                let (status, bytes) = daemon.request("POST", &format!("/api/v1/uploads/{id}/commit"), Body::empty()).await?;
                if status == StatusCode::CONFLICT {
                    let value: Value = serde_json::from_slice(&bytes)?;
                    assert_eq!(value["kind"], "Conflict");
                    return TestResult::Ok(());
                }
                assert_eq!(status, StatusCode::BAD_REQUEST);
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await??;
        drop(sender);
        assert_eq!(in_flight.await??.status(), StatusCode::NO_CONTENT);
        daemon.stop().await;
        Ok(())
    }

    #[test]
    fn a_full_volume_reaches_the_client_as_507() {
        let full = || ApiError::from(std::io::Error::from(std::io::ErrorKind::StorageFull));
        assert!(matches!(
            api::CreateUploadResponse::from(full()),
            api::CreateUploadResponse::Status507_TheVolumeHasNoSpaceLeft(_)
        ));
        assert!(matches!(
            api::CommitUploadResponse::from(full()),
            api::CommitUploadResponse::Status507_TheVolumeHasNoSpaceLeft(_)
        ));
    }

    #[tokio::test]
    async fn uploads_reject_an_offset_mismatch_with_409() -> TestResult {
        let daemon = TestDaemon::start().await?;
        let id = create(&daemon, 10).await?;
        assert_eq!(put(&daemon, &id, 3, b"abc").await?, StatusCode::CONFLICT);
        assert_eq!(put(&daemon, &id, 0, b"abc").await?, StatusCode::NO_CONTENT);
        assert_eq!(put(&daemon, &id, 0, b"abc").await?, StatusCode::CONFLICT);
        daemon.stop().await;
        Ok(())
    }

    #[tokio::test]
    async fn uploads_reject_a_chunk_over_32_mib_with_400() -> TestResult {
        let daemon = TestDaemon::start().await?;
        let size = MAX_BODY_BYTES as i64 * 2;
        let id = create(&daemon, size).await?;
        let chunk = vec![1u8; MAX_BODY_BYTES as usize + 1];
        let uri = format!("/api/v1/uploads/{id}/content?offset=0");
        let (status, bytes) = daemon.request("PUT", &uri, Body::from(chunk)).await?;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let value: Value = serde_json::from_slice(&bytes)?;
        assert_eq!(value["kind"], "LimitExceeded");
        let (_, state) = daemon.get_json(&format!("/api/v1/uploads/{id}")).await?;
        assert_eq!(state["committedSize"], 0);
        assert_eq!(put(&daemon, &id, 0, &vec![1u8; MAX_BODY_BYTES as usize]).await?, StatusCode::NO_CONTENT);
        daemon.stop().await;
        Ok(())
    }

    #[tokio::test]
    async fn uploads_reject_a_chunk_beyond_the_declared_size_with_400() -> TestResult {
        let daemon = TestDaemon::start().await?;
        let id = create(&daemon, 2).await?;
        assert_eq!(put(&daemon, &id, 0, b"abc").await?, StatusCode::BAD_REQUEST);
        let (_, state) = daemon.get_json(&format!("/api/v1/uploads/{id}")).await?;
        assert_eq!(state["committedSize"], 0);
        daemon.stop().await;
        Ok(())
    }

    #[tokio::test]
    async fn uploads_reject_a_commit_of_missing_bytes_with_400() -> TestResult {
        let daemon = TestDaemon::start().await?;
        let id = create(&daemon, 10).await?;
        assert_eq!(put(&daemon, &id, 0, b"abc").await?, StatusCode::NO_CONTENT);
        let (status, _) = daemon.request("POST", &format!("/api/v1/uploads/{id}/commit"), Body::empty()).await?;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, _) = daemon.get_json(&format!("/api/v1/uploads/{id}")).await?;
        assert_eq!(status, StatusCode::OK);
        daemon.stop().await;
        Ok(())
    }

    #[tokio::test]
    async fn uploads_reject_a_bad_name_and_a_negative_size() -> TestResult {
        let daemon = TestDaemon::start().await?;
        let (status, _) = daemon.request_json("POST", "/api/v1/uploads", &json!({"fileName": "a/b", "size": 1})).await?;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, _) = daemon.request_json("POST", "/api/v1/uploads", &json!({"fileName": "a", "size": -1})).await?;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        daemon.stop().await;
        Ok(())
    }

    #[tokio::test]
    async fn uploads_delete_removes_the_session_and_its_bytes() -> TestResult {
        let daemon = TestDaemon::start().await?;
        let id = create(&daemon, 10).await?;
        assert_eq!(put(&daemon, &id, 0, b"abc").await?, StatusCode::NO_CONTENT);
        let (status, _) = daemon.request("DELETE", &format!("/api/v1/uploads/{id}"), Body::empty()).await?;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (status, _) = daemon.request("DELETE", &format!("/api/v1/uploads/{id}"), Body::empty()).await?;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(put(&daemon, &id, 3, b"abc").await?, StatusCode::NOT_FOUND);
        assert_eq!(std::fs::read_dir(daemon.dir.path().join("state/uploads"))?.count(), 0);
        daemon.stop().await;
        Ok(())
    }
}
