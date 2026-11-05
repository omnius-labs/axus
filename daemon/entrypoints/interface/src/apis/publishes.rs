use async_trait::async_trait;
use axum::extract::*;
use axum_extra::extract::CookieJar;
use bytes::Bytes;
use headers::Host;
use http::Method;
use serde::{Deserialize, Serialize};

use crate::{models, types::*};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[must_use]
#[allow(clippy::large_enum_variant)]
pub enum CancelPublishResponse {
    /// Publish cancelled.
    Status204_PublishCancelled,
    /// The resource was not found.
    Status404_TheResourceWasNotFound(models::ErrorResponse),
    /// The request conflicts with the current state.
    Status409_TheRequestConflictsWithTheCurrentState(models::ErrorResponse),
    /// Internal server error.
    Status500_InternalServerError(models::ErrorResponse),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[must_use]
#[allow(clippy::large_enum_variant)]
pub enum CommitPublishResponse {
    /// Encoding started.
    Status202_EncodingStarted(models::PublishResponse),
    /// The request is invalid.
    Status400_TheRequestIsInvalid(models::ErrorResponse),
    /// The resource was not found.
    Status404_TheResourceWasNotFound(models::ErrorResponse),
    /// The request conflicts with the current state.
    Status409_TheRequestConflictsWithTheCurrentState(models::ErrorResponse),
    /// Internal server error.
    Status500_InternalServerError(models::ErrorResponse),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[must_use]
#[allow(clippy::large_enum_variant)]
pub enum CreatePublishResponse {
    /// Uploading publish created.
    Status201_UploadingPublishCreated(models::PublishResponse),
    /// The request is invalid.
    Status400_TheRequestIsInvalid(models::ErrorResponse),
    /// The declared upload size exceeds the configured limit.
    Status413_TheDeclaredUploadSizeExceedsTheConfiguredLimit(models::ErrorResponse),
    /// Internal server error.
    Status500_InternalServerError(models::ErrorResponse),
    /// The daemon does not have enough space for the upload.
    Status507_TheDaemonDoesNotHaveEnoughSpaceForTheUpload(models::ErrorResponse),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[must_use]
#[allow(clippy::large_enum_variant)]
pub enum GetPublishResponse {
    /// Publish state.
    Status200_PublishState(models::PublishResponse),
    /// The resource was not found.
    Status404_TheResourceWasNotFound(models::ErrorResponse),
    /// Internal server error.
    Status500_InternalServerError(models::ErrorResponse),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[must_use]
#[allow(clippy::large_enum_variant)]
pub enum ListPublishesResponse {
    /// Known publishes.
    Status200_KnownPublishes(models::PublishList),
    /// Internal server error.
    Status500_InternalServerError(models::ErrorResponse),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[must_use]
#[allow(clippy::large_enum_variant)]
pub enum RemovePublishResponse {
    /// Publish removed.
    Status204_PublishRemoved,
    /// The resource was not found.
    Status404_TheResourceWasNotFound(models::ErrorResponse),
    /// The request conflicts with the current state.
    Status409_TheRequestConflictsWithTheCurrentState(models::ErrorResponse),
    /// Internal server error.
    Status500_InternalServerError(models::ErrorResponse),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[must_use]
#[allow(clippy::large_enum_variant)]
pub enum WritePublishContentResponse {
    /// Chunk accepted.
    Status204_ChunkAccepted,
    /// The request is invalid.
    Status400_TheRequestIsInvalid(models::ErrorResponse),
    /// The resource was not found.
    Status404_TheResourceWasNotFound(models::ErrorResponse),
    /// The request conflicts with the current state.
    Status409_TheRequestConflictsWithTheCurrentState(models::ErrorResponse),
    /// The declared upload size exceeds the configured limit.
    Status413_TheDeclaredUploadSizeExceedsTheConfiguredLimit(models::ErrorResponse),
    /// Internal server error.
    Status500_InternalServerError(models::ErrorResponse),
    /// The daemon does not have enough space for the upload.
    Status507_TheDaemonDoesNotHaveEnoughSpaceForTheUpload(models::ErrorResponse),
}

/// Publishes
#[async_trait]
#[allow(clippy::ptr_arg)]
pub trait Publishes<E: std::fmt::Debug + Send + Sync + 'static = ()>: super::ErrorHandler<E> {
    /// Cancel a publish.
    ///
    /// CancelPublish - POST /api/v1/publishes/{publishId}/cancel
    async fn cancel_publish(&self, method: &Method, host: &Host, cookies: &CookieJar, path_params: &models::CancelPublishPathParams) -> Result<CancelPublishResponse, E>;

    /// Commit an upload.
    ///
    /// CommitPublish - POST /api/v1/publishes/{publishId}/commit
    async fn commit_publish(&self, method: &Method, host: &Host, cookies: &CookieJar, path_params: &models::CommitPublishPathParams) -> Result<CommitPublishResponse, E>;

    /// Create a publish.
    ///
    /// CreatePublish - POST /api/v1/publishes
    async fn create_publish(&self, method: &Method, host: &Host, cookies: &CookieJar, body: &models::PublishCreateRequest) -> Result<CreatePublishResponse, E>;

    /// Get a publish.
    ///
    /// GetPublish - GET /api/v1/publishes/{publishId}
    async fn get_publish(&self, method: &Method, host: &Host, cookies: &CookieJar, path_params: &models::GetPublishPathParams) -> Result<GetPublishResponse, E>;

    /// List publishes.
    ///
    /// ListPublishes - GET /api/v1/publishes
    async fn list_publishes(&self, method: &Method, host: &Host, cookies: &CookieJar, query_params: &models::ListPublishesQueryParams) -> Result<ListPublishesResponse, E>;

    /// Remove a publish.
    ///
    /// RemovePublish - DELETE /api/v1/publishes/{publishId}
    async fn remove_publish(&self, method: &Method, host: &Host, cookies: &CookieJar, path_params: &models::RemovePublishPathParams) -> Result<RemovePublishResponse, E>;

    /// Write an upload chunk.
    ///
    /// WritePublishContent - PUT /api/v1/publishes/{publishId}/content
    async fn write_publish_content(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::WritePublishContentPathParams,
        query_params: &models::WritePublishContentQueryParams,
        body: &Bytes,
    ) -> Result<WritePublishContentResponse, E>;
}
