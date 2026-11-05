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
pub enum CommitUploadResponse {
    /// All bytes arrived and encoding is queued.
    Status202_AllBytesArrivedAndEncodingIsQueued(models::Publication),
    /// The request is invalid.
    Status400_TheRequestIsInvalid(models::ErrorResponse),
    /// The resource does not exist.
    Status404_TheResourceDoesNotExist(models::ErrorResponse),
    /// The resource's state or an existing entry conflicts with the request.
    Status409_TheResource(models::ErrorResponse),
    /// Internal server error.
    Status500_InternalServerError(models::ErrorResponse),
    /// The volume has no space left.
    Status507_TheVolumeHasNoSpaceLeft(models::ErrorResponse),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[must_use]
#[allow(clippy::large_enum_variant)]
pub enum CreateUploadResponse {
    /// Session created.
    Status201_SessionCreated(models::UploadSession),
    /// The request is invalid.
    Status400_TheRequestIsInvalid(models::ErrorResponse),
    /// Internal server error.
    Status500_InternalServerError(models::ErrorResponse),
    /// The volume has no space left.
    Status507_TheVolumeHasNoSpaceLeft(models::ErrorResponse),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[must_use]
#[allow(clippy::large_enum_variant)]
pub enum DeleteUploadResponse {
    /// The session and its bytes are gone.
    Status204_TheSessionAndItsBytesAreGone,
    /// The resource does not exist.
    Status404_TheResourceDoesNotExist(models::ErrorResponse),
    /// Internal server error.
    Status500_InternalServerError(models::ErrorResponse),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[must_use]
#[allow(clippy::large_enum_variant)]
pub enum GetUploadResponse {
    /// Session state.
    Status200_SessionState(models::UploadSession),
    /// The resource does not exist.
    Status404_TheResourceDoesNotExist(models::ErrorResponse),
    /// Internal server error.
    Status500_InternalServerError(models::ErrorResponse),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[must_use]
#[allow(clippy::large_enum_variant)]
pub enum WriteUploadContentResponse {
    /// Chunk accepted.
    Status204_ChunkAccepted,
    /// The request is invalid.
    Status400_TheRequestIsInvalid(models::ErrorResponse),
    /// The resource does not exist.
    Status404_TheResourceDoesNotExist(models::ErrorResponse),
    /// The resource's state or an existing entry conflicts with the request.
    Status409_TheResource(models::ErrorResponse),
    /// Internal server error.
    Status500_InternalServerError(models::ErrorResponse),
    /// The volume has no space left.
    Status507_TheVolumeHasNoSpaceLeft(models::ErrorResponse),
}

/// Uploads
#[async_trait]
#[allow(clippy::ptr_arg)]
pub trait Uploads<E: std::fmt::Debug + Send + Sync + 'static = ()>: super::ErrorHandler<E> {
    /// Finish an upload session and publish the file.
    ///
    /// CommitUpload - POST /api/v1/uploads/{uploadId}/commit
    async fn commit_upload(&self, method: &Method, host: &Host, cookies: &CookieJar, path_params: &models::CommitUploadPathParams) -> Result<CommitUploadResponse, E>;

    /// Start an upload session.
    ///
    /// CreateUpload - POST /api/v1/uploads
    async fn create_upload(&self, method: &Method, host: &Host, cookies: &CookieJar, body: &models::CreateUploadRequest) -> Result<CreateUploadResponse, E>;

    /// Abort an upload session.
    ///
    /// DeleteUpload - DELETE /api/v1/uploads/{uploadId}
    async fn delete_upload(&self, method: &Method, host: &Host, cookies: &CookieJar, path_params: &models::DeleteUploadPathParams) -> Result<DeleteUploadResponse, E>;

    /// Get an upload session.
    ///
    /// GetUpload - GET /api/v1/uploads/{uploadId}
    async fn get_upload(&self, method: &Method, host: &Host, cookies: &CookieJar, path_params: &models::GetUploadPathParams) -> Result<GetUploadResponse, E>;

    /// Append a chunk of 32 MiB or less.
    ///
    /// WriteUploadContent - PUT /api/v1/uploads/{uploadId}/content
    async fn write_upload_content(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::WriteUploadContentPathParams,
        query_params: &models::WriteUploadContentQueryParams,
        body: &Bytes,
    ) -> Result<WriteUploadContentResponse, E>;
}
