use async_trait::async_trait;
use axum::body::Body;
use axum_extra::extract::CookieJar;
use bytes::Bytes;
use headers::Host;
use http::Method;

use omnius_axus_interface::{
    apis::publishes::{
        CancelPublishResponse, CommitPublishResponse, CreatePublishResponse, GetPublishResponse, ListPublishesResponse, Publishes, RemovePublishResponse,
        WritePublishContentResponse,
    },
    models,
};

use crate::interface::ApiServer;

pub mod route;

#[allow(unused_variables)]
#[async_trait]
impl Publishes for ApiServer {
    async fn cancel_publish(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::CancelPublishPathParams,
    ) -> std::result::Result<CancelPublishResponse, ()> {
        Ok(match self.state.publish.cancel(&path_params.publish_id).await {
            Ok(()) => CancelPublishResponse::Status204_PublishCancelled,
            Err(error) => CancelPublishResponse::from(error),
        })
    }

    async fn commit_publish(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::CommitPublishPathParams,
    ) -> std::result::Result<CommitPublishResponse, ()> {
        Ok(match self.state.publish.commit(&path_params.publish_id).await {
            Ok(publish) => CommitPublishResponse::Status202_EncodingStarted(models::PublishResponse::new(publish)),
            Err(error) => CommitPublishResponse::from(error),
        })
    }

    async fn create_publish(&self, method: &Method, host: &Host, cookies: &CookieJar, body: &models::PublishCreateRequest) -> std::result::Result<CreatePublishResponse, ()> {
        Ok(match self.state.publish.create(body).await {
            Ok(publish) => CreatePublishResponse::Status201_UploadingPublishCreated(models::PublishResponse::new(publish)),
            Err(error) => CreatePublishResponse::from(error),
        })
    }

    async fn get_publish(&self, method: &Method, host: &Host, cookies: &CookieJar, path_params: &models::GetPublishPathParams) -> std::result::Result<GetPublishResponse, ()> {
        Ok(match self.state.publish.get(&path_params.publish_id).await {
            Ok(Some(publish)) => GetPublishResponse::Status200_PublishState(models::PublishResponse::new(publish)),
            Ok(None) => GetPublishResponse::from(crate::error::ApiError::not_found("publish not found")),
            Err(error) => GetPublishResponse::from(error),
        })
    }

    async fn list_publishes(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        query_params: &models::ListPublishesQueryParams,
    ) -> std::result::Result<ListPublishesResponse, ()> {
        Ok(match self.state.publish.list(query_params.limit, query_params.offset).await {
            Ok(publishes) => ListPublishesResponse::Status200_KnownPublishes(publishes),
            Err(error) => ListPublishesResponse::from(error),
        })
    }

    async fn remove_publish(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::RemovePublishPathParams,
    ) -> std::result::Result<RemovePublishResponse, ()> {
        Ok(match self.state.publish.remove(&path_params.publish_id).await {
            Ok(()) => RemovePublishResponse::Status204_PublishRemoved,
            Err(error) => RemovePublishResponse::from(error),
        })
    }

    async fn write_publish_content(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::WritePublishContentPathParams,
        query_params: &models::WritePublishContentQueryParams,
        body: &Bytes,
    ) -> std::result::Result<WritePublishContentResponse, ()> {
        Ok(
            match self.state.publish.write(&path_params.publish_id, query_params.offset, Body::from(body.clone())).await {
                Ok(()) => WritePublishContentResponse::Status204_ChunkAccepted,
                Err(error) => WritePublishContentResponse::from(error),
            },
        )
    }
}

crate::impl_api_response!(
    CreatePublishResponse;
    InvalidRequest => Status400_TheRequestIsInvalid,
    PayloadTooLarge => Status413_TheDeclaredUploadSizeExceedsTheConfiguredLimit,
    InsufficientStorage => Status507_TheDaemonDoesNotHaveEnoughSpaceForTheUpload,
);
crate::impl_api_response!(
    WritePublishContentResponse;
    InvalidRequest => Status400_TheRequestIsInvalid,
    NotFound => Status404_TheResourceWasNotFound,
    Conflict => Status409_TheRequestConflictsWithTheCurrentState,
    PayloadTooLarge => Status413_TheDeclaredUploadSizeExceedsTheConfiguredLimit,
    InsufficientStorage => Status507_TheDaemonDoesNotHaveEnoughSpaceForTheUpload,
);
crate::impl_api_response!(
    CommitPublishResponse;
    InvalidRequest => Status400_TheRequestIsInvalid,
    NotFound => Status404_TheResourceWasNotFound,
    Conflict => Status409_TheRequestConflictsWithTheCurrentState,
);
crate::impl_api_response!(
    GetPublishResponse;
    NotFound => Status404_TheResourceWasNotFound,
);
crate::impl_api_response!(ListPublishesResponse;);
crate::impl_api_response!(
    CancelPublishResponse;
    NotFound => Status404_TheResourceWasNotFound,
    Conflict => Status409_TheRequestConflictsWithTheCurrentState,
);
crate::impl_api_response!(
    RemovePublishResponse;
    NotFound => Status404_TheResourceWasNotFound,
    Conflict => Status409_TheRequestConflictsWithTheCurrentState,
);
