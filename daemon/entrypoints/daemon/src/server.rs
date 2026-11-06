use std::{path::PathBuf, str::FromStr as _, sync::Arc, time::SystemTime};

use async_trait::async_trait;
use axum::{
    Json,
    body::Body,
    extract::{Path, Request, State},
    http::{HeaderMap, Method, StatusCode, header},
    response::{IntoResponse, Response},
};
use axum_extra::extract::CookieJar;
use bytes::Bytes;
use futures_util::StreamExt as _;
use headers::Host;
use serde_json::json;
use tokio::io::{AsyncReadExt as _, AsyncSeekExt as _};
use tokio_util::{io::ReaderStream, sync::CancellationToken};
use tower::ServiceExt as _;

use omnius_axus_engine::{
    ErrorKind as EngineErrorKind,
    model::{AppendContentResult, Publication, PublicationRemoveResult, PublicationStatus, SubscribedFile, SubscribedFileStatus},
};
use omnius_axus_interface::{
    apis::{
        authentication as authentication_api,
        health::{GetHealthResponse, Health},
        library as library_api, publications as publications_api, subscriptions as subscriptions_api,
    },
    models,
};
use omnius_core_omnikit::generated::omni_hash::OmniHash;

use crate::{prelude::*, state::DaemonState};

const CONTENT_PATH: &str = "/api/v1";

#[derive(serde::Deserialize)]
struct LoginBody {
    name: String,
    password: String,
}
const MAX_CHUNK_BYTES: usize = 64 * 1024 * 1024;

pub struct ApiServer {
    state: Arc<DaemonState>,
}

impl omnius_axus_interface::apis::ErrorHandler for ApiServer {}

#[allow(unused_variables)]
#[async_trait]
impl Health for ApiServer {
    async fn get_health(&self, method: &Method, host: &Host, cookies: &CookieJar) -> std::result::Result<GetHealthResponse, ()> {
        Ok(GetHealthResponse::Status200_TheDaemonIsRunning(models::HealthResponse::new(true)))
    }
}

/// What a request failed with, in the terms of the REST contract.
pub struct ApiError {
    status: StatusCode,
    body: models::ErrorResponse,
}

impl ApiError {
    fn new(status: StatusCode, kind: models::ErrorKind, message: impl Into<String>) -> Self {
        let mut body = models::ErrorResponse::new(kind);
        body.message = Some(message.into());
        Self { status, body }
    }

    fn invalid_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, models::ErrorKind::InvalidRequest, message)
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, models::ErrorKind::NotFound, message)
    }

    fn conflict(message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, models::ErrorKind::Conflict, message)
    }

    fn payload_too_large(message: impl Into<String>) -> Self {
        Self::new(StatusCode::PAYLOAD_TOO_LARGE, models::ErrorKind::PayloadTooLarge, message)
    }

    fn range_not_satisfiable(message: impl Into<String>) -> Self {
        Self::new(StatusCode::RANGE_NOT_SATISFIABLE, models::ErrorKind::InvalidRequest, message)
    }

    fn internal(error: &dyn std::fmt::Debug) -> Self {
        error!(?error, "request failed");
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, models::ErrorKind::InternalServerError, "unexpected server error")
    }

    fn with_received_bytes(mut self, received: i64) -> Self {
        self.body.received_bytes = Some(received);
        self
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(self.body)).into_response()
    }
}

impl From<std::io::Error> for ApiError {
    fn from(error: std::io::Error) -> Self {
        if error.kind() == std::io::ErrorKind::StorageFull {
            return Self::new(StatusCode::INSUFFICIENT_STORAGE, models::ErrorKind::InsufficientStorage, "no space left on the volume");
        }
        Self::internal(&error)
    }
}

impl From<crate::error::Error> for ApiError {
    fn from(error: crate::error::Error) -> Self {
        let message = omnius_core_base::error::OmniError::message(&error).unwrap_or("rejected").to_string();
        match omnius_core_base::error::OmniError::kind(&error) {
            crate::error::ErrorKind::NotFound => Self::not_found(message),
            _ => Self::invalid_request(message),
        }
    }
}

impl From<omnius_axus_engine::Error> for ApiError {
    fn from(error: omnius_axus_engine::Error) -> Self {
        let message = error.message().unwrap_or("rejected").to_string();
        match error.kind() {
            EngineErrorKind::NotFound => Self::not_found(message),
            EngineErrorKind::AlreadyExists | EngineErrorKind::Reject => Self::conflict(message),
            EngineErrorKind::InvalidFormat => Self::invalid_request(message),
            _ => Self::internal(&error),
        }
    }
}

/// Maps an [`ApiError`] onto the generated response enum of one operation.
macro_rules! impl_from_api_error {
    ($response:ty { $($status:path => $variant:ident),* $(,)? }) => {
        impl From<ApiError> for $response {
            fn from(error: ApiError) -> Self {
                match error.status {
                    $($status => Self::$variant(error.body),)*
                    _ => Self::Status500_InternalServerError(error.body),
                }
            }
        }
    };
}

impl_from_api_error!(publications_api::CreatePublicationResponse {
    StatusCode::BAD_REQUEST => Status400_TheRequestIsMalformed,
    StatusCode::CONFLICT => Status409_TheRequestConflictsWithTheCurrentState,
    StatusCode::PAYLOAD_TOO_LARGE => Status413_TheContentIsOverTheDeclaredOrConfiguredSize,
    StatusCode::INSUFFICIENT_STORAGE => Status507_TheDestinationHasInsufficientSpace,
});
impl_from_api_error!(publications_api::ListPublicationsResponse {});
impl_from_api_error!(publications_api::GetPublicationResponse {
    StatusCode::NOT_FOUND => Status404_TheResourceDoesNotExist,
});
impl_from_api_error!(publications_api::CancelPublicationResponse {
    StatusCode::NOT_FOUND => Status404_TheResourceDoesNotExist,
    StatusCode::CONFLICT => Status409_TheRequestConflictsWithTheCurrentState,
});
impl_from_api_error!(publications_api::DeletePublicationResponse {
    StatusCode::NOT_FOUND => Status404_TheResourceDoesNotExist,
    StatusCode::CONFLICT => Status409_TheRequestConflictsWithTheCurrentState,
});
impl_from_api_error!(library_api::ListLibraryRootsResponse {});
impl_from_api_error!(library_api::ListLibraryEntriesResponse {
    StatusCode::BAD_REQUEST => Status400_TheRequestIsMalformed,
    StatusCode::NOT_FOUND => Status404_TheResourceDoesNotExist,
});
impl_from_api_error!(subscriptions_api::CreateSubscriptionResponse {
    StatusCode::BAD_REQUEST => Status400_TheRequestIsMalformed,
    StatusCode::CONFLICT => Status409_TheRequestConflictsWithTheCurrentState,
});
impl_from_api_error!(subscriptions_api::ListSubscriptionsResponse {});
impl_from_api_error!(subscriptions_api::GetSubscriptionResponse {
    StatusCode::NOT_FOUND => Status404_TheResourceDoesNotExist,
});
impl_from_api_error!(subscriptions_api::CancelSubscriptionResponse {
    StatusCode::NOT_FOUND => Status404_TheResourceDoesNotExist,
    StatusCode::CONFLICT => Status409_TheRequestConflictsWithTheCurrentState,
});
impl_from_api_error!(subscriptions_api::DeleteSubscriptionResponse {
    StatusCode::NOT_FOUND => Status404_TheResourceDoesNotExist,
    StatusCode::CONFLICT => Status409_TheRequestConflictsWithTheCurrentState,
});

struct PublicationModel;

impl PublicationModel {
    fn from_engine(publication: Publication) -> models::Publication {
        let status = match publication.status {
            PublicationStatus::Receiving => models::PublicationStatus::Receiving,
            PublicationStatus::Pending => models::PublicationStatus::Pending,
            PublicationStatus::Processing => models::PublicationStatus::Processing,
            PublicationStatus::Committed => models::PublicationStatus::Committed,
            PublicationStatus::Failed => models::PublicationStatus::Failed,
            PublicationStatus::Canceled => models::PublicationStatus::Canceled,
        };
        let mut model = models::Publication::new(
            publication.id,
            publication.file_name,
            publication.size,
            publication.received_bytes,
            status,
            publication.created_at,
            publication.updated_at,
        );
        model.root_hash = publication.root_hash.map(|hash| hash.to_string());
        model.failed_reason = publication.failed_reason;
        model
    }
}

struct SubscriptionModel;

impl SubscriptionModel {
    fn from_engine(subscription: SubscribedFile) -> models::Subscription {
        let status = match subscription.status {
            SubscribedFileStatus::Downloading => models::SubscriptionStatus::Downloading,
            SubscribedFileStatus::Decoding => models::SubscriptionStatus::Decoding,
            SubscribedFileStatus::Finalizing => models::SubscriptionStatus::Finalizing,
            SubscribedFileStatus::Completed => models::SubscriptionStatus::Completed,
            SubscribedFileStatus::Failed => models::SubscriptionStatus::Failed,
            SubscribedFileStatus::Canceled => models::SubscriptionStatus::Canceled,
            SubscribedFileStatus::Unknown => models::SubscriptionStatus::Failed,
        };
        let mut model = models::Subscription::new(
            subscription.id,
            subscription.root_hash.to_string(),
            subscription.output_name,
            status,
            i64::from(subscription.block_count_downloaded),
            i64::from(subscription.block_count_total),
            subscription.created_at,
            subscription.updated_at,
        );
        if subscription.rank != SubscribedFile::UNKNOWN_ROOT_RANK {
            model.rank = Some(i64::from(subscription.rank));
        }
        model.failed_reason = subscription.failed_reason;
        model
    }
}

#[allow(unused_variables)]
#[async_trait]
impl publications_api::Publications for ApiServer {
    async fn create_publication(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        body: &models::CreatePublicationRequest,
    ) -> std::result::Result<publications_api::CreatePublicationResponse, ()> {
        let result: std::result::Result<models::Publication, ApiError> = async {
            let source = body.source.as_ref();
            if source.is_some_and(|source| source.kind == "library") {
                let root_id = source
                    .and_then(|source| source.root_id.as_deref())
                    .ok_or_else(|| ApiError::invalid_request("rootId is required for a library source"))?;
                let relative = source.and_then(|source| source.path.as_deref()).unwrap_or_default();
                let (path, size, modified_at) = self.state.library().resolve_file(root_id, relative)?;
                let file_name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or_else(|| ApiError::invalid_request("the library file name is not UTF-8"))?;
                let publication = self
                    .state
                    .service()
                    .create_library_publication(&path.to_string_lossy(), file_name, size, modified_at)
                    .await?;
                return Ok(PublicationModel::from_engine(publication));
            }
            let file_name = body.file_name.as_deref().ok_or_else(|| ApiError::invalid_request("fileName is required"))?;
            let size = body.size.ok_or_else(|| ApiError::invalid_request("size is required"))?;
            FileName::validate(file_name)?;
            if size < 0 {
                return Err(ApiError::invalid_request("the declared size must not be negative"));
            }
            if self.state.conf.api.max_upload_size > 0 && size > self.state.conf.api.max_upload_size {
                return Err(ApiError::payload_too_large("the declared size is over api.max_upload_size"));
            }
            let publication = self.state.service().create_publication(&self.state.uploads_dir(), file_name, size).await?;
            Ok(PublicationModel::from_engine(publication))
        }
        .await;
        Ok(match result {
            Ok(publication) => publications_api::CreatePublicationResponse::Status202_ThePublicationIsReadyToReceiveContent(publication),
            Err(error) => error.into(),
        })
    }

    async fn list_publications(&self, method: &Method, host: &Host, cookies: &CookieJar) -> std::result::Result<publications_api::ListPublicationsResponse, ()> {
        match self.state.service().list_publications().await {
            Ok(publications) => Ok(publications_api::ListPublicationsResponse::Status200_KnownPublications(models::PublicationList::new(
                publications.into_iter().map(PublicationModel::from_engine).collect(),
            ))),
            Err(error) => Ok(ApiError::from(error).into()),
        }
    }

    async fn get_publication(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::GetPublicationPathParams,
    ) -> std::result::Result<publications_api::GetPublicationResponse, ()> {
        match self.state.service().get_publication(&path_params.publication_id).await {
            Ok(Some(publication)) => Ok(publications_api::GetPublicationResponse::Status200_PublicationState(PublicationModel::from_engine(
                publication,
            ))),
            Ok(None) => Ok(ApiError::not_found("no such publication").into()),
            Err(error) => Ok(ApiError::from(error).into()),
        }
    }

    async fn cancel_publication(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::CancelPublicationPathParams,
    ) -> std::result::Result<publications_api::CancelPublicationResponse, ()> {
        let id = &path_params.publication_id;
        let result = async {
            let Some(current) = self.state.service().get_publication(id).await? else {
                return Err(ApiError::not_found("no such publication"));
            };
            if matches!(current.status, PublicationStatus::Committed) {
                return Err(ApiError::conflict("the publication is already committed"));
            }
            self.state.service().cancel_publication(id).await?;
            let updated = self.state.service().get_publication(id).await?.unwrap_or(current);
            Ok(PublicationModel::from_engine(updated))
        }
        .await;
        Ok(match result {
            Ok(publication) => publications_api::CancelPublicationResponse::Status200_ThePublicationStateAfterTheRequest(publication),
            Err(error) => error.into(),
        })
    }

    async fn delete_publication(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::DeletePublicationPathParams,
    ) -> std::result::Result<publications_api::DeletePublicationResponse, ()> {
        match self.state.service().remove_publication(&path_params.publication_id).await {
            Ok(PublicationRemoveResult::Removed) => Ok(publications_api::DeletePublicationResponse::Status204_ThePublicationIsGone),
            Ok(PublicationRemoveResult::Active) => Ok(ApiError::conflict("cancel the publication first").into()),
            Ok(PublicationRemoveResult::NotFound) => Ok(ApiError::not_found("no such publication").into()),
            Err(error) => Ok(ApiError::from(error).into()),
        }
    }

    async fn write_publication_content(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        header_params: &models::WritePublicationContentHeaderParams,
        path_params: &models::WritePublicationContentPathParams,
        body: &Bytes,
    ) -> std::result::Result<publications_api::WritePublicationContentResponse, ()> {
        Err(())
    }
}

#[allow(unused_variables)]
#[async_trait]
impl authentication_api::Authentication for ApiServer {
    /// The setup routes are answered by [`ApiServer::setup`] and [`ApiServer::setup_status`] in front of the generated router.
    async fn create_owner(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        body: &models::Credentials,
    ) -> std::result::Result<authentication_api::CreateOwnerResponse, ()> {
        Err(())
    }

    /// [`ApiServer::login`] answers this operation in front of the generated router.
    async fn create_session(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        header_params: &models::CreateSessionHeaderParams,
        body: &models::Credentials,
    ) -> std::result::Result<authentication_api::CreateSessionResponse, ()> {
        Err(())
    }

    /// [`ApiServer::logout`] answers this operation in front of the generated router.
    async fn delete_session(&self, method: &Method, host: &Host, cookies: &CookieJar) -> std::result::Result<authentication_api::DeleteSessionResponse, ()> {
        Err(())
    }

    /// [`ApiServer::setup_status`] answers this operation in front of the generated router.
    async fn get_setup_status(&self, method: &Method, host: &Host, cookies: &CookieJar) -> std::result::Result<authentication_api::GetSetupStatusResponse, ()> {
        Err(())
    }
}

#[allow(unused_variables)]
#[async_trait]
impl library_api::Library for ApiServer {
    async fn list_library_roots(&self, method: &Method, host: &Host, cookies: &CookieJar) -> std::result::Result<library_api::ListLibraryRootsResponse, ()> {
        let roots = self
            .state
            .library()
            .list()
            .iter()
            .map(|root| models::LibraryRoot::new(root.id.clone(), root.name.clone()))
            .collect();
        Ok(library_api::ListLibraryRootsResponse::Status200_LibraryRoots(models::LibraryRootList::new(roots)))
    }

    async fn list_library_entries(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::ListLibraryEntriesPathParams,
        query_params: &models::ListLibraryEntriesQueryParams,
    ) -> std::result::Result<library_api::ListLibraryEntriesResponse, ()> {
        match self.state.library().entries(&path_params.root_id, query_params.path.as_deref().unwrap_or_default()).await {
            Ok(entries) => {
                let entries = entries
                    .into_iter()
                    .map(|entry| {
                        let kind = match entry.kind {
                            crate::library::EntryKind::File => "file",
                            crate::library::EntryKind::Directory => "directory",
                        };
                        let mut model = models::LibraryEntry::new(entry.name, kind.to_string());
                        model.size = Some(entry.size);
                        model
                    })
                    .collect();
                Ok(library_api::ListLibraryEntriesResponse::Status200_EntriesOfTheDirectory(models::LibraryEntryList::new(
                    entries,
                )))
            }
            Err(error) => Ok(ApiError::from(error).into()),
        }
    }
}

#[allow(unused_variables)]
#[async_trait]
impl subscriptions_api::Subscriptions for ApiServer {
    async fn create_subscription(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        body: &models::CreateSubscriptionRequest,
    ) -> std::result::Result<subscriptions_api::CreateSubscriptionResponse, ()> {
        let result: std::result::Result<models::Subscription, ApiError> = async {
            FileName::validate(&body.file_name)?;
            let root_hash = OmniHash::from_str(&body.root_hash).map_err(|_| ApiError::invalid_request("the root hash is invalid"))?;
            let output_path = self.state.conf.storage.download_dir.join(&body.file_name);
            let output_path = output_path.to_str().ok_or_else(|| ApiError::invalid_request("the file name is not valid UTF-8"))?;
            let id = self.state.service().subscribe(&root_hash, output_path).await?;
            let subscription = self
                .state
                .service()
                .find_subscription(&id)
                .await?
                .ok_or_else(|| ApiError::internal(&"the subscription disappeared"))?;
            Ok(SubscriptionModel::from_engine(subscription))
        }
        .await;
        Ok(match result {
            Ok(subscription) => subscriptions_api::CreateSubscriptionResponse::Status202_TheSubscriptionIsFetching(subscription),
            Err(error) => error.into(),
        })
    }

    async fn list_subscriptions(&self, method: &Method, host: &Host, cookies: &CookieJar) -> std::result::Result<subscriptions_api::ListSubscriptionsResponse, ()> {
        match self.state.service().list_subscriptions().await {
            Ok(subscriptions) => Ok(subscriptions_api::ListSubscriptionsResponse::Status200_KnownSubscriptions(models::SubscriptionList::new(
                subscriptions.into_iter().map(SubscriptionModel::from_engine).collect(),
            ))),
            Err(error) => Ok(ApiError::from(error).into()),
        }
    }

    async fn get_subscription(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::GetSubscriptionPathParams,
    ) -> std::result::Result<subscriptions_api::GetSubscriptionResponse, ()> {
        match self.state.service().find_subscription(&path_params.subscription_id).await {
            Ok(Some(subscription)) => Ok(subscriptions_api::GetSubscriptionResponse::Status200_SubscriptionState(SubscriptionModel::from_engine(
                subscription,
            ))),
            Ok(None) => Ok(ApiError::not_found("no such subscription").into()),
            Err(error) => Ok(ApiError::from(error).into()),
        }
    }

    async fn cancel_subscription(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::CancelSubscriptionPathParams,
    ) -> std::result::Result<subscriptions_api::CancelSubscriptionResponse, ()> {
        let id = &path_params.subscription_id;
        let result = async {
            let Some(current) = self.state.service().find_subscription(id).await? else {
                return Err(ApiError::not_found("no such subscription"));
            };
            if matches!(current.status, SubscribedFileStatus::Finalizing) {
                return Err(ApiError::conflict("the subscription is finalizing"));
            }
            self.state.service().cancel_subscription(id).await?;
            let updated = self.state.service().find_subscription(id).await?.unwrap_or(current);
            Ok(SubscriptionModel::from_engine(updated))
        }
        .await;
        Ok(match result {
            Ok(subscription) => subscriptions_api::CancelSubscriptionResponse::Status200_TheSubscriptionStateAfterTheRequest(subscription),
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
    ) -> std::result::Result<subscriptions_api::DeleteSubscriptionResponse, ()> {
        let id = &path_params.subscription_id;
        let result = async {
            let Some(current) = self.state.service().find_subscription(id).await? else {
                return Err(ApiError::not_found("no such subscription"));
            };
            if matches!(
                current.status,
                SubscribedFileStatus::Downloading | SubscribedFileStatus::Decoding | SubscribedFileStatus::Finalizing
            ) {
                return Err(ApiError::conflict("cancel the subscription first"));
            }
            if query_params.delete_output.unwrap_or(false) {
                let path = current.output_path();
                match tokio::fs::remove_file(&path).await {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            }
            self.state.service().remove_subscription(id).await?;
            Ok(())
        }
        .await;
        Ok(match result {
            Ok(()) => subscriptions_api::DeleteSubscriptionResponse::Status204_TheSubscriptionIsGone,
            Err(error) => error.into(),
        })
    }

    async fn get_subscription_content(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        header_params: &models::GetSubscriptionContentHeaderParams,
        path_params: &models::GetSubscriptionContentPathParams,
    ) -> std::result::Result<subscriptions_api::GetSubscriptionContentResponse, ()> {
        Err(())
    }
}

/// A name from a client, used as one path component under a directory the daemon chose.
struct FileName;

impl FileName {
    const MAX_BYTES: usize = 255;

    fn validate(name: &str) -> std::result::Result<(), ApiError> {
        let forbidden = name.contains(['/', '\\', ':', '\0']);
        let mut components = std::path::Path::new(name).components();
        let one_component = matches!(components.next(), Some(std::path::Component::Normal(_))) && components.next().is_none();
        if forbidden || !one_component || name.len() > Self::MAX_BYTES {
            return Err(ApiError::invalid_request("a file name must be one path component of at most 255 bytes"));
        }
        Ok(())
    }
}

struct ContentRange {
    start: i64,
    end: i64,
}

impl ContentRange {
    fn parse(value: &str) -> std::result::Result<Self, ApiError> {
        let value = value
            .strip_prefix("bytes ")
            .ok_or_else(|| ApiError::invalid_request("Content-Range must start with `bytes `"))?;
        let (range, _size) = value.split_once('/').ok_or_else(|| ApiError::invalid_request("Content-Range must carry the total size"))?;
        let (start, end) = range.split_once('-').ok_or_else(|| ApiError::invalid_request("Content-Range must carry a range"))?;
        let start = start.parse::<i64>().map_err(|_| ApiError::invalid_request("Content-Range start is invalid"))?;
        let end = end.parse::<i64>().map_err(|_| ApiError::invalid_request("Content-Range end is invalid"))?;
        if start < 0 || end < start {
            return Err(ApiError::invalid_request("Content-Range is invalid"));
        }
        Ok(Self { start, end })
    }
}

impl ApiServer {
    fn router(state: Arc<DaemonState>) -> axum::Router {
        let generated = omnius_axus_interface::server::new(Arc::new(Self { state: state.clone() })).fallback(Self::no_route);
        let api = axum::Router::new()
            .route("/api/v1/setup/status", axum::routing::get(Self::setup_status))
            .route("/api/v1/setup", axum::routing::post(Self::setup))
            .route("/api/v1/auth/session", axum::routing::post(Self::login).delete(Self::logout))
            .route(
                &format!("{CONTENT_PATH}/publications/{{publicationId}}/content"),
                axum::routing::put(Self::put_publication_content).fallback_service(generated.clone()),
            )
            .route(
                &format!("{CONTENT_PATH}/subscriptions/{{subscriptionId}}/content"),
                axum::routing::get(Self::get_subscription_content).fallback_service(generated.clone()),
            )
            .with_state(state.clone())
            .fallback_service(generated)
            .layer(axum::middleware::from_fn_with_state(state.clone(), Self::auth_middleware));
        match &state.conf.web.dir {
            Some(dir) => Self::static_router(dir.clone(), api),
            None => api,
        }
    }

    pub async fn serve(state: Arc<DaemonState>, token: CancellationToken) -> Result<()> {
        if !state.conf.api.listens_only_on_loopback() {
            warn!(addr = state.conf.api.listen_addr, "the API has no TLS; limit its reach outside the daemon");
        }
        let app = Self::router(state.clone());
        let listener = tokio::net::TcpListener::bind(state.conf.api.listen_addr.clone()).await?;
        info!(addr = state.conf.api.listen_addr, "listen start");
        axum::serve(listener, app).with_graceful_shutdown(token.cancelled_owned()).await?;
        Ok(())
    }

    async fn auth_middleware(State(state): State<Arc<DaemonState>>, cookies: CookieJar, request: Request, next: axum::middleware::Next) -> Response {
        let path = request.uri().path();
        let method = request.method().clone();
        let public = path == "/api/v1/health" || path == "/api/v1/setup" || path == "/api/v1/setup/status" || (method == Method::POST && path == "/api/v1/auth/session");
        if public {
            return next.run(request).await;
        }

        let bearer = request
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .map(str::to_string);
        let cookie = cookies
            .get("__Host-axus_session")
            .or_else(|| cookies.get("axus_session"))
            .map(|cookie| cookie.value().to_string());
        let Some(token) = bearer.clone().or_else(|| cookie.clone()) else {
            return Self::unauthorized();
        };
        match state.owner().authenticate(&token).await {
            Ok(true) => {}
            Ok(false) => return Self::unauthorized(),
            Err(error) => return ApiError::internal(&error).into_response(),
        }
        if bearer.is_none() && !matches!(method, Method::GET | Method::HEAD) {
            let host = request.headers().get(header::HOST).and_then(|value| value.to_str().ok()).unwrap_or_default();
            let origin = request.headers().get(header::ORIGIN).and_then(|value| value.to_str().ok());
            if origin != Some(format!("http://{host}").as_str()) {
                return ApiError::new(StatusCode::FORBIDDEN, models::ErrorKind::Forbidden, "the origin of the request is not the daemon").into_response();
            }
        }
        next.run(request).await
    }

    async fn setup_status(State(state): State<Arc<DaemonState>>) -> Response {
        match state.owner().initialized().await {
            Ok(initialized) => Json(json!({ "initialized": initialized })).into_response(),
            Err(error) => ApiError::internal(&error).into_response(),
        }
    }

    async fn setup(State(state): State<Arc<DaemonState>>, Json(body): Json<LoginBody>) -> Response {
        if body.name.is_empty() || body.password.len() < 8 {
            return ApiError::invalid_request("the owner name must not be empty and the password must have at least 8 bytes").into_response();
        }
        match state.owner().setup(&body.name, &body.password).await {
            Ok(true) => StatusCode::NO_CONTENT.into_response(),
            Ok(false) => ApiError::conflict("the owner is already registered").into_response(),
            Err(error) => ApiError::internal(&error).into_response(),
        }
    }

    async fn login(State(state): State<Arc<DaemonState>>, headers: HeaderMap, Json(body): Json<LoginBody>) -> Response {
        let token = match state.owner().login(&body.name, &body.password).await {
            Ok(Some(token)) => token,
            Ok(None) => return ApiError::new(StatusCode::UNAUTHORIZED, models::ErrorKind::Unauthorized, "the name or password is wrong").into_response(),
            Err(error) => return ApiError::internal(&error).into_response(),
        };
        if headers.get("x-axus-transport").and_then(|value| value.to_str().ok()) == Some("bearer") {
            return Json(json!({ "token": token })).into_response();
        }
        let cookie = if state.conf.web.insecure_cookie {
            format!("axus_session={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age=2592000")
        } else {
            format!("__Host-axus_session={token}; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age=2592000")
        };
        (StatusCode::NO_CONTENT, [(header::SET_COOKIE, cookie)]).into_response()
    }

    async fn logout(State(state): State<Arc<DaemonState>>, cookies: CookieJar, headers: HeaderMap) -> Response {
        let bearer = headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .map(str::to_string);
        let cookie = cookies
            .get("__Host-axus_session")
            .or_else(|| cookies.get("axus_session"))
            .map(|cookie| cookie.value().to_string());
        if let Some(token) = bearer.or(cookie)
            && let Err(error) = state.owner().logout(&token).await
        {
            return ApiError::internal(&error).into_response();
        }
        let cookie = if state.conf.web.insecure_cookie {
            "axus_session=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0"
        } else {
            "__Host-axus_session=; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age=0"
        };
        (StatusCode::NO_CONTENT, [(header::SET_COOKIE, cookie)]).into_response()
    }

    fn unauthorized() -> Response {
        ApiError::new(StatusCode::UNAUTHORIZED, models::ErrorKind::Unauthorized, "a session is required").into_response()
    }

    async fn no_route() -> ApiError {
        ApiError::not_found("no such API path")
    }

    async fn put_publication_content(State(state): State<Arc<DaemonState>>, Path(id): Path<String>, headers: HeaderMap, request: Request) -> Response {
        match Self::put_publication_content_inner(&state, &id, &headers, request).await {
            Ok(publication) => (StatusCode::OK, Json(PublicationModel::from_engine(publication))).into_response(),
            Err(error) => error.into_response(),
        }
    }

    async fn put_publication_content_inner(state: &DaemonState, id: &str, headers: &HeaderMap, request: Request) -> std::result::Result<Publication, ApiError> {
        let range = headers
            .get("content-range")
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| ApiError::invalid_request("Content-Range is required"))?;
        let range = ContentRange::parse(range)?;
        let Some(current) = state.service().get_publication(id).await? else {
            return Err(ApiError::not_found("no such publication"));
        };
        if current.status != PublicationStatus::Receiving {
            return Err(ApiError::conflict("the publication is not receiving content"));
        }

        let mut offset = range.start;
        let mut stream = request.into_body().into_data_stream();
        let mut total = 0_i64;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| ApiError::invalid_request(format!("the body could not be read: {error}")))?;
            if chunk.len() > MAX_CHUNK_BYTES {
                return Err(ApiError::payload_too_large("a chunk is too large"));
            }
            total += i64::try_from(chunk.len()).unwrap_or(i64::MAX);
            if total > range.end - range.start + 1 {
                return Err(ApiError::payload_too_large("the body is longer than Content-Range"));
            }
            let result = state.service().append_publication_content(id, offset, chunk).await?;
            match result {
                AppendContentResult::Accepted { received_bytes } => offset = received_bytes,
                AppendContentResult::OffsetMismatch { received_bytes } => {
                    return Err(ApiError::conflict("the offset does not match the received offset").with_received_bytes(received_bytes));
                }
                AppendContentResult::TooLarge { size } => {
                    return Err(ApiError::payload_too_large(format!("the content is over the declared size {size}")));
                }
                AppendContentResult::NotReceiving => {
                    return Err(ApiError::conflict("the publication is not receiving content"));
                }
            }
        }
        if total != range.end - range.start + 1 {
            return Err(ApiError::invalid_request("the body is shorter than Content-Range"));
        }
        let Some(publication) = state.service().get_publication(id).await? else {
            return Err(ApiError::not_found("no such publication"));
        };
        Ok(publication)
    }

    async fn get_subscription_content(State(state): State<Arc<DaemonState>>, Path(id): Path<String>, headers: HeaderMap) -> Response {
        match Self::get_subscription_content_inner(&state, &id, &headers).await {
            Ok(response) => response,
            Err(error) => error.into_response(),
        }
    }

    async fn get_subscription_content_inner(state: &DaemonState, id: &str, headers: &HeaderMap) -> std::result::Result<Response, ApiError> {
        let Some(subscription) = state.service().find_subscription(id).await? else {
            return Err(ApiError::not_found("no such subscription"));
        };
        if subscription.status != SubscribedFileStatus::Completed {
            return Err(ApiError::conflict("the subscription is not completed"));
        }
        let path = subscription.output_path();
        let metadata = tokio::fs::metadata(&path).await?;
        let size = metadata.len();
        let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let modified = modified.duration_since(SystemTime::UNIX_EPOCH).map(|value| value.as_secs()).unwrap_or(0);
        let etag = format!("\"{size}-{modified}\"");

        let mut builder = Response::builder()
            .header(header::CONTENT_TYPE, "application/octet-stream")
            .header(header::CONTENT_DISPOSITION, format!("attachment; filename=\"{}\"", subscription.output_name))
            .header("x-content-type-options", "nosniff")
            .header(header::ACCEPT_RANGES, "bytes")
            .header(header::ETAG, etag.clone());

        if headers.get(header::IF_NONE_MATCH).and_then(|value| value.to_str().ok()).is_some_and(|value| value == etag) {
            return Ok(builder.status(StatusCode::NOT_MODIFIED).body(Body::empty()).unwrap());
        }

        let mut file = tokio::fs::File::open(&path).await?;
        let (status, start, length) = match headers.get(header::RANGE).and_then(|value| value.to_str().ok()) {
            Some(value) => {
                let (start, end) = Self::parse_range(value, size)?;
                builder = builder.header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{size}"));
                (StatusCode::PARTIAL_CONTENT, start, end - start + 1)
            }
            None => (StatusCode::OK, 0, size),
        };
        builder = builder.header(header::CONTENT_LENGTH, length.to_string());
        if start > 0 {
            file.seek(std::io::SeekFrom::Start(start)).await?;
        }
        let stream = ReaderStream::new(file.take(length));
        Ok(builder.status(status).body(Body::from_stream(stream)).unwrap())
    }

    fn parse_range(value: &str, size: u64) -> std::result::Result<(u64, u64), ApiError> {
        let value = value
            .strip_prefix("bytes=")
            .ok_or_else(|| ApiError::range_not_satisfiable("Range must start with `bytes=`"))?;
        let (start, end) = value.split_once('-').ok_or_else(|| ApiError::range_not_satisfiable("Range must carry a range"))?;
        if start.is_empty() {
            let suffix = end.parse::<u64>().map_err(|_| ApiError::range_not_satisfiable("Range is invalid"))?;
            if suffix == 0 || size == 0 {
                return Err(ApiError::range_not_satisfiable("Range is outside the output"));
            }
            let start = size.saturating_sub(suffix);
            return Ok((start, size - 1));
        }
        let start = start.parse::<u64>().map_err(|_| ApiError::range_not_satisfiable("Range is invalid"))?;
        let end = if end.is_empty() {
            size.saturating_sub(1)
        } else {
            end.parse::<u64>().map_err(|_| ApiError::range_not_satisfiable("Range is invalid"))?
        };
        if start >= size || end < start {
            return Err(ApiError::range_not_satisfiable("Range is outside the output"));
        }
        Ok((start, end.min(size - 1)))
    }

    fn static_router(dir: PathBuf, api: axum::Router) -> axum::Router {
        let api = Arc::new(api);
        let files = Arc::new(dir);
        axum::Router::new().fallback(move |request: Request| {
            let api = api.clone();
            let files = files.clone();
            async move {
                if request.uri().path().starts_with("/api") {
                    return api
                        .as_ref()
                        .clone()
                        .oneshot(request)
                        .await
                        .unwrap_or_else(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response());
                }
                let relative = request.uri().path().trim_start_matches('/');
                let mut resolved = files.as_ref().clone();
                for component in std::path::Path::new(relative).components() {
                    if let std::path::Component::Normal(part) = component {
                        resolved.push(part);
                    }
                }
                let target = if resolved.is_file() { resolved } else { files.join("index.html") };
                match tokio::fs::read(&target).await {
                    Ok(bytes) => ([(header::CONTENT_TYPE, Self::content_type(&target))], bytes).into_response(),
                    Err(error) => (StatusCode::NOT_FOUND, error.to_string()).into_response(),
                }
            }
        })
    }

    fn content_type(path: &std::path::Path) -> &'static str {
        match path.extension().and_then(|extension| extension.to_str()).unwrap_or_default() {
            "html" => "text/html; charset=utf-8",
            "js" => "text/javascript; charset=utf-8",
            "mjs" => "text/javascript; charset=utf-8",
            "css" => "text/css; charset=utf-8",
            "json" => "application/json",
            "svg" => "image/svg+xml",
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "ico" => "image/x-icon",
            "woff" => "font/woff",
            "woff2" => "font/woff2",
            "map" => "application/json",
            _ => "application/octet-stream",
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{path::Path, time::Duration};

    use axum::body::{Body, to_bytes};
    use http::{Request, StatusCode, header::CONTENT_TYPE};
    use omnius_axus_engine::{
        model::NodeProfile,
        service::{AxusServiceOption, NodeFinderIntervals},
    };
    use serde_json::{Value, json};
    use tempfile::TempDir;
    use testresult::TestResult;

    use crate::config::{ApiConfig, CoreConfig, DaemonConfig, LoggingConfig, P2pConfig, StorageConfig, WebConfig};

    use super::*;

    const WATCHDOG: Duration = Duration::from_secs(120);

    struct TestDaemon {
        _dir: TempDir,
        state: Arc<DaemonState>,
        router: axum::Router,
        cookie: Option<String>,
    }

    impl TestDaemon {
        async fn start() -> TestResult<Self> {
            Self::start_with(vec![], None, vec![]).await
        }

        async fn start_with(bootstrap_nodes: Vec<NodeProfile>, web_dir: Option<&Path>, library_roots: Vec<std::path::PathBuf>) -> TestResult<Self> {
            let dir = tempfile::tempdir()?;
            let port = std::net::TcpListener::bind("127.0.0.1:0")?.local_addr()?.port();
            let conf = DaemonConfig {
                core: CoreConfig {
                    state_dir: dir.path().join("state"),
                },
                api: ApiConfig {
                    listen_addr: "127.0.0.1:0".to_string(),
                    max_upload_size: 0,
                },
                p2p: P2pConfig {
                    listen_addr: format!("127.0.0.1:{port}"),
                    advertise_addrs: vec![],
                    use_upnp: false,
                    bootstrap_nodes: bootstrap_nodes.iter().map(|profile| profile.to_uri()).collect::<std::result::Result<_, _>>()?,
                },
                logging: LoggingConfig {
                    level: "info".to_string(),
                    json: false,
                },
                storage: StorageConfig {
                    download_dir: dir.path().join("downloads"),
                    library_roots,
                },
                web: WebConfig {
                    dir: web_dir.map(Path::to_path_buf),
                    insecure_cookie: true,
                },
            };
            let option = AxusServiceOption {
                node_finder_intervals: NodeFinderIntervals {
                    connect: Duration::from_millis(100),
                    compute: Duration::from_millis(100),
                    communicate: Duration::from_millis(500),
                    lookup: Duration::from_millis(100),
                },
                bootstrap_node_profiles: bootstrap_nodes,
                ..Default::default()
            };
            let state = Arc::new(DaemonState::for_test(conf, option).await?);
            let router = ApiServer::router(state.clone());
            let mut daemon = Self {
                _dir: dir,
                state,
                router,
                cookie: None,
            };
            daemon.setup_and_login().await?;
            Ok(daemon)
        }

        async fn stop(self) {
            self.state.shutdown().await;
        }

        async fn stop_for_restart(self) -> (TempDir, DaemonConfig, Option<String>) {
            self.state.shutdown().await;
            let Self { _dir, state, router, cookie } = self;
            let conf = state.conf.clone();
            // Dropping the router and the state releases the P2P port and the stores, as a process exit does.
            drop(router);
            drop(state);
            (_dir, conf, cookie)
        }

        async fn restart(dir: TempDir, conf: DaemonConfig, cookie: Option<String>) -> TestResult<Self> {
            let state = Arc::new(DaemonState::for_test(conf, AxusServiceOption::default()).await?);
            let router = ApiServer::router(state.clone());
            Ok(Self { _dir: dir, state, router, cookie })
        }

        async fn start_bare() -> TestResult<Self> {
            let dir = tempfile::tempdir()?;
            let port = std::net::TcpListener::bind("127.0.0.1:0")?.local_addr()?.port();
            let conf = DaemonConfig {
                core: CoreConfig {
                    state_dir: dir.path().join("state"),
                },
                api: ApiConfig {
                    listen_addr: "127.0.0.1:0".to_string(),
                    max_upload_size: 0,
                },
                p2p: P2pConfig {
                    listen_addr: format!("127.0.0.1:{port}"),
                    advertise_addrs: vec![],
                    use_upnp: false,
                    bootstrap_nodes: vec![],
                },
                logging: LoggingConfig {
                    level: "info".to_string(),
                    json: false,
                },
                storage: StorageConfig {
                    download_dir: dir.path().join("downloads"),
                    library_roots: vec![],
                },
                web: WebConfig { dir: None, insecure_cookie: true },
            };
            let state = Arc::new(DaemonState::for_test(conf, AxusServiceOption::default()).await?);
            let router = ApiServer::router(state.clone());
            Ok(Self {
                _dir: dir,
                state,
                router,
                cookie: None,
            })
        }

        async fn setup_and_login(&mut self) -> TestResult<()> {
            let (status, _) = self.request_raw_json("POST", "/api/v1/setup", &json!({"name": "owner", "password": "password123"})).await?;
            assert_eq!(status, StatusCode::NO_CONTENT);
            let request = Request::builder()
                .method("POST")
                .uri("/api/v1/auth/session")
                .header("host", "localhost")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&json!({"name": "owner", "password": "password123"}))?))?;
            let response = self.send_raw(request).await?;
            assert_eq!(response.status(), StatusCode::NO_CONTENT);
            let set_cookie = response.headers().get(http::header::SET_COOKIE).unwrap().to_str()?.to_string();
            self.cookie = Some(set_cookie.split(';').next().unwrap().to_string());
            Ok(())
        }

        async fn send(&self, mut request: Request<Body>) -> TestResult<axum::response::Response> {
            if let Some(cookie) = &self.cookie {
                request.headers_mut().insert(http::header::COOKIE, http::HeaderValue::from_str(cookie)?);
                request.headers_mut().insert(http::header::ORIGIN, http::HeaderValue::from_static("http://localhost"));
            }
            self.send_raw(request).await
        }

        async fn send_raw(&self, request: Request<Body>) -> TestResult<axum::response::Response> {
            Ok(self.router.clone().oneshot(request).await?)
        }

        async fn request_raw_json(&self, method: &str, uri: &str, body: &Value) -> TestResult<(StatusCode, Value)> {
            let request = Request::builder()
                .method(method)
                .uri(uri)
                .header("host", "localhost")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(body)?))?;
            let response = self.send_raw(request).await?;
            let status = response.status();
            let bytes = to_bytes(response.into_body(), usize::MAX).await?;
            Ok((status, serde_json::from_slice(&bytes).unwrap_or(Value::Null)))
        }

        async fn request(&self, method: &str, uri: &str, body: Body) -> TestResult<(StatusCode, Bytes)> {
            let request = Request::builder().method(method).uri(uri).header("host", "localhost").body(body)?;
            let response = self.send(request).await?;
            let status = response.status();
            Ok((status, to_bytes(response.into_body(), usize::MAX).await?))
        }

        async fn request_json(&self, method: &str, uri: &str, body: &Value) -> TestResult<(StatusCode, Value)> {
            let request = Request::builder()
                .method(method)
                .uri(uri)
                .header("host", "localhost")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(body)?))?;
            let response = self.send(request).await?;
            let status = response.status();
            let bytes = to_bytes(response.into_body(), usize::MAX).await?;
            Ok((status, serde_json::from_slice(&bytes).unwrap_or(Value::Null)))
        }

        async fn get_json(&self, uri: &str) -> TestResult<(StatusCode, Value)> {
            let (status, bytes) = self.request("GET", uri, Body::empty()).await?;
            Ok((status, serde_json::from_slice(&bytes).unwrap_or(Value::Null)))
        }

        async fn create_publication(&self, name: &str, size: usize) -> TestResult<Value> {
            let (status, value) = self.request_json("POST", "/api/v1/publications", &json!({"fileName": name, "size": size})).await?;
            assert_eq!(status, StatusCode::ACCEPTED, "{value}");
            Ok(value)
        }

        async fn put_chunk(&self, id: &str, offset: usize, bytes: &[u8], total: usize) -> TestResult<axum::response::Response> {
            let end = offset + bytes.len() - 1;
            let request = Request::builder()
                .method("PUT")
                .uri(format!("/api/v1/publications/{id}/content"))
                .header("host", "localhost")
                .header("content-range", format!("bytes {offset}-{end}/{total}"))
                .body(Body::from(bytes.to_vec()))?;
            self.send(request).await
        }

        async fn wait_status(&self, uri: &str, status: &str) -> TestResult<Value> {
            tokio::time::timeout(WATCHDOG, async {
                loop {
                    let (_, value) = self.get_json(uri).await?;
                    if value["status"] == status {
                        return TestResult::Ok(value);
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await?
        }
    }

    #[tokio::test]
    async fn health_and_an_unknown_api_path_answer() -> TestResult {
        let daemon = TestDaemon::start().await?;
        let (status, value) = daemon.get_json("/api/v1/health").await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value["result"], true);
        let (status, value) = daemon.get_json("/api/v1/unknown").await?;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(value["kind"], "NotFound");
        daemon.stop().await;
        Ok(())
    }

    #[tokio::test]
    async fn an_offset_mismatch_reports_the_received_bytes() -> TestResult {
        let daemon = TestDaemon::start().await?;
        let publication = daemon.create_publication("a.bin", 4).await?;
        let id = publication["id"].as_str().unwrap();
        assert_eq!(publication["status"], "Receiving");
        assert_eq!(publication["receivedBytes"], 0);

        let accepted = daemon.put_chunk(id, 0, b"ab", 4).await?;
        assert_eq!(accepted.status(), StatusCode::OK);
        let value: Value = serde_json::from_slice(&to_bytes(accepted.into_body(), usize::MAX).await?)?;
        assert_eq!(value["receivedBytes"], 2);
        assert_eq!(value["status"], "Receiving");

        let rejected = daemon.put_chunk(id, 3, b"d", 4).await?;
        assert_eq!(rejected.status(), StatusCode::CONFLICT);
        let value: Value = serde_json::from_slice(&to_bytes(rejected.into_body(), usize::MAX).await?)?;
        assert_eq!(value["receivedBytes"], 2);

        daemon.stop().await;
        Ok(())
    }

    #[tokio::test]
    async fn a_canceled_publication_stays_until_it_is_deleted() -> TestResult {
        let daemon = TestDaemon::start().await?;
        let publication = daemon.create_publication("b.bin", 4).await?;
        let id = publication["id"].as_str().unwrap();
        let (status, value) = daemon.request_json("POST", &format!("/api/v1/publications/{id}/cancel"), &json!({})).await?;
        assert_eq!(status, StatusCode::OK, "{value}");
        assert_eq!(value["status"], "Canceled");

        let (status, value) = daemon.get_json(&format!("/api/v1/publications/{id}")).await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value["status"], "Canceled");

        let (status, _) = daemon.request("DELETE", &format!("/api/v1/publications/{id}"), Body::empty()).await?;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (status, _) = daemon.get_json(&format!("/api/v1/publications/{id}")).await?;
        assert_eq!(status, StatusCode::NOT_FOUND);
        daemon.stop().await;
        Ok(())
    }

    #[tokio::test]
    async fn two_daemons_transfer_a_file_through_the_api() -> TestResult {
        let publisher = TestDaemon::start().await?;
        let subscriber = TestDaemon::start_with(vec![publisher.state.service().my_node_profile().clone()], None, vec![]).await?;
        let content: Vec<u8> = (0..3 * 1024 * 1024 + 17).map(|i: u32| (i % 251) as u8).collect();

        let publication = publisher.create_publication("movie.bin", content.len()).await?;
        let id = publication["id"].as_str().unwrap();
        let chunk = 1024 * 1024;
        for offset in (0..content.len()).step_by(chunk) {
            let end = (offset + chunk).min(content.len());
            let response = publisher.put_chunk(id, offset, &content[offset..end], content.len()).await?;
            assert_eq!(response.status(), StatusCode::OK);
        }
        let uri = format!("/api/v1/publications/{id}");
        let committed = publisher.wait_status(&uri, "Committed").await?;
        let root_hash = committed["rootHash"].as_str().unwrap();

        let (status, subscription) = subscriber
            .request_json("POST", "/api/v1/subscriptions", &json!({"rootHash": root_hash, "fileName": "movie.bin"}))
            .await?;
        assert_eq!(status, StatusCode::ACCEPTED, "{subscription}");
        let id = subscription["id"].as_str().unwrap();
        let uri = format!("/api/v1/subscriptions/{id}");
        subscriber.wait_status(&uri, "Completed").await?;

        let (status, downloaded) = subscriber.request("GET", &format!("{uri}/content"), Body::empty()).await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(downloaded.as_ref(), content.as_slice());

        let request = Request::builder()
            .uri(format!("{uri}/content"))
            .header("host", "localhost")
            .header("range", "bytes=10-19")
            .body(Body::empty())?;
        let response = subscriber.send(request).await?;
        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(response.headers()["content-type"], "application/octet-stream");
        assert_eq!(response.headers()["x-content-type-options"], "nosniff");
        assert!(response.headers()["content-disposition"].to_str()?.starts_with("attachment"));
        let range = to_bytes(response.into_body(), usize::MAX).await?;
        assert_eq!(range.as_ref(), &content[10..20]);

        subscriber.stop().await;
        publisher.stop().await;
        Ok(())
    }

    #[tokio::test]
    async fn static_serving_falls_back_to_index() -> TestResult {
        let web = tempfile::tempdir()?;
        std::fs::write(web.path().join("index.html"), b"<html>axus</html>")?;
        let daemon = TestDaemon::start_with(vec![], Some(web.path()), vec![]).await?;
        let (status, body) = daemon.request("GET", "/publications/1", Body::empty()).await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.as_ref(), b"<html>axus</html>");
        let (status, value) = daemon.get_json("/api/v1/unknown").await?;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(value["kind"], "NotFound");
        daemon.stop().await;
        Ok(())
    }

    #[tokio::test]
    async fn the_owner_registers_once_and_login_is_required() -> TestResult {
        let daemon = TestDaemon::start_bare().await?;
        let (status, value) = daemon.request_raw_json("GET", "/api/v1/setup/status", &json!({})).await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value["initialized"], false);
        let (status, _) = daemon.request_raw_json("GET", "/api/v1/publications", &json!({})).await?;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        let (status, _) = daemon
            .request_raw_json("POST", "/api/v1/setup", &json!({"name": "owner", "password": "password123"}))
            .await?;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (status, _) = daemon
            .request_raw_json("POST", "/api/v1/setup", &json!({"name": "owner", "password": "password123"}))
            .await?;
        assert_eq!(status, StatusCode::CONFLICT);
        let (status, _) = daemon
            .request_raw_json("POST", "/api/v1/auth/session", &json!({"name": "owner", "password": "wrong"}))
            .await?;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        let response = daemon
            .send_raw(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/session")
                    .header("host", "localhost")
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(serde_json::to_vec(&json!({"name": "owner", "password": "password123"}))?))?,
            )
            .await?;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let cookie = response.headers().get(http::header::SET_COOKIE).unwrap().to_str()?.split(';').next().unwrap().to_string();
        let with_cookie = |uri: &str| {
            Request::builder()
                .method("GET")
                .uri(uri)
                .header("host", "localhost")
                .header(http::header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap()
        };
        assert_eq!(daemon.send_raw(with_cookie("/api/v1/publications")).await?.status(), StatusCode::OK);
        let logout = Request::builder()
            .method("DELETE")
            .uri("/api/v1/auth/session")
            .header("host", "localhost")
            .header(http::header::COOKIE, &cookie)
            .header(http::header::ORIGIN, "http://localhost")
            .body(Body::empty())?;
        assert_eq!(daemon.send_raw(logout).await?.status(), StatusCode::NO_CONTENT);
        assert_eq!(daemon.send_raw(with_cookie("/api/v1/publications")).await?.status(), StatusCode::UNAUTHORIZED);
        daemon.stop().await;
        Ok(())
    }

    #[tokio::test]
    async fn a_cookie_request_from_another_origin_is_forbidden() -> TestResult {
        let daemon = TestDaemon::start().await?;
        let cookie = daemon.cookie.clone().unwrap();
        let body = serde_json::to_vec(&json!({"fileName": "a.bin", "size": 1}))?;
        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/publications")
            .header("host", "localhost")
            .header(http::header::COOKIE, &cookie)
            .header(http::header::ORIGIN, "http://evil.example")
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(body.clone()))?;
        assert_eq!(daemon.send_raw(request).await?.status(), StatusCode::FORBIDDEN);
        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/publications")
            .header("host", "localhost")
            .header(http::header::COOKIE, &cookie)
            .header(http::header::ORIGIN, "http://localhost")
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(body))?;
        assert_eq!(daemon.send_raw(request).await?.status(), StatusCode::ACCEPTED);
        daemon.stop().await;
        Ok(())
    }

    #[tokio::test]
    async fn a_bearer_session_needs_no_cookie() -> TestResult {
        let daemon = TestDaemon::start().await?;
        let response = daemon
            .send_raw(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/session")
                    .header("host", "localhost")
                    .header(CONTENT_TYPE, "application/json")
                    .header("x-axus-transport", "bearer")
                    .body(Body::from(serde_json::to_vec(&json!({"name": "owner", "password": "password123"}))?))?,
            )
            .await?;
        assert_eq!(response.status(), StatusCode::OK);
        let value: Value = serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await?)?;
        let token = value["token"].as_str().unwrap();
        let request = Request::builder()
            .method("GET")
            .uri("/api/v1/publications")
            .header("host", "localhost")
            .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())?;
        assert_eq!(daemon.send_raw(request).await?.status(), StatusCode::OK);
        daemon.stop().await;
        Ok(())
    }

    #[tokio::test]
    async fn an_upload_and_the_session_resume_after_a_restart() -> TestResult {
        let daemon = TestDaemon::start().await?;
        let content: Vec<u8> = (0..2 * 1024 * 1024 + 3).map(|i: u32| (i % 251) as u8).collect();
        let publication = daemon.create_publication("resume.bin", content.len()).await?;
        let id = publication["id"].as_str().unwrap().to_string();
        let split = content.len() / 2;
        assert_eq!(daemon.put_chunk(&id, 0, &content[..split], content.len()).await?.status(), StatusCode::OK);

        let (dir, conf, cookie) = daemon.stop_for_restart().await;
        let daemon = TestDaemon::restart(dir, conf, cookie).await?;

        let (status, value) = daemon.get_json(&format!("/api/v1/publications/{id}")).await?;
        assert_eq!(status, StatusCode::OK, "the session must survive the restart: {value}");
        assert_eq!(value["receivedBytes"], split);
        assert_eq!(value["status"], "Receiving");

        assert_eq!(daemon.put_chunk(&id, split, &content[split..], content.len()).await?.status(), StatusCode::OK);
        let committed = daemon.wait_status(&format!("/api/v1/publications/{id}"), "Committed").await?;
        assert!(committed["rootHash"].is_string());

        daemon.stop().await;
        Ok(())
    }

    #[tokio::test]
    async fn a_library_file_publishes_without_an_upload() -> TestResult {
        let library = tempfile::tempdir()?;
        std::fs::write(library.path().join("movie.bin"), b"a library file")?;
        let daemon = TestDaemon::start_with(vec![], None, vec![library.path().to_path_buf()]).await?;

        let (status, roots) = daemon.get_json("/api/v1/library/roots").await?;
        assert_eq!(status, StatusCode::OK);
        let root_id = roots["roots"][0]["id"].as_str().unwrap();
        let (status, entries) = daemon.get_json(&format!("/api/v1/library/roots/{root_id}/entries")).await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(entries["entries"][0]["name"], "movie.bin");
        assert_eq!(entries["entries"][0]["kind"], "file");

        let (status, _) = daemon
            .request_json(
                "POST",
                "/api/v1/publications",
                &json!({"source": {"kind": "library", "rootId": root_id, "path": "movie.bin"}}),
            )
            .await?;
        assert_eq!(status, StatusCode::ACCEPTED);
        let (status, _) = daemon
            .request_json(
                "POST",
                "/api/v1/publications",
                &json!({"source": {"kind": "library", "rootId": root_id, "path": "../movie.bin"}}),
            )
            .await?;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let (_, list) = daemon.get_json("/api/v1/publications").await?;
        let id = list["publications"][0]["id"].as_str().unwrap();
        let committed = daemon.wait_status(&format!("/api/v1/publications/{id}"), "Committed").await?;
        assert!(committed["rootHash"].is_string());
        daemon.stop().await;
        Ok(())
    }
}
