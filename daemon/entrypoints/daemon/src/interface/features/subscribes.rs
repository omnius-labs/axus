use async_trait::async_trait;
use axum_extra::extract::CookieJar;
use headers::Host;
use http::Method;
use omnius_axus_interface::types::ByteArray;
use tokio::io::AsyncReadExt as _;

use omnius_axus_interface::{
    apis::subscribes::{
        CancelSubscribeResponse, CreateSubscribeResponse, GetSubscribeResponse, ListSubscribesResponse, ReadSubscribeContentResponse, RemoveSubscribeResponse, Subscribes,
    },
    models,
};

use crate::interface::ApiServer;

pub mod route;

#[allow(unused_variables)]
#[async_trait]
impl Subscribes for ApiServer {
    async fn cancel_subscribe(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::CancelSubscribePathParams,
    ) -> std::result::Result<CancelSubscribeResponse, ()> {
        Ok(match self.state.subscribe.cancel(&path_params.subscribe_id).await {
            Ok(subscribe) => CancelSubscribeResponse::Status202_CancellationRequested(models::SubscribeResponse::new(subscribe)),
            Err(error) => CancelSubscribeResponse::from(error),
        })
    }

    async fn create_subscribe(&self, method: &Method, host: &Host, cookies: &CookieJar, body: &models::SubscribeCreateRequest) -> std::result::Result<CreateSubscribeResponse, ()> {
        Ok(match self.state.subscribe.create(body).await {
            Ok(subscribe) => CreateSubscribeResponse::Status202_SubscriptionStarted(models::SubscribeResponse::new(subscribe)),
            Err(error) => CreateSubscribeResponse::from(error),
        })
    }

    async fn get_subscribe(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::GetSubscribePathParams,
    ) -> std::result::Result<GetSubscribeResponse, ()> {
        Ok(match self.state.subscribe.get(&path_params.subscribe_id).await {
            Ok(Some(subscribe)) => GetSubscribeResponse::Status200_SubscriptionState(models::SubscribeResponse::new(subscribe)),
            Ok(None) => GetSubscribeResponse::from(crate::error::ApiError::not_found("subscription not found")),
            Err(error) => GetSubscribeResponse::from(error),
        })
    }

    async fn list_subscribes(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        query_params: &models::ListSubscribesQueryParams,
    ) -> std::result::Result<ListSubscribesResponse, ()> {
        Ok(match self.state.subscribe.list(query_params.limit, query_params.offset).await {
            Ok(subscribes) => ListSubscribesResponse::Status200_KnownSubscriptions(subscribes),
            Err(error) => ListSubscribesResponse::from(error),
        })
    }

    async fn read_subscribe_content(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        header_params: &models::ReadSubscribeContentHeaderParams,
        path_params: &models::ReadSubscribeContentPathParams,
    ) -> std::result::Result<ReadSubscribeContentResponse, ()> {
        let _ = header_params;
        let download = match self.state.subscribe.download(&path_params.subscribe_id).await {
            Ok(download) => download,
            Err(error) => return Ok(ReadSubscribeContentResponse::from(error)),
        };
        let mut file = match tokio::fs::File::open(&download.path).await {
            Ok(file) => file,
            Err(_) => {
                return Ok(ReadSubscribeContentResponse::from(crate::error::ApiError::gone(
                    "the subscription output is no longer available",
                )));
            }
        };
        let mut bytes = Vec::new();
        if let Err(error) = file.read_to_end(&mut bytes).await {
            return Ok(ReadSubscribeContentResponse::from(crate::error::ApiError::internal(format!(
                "cannot read the subscription output: {error}"
            ))));
        }
        Ok(ReadSubscribeContentResponse::Status200_CompleteOutput {
            body: ByteArray(bytes),
            accept_ranges: Some("bytes".to_string()),
            content_range: None,
            e_tag: Some(download.etag),
        })
    }

    async fn remove_subscribe(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::RemoveSubscribePathParams,
    ) -> std::result::Result<RemoveSubscribeResponse, ()> {
        Ok(match self.state.subscribe.remove(&path_params.subscribe_id).await {
            Ok(()) => RemoveSubscribeResponse::Status204_SubscriptionRemoved,
            Err(error) => RemoveSubscribeResponse::from(error),
        })
    }
}

crate::impl_api_response!(
    CreateSubscribeResponse;
    InvalidRequest => Status400_TheRequestIsInvalid,
    NotFound => Status404_TheResourceWasNotFound,
    Conflict => Status409_TheRequestConflictsWithTheCurrentState,
);
crate::impl_api_response!(
    GetSubscribeResponse;
    NotFound => Status404_TheResourceWasNotFound,
);
crate::impl_api_response!(ListSubscribesResponse;);
crate::impl_api_response!(
    CancelSubscribeResponse;
    NotFound => Status404_TheResourceWasNotFound,
    Conflict => Status409_TheRequestConflictsWithTheCurrentState,
);
crate::impl_api_response!(
    RemoveSubscribeResponse;
    NotFound => Status404_TheResourceWasNotFound,
    Conflict => Status409_TheRequestConflictsWithTheCurrentState,
);
crate::impl_api_response!(
    ReadSubscribeContentResponse;
    NotFound => Status404_TheResourceWasNotFound,
    Conflict => Status409_TheRequestConflictsWithTheCurrentState,
    Gone => Status410_TheResourceIsNoLongerAvailable,
);
