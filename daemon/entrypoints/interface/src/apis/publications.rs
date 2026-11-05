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
pub enum CreatePublicationResponse {
    /// The file was received and encoding is queued.
    Status202_TheFileWasReceivedAndEncodingIsQueued(models::Publication),
    /// The request is invalid.
    Status400_TheRequestIsInvalid(models::ErrorResponse),
    /// The body is over 32 MiB, and an upload session must be used.
    Status413_TheBodyIsOver(models::ErrorResponse),
    /// Internal server error.
    Status500_InternalServerError(models::ErrorResponse),
    /// The volume has no space left.
    Status507_TheVolumeHasNoSpaceLeft(models::ErrorResponse),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[must_use]
#[allow(clippy::large_enum_variant)]
pub enum DeletePublicationResponse {
    /// The publication is gone.
    Status204_ThePublicationIsGone,
    /// The resource does not exist.
    Status404_TheResourceDoesNotExist(models::ErrorResponse),
    /// Internal server error.
    Status500_InternalServerError(models::ErrorResponse),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[must_use]
#[allow(clippy::large_enum_variant)]
pub enum GetPublicationResponse {
    /// Publication state.
    Status200_PublicationState(models::Publication),
    /// The resource does not exist.
    Status404_TheResourceDoesNotExist(models::ErrorResponse),
    /// Internal server error.
    Status500_InternalServerError(models::ErrorResponse),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[must_use]
#[allow(clippy::large_enum_variant)]
pub enum ListPublicationsResponse {
    /// Known publications.
    Status200_KnownPublications(models::PublicationList),
    /// Internal server error.
    Status500_InternalServerError(models::ErrorResponse),
}

/// Publications
#[async_trait]
#[allow(clippy::ptr_arg)]
pub trait Publications<E: std::fmt::Debug + Send + Sync + 'static = ()>: super::ErrorHandler<E> {
    /// Upload and publish a file of 32 MiB or less.
    ///
    /// CreatePublication - POST /api/v1/publications
    async fn create_publication(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        query_params: &models::CreatePublicationQueryParams,
        body: &Bytes,
    ) -> Result<CreatePublicationResponse, E>;

    /// Cancel or stop a publication.
    ///
    /// DeletePublication - DELETE /api/v1/publications/{publicationId}
    async fn delete_publication(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::DeletePublicationPathParams,
    ) -> Result<DeletePublicationResponse, E>;

    /// Get a publication.
    ///
    /// GetPublication - GET /api/v1/publications/{publicationId}
    async fn get_publication(&self, method: &Method, host: &Host, cookies: &CookieJar, path_params: &models::GetPublicationPathParams) -> Result<GetPublicationResponse, E>;

    /// List publications.
    ///
    /// ListPublications - GET /api/v1/publications
    async fn list_publications(&self, method: &Method, host: &Host, cookies: &CookieJar) -> Result<ListPublicationsResponse, E>;
}
