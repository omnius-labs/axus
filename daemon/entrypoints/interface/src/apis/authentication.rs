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
pub enum CreateOwnerResponse {
    /// The owner is registered.
    Status204_TheOwnerIsRegistered,
    /// The request is malformed.
    Status400_TheRequestIsMalformed(models::ErrorResponse),
    /// The request conflicts with the current state.
    Status409_TheRequestConflictsWithTheCurrentState(models::ErrorResponse),
    /// Internal server error.
    Status500_InternalServerError(models::ErrorResponse),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[must_use]
#[allow(clippy::large_enum_variant)]
pub enum CreateSessionResponse {
    /// The session cookie is set.
    Status204_TheSessionCookieIsSet,
    /// The session token for a bearer client.
    Status200_TheSessionTokenForABearerClient(models::LoginResponse),
    /// A session is required.
    Status401_ASessionIsRequired(models::ErrorResponse),
    /// Internal server error.
    Status500_InternalServerError(models::ErrorResponse),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[must_use]
#[allow(clippy::large_enum_variant)]
pub enum DeleteSessionResponse {
    /// The session is gone.
    Status204_TheSessionIsGone,
    /// A session is required.
    Status401_ASessionIsRequired(models::ErrorResponse),
    /// Internal server error.
    Status500_InternalServerError(models::ErrorResponse),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[must_use]
#[allow(clippy::large_enum_variant)]
pub enum GetSetupStatusResponse {
    /// Setup state.
    Status200_SetupState(models::SetupStatus),
    /// Internal server error.
    Status500_InternalServerError(models::ErrorResponse),
}

/// Authentication
#[async_trait]
#[allow(clippy::ptr_arg)]
pub trait Authentication<E: std::fmt::Debug + Send + Sync + 'static = ()>: super::ErrorHandler<E> {
    /// Register the owner.
    ///
    /// CreateOwner - POST /api/v1/setup
    async fn create_owner(&self, method: &Method, host: &Host, cookies: &CookieJar, body: &models::Credentials) -> Result<CreateOwnerResponse, E>;

    /// Log in.
    ///
    /// CreateSession - POST /api/v1/auth/session
    async fn create_session(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        header_params: &models::CreateSessionHeaderParams,
        body: &models::Credentials,
    ) -> Result<CreateSessionResponse, E>;

    /// Log out.
    ///
    /// DeleteSession - DELETE /api/v1/auth/session
    async fn delete_session(&self, method: &Method, host: &Host, cookies: &CookieJar) -> Result<DeleteSessionResponse, E>;

    /// Report whether the owner is registered.
    ///
    /// GetSetupStatus - GET /api/v1/setup/status
    async fn get_setup_status(&self, method: &Method, host: &Host, cookies: &CookieJar) -> Result<GetSetupStatusResponse, E>;
}
