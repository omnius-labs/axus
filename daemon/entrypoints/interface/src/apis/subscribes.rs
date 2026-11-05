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
pub enum CancelSubscribeResponse {
    /// Cancellation requested.
    Status202_CancellationRequested(models::SubscribeResponse),
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
pub enum CreateSubscribeResponse {
    /// Subscription started.
    Status202_SubscriptionStarted(models::SubscribeResponse),
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
pub enum GetSubscribeResponse {
    /// Subscription state.
    Status200_SubscriptionState(models::SubscribeResponse),
    /// The resource was not found.
    Status404_TheResourceWasNotFound(models::ErrorResponse),
    /// Internal server error.
    Status500_InternalServerError(models::ErrorResponse),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[must_use]
#[allow(clippy::large_enum_variant)]
pub enum ListSubscribesResponse {
    /// Known subscriptions.
    Status200_KnownSubscriptions(models::SubscribeList),
    /// Internal server error.
    Status500_InternalServerError(models::ErrorResponse),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[must_use]
#[allow(clippy::large_enum_variant)]
pub enum ReadSubscribeContentResponse {
    /// Complete output.
    Status200_CompleteOutput {
        body: ByteArray,
        accept_ranges: Option<String>,
        content_range: Option<String>,
        e_tag: Option<String>,
    },
    /// Partial output.
    Status206_PartialOutput {
        body: ByteArray,
        accept_ranges: Option<String>,
        content_range: Option<String>,
        e_tag: Option<String>,
    },
    /// Output not modified.
    Status304_OutputNotModified,
    /// The resource was not found.
    Status404_TheResourceWasNotFound(models::ErrorResponse),
    /// The request conflicts with the current state.
    Status409_TheRequestConflictsWithTheCurrentState(models::ErrorResponse),
    /// The resource is no longer available.
    Status410_TheResourceIsNoLongerAvailable(models::ErrorResponse),
    /// The requested byte range cannot be satisfied.
    Status416_TheRequestedByteRangeCannotBeSatisfied(models::ErrorResponse),
    /// Internal server error.
    Status500_InternalServerError(models::ErrorResponse),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[must_use]
#[allow(clippy::large_enum_variant)]
pub enum RemoveSubscribeResponse {
    /// Subscription removed.
    Status204_SubscriptionRemoved,
    /// The resource was not found.
    Status404_TheResourceWasNotFound(models::ErrorResponse),
    /// The request conflicts with the current state.
    Status409_TheRequestConflictsWithTheCurrentState(models::ErrorResponse),
    /// Internal server error.
    Status500_InternalServerError(models::ErrorResponse),
}

/// Subscribes
#[async_trait]
#[allow(clippy::ptr_arg)]
pub trait Subscribes<E: std::fmt::Debug + Send + Sync + 'static = ()>: super::ErrorHandler<E> {
    /// Cancel a subscription.
    ///
    /// CancelSubscribe - POST /api/v1/subscribes/{subscribeId}/cancel
    async fn cancel_subscribe(&self, method: &Method, host: &Host, cookies: &CookieJar, path_params: &models::CancelSubscribePathParams) -> Result<CancelSubscribeResponse, E>;

    /// Start a subscription.
    ///
    /// CreateSubscribe - POST /api/v1/subscribes
    async fn create_subscribe(&self, method: &Method, host: &Host, cookies: &CookieJar, body: &models::SubscribeCreateRequest) -> Result<CreateSubscribeResponse, E>;

    /// Get a subscription.
    ///
    /// GetSubscribe - GET /api/v1/subscribes/{subscribeId}
    async fn get_subscribe(&self, method: &Method, host: &Host, cookies: &CookieJar, path_params: &models::GetSubscribePathParams) -> Result<GetSubscribeResponse, E>;

    /// List subscriptions.
    ///
    /// ListSubscribes - GET /api/v1/subscribes
    async fn list_subscribes(&self, method: &Method, host: &Host, cookies: &CookieJar, query_params: &models::ListSubscribesQueryParams) -> Result<ListSubscribesResponse, E>;

    /// Read a completed subscription output.
    ///
    /// ReadSubscribeContent - GET /api/v1/subscribes/{subscribeId}/content
    async fn read_subscribe_content(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        header_params: &models::ReadSubscribeContentHeaderParams,
        path_params: &models::ReadSubscribeContentPathParams,
    ) -> Result<ReadSubscribeContentResponse, E>;

    /// Remove a subscription.
    ///
    /// RemoveSubscribe - DELETE /api/v1/subscribes/{subscribeId}
    async fn remove_subscribe(&self, method: &Method, host: &Host, cookies: &CookieJar, path_params: &models::RemoveSubscribePathParams) -> Result<RemoveSubscribeResponse, E>;
}
