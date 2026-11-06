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
pub enum CancelPublicationResponse {
    /// The publication state after the request.
    Status200_ThePublicationStateAfterTheRequest(models::Publication),
    /// The resource does not exist.
    Status404_TheResourceDoesNotExist(models::ErrorResponse),
    /// The request conflicts with the current state.
    Status409_TheRequestConflictsWithTheCurrentState(models::ErrorResponse),
    /// Internal server error.
    Status500_InternalServerError(models::ErrorResponse),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[must_use]
#[allow(clippy::large_enum_variant)]
pub enum CreatePublicationResponse {
    /// The publication is ready to receive content.
    Status202_ThePublicationIsReadyToReceiveContent(models::Publication),
    /// The request is malformed.
    Status400_TheRequestIsMalformed(models::ErrorResponse),
    /// The request conflicts with the current state.
    Status409_TheRequestConflictsWithTheCurrentState(models::ErrorResponse),
    /// The content is over the declared or configured size.
    Status413_TheContentIsOverTheDeclaredOrConfiguredSize(models::ErrorResponse),
    /// The destination has insufficient space.
    Status507_TheDestinationHasInsufficientSpace(models::ErrorResponse),
    /// Internal server error.
    Status500_InternalServerError(models::ErrorResponse),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[must_use]
#[allow(clippy::large_enum_variant)]
pub enum DeletePublicationResponse {
    /// The publication is gone.
    Status204_ThePublicationIsGone,
    /// The resource does not exist.
    Status404_TheResourceDoesNotExist(models::ErrorResponse),
    /// The request conflicts with the current state.
    Status409_TheRequestConflictsWithTheCurrentState(models::ErrorResponse),
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

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[must_use]
#[allow(clippy::large_enum_variant)]
pub enum WritePublicationContentResponse {
    /// The content was accepted.
    Status200_TheContentWasAccepted(models::Publication),
    /// The request is malformed.
    Status400_TheRequestIsMalformed(models::ErrorResponse),
    /// The resource does not exist.
    Status404_TheResourceDoesNotExist(models::ErrorResponse),
    /// The request conflicts with the current state.
    Status409_TheRequestConflictsWithTheCurrentState(models::ErrorResponse),
    /// The content is over the declared or configured size.
    Status413_TheContentIsOverTheDeclaredOrConfiguredSize(models::ErrorResponse),
    /// The destination has insufficient space.
    Status507_TheDestinationHasInsufficientSpace(models::ErrorResponse),
    /// Internal server error.
    Status500_InternalServerError(models::ErrorResponse),
}

/// Publications
#[async_trait]
#[allow(clippy::ptr_arg)]
pub trait Publications<E: std::fmt::Debug + Send + Sync + 'static = ()>: super::ErrorHandler<E> {
    /// Cancel a publication.
    ///
    /// CancelPublication - POST /api/v1/publications/{publicationId}/cancel
    async fn cancel_publication(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::CancelPublicationPathParams,
    ) -> Result<CancelPublicationResponse, E>;

    /// Declare a publication and start receiving its content.
    ///
    /// CreatePublication - POST /api/v1/publications
    async fn create_publication(&self, method: &Method, host: &Host, cookies: &CookieJar, body: &models::CreatePublicationRequest) -> Result<CreatePublicationResponse, E>;

    /// Remove a publication.
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

    /// Append content at the received offset.
    ///
    /// WritePublicationContent - PUT /api/v1/publications/{publicationId}/content
    async fn write_publication_content(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        header_params: &models::WritePublicationContentHeaderParams,
        path_params: &models::WritePublicationContentPathParams,
        body: &Bytes,
    ) -> Result<WritePublicationContentResponse, E>;
}
