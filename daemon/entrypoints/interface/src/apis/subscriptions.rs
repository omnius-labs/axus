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
pub enum CancelSubscriptionResponse {
    /// Subscription state.
    Status200_SubscriptionState(models::Subscription),
    /// The resource does not exist.
    Status404_TheResourceDoesNotExist(models::ErrorResponse),
    /// The resource's state or an existing entry conflicts with the request.
    Status409_TheResource(models::ErrorResponse),
    /// Internal server error.
    Status500_InternalServerError(models::ErrorResponse),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[must_use]
#[allow(clippy::large_enum_variant)]
pub enum CreateSubscriptionResponse {
    /// The fetch is queued.
    Status202_TheFetchIsQueued(models::Subscription),
    /// The request is invalid.
    Status400_TheRequestIsInvalid(models::ErrorResponse),
    /// The resource's state or an existing entry conflicts with the request.
    Status409_TheResource(models::ErrorResponse),
    /// Internal server error.
    Status500_InternalServerError(models::ErrorResponse),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[must_use]
#[allow(clippy::large_enum_variant)]
pub enum DeleteSubscriptionResponse {
    /// The subscription is gone.
    Status204_TheSubscriptionIsGone,
    /// The resource does not exist.
    Status404_TheResourceDoesNotExist(models::ErrorResponse),
    /// The resource's state or an existing entry conflicts with the request.
    Status409_TheResource(models::ErrorResponse),
    /// Internal server error.
    Status500_InternalServerError(models::ErrorResponse),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[must_use]
#[allow(clippy::large_enum_variant)]
pub enum GetSubscriptionResponse {
    /// Subscription state.
    Status200_SubscriptionState(models::Subscription),
    /// The resource does not exist.
    Status404_TheResourceDoesNotExist(models::ErrorResponse),
    /// Internal server error.
    Status500_InternalServerError(models::ErrorResponse),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[must_use]
#[allow(clippy::large_enum_variant)]
pub enum GetSubscriptionContentResponse {
    /// The whole output.
    Status200_TheWholeOutput(ByteArray),
    /// The requested range of the output.
    Status206_TheRequestedRangeOfTheOutput(ByteArray),
    /// The resource does not exist.
    Status404_TheResourceDoesNotExist(models::ErrorResponse),
    /// The resource's state or an existing entry conflicts with the request.
    Status409_TheResource(models::ErrorResponse),
    /// The range cannot be satisfied.
    Status416_TheRangeCannotBeSatisfied,
    /// Internal server error.
    Status500_InternalServerError(models::ErrorResponse),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[must_use]
#[allow(clippy::large_enum_variant)]
pub enum ListSubscriptionsResponse {
    /// Known subscriptions.
    Status200_KnownSubscriptions(models::SubscriptionList),
    /// Internal server error.
    Status500_InternalServerError(models::ErrorResponse),
}

/// Subscriptions
#[async_trait]
#[allow(clippy::ptr_arg)]
pub trait Subscriptions<E: std::fmt::Debug + Send + Sync + 'static = ()>: super::ErrorHandler<E> {
    /// Cancel a subscription.
    ///
    /// CancelSubscription - POST /api/v1/subscriptions/{subscriptionId}/cancel
    async fn cancel_subscription(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::CancelSubscriptionPathParams,
    ) -> Result<CancelSubscriptionResponse, E>;

    /// Start fetching a file.
    ///
    /// CreateSubscription - POST /api/v1/subscriptions
    async fn create_subscription(&self, method: &Method, host: &Host, cookies: &CookieJar, body: &models::CreateSubscriptionRequest) -> Result<CreateSubscriptionResponse, E>;

    /// Delete a subscription.
    ///
    /// DeleteSubscription - DELETE /api/v1/subscriptions/{subscriptionId}
    async fn delete_subscription(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::DeleteSubscriptionPathParams,
        query_params: &models::DeleteSubscriptionQueryParams,
    ) -> Result<DeleteSubscriptionResponse, E>;

    /// Get a subscription.
    ///
    /// GetSubscription - GET /api/v1/subscriptions/{subscriptionId}
    async fn get_subscription(&self, method: &Method, host: &Host, cookies: &CookieJar, path_params: &models::GetSubscriptionPathParams) -> Result<GetSubscriptionResponse, E>;

    /// Download a completed output.
    ///
    /// GetSubscriptionContent - GET /api/v1/subscriptions/{subscriptionId}/content
    async fn get_subscription_content(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::GetSubscriptionContentPathParams,
    ) -> Result<GetSubscriptionContentResponse, E>;

    /// List subscriptions.
    ///
    /// ListSubscriptions - GET /api/v1/subscriptions
    async fn list_subscriptions(&self, method: &Method, host: &Host, cookies: &CookieJar) -> Result<ListSubscriptionsResponse, E>;
}
