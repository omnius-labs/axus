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
use tokio::io::{AsyncReadExt as _, AsyncSeekExt as _};
use tokio_util::{io::ReaderStream, sync::CancellationToken};
use tower::ServiceExt as _;

use omnius_axus_engine::{
    ErrorKind as EngineErrorKind,
    model::{AppendContentResult, Publication, PublicationRemoveResult, PublicationStatus, SubscribedFile, SubscribedFileStatus},
};
use omnius_axus_interface::{
    apis::{
        health::{GetHealthResponse, Health},
        publications as publications_api, subscriptions as subscriptions_api,
    },
    models,
};
use omnius_core_omnikit::generated::omni_hash::OmniHash;

use crate::{prelude::*, state::DaemonState};

const CONTENT_PATH: &str = "/api/v1";
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
        let result = async {
            FileName::validate(&body.file_name)?;
            if body.size < 0 {
                return Err(ApiError::invalid_request("the declared size must not be negative"));
            }
            if self.state.conf.api.max_upload_size > 0 && body.size > self.state.conf.api.max_upload_size {
                return Err(ApiError::payload_too_large("the declared size is over api.max_upload_size"));
            }
            let publication = self.state.service().create_publication(&self.state.uploads_dir(), &body.file_name, body.size).await?;
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
        let generated = omnius_axus_interface::server::new(Arc::new(Self { state: state.clone() }));
        let api = axum::Router::new()
            .route(
                &format!("{CONTENT_PATH}/publications/{{publicationId}}/content"),
                axum::routing::put(Self::put_publication_content).fallback_service(generated.clone()),
            )
            .route(
                &format!("{CONTENT_PATH}/subscriptions/{{subscriptionId}}/content"),
                axum::routing::get(Self::get_subscription_content).fallback_service(generated.clone()),
            )
            .with_state(state.clone())
            .fallback_service(generated);
        match &state.conf.web.dir {
            Some(dir) => Self::static_router(dir.clone(), api),
            None => api,
        }
    }

    pub async fn serve(state: Arc<DaemonState>, token: CancellationToken) -> Result<()> {
        let app = Self::router(state.clone());
        let listener = tokio::net::TcpListener::bind(state.conf.api.listen_addr.clone()).await?;
        info!(addr = state.conf.api.listen_addr, "listen start");
        axum::serve(listener, app).with_graceful_shutdown(token.cancelled_owned()).await?;
        Ok(())
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
