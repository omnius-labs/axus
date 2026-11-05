use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    body::Bytes,
    extract::{Query, Request, State, rejection::QueryRejection},
    response::{IntoResponse as _, Response},
};
use axum_extra::extract::CookieJar;
use headers::Host;
use http::{Method, StatusCode, header::CONTENT_LENGTH};

use omnius_axus_engine::model::{Publication, PublicationStatus};
use omnius_axus_interface::{apis::publications as api, models};

use crate::state::DaemonState;

use super::{
    ApiServer,
    api_error::{ApiError, impl_from_api_error},
    file_name::FileName,
    upload_session::MAX_BODY_BYTES,
};

impl_from_api_error!(api::GetPublicationResponse {
    StatusCode::NOT_FOUND => Status404_TheResourceDoesNotExist,
});
impl_from_api_error!(api::DeletePublicationResponse {
    StatusCode::NOT_FOUND => Status404_TheResourceDoesNotExist,
});

/// Converts the engine's publication to the contract's.
pub struct PublicationModel;

impl PublicationModel {
    pub fn from_engine(publication: Publication) -> models::Publication {
        let status = match publication.status {
            PublicationStatus::Pending => models::PublicationStatus::Pending,
            PublicationStatus::Processing => models::PublicationStatus::Processing,
            PublicationStatus::Failed => models::PublicationStatus::Failed,
            PublicationStatus::Committed => models::PublicationStatus::Committed,
        };
        let mut model = models::Publication::new(publication.id, publication.file_name, status, publication.created_at, publication.updated_at);
        model.root_hash = publication.root_hash.map(|hash| hash.to_string());
        model.failed_reason = publication.failed_reason;
        model
    }
}

#[allow(unused_variables)]
#[async_trait]
impl api::Publications for ApiServer {
    /// [`PublicationRoute`] answers this operation in front of the generated router.
    async fn create_publication(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        query_params: &models::CreatePublicationQueryParams,
        body: &Bytes,
    ) -> std::result::Result<api::CreatePublicationResponse, ()> {
        Err(())
    }

    async fn delete_publication(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::DeletePublicationPathParams,
    ) -> std::result::Result<api::DeletePublicationResponse, ()> {
        Ok(match self.state.service().delete_publication(&path_params.publication_id).await {
            Ok(true) => api::DeletePublicationResponse::Status204_ThePublicationIsGone,
            Ok(false) => ApiError::not_found("no such publication").into(),
            Err(error) => ApiError::from(error).into(),
        })
    }

    async fn get_publication(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::GetPublicationPathParams,
    ) -> std::result::Result<api::GetPublicationResponse, ()> {
        Ok(match self.state.service().find_publication(&path_params.publication_id).await {
            Ok(Some(publication)) => api::GetPublicationResponse::Status200_PublicationState(PublicationModel::from_engine(publication)),
            Ok(None) => ApiError::not_found("no such publication").into(),
            Err(error) => ApiError::from(error).into(),
        })
    }

    async fn list_publications(&self, method: &Method, host: &Host, cookies: &CookieJar) -> std::result::Result<api::ListPublicationsResponse, ()> {
        Ok(match self.state.service().publications().await {
            Ok(publications) => {
                api::ListPublicationsResponse::Status200_KnownPublications(models::PublicationList::new(publications.into_iter().map(PublicationModel::from_engine).collect()))
            }
            Err(error) => api::ListPublicationsResponse::Status500_InternalServerError(ApiError::from(error).body),
        })
    }
}

/// `POST /publications`, which streams the body to disk instead of reading it into memory.
pub struct PublicationRoute;

impl PublicationRoute {
    pub const PATH: &'static str = "/api/v1/publications";

    pub async fn post(
        State(state): State<Arc<DaemonState>>,
        query: std::result::Result<Query<models::CreatePublicationQueryParams>, QueryRejection>,
        request: Request,
    ) -> Response {
        // A closed connection drops this future, and the task still gets to delete a partial file.
        let task = tokio::spawn(async move { Self::publish(&state, query, request).await });
        match task.await {
            Ok(Ok(publication)) => (StatusCode::ACCEPTED, axum::Json(publication)).into_response(),
            Ok(Err(error)) => error.into_response(),
            Err(error) => ApiError::from(std::io::Error::other(error)).into_response(),
        }
    }

    async fn publish(
        state: &DaemonState,
        query: std::result::Result<Query<models::CreatePublicationQueryParams>, QueryRejection>,
        request: Request,
    ) -> std::result::Result<models::Publication, ApiError> {
        let Query(params) = query.map_err(|error| ApiError::invalid_request(error.body_text()))?;
        FileName::validate(&params.file_name)?;
        let declared = request
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok());
        if declared.is_some_and(|declared| declared > MAX_BODY_BYTES) {
            return Err(ApiError::payload_too_large("a file over 32 MiB must use an upload session"));
        }
        let staged = state.uploads.receive_whole(request.into_body()).await?;
        Self::hand_over(state, &staged, &params.file_name).await
    }

    /// Hands a received file to the engine, deleting it when the engine did not take it.
    pub async fn hand_over(state: &DaemonState, staged: &std::path::Path, file_name: &str) -> std::result::Result<models::Publication, ApiError> {
        match state.service().publish_staged_file(staged, file_name).await {
            Ok(publication) => Ok(PublicationModel::from_engine(publication)),
            Err(error) => {
                let _ = tokio::fs::remove_file(staged).await;
                Err(error.into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use http::StatusCode;
    use serde_json::Value;
    use testresult::TestResult;

    use crate::server::tests::TestDaemon;

    use super::MAX_BODY_BYTES;

    fn staged_entries(daemon: &TestDaemon) -> usize {
        std::fs::read_dir(daemon.dir.path().join("state/file_exchanger/file_publisher/staged")).unwrap().count()
    }

    async fn wait_no_staged(daemon: &TestDaemon) {
        for _ in 0..6000 {
            if staged_entries(daemon) == 0 {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        panic!("a staged file stayed");
    }

    #[tokio::test]
    async fn publications_accepts_32_mib_and_reaches_committed() -> TestResult {
        let daemon = TestDaemon::start().await?;
        let content = vec![7u8; MAX_BODY_BYTES as usize];
        let publication = daemon.publish("big.bin", &content).await?;
        assert!(matches!(publication["status"].as_str(), Some("Pending" | "Processing")));
        assert!(publication.get("rootHash").is_none());
        let uri = format!("/api/v1/publications/{}", publication["id"].as_str().unwrap());
        let committed = daemon.wait_status(&uri, "Committed").await?;
        assert!(committed["rootHash"].as_str().is_some());
        assert_eq!(committed["fileName"], "big.bin");
        wait_no_staged(&daemon).await;
        daemon.stop().await;
        Ok(())
    }

    #[tokio::test]
    async fn publications_rejects_a_body_over_32_mib() -> TestResult {
        let daemon = TestDaemon::start().await?;
        let content = vec![7u8; MAX_BODY_BYTES as usize + 1];
        let (status, _) = daemon.request("POST", "/api/v1/publications?fileName=big.bin", Body::from(content.clone())).await?;
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);

        // A stream declares no length, so the limit applies while the bytes arrive.
        let stream = futures_util::stream::iter([std::io::Result::Ok(axum::body::Bytes::from(content))]);
        let (status, bytes) = daemon.request("POST", "/api/v1/publications?fileName=big.bin", Body::from_stream(stream)).await?;
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
        let value: Value = serde_json::from_slice(&bytes)?;
        assert_eq!(value["kind"], "PayloadTooLarge");

        assert_eq!(std::fs::read_dir(daemon.dir.path().join("state/uploads"))?.count(), 0);
        daemon.stop().await;
        Ok(())
    }

    #[tokio::test]
    async fn publications_rejects_a_bad_file_name() -> TestResult {
        let daemon = TestDaemon::start().await?;
        for uri in [
            "/api/v1/publications",
            "/api/v1/publications?fileName=a%2Fb",
            "/api/v1/publications?fileName=..",
            "/api/v1/publications?fileName=",
        ] {
            let (status, _) = daemon.request("POST", uri, Body::from("x")).await?;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}");
        }
        daemon.stop().await;
        Ok(())
    }

    #[tokio::test]
    async fn publications_keep_the_root_hash_after_a_repeated_upload_and_a_restart_of_the_store() -> TestResult {
        let daemon = TestDaemon::start().await?;
        let first = daemon.publish("a.bin", b"content").await?;
        let first_uri = format!("/api/v1/publications/{}", first["id"].as_str().unwrap());
        let committed = daemon.wait_status(&first_uri, "Committed").await?;
        let second = daemon.publish("a.bin", b"content").await?;
        let second_uri = format!("/api/v1/publications/{}", second["id"].as_str().unwrap());
        let repeated = daemon.wait_status(&second_uri, "Committed").await?;
        assert_eq!(committed["rootHash"], repeated["rootHash"]);

        let (status, listed) = daemon.get_json("/api/v1/publications").await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(listed["publications"].as_array().unwrap().len(), 1);

        let (dir, conf) = daemon.stop_for_restart().await;
        let daemon = TestDaemon::restart(dir, conf).await?;
        let (status, after) = daemon.get_json(&second_uri).await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(after["status"], "Committed");
        assert_eq!(after["rootHash"], committed["rootHash"]);
        daemon.stop().await;
        Ok(())
    }

    #[tokio::test]
    async fn a_failed_encode_reports_failed_with_its_reason() -> TestResult {
        let daemon = TestDaemon::start().await?;
        let (dir, conf) = daemon.stop_for_restart().await;

        // A row whose source is gone is what an encode fails on, and the restart hands it to the real encoder.
        let options = sqlx::sqlite::SqliteConnectOptions::new().filename(dir.path().join("state/file_exchanger/file_publisher/repo/sqlite.db"));
        let db = sqlx::SqlitePool::connect_with(options).await?;
        sqlx::query(
            "INSERT INTO uncommitted_files (id, file_path, file_name, block_size, attrs, priority, status, failed_reason, created_at, updated_at)
                VALUES ('broken', ?, 'broken.bin', 1048576, NULL, 0, 'Pending', NULL, ?, ?)",
        )
        .bind(dir.path().join("missing").to_str().unwrap())
        .bind(chrono::Utc::now().naive_utc())
        .bind(chrono::Utc::now().naive_utc())
        .execute(&db)
        .await?;
        db.close().await;

        let daemon = TestDaemon::restart(dir, conf).await?;
        let failed = daemon.wait_status("/api/v1/publications/broken", "Failed").await?;
        assert!(failed["failedReason"].as_str().is_some_and(|reason| !reason.is_empty()));
        assert!(failed.get("rootHash").is_none());
        let (status, _) = daemon.request("DELETE", "/api/v1/publications/broken", Body::empty()).await?;
        assert_eq!(status, StatusCode::NO_CONTENT);
        daemon.stop().await;
        Ok(())
    }

    #[tokio::test]
    async fn deleting_stops_a_committed_publication_but_not_another_name_of_the_same_content() -> TestResult {
        let daemon = TestDaemon::start().await?;
        let a = daemon.publish("a.bin", b"content").await?;
        let a_uri = format!("/api/v1/publications/{}", a["id"].as_str().unwrap());
        let committed = daemon.wait_status(&a_uri, "Committed").await?;
        let b = daemon.publish("b.bin", b"content").await?;
        let b_uri = format!("/api/v1/publications/{}", b["id"].as_str().unwrap());
        daemon.wait_status(&b_uri, "Committed").await?;

        let (status, _) = daemon.request("DELETE", &a_uri, Body::empty()).await?;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (status, _) = daemon.request("DELETE", &a_uri, Body::empty()).await?;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (status, _) = daemon.get_json(&a_uri).await?;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (_, remaining) = daemon.get_json(&b_uri).await?;
        assert_eq!(remaining["rootHash"], committed["rootHash"]);

        let (status, _) = daemon.request("DELETE", &b_uri, Body::empty()).await?;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (_, listed) = daemon.get_json("/api/v1/publications").await?;
        assert!(listed["publications"].as_array().unwrap().is_empty());
        wait_no_staged(&daemon).await;
        daemon.stop().await;
        Ok(())
    }

    #[tokio::test]
    async fn deleting_cancels_an_uncommitted_publication_and_leaves_no_staged_file() -> TestResult {
        let daemon = TestDaemon::start().await?;
        let content = vec![7u8; 8 * 1024 * 1024];
        let publication = daemon.publish("a.bin", &content).await?;
        let uri = format!("/api/v1/publications/{}", publication["id"].as_str().unwrap());
        let (status, _) = daemon.request("DELETE", &uri, Body::empty()).await?;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (status, _) = daemon.get_json(&uri).await?;
        assert_eq!(status, StatusCode::NOT_FOUND);
        wait_no_staged(&daemon).await;
        daemon.stop().await;
        Ok(())
    }
}
