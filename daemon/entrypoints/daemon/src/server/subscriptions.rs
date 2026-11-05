use std::{str::FromStr as _, sync::Arc};

use async_trait::async_trait;
use axum::{
    extract::{Path, Request, State},
    response::{IntoResponse as _, Response},
};
use axum_extra::extract::CookieJar;
use headers::Host;
use http::{
    HeaderValue, Method, StatusCode,
    header::{CONTENT_DISPOSITION, X_CONTENT_TYPE_OPTIONS},
};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use tower::ServiceExt as _;
use tower_http::services::ServeFile;

use omnius_axus_engine::model::{SubscribedFile, SubscribedFileStatus};
use omnius_axus_interface::{apis::subscriptions as api, models};
use omnius_core_omnikit::generated::omni_hash::{OmniHash, OmniHashAlgorithmType};

use crate::state::DaemonState;

use super::{
    ApiServer,
    api_error::{ApiError, impl_from_api_error},
    file_name::FileName,
};

impl_from_api_error!(api::GetSubscriptionResponse {
    StatusCode::NOT_FOUND => Status404_TheResourceDoesNotExist,
});
impl_from_api_error!(api::CreateSubscriptionResponse {
    StatusCode::BAD_REQUEST => Status400_TheRequestIsInvalid,
    StatusCode::CONFLICT => Status409_TheResource,
});
impl_from_api_error!(api::CancelSubscriptionResponse {
    StatusCode::NOT_FOUND => Status404_TheResourceDoesNotExist,
    StatusCode::CONFLICT => Status409_TheResource,
});
impl_from_api_error!(api::DeleteSubscriptionResponse {
    StatusCode::NOT_FOUND => Status404_TheResourceDoesNotExist,
    StatusCode::CONFLICT => Status409_TheResource,
});

/// Converts the engine's subscription to the contract's.
struct SubscriptionModel;

impl SubscriptionModel {
    fn from_engine(file: SubscribedFile) -> models::Subscription {
        let status = match file.status {
            SubscribedFileStatus::Downloading => models::SubscriptionStatus::Downloading,
            SubscribedFileStatus::Decoding => models::SubscriptionStatus::Decoding,
            SubscribedFileStatus::Finalizing => models::SubscriptionStatus::Finalizing,
            SubscribedFileStatus::Completed => models::SubscriptionStatus::Completed,
            SubscribedFileStatus::Failed | SubscribedFileStatus::Unknown => models::SubscriptionStatus::Failed,
            SubscribedFileStatus::Canceled => models::SubscriptionStatus::Canceled,
        };
        let mut model = models::Subscription::new(
            file.id,
            file.root_hash.to_string(),
            file.output_name,
            status,
            i64::from(file.block_count_downloaded),
            i64::from(file.block_count_total),
            file.created_at,
            file.updated_at,
        );
        model.rank = (file.rank != SubscribedFile::UNKNOWN_ROOT_RANK).then_some(i64::from(file.rank));
        model.failed_reason = file.failed_reason;
        model
    }
}

#[allow(unused_variables)]
#[async_trait]
impl api::Subscriptions for ApiServer {
    async fn cancel_subscription(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::CancelSubscriptionPathParams,
    ) -> std::result::Result<api::CancelSubscriptionResponse, ()> {
        Ok(match self.cancel(&path_params.subscription_id).await {
            Ok(subscription) => api::CancelSubscriptionResponse::Status200_SubscriptionState(subscription),
            Err(error) => error.into(),
        })
    }

    async fn create_subscription(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        body: &models::CreateSubscriptionRequest,
    ) -> std::result::Result<api::CreateSubscriptionResponse, ()> {
        Ok(match self.subscribe(body).await {
            Ok(subscription) => api::CreateSubscriptionResponse::Status202_TheFetchIsQueued(subscription),
            Err(error) => error.into(),
        })
    }

    async fn delete_subscription(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::DeleteSubscriptionPathParams,
        query_params: &models::DeleteSubscriptionQueryParams,
    ) -> std::result::Result<api::DeleteSubscriptionResponse, ()> {
        let delete_output = query_params.delete_output.unwrap_or(false);
        Ok(match self.remove(&path_params.subscription_id, delete_output).await {
            Ok(()) => api::DeleteSubscriptionResponse::Status204_TheSubscriptionIsGone,
            Err(error) => error.into(),
        })
    }

    async fn get_subscription(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::GetSubscriptionPathParams,
    ) -> std::result::Result<api::GetSubscriptionResponse, ()> {
        Ok(match self.state.service().find_subscription(&path_params.subscription_id).await {
            Ok(Some(file)) => api::GetSubscriptionResponse::Status200_SubscriptionState(SubscriptionModel::from_engine(file)),
            Ok(None) => ApiError::not_found("no such subscription").into(),
            Err(error) => ApiError::from(error).into(),
        })
    }

    /// [`SubscriptionContentRoute`] answers this operation in front of the generated router.
    async fn get_subscription_content(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::GetSubscriptionContentPathParams,
    ) -> std::result::Result<api::GetSubscriptionContentResponse, ()> {
        Err(())
    }

    async fn list_subscriptions(&self, method: &Method, host: &Host, cookies: &CookieJar) -> std::result::Result<api::ListSubscriptionsResponse, ()> {
        Ok(match self.state.service().subscriptions().await {
            Ok(files) => {
                api::ListSubscriptionsResponse::Status200_KnownSubscriptions(models::SubscriptionList::new(files.into_iter().map(SubscriptionModel::from_engine).collect()))
            }
            Err(error) => api::ListSubscriptionsResponse::Status500_InternalServerError(ApiError::from(error).body),
        })
    }
}

impl ApiServer {
    async fn subscribe(&self, body: &models::CreateSubscriptionRequest) -> std::result::Result<models::Subscription, ApiError> {
        FileName::validate(&body.file_name)?;
        // A hash without an algorithm never matches a block, so the subscription could never finish.
        let root_hash = OmniHash::from_str(&body.root_hash)
            .ok()
            .filter(|hash| hash.typ != OmniHashAlgorithmType::None && !hash.value.is_empty())
            .ok_or_else(|| ApiError::invalid_request("the root hash is invalid"))?;
        let output_path = self.state.conf.storage.download_dir.join(&body.file_name);
        let file = self.state.service().subscribe(&root_hash, &output_path).await?;
        Ok(SubscriptionModel::from_engine(file))
    }

    async fn find(&self, id: &str) -> std::result::Result<SubscribedFile, ApiError> {
        self.state.service().find_subscription(id).await?.ok_or_else(|| ApiError::not_found("no such subscription"))
    }

    async fn cancel(&self, id: &str) -> std::result::Result<models::Subscription, ApiError> {
        self.find(id).await?;
        self.state.service().cancel_subscription(id).await?;
        Ok(SubscriptionModel::from_engine(self.find(id).await?))
    }

    async fn remove(&self, id: &str, delete_output: bool) -> std::result::Result<(), ApiError> {
        self.find(id).await?;
        self.state.service().remove_subscription(id, delete_output).await?;
        Ok(())
    }
}

/// `GET /subscriptions/{id}/content`, which streams the output instead of reading it into memory.
pub struct SubscriptionContentRoute;

impl SubscriptionContentRoute {
    pub const PATH: &'static str = "/api/v1/subscriptions/{subscription_id}/content";

    /// RFC 5987's `attr-char`, which stays unencoded in `filename*`.
    const ATTR_CHAR_ENCODE: &'static AsciiSet = &NON_ALPHANUMERIC
        .remove(b'!')
        .remove(b'#')
        .remove(b'$')
        .remove(b'&')
        .remove(b'+')
        .remove(b'-')
        .remove(b'.')
        .remove(b'^')
        .remove(b'_')
        .remove(b'`')
        .remove(b'|')
        .remove(b'~');

    pub async fn get(State(state): State<Arc<DaemonState>>, Path(id): Path<String>, request: Request) -> Response {
        match Self::serve(&state, &id, request).await {
            Ok(response) => response,
            Err(error) => error.into_response(),
        }
    }

    async fn serve(state: &DaemonState, id: &str, request: Request) -> std::result::Result<Response, ApiError> {
        let file = state.service().find_subscription(id).await?.ok_or_else(|| ApiError::not_found("no such subscription"))?;
        if file.status != SubscribedFileStatus::Completed {
            return Err(ApiError::conflict("the subscription is not completed"));
        }
        let path = file.output_path();
        match tokio::fs::metadata(&path).await {
            Ok(metadata) if metadata.is_file() => {}
            Ok(_) => return Err(ApiError::not_found("the output is gone")),
            Err(error) if matches!(error.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory) => return Err(ApiError::not_found("the output is gone")),
            Err(error) => return Err(error.into()),
        }
        let response = match ServeFile::new_with_mime(&path, &mime::APPLICATION_OCTET_STREAM).oneshot(request).await {
            Ok(response) => response,
            Err(never) => match never {},
        };
        let mut response = response.into_response();
        let headers = response.headers_mut();
        headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
        let disposition = format!("attachment; filename*=UTF-8''{}", utf8_percent_encode(&file.output_name, Self::ATTR_CHAR_ENCODE));
        headers.insert(
            CONTENT_DISPOSITION,
            HeaderValue::from_str(&disposition).map_err(|_| ApiError::invalid_request("the output name cannot be sent"))?,
        );
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use http::{Request, StatusCode};
    use serde_json::{Value, json};
    use testresult::TestResult;

    use omnius_core_omnikit::generated::omni_hash::{OmniHash, OmniHashAlgorithmType};

    use crate::server::tests::TestDaemon;

    async fn root_hash(daemon: &TestDaemon, content: &[u8]) -> TestResult<String> {
        let publication = daemon.publish("source.bin", content).await?;
        let uri = format!("/api/v1/publications/{}", publication["id"].as_str().unwrap());
        Ok(daemon.wait_status(&uri, "Committed").await?["rootHash"].as_str().unwrap().to_string())
    }

    async fn subscribe(daemon: &TestDaemon, root_hash: &str, name: &str) -> TestResult<(StatusCode, Value)> {
        daemon
            .request_json("POST", "/api/v1/subscriptions", &json!({"rootHash": root_hash, "fileName": name}))
            .await
    }

    /// A hash that is valid but held by nobody, so the subscription stays Downloading.
    fn unheld_hash() -> String {
        OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, b"held by nobody").to_string()
    }

    #[tokio::test]
    async fn subscriptions_place_the_output_in_the_download_directory() -> TestResult {
        let daemon = TestDaemon::start().await?;
        let hash = unheld_hash();
        let (status, value) = subscribe(&daemon, &hash, "out.bin").await?;
        assert_eq!(status, StatusCode::ACCEPTED);
        assert_eq!(value["fileName"], "out.bin");
        let downloads = std::fs::canonicalize(daemon.dir.path().join("downloads"))?;
        let file = daemon.state.service().find_subscription(value["id"].as_str().unwrap()).await?.unwrap();
        assert_eq!(file.output_path(), downloads.join("out.bin"));
        let (status, listed) = daemon.get_json("/api/v1/subscriptions").await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(listed["subscriptions"].as_array().unwrap().len(), 1);
        daemon.stop().await;
        Ok(())
    }

    #[tokio::test]
    async fn subscriptions_reject_a_name_that_is_not_one_component_and_a_bad_hash() -> TestResult {
        let daemon = TestDaemon::start().await?;
        let hash = unheld_hash();
        for name in ["", ".", "..", "a/b", "../escape", "/etc/passwd", "a\\b", ".out.axus-1.000000000.ab.part"] {
            assert_eq!(subscribe(&daemon, &hash, name).await?.0, StatusCode::BAD_REQUEST, "{name}");
        }
        for bad_hash in ["not-a-hash", "none:AAAA", "unknown:AAAA", "sha3_256:"] {
            assert_eq!(subscribe(&daemon, bad_hash, "out.bin").await?.0, StatusCode::BAD_REQUEST, "{bad_hash}");
        }
        daemon.stop().await;
        Ok(())
    }

    #[tokio::test]
    async fn subscriptions_reject_a_name_in_use_with_409() -> TestResult {
        let daemon = TestDaemon::start().await?;
        let hash = unheld_hash();
        assert_eq!(subscribe(&daemon, &hash, "active.bin").await?.0, StatusCode::ACCEPTED);
        assert_eq!(subscribe(&daemon, &hash, "active.bin").await?.0, StatusCode::CONFLICT);
        std::fs::write(daemon.dir.path().join("downloads/existing.bin"), "existing")?;
        assert_eq!(subscribe(&daemon, &hash, "existing.bin").await?.0, StatusCode::CONFLICT);
        assert_eq!(std::fs::read(daemon.dir.path().join("downloads/existing.bin"))?, b"existing");
        daemon.stop().await;
        Ok(())
    }

    #[tokio::test]
    async fn subscriptions_cancel_keeps_the_row_and_unknown_ids_return_404() -> TestResult {
        let daemon = TestDaemon::start().await?;
        let hash = unheld_hash();
        let (_, value) = subscribe(&daemon, &hash, "out.bin").await?;
        let id = value["id"].as_str().unwrap();
        let (status, canceled) = daemon.request_json("POST", &format!("/api/v1/subscriptions/{id}/cancel"), &Value::Null).await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(canceled["status"], "Canceled");
        let (status, value) = daemon.get_json(&format!("/api/v1/subscriptions/{id}")).await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value["status"], "Canceled");

        for (method, uri) in [
            ("GET", "/api/v1/subscriptions/unknown"),
            ("DELETE", "/api/v1/subscriptions/unknown"),
            ("POST", "/api/v1/subscriptions/unknown/cancel"),
            ("GET", "/api/v1/subscriptions/unknown/content"),
        ] {
            assert_eq!(daemon.request(method, uri, Body::empty()).await?.0, StatusCode::NOT_FOUND, "{method} {uri}");
        }
        daemon.stop().await;
        Ok(())
    }

    #[tokio::test]
    async fn subscriptions_cancel_delete_and_content_are_a_conflict_while_finalizing() -> TestResult {
        let daemon = TestDaemon::start().await?;
        let (dir, conf) = daemon.stop_for_restart().await;

        // A row whose destination directory is unreachable stays Finalizing, because the placement keeps retrying.
        let options = sqlx::sqlite::SqliteConnectOptions::new().filename(dir.path().join("state/file_exchanger/file_subscriber/repo/sqlite.db"));
        let db = sqlx::SqlitePool::connect_with(options).await?;
        sqlx::query(
            "INSERT INTO files (id, root_hash, output_directory, output_name, rank, block_count_downloaded, block_count_total, attrs, priority, status, failed_reason, created_at, updated_at)
                VALUES ('finalizing', ?, ?, 'out.bin', 0, 1, 1, NULL, 0, 'Finalizing', NULL, ?, ?)",
        )
        .bind(unheld_hash())
        .bind(dir.path().join("unreachable").to_str().unwrap())
        .bind(chrono::Utc::now().naive_utc())
        .bind(chrono::Utc::now().naive_utc())
        .execute(&db)
        .await?;
        db.close().await;

        let daemon = TestDaemon::restart(dir, conf).await?;
        let uri = "/api/v1/subscriptions/finalizing";
        let (status, value) = daemon.get_json(uri).await?;
        assert_eq!((status, value["status"].as_str()), (StatusCode::OK, Some("Finalizing")));
        assert_eq!(daemon.request("POST", &format!("{uri}/cancel"), Body::empty()).await?.0, StatusCode::CONFLICT);
        assert_eq!(daemon.request("DELETE", uri, Body::empty()).await?.0, StatusCode::CONFLICT);
        assert_eq!(daemon.request("DELETE", &format!("{uri}?deleteOutput=true"), Body::empty()).await?.0, StatusCode::CONFLICT);
        assert_eq!(daemon.request("GET", &format!("{uri}/content"), Body::empty()).await?.0, StatusCode::CONFLICT);
        let (_, value) = daemon.get_json(uri).await?;
        assert_eq!(value["status"], "Finalizing");
        daemon.stop().await;
        Ok(())
    }

    #[tokio::test]
    async fn subscriptions_content_is_a_conflict_until_completed() -> TestResult {
        let daemon = TestDaemon::start().await?;
        let hash = unheld_hash();
        let (_, value) = subscribe(&daemon, &hash, "out.bin").await?;
        let id = value["id"].as_str().unwrap();
        daemon.request("POST", &format!("/api/v1/subscriptions/{id}/cancel"), Body::empty()).await?;
        let (status, _) = daemon.request("GET", &format!("/api/v1/subscriptions/{id}/content"), Body::empty()).await?;
        assert_eq!(status, StatusCode::CONFLICT);
        daemon.stop().await;
        Ok(())
    }

    #[tokio::test]
    async fn subscriptions_delete_keeps_the_output_unless_asked() -> TestResult {
        let publisher = TestDaemon::start().await?;
        let subscriber = TestDaemon::start_with(vec![publisher.state.service().my_node_profile()], None).await?;
        let hash = root_hash(&publisher, b"content").await?;
        let output = subscriber.dir.path().join("downloads/out.bin");

        let (_, value) = subscribe(&subscriber, &hash, "out.bin").await?;
        let uri = format!("/api/v1/subscriptions/{}", value["id"].as_str().unwrap());
        subscriber.wait_status(&uri, "Completed").await?;
        let (status, _) = subscriber.request("DELETE", &uri, Body::empty()).await?;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert_eq!(std::fs::read(&output)?, b"content");

        std::fs::remove_file(&output)?;
        let (_, value) = subscribe(&subscriber, &hash, "out.bin").await?;
        let uri = format!("/api/v1/subscriptions/{}", value["id"].as_str().unwrap());
        subscriber.wait_status(&uri, "Completed").await?;
        let (status, downloaded) = subscriber.request("GET", &format!("{uri}/content"), Body::empty()).await?;
        assert_eq!((status, downloaded.as_ref()), (StatusCode::OK, b"content".as_ref()));

        std::fs::remove_file(&output)?;
        let (status, _) = subscriber.request("GET", &format!("{uri}/content"), Body::empty()).await?;
        assert_eq!(status, StatusCode::NOT_FOUND);
        std::fs::write(&output, "content")?;

        let (status, _) = subscriber.request("DELETE", &format!("{uri}?deleteOutput=true"), Body::empty()).await?;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert!(!output.exists());

        subscriber.stop().await;
        publisher.stop().await;
        Ok(())
    }

    #[tokio::test]
    async fn subscriptions_content_encodes_the_file_name_as_an_attachment() -> TestResult {
        let publisher = TestDaemon::start().await?;
        let subscriber = TestDaemon::start_with(vec![publisher.state.service().my_node_profile()], None).await?;
        let hash = root_hash(&publisher, b"<script>alert(1)</script>").await?;
        let (_, value) = subscribe(&subscriber, &hash, "日本語 a.html").await?;
        let uri = format!("/api/v1/subscriptions/{}", value["id"].as_str().unwrap());
        subscriber.wait_status(&uri, "Completed").await?;
        let request = Request::builder().uri(format!("{uri}/content")).header("host", "localhost").body(Body::empty())?;
        let response = subscriber.send(request).await?;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["content-type"], "application/octet-stream");
        assert_eq!(response.headers()["x-content-type-options"], "nosniff");
        assert_eq!(
            response.headers()["content-disposition"],
            "attachment; filename*=UTF-8''%E6%97%A5%E6%9C%AC%E8%AA%9E%20a.html"
        );
        subscriber.stop().await;
        publisher.stop().await;
        Ok(())
    }
}
