use std::collections::HashMap;

use axum::{body::Body, extract::*, response::Response, routing::*};
use axum_extra::{
    TypedHeader,
    extract::{CookieJar, Query as QueryExtra},
};
use bytes::Bytes;
use headers::Host;
use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, header::CONTENT_TYPE};
use tracing::error;
use validator::{Validate, ValidationErrors};

#[allow(unused_imports)]
use crate::{apis, models};
use crate::{header, types::*};
#[allow(unused_imports)]
use crate::{models::check_xss_map, models::check_xss_map_nested, models::check_xss_map_string, models::check_xss_string, models::check_xss_vec_string};

/// Setup API Server.
pub fn new<I, A, E>(api_impl: I) -> Router
where
    I: AsRef<A> + Clone + Send + Sync + 'static,
    A: apis::health::Health<E> + apis::publishes::Publishes<E> + apis::subscribes::Subscribes<E> + Send + Sync + 'static,
    E: std::fmt::Debug + Send + Sync + 'static,
{
    // build our application with a route
    Router::new()
        .route("/api/v1/health", get(get_health::<I, A, E>))
        .route("/api/v1/publishes", get(list_publishes::<I, A, E>).post(create_publish::<I, A, E>))
        .route("/api/v1/publishes/{publish_id}", delete(remove_publish::<I, A, E>).get(get_publish::<I, A, E>))
        .route("/api/v1/publishes/{publish_id}/cancel", post(cancel_publish::<I, A, E>))
        .route("/api/v1/publishes/{publish_id}/commit", post(commit_publish::<I, A, E>))
        .route("/api/v1/publishes/{publish_id}/content", put(write_publish_content::<I, A, E>))
        .route("/api/v1/subscribes", get(list_subscribes::<I, A, E>).post(create_subscribe::<I, A, E>))
        .route("/api/v1/subscribes/{subscribe_id}", delete(remove_subscribe::<I, A, E>).get(get_subscribe::<I, A, E>))
        .route("/api/v1/subscribes/{subscribe_id}/cancel", post(cancel_subscribe::<I, A, E>))
        .route("/api/v1/subscribes/{subscribe_id}/content", get(read_subscribe_content::<I, A, E>))
        .with_state(api_impl)
}

#[tracing::instrument(skip_all)]
fn get_health_validation() -> std::result::Result<(), ValidationErrors> {
    Ok(())
}
/// GetHealth - GET /api/v1/health
#[tracing::instrument(skip_all)]
async fn get_health<I, A, E>(method: Method, TypedHeader(host): TypedHeader<Host>, cookies: CookieJar, State(api_impl): State<I>) -> Result<Response, StatusCode>
where
    I: AsRef<A> + Send + Sync,
    A: apis::health::Health<E> + Send + Sync,
    E: std::fmt::Debug + Send + Sync + 'static,
{
    #[allow(clippy::redundant_closure)]
    let validation = tokio::task::spawn_blocking(move || get_health_validation()).await.unwrap();

    let Ok(()) = validation else {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from(validation.unwrap_err().to_string()))
            .map_err(|_| StatusCode::BAD_REQUEST);
    };

    let result = api_impl.as_ref().get_health(&method, &host, &cookies).await;

    let mut response = Response::builder();

    let resp = match result {
        Ok(rsp) => match rsp {
            apis::health::GetHealthResponse::Status200_TheDaemonIsRunning(body) => {
                let mut response = response.status(200);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
            apis::health::GetHealthResponse::Status500_InternalServerError(body) => {
                let mut response = response.status(500);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
        },
        Err(why) => {
            // Application code returned an error. This should not happen, as the implementation should
            // return a valid response.
            return api_impl.as_ref().handle_error(&method, &host, &cookies, why).await;
        }
    };

    resp.map_err(|e| {
        error!(error = ?e);
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

#[tracing::instrument(skip_all)]
fn cancel_publish_validation(path_params: models::CancelPublishPathParams) -> std::result::Result<(models::CancelPublishPathParams,), ValidationErrors> {
    path_params.validate()?;

    Ok((path_params,))
}
/// CancelPublish - POST /api/v1/publishes/{publishId}/cancel
#[tracing::instrument(skip_all)]
async fn cancel_publish<I, A, E>(
    method: Method,
    TypedHeader(host): TypedHeader<Host>,
    cookies: CookieJar,
    Path(path_params): Path<models::CancelPublishPathParams>,
    State(api_impl): State<I>,
) -> Result<Response, StatusCode>
where
    I: AsRef<A> + Send + Sync,
    A: apis::publishes::Publishes<E> + Send + Sync,
    E: std::fmt::Debug + Send + Sync + 'static,
{
    #[allow(clippy::redundant_closure)]
    let validation = tokio::task::spawn_blocking(move || cancel_publish_validation(path_params)).await.unwrap();

    let Ok((path_params,)) = validation else {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from(validation.unwrap_err().to_string()))
            .map_err(|_| StatusCode::BAD_REQUEST);
    };

    let result = api_impl.as_ref().cancel_publish(&method, &host, &cookies, &path_params).await;

    let mut response = Response::builder();

    let resp = match result {
        Ok(rsp) => match rsp {
            apis::publishes::CancelPublishResponse::Status204_PublishCancelled => {
                let mut response = response.status(204);
                response.body(Body::empty())
            }
            apis::publishes::CancelPublishResponse::Status404_TheResourceWasNotFound(body) => {
                let mut response = response.status(404);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
            apis::publishes::CancelPublishResponse::Status409_TheRequestConflictsWithTheCurrentState(body) => {
                let mut response = response.status(409);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
            apis::publishes::CancelPublishResponse::Status500_InternalServerError(body) => {
                let mut response = response.status(500);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
        },
        Err(why) => {
            // Application code returned an error. This should not happen, as the implementation should
            // return a valid response.
            return api_impl.as_ref().handle_error(&method, &host, &cookies, why).await;
        }
    };

    resp.map_err(|e| {
        error!(error = ?e);
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

#[tracing::instrument(skip_all)]
fn commit_publish_validation(path_params: models::CommitPublishPathParams) -> std::result::Result<(models::CommitPublishPathParams,), ValidationErrors> {
    path_params.validate()?;

    Ok((path_params,))
}
/// CommitPublish - POST /api/v1/publishes/{publishId}/commit
#[tracing::instrument(skip_all)]
async fn commit_publish<I, A, E>(
    method: Method,
    TypedHeader(host): TypedHeader<Host>,
    cookies: CookieJar,
    Path(path_params): Path<models::CommitPublishPathParams>,
    State(api_impl): State<I>,
) -> Result<Response, StatusCode>
where
    I: AsRef<A> + Send + Sync,
    A: apis::publishes::Publishes<E> + Send + Sync,
    E: std::fmt::Debug + Send + Sync + 'static,
{
    #[allow(clippy::redundant_closure)]
    let validation = tokio::task::spawn_blocking(move || commit_publish_validation(path_params)).await.unwrap();

    let Ok((path_params,)) = validation else {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from(validation.unwrap_err().to_string()))
            .map_err(|_| StatusCode::BAD_REQUEST);
    };

    let result = api_impl.as_ref().commit_publish(&method, &host, &cookies, &path_params).await;

    let mut response = Response::builder();

    let resp = match result {
        Ok(rsp) => match rsp {
            apis::publishes::CommitPublishResponse::Status202_EncodingStarted(body) => {
                let mut response = response.status(202);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
            apis::publishes::CommitPublishResponse::Status400_TheRequestIsInvalid(body) => {
                let mut response = response.status(400);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
            apis::publishes::CommitPublishResponse::Status404_TheResourceWasNotFound(body) => {
                let mut response = response.status(404);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
            apis::publishes::CommitPublishResponse::Status409_TheRequestConflictsWithTheCurrentState(body) => {
                let mut response = response.status(409);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
            apis::publishes::CommitPublishResponse::Status500_InternalServerError(body) => {
                let mut response = response.status(500);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
        },
        Err(why) => {
            // Application code returned an error. This should not happen, as the implementation should
            // return a valid response.
            return api_impl.as_ref().handle_error(&method, &host, &cookies, why).await;
        }
    };

    resp.map_err(|e| {
        error!(error = ?e);
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

#[derive(validator::Validate)]
#[allow(dead_code)]
struct CreatePublishBodyValidator<'a> {
    #[validate(nested)]
    body: &'a models::PublishCreateRequest,
}

#[tracing::instrument(skip_all)]
fn create_publish_validation(body: models::PublishCreateRequest) -> std::result::Result<(models::PublishCreateRequest,), ValidationErrors> {
    let b = CreatePublishBodyValidator { body: &body };
    b.validate()?;

    Ok((body,))
}
/// CreatePublish - POST /api/v1/publishes
#[tracing::instrument(skip_all)]
async fn create_publish<I, A, E>(
    method: Method,
    TypedHeader(host): TypedHeader<Host>,
    cookies: CookieJar,
    State(api_impl): State<I>,
    Json(body): Json<models::PublishCreateRequest>,
) -> Result<Response, StatusCode>
where
    I: AsRef<A> + Send + Sync,
    A: apis::publishes::Publishes<E> + Send + Sync,
    E: std::fmt::Debug + Send + Sync + 'static,
{
    #[allow(clippy::redundant_closure)]
    let validation = tokio::task::spawn_blocking(move || create_publish_validation(body)).await.unwrap();

    let Ok((body,)) = validation else {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from(validation.unwrap_err().to_string()))
            .map_err(|_| StatusCode::BAD_REQUEST);
    };

    let result = api_impl.as_ref().create_publish(&method, &host, &cookies, &body).await;

    let mut response = Response::builder();

    let resp = match result {
        Ok(rsp) => match rsp {
            apis::publishes::CreatePublishResponse::Status201_UploadingPublishCreated(body) => {
                let mut response = response.status(201);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
            apis::publishes::CreatePublishResponse::Status400_TheRequestIsInvalid(body) => {
                let mut response = response.status(400);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
            apis::publishes::CreatePublishResponse::Status413_TheDeclaredUploadSizeExceedsTheConfiguredLimit(body) => {
                let mut response = response.status(413);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
            apis::publishes::CreatePublishResponse::Status500_InternalServerError(body) => {
                let mut response = response.status(500);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
            apis::publishes::CreatePublishResponse::Status507_TheDaemonDoesNotHaveEnoughSpaceForTheUpload(body) => {
                let mut response = response.status(507);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
        },
        Err(why) => {
            // Application code returned an error. This should not happen, as the implementation should
            // return a valid response.
            return api_impl.as_ref().handle_error(&method, &host, &cookies, why).await;
        }
    };

    resp.map_err(|e| {
        error!(error = ?e);
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

#[tracing::instrument(skip_all)]
fn get_publish_validation(path_params: models::GetPublishPathParams) -> std::result::Result<(models::GetPublishPathParams,), ValidationErrors> {
    path_params.validate()?;

    Ok((path_params,))
}
/// GetPublish - GET /api/v1/publishes/{publishId}
#[tracing::instrument(skip_all)]
async fn get_publish<I, A, E>(
    method: Method,
    TypedHeader(host): TypedHeader<Host>,
    cookies: CookieJar,
    Path(path_params): Path<models::GetPublishPathParams>,
    State(api_impl): State<I>,
) -> Result<Response, StatusCode>
where
    I: AsRef<A> + Send + Sync,
    A: apis::publishes::Publishes<E> + Send + Sync,
    E: std::fmt::Debug + Send + Sync + 'static,
{
    #[allow(clippy::redundant_closure)]
    let validation = tokio::task::spawn_blocking(move || get_publish_validation(path_params)).await.unwrap();

    let Ok((path_params,)) = validation else {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from(validation.unwrap_err().to_string()))
            .map_err(|_| StatusCode::BAD_REQUEST);
    };

    let result = api_impl.as_ref().get_publish(&method, &host, &cookies, &path_params).await;

    let mut response = Response::builder();

    let resp = match result {
        Ok(rsp) => match rsp {
            apis::publishes::GetPublishResponse::Status200_PublishState(body) => {
                let mut response = response.status(200);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
            apis::publishes::GetPublishResponse::Status404_TheResourceWasNotFound(body) => {
                let mut response = response.status(404);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
            apis::publishes::GetPublishResponse::Status500_InternalServerError(body) => {
                let mut response = response.status(500);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
        },
        Err(why) => {
            // Application code returned an error. This should not happen, as the implementation should
            // return a valid response.
            return api_impl.as_ref().handle_error(&method, &host, &cookies, why).await;
        }
    };

    resp.map_err(|e| {
        error!(error = ?e);
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

#[tracing::instrument(skip_all)]
fn list_publishes_validation(query_params: models::ListPublishesQueryParams) -> std::result::Result<(models::ListPublishesQueryParams,), ValidationErrors> {
    query_params.validate()?;

    Ok((query_params,))
}
/// ListPublishes - GET /api/v1/publishes
#[tracing::instrument(skip_all)]
async fn list_publishes<I, A, E>(
    method: Method,
    TypedHeader(host): TypedHeader<Host>,
    cookies: CookieJar,
    QueryExtra(query_params): QueryExtra<models::ListPublishesQueryParams>,
    State(api_impl): State<I>,
) -> Result<Response, StatusCode>
where
    I: AsRef<A> + Send + Sync,
    A: apis::publishes::Publishes<E> + Send + Sync,
    E: std::fmt::Debug + Send + Sync + 'static,
{
    #[allow(clippy::redundant_closure)]
    let validation = tokio::task::spawn_blocking(move || list_publishes_validation(query_params)).await.unwrap();

    let Ok((query_params,)) = validation else {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from(validation.unwrap_err().to_string()))
            .map_err(|_| StatusCode::BAD_REQUEST);
    };

    let result = api_impl.as_ref().list_publishes(&method, &host, &cookies, &query_params).await;

    let mut response = Response::builder();

    let resp = match result {
        Ok(rsp) => match rsp {
            apis::publishes::ListPublishesResponse::Status200_KnownPublishes(body) => {
                let mut response = response.status(200);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
            apis::publishes::ListPublishesResponse::Status500_InternalServerError(body) => {
                let mut response = response.status(500);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
        },
        Err(why) => {
            // Application code returned an error. This should not happen, as the implementation should
            // return a valid response.
            return api_impl.as_ref().handle_error(&method, &host, &cookies, why).await;
        }
    };

    resp.map_err(|e| {
        error!(error = ?e);
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

#[tracing::instrument(skip_all)]
fn remove_publish_validation(path_params: models::RemovePublishPathParams) -> std::result::Result<(models::RemovePublishPathParams,), ValidationErrors> {
    path_params.validate()?;

    Ok((path_params,))
}
/// RemovePublish - DELETE /api/v1/publishes/{publishId}
#[tracing::instrument(skip_all)]
async fn remove_publish<I, A, E>(
    method: Method,
    TypedHeader(host): TypedHeader<Host>,
    cookies: CookieJar,
    Path(path_params): Path<models::RemovePublishPathParams>,
    State(api_impl): State<I>,
) -> Result<Response, StatusCode>
where
    I: AsRef<A> + Send + Sync,
    A: apis::publishes::Publishes<E> + Send + Sync,
    E: std::fmt::Debug + Send + Sync + 'static,
{
    #[allow(clippy::redundant_closure)]
    let validation = tokio::task::spawn_blocking(move || remove_publish_validation(path_params)).await.unwrap();

    let Ok((path_params,)) = validation else {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from(validation.unwrap_err().to_string()))
            .map_err(|_| StatusCode::BAD_REQUEST);
    };

    let result = api_impl.as_ref().remove_publish(&method, &host, &cookies, &path_params).await;

    let mut response = Response::builder();

    let resp = match result {
        Ok(rsp) => match rsp {
            apis::publishes::RemovePublishResponse::Status204_PublishRemoved => {
                let mut response = response.status(204);
                response.body(Body::empty())
            }
            apis::publishes::RemovePublishResponse::Status404_TheResourceWasNotFound(body) => {
                let mut response = response.status(404);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
            apis::publishes::RemovePublishResponse::Status409_TheRequestConflictsWithTheCurrentState(body) => {
                let mut response = response.status(409);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
            apis::publishes::RemovePublishResponse::Status500_InternalServerError(body) => {
                let mut response = response.status(500);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
        },
        Err(why) => {
            // Application code returned an error. This should not happen, as the implementation should
            // return a valid response.
            return api_impl.as_ref().handle_error(&method, &host, &cookies, why).await;
        }
    };

    resp.map_err(|e| {
        error!(error = ?e);
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

#[derive(validator::Validate)]
#[allow(dead_code)]
struct WritePublishContentBodyValidator<'a> {
    body: &'a [u8],
}

#[tracing::instrument(skip_all)]
fn write_publish_content_validation(
    path_params: models::WritePublishContentPathParams,
    query_params: models::WritePublishContentQueryParams,
    body: Bytes,
) -> std::result::Result<(models::WritePublishContentPathParams, models::WritePublishContentQueryParams, Bytes), ValidationErrors> {
    path_params.validate()?;
    query_params.validate()?;

    Ok((path_params, query_params, body))
}
/// WritePublishContent - PUT /api/v1/publishes/{publishId}/content
#[tracing::instrument(skip_all)]
async fn write_publish_content<I, A, E>(
    method: Method,
    TypedHeader(host): TypedHeader<Host>,
    cookies: CookieJar,
    Path(path_params): Path<models::WritePublishContentPathParams>,
    QueryExtra(query_params): QueryExtra<models::WritePublishContentQueryParams>,
    State(api_impl): State<I>,
    body: Bytes,
) -> Result<Response, StatusCode>
where
    I: AsRef<A> + Send + Sync,
    A: apis::publishes::Publishes<E> + Send + Sync,
    E: std::fmt::Debug + Send + Sync + 'static,
{
    #[allow(clippy::redundant_closure)]
    let validation = tokio::task::spawn_blocking(move || write_publish_content_validation(path_params, query_params, body))
        .await
        .unwrap();

    let Ok((path_params, query_params, body)) = validation else {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from(validation.unwrap_err().to_string()))
            .map_err(|_| StatusCode::BAD_REQUEST);
    };

    let result = api_impl.as_ref().write_publish_content(&method, &host, &cookies, &path_params, &query_params, &body).await;

    let mut response = Response::builder();

    let resp = match result {
        Ok(rsp) => match rsp {
            apis::publishes::WritePublishContentResponse::Status204_ChunkAccepted => {
                let mut response = response.status(204);
                response.body(Body::empty())
            }
            apis::publishes::WritePublishContentResponse::Status400_TheRequestIsInvalid(body) => {
                let mut response = response.status(400);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
            apis::publishes::WritePublishContentResponse::Status404_TheResourceWasNotFound(body) => {
                let mut response = response.status(404);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
            apis::publishes::WritePublishContentResponse::Status409_TheRequestConflictsWithTheCurrentState(body) => {
                let mut response = response.status(409);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
            apis::publishes::WritePublishContentResponse::Status413_TheDeclaredUploadSizeExceedsTheConfiguredLimit(body) => {
                let mut response = response.status(413);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
            apis::publishes::WritePublishContentResponse::Status500_InternalServerError(body) => {
                let mut response = response.status(500);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
            apis::publishes::WritePublishContentResponse::Status507_TheDaemonDoesNotHaveEnoughSpaceForTheUpload(body) => {
                let mut response = response.status(507);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
        },
        Err(why) => {
            // Application code returned an error. This should not happen, as the implementation should
            // return a valid response.
            return api_impl.as_ref().handle_error(&method, &host, &cookies, why).await;
        }
    };

    resp.map_err(|e| {
        error!(error = ?e);
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

#[tracing::instrument(skip_all)]
fn cancel_subscribe_validation(path_params: models::CancelSubscribePathParams) -> std::result::Result<(models::CancelSubscribePathParams,), ValidationErrors> {
    path_params.validate()?;

    Ok((path_params,))
}
/// CancelSubscribe - POST /api/v1/subscribes/{subscribeId}/cancel
#[tracing::instrument(skip_all)]
async fn cancel_subscribe<I, A, E>(
    method: Method,
    TypedHeader(host): TypedHeader<Host>,
    cookies: CookieJar,
    Path(path_params): Path<models::CancelSubscribePathParams>,
    State(api_impl): State<I>,
) -> Result<Response, StatusCode>
where
    I: AsRef<A> + Send + Sync,
    A: apis::subscribes::Subscribes<E> + Send + Sync,
    E: std::fmt::Debug + Send + Sync + 'static,
{
    #[allow(clippy::redundant_closure)]
    let validation = tokio::task::spawn_blocking(move || cancel_subscribe_validation(path_params)).await.unwrap();

    let Ok((path_params,)) = validation else {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from(validation.unwrap_err().to_string()))
            .map_err(|_| StatusCode::BAD_REQUEST);
    };

    let result = api_impl.as_ref().cancel_subscribe(&method, &host, &cookies, &path_params).await;

    let mut response = Response::builder();

    let resp = match result {
        Ok(rsp) => match rsp {
            apis::subscribes::CancelSubscribeResponse::Status202_CancellationRequested(body) => {
                let mut response = response.status(202);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
            apis::subscribes::CancelSubscribeResponse::Status404_TheResourceWasNotFound(body) => {
                let mut response = response.status(404);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
            apis::subscribes::CancelSubscribeResponse::Status409_TheRequestConflictsWithTheCurrentState(body) => {
                let mut response = response.status(409);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
            apis::subscribes::CancelSubscribeResponse::Status500_InternalServerError(body) => {
                let mut response = response.status(500);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
        },
        Err(why) => {
            // Application code returned an error. This should not happen, as the implementation should
            // return a valid response.
            return api_impl.as_ref().handle_error(&method, &host, &cookies, why).await;
        }
    };

    resp.map_err(|e| {
        error!(error = ?e);
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

#[derive(validator::Validate)]
#[allow(dead_code)]
struct CreateSubscribeBodyValidator<'a> {
    #[validate(nested)]
    body: &'a models::SubscribeCreateRequest,
}

#[tracing::instrument(skip_all)]
fn create_subscribe_validation(body: models::SubscribeCreateRequest) -> std::result::Result<(models::SubscribeCreateRequest,), ValidationErrors> {
    let b = CreateSubscribeBodyValidator { body: &body };
    b.validate()?;

    Ok((body,))
}
/// CreateSubscribe - POST /api/v1/subscribes
#[tracing::instrument(skip_all)]
async fn create_subscribe<I, A, E>(
    method: Method,
    TypedHeader(host): TypedHeader<Host>,
    cookies: CookieJar,
    State(api_impl): State<I>,
    Json(body): Json<models::SubscribeCreateRequest>,
) -> Result<Response, StatusCode>
where
    I: AsRef<A> + Send + Sync,
    A: apis::subscribes::Subscribes<E> + Send + Sync,
    E: std::fmt::Debug + Send + Sync + 'static,
{
    #[allow(clippy::redundant_closure)]
    let validation = tokio::task::spawn_blocking(move || create_subscribe_validation(body)).await.unwrap();

    let Ok((body,)) = validation else {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from(validation.unwrap_err().to_string()))
            .map_err(|_| StatusCode::BAD_REQUEST);
    };

    let result = api_impl.as_ref().create_subscribe(&method, &host, &cookies, &body).await;

    let mut response = Response::builder();

    let resp = match result {
        Ok(rsp) => match rsp {
            apis::subscribes::CreateSubscribeResponse::Status202_SubscriptionStarted(body) => {
                let mut response = response.status(202);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
            apis::subscribes::CreateSubscribeResponse::Status400_TheRequestIsInvalid(body) => {
                let mut response = response.status(400);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
            apis::subscribes::CreateSubscribeResponse::Status404_TheResourceWasNotFound(body) => {
                let mut response = response.status(404);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
            apis::subscribes::CreateSubscribeResponse::Status409_TheRequestConflictsWithTheCurrentState(body) => {
                let mut response = response.status(409);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
            apis::subscribes::CreateSubscribeResponse::Status500_InternalServerError(body) => {
                let mut response = response.status(500);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
        },
        Err(why) => {
            // Application code returned an error. This should not happen, as the implementation should
            // return a valid response.
            return api_impl.as_ref().handle_error(&method, &host, &cookies, why).await;
        }
    };

    resp.map_err(|e| {
        error!(error = ?e);
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

#[tracing::instrument(skip_all)]
fn get_subscribe_validation(path_params: models::GetSubscribePathParams) -> std::result::Result<(models::GetSubscribePathParams,), ValidationErrors> {
    path_params.validate()?;

    Ok((path_params,))
}
/// GetSubscribe - GET /api/v1/subscribes/{subscribeId}
#[tracing::instrument(skip_all)]
async fn get_subscribe<I, A, E>(
    method: Method,
    TypedHeader(host): TypedHeader<Host>,
    cookies: CookieJar,
    Path(path_params): Path<models::GetSubscribePathParams>,
    State(api_impl): State<I>,
) -> Result<Response, StatusCode>
where
    I: AsRef<A> + Send + Sync,
    A: apis::subscribes::Subscribes<E> + Send + Sync,
    E: std::fmt::Debug + Send + Sync + 'static,
{
    #[allow(clippy::redundant_closure)]
    let validation = tokio::task::spawn_blocking(move || get_subscribe_validation(path_params)).await.unwrap();

    let Ok((path_params,)) = validation else {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from(validation.unwrap_err().to_string()))
            .map_err(|_| StatusCode::BAD_REQUEST);
    };

    let result = api_impl.as_ref().get_subscribe(&method, &host, &cookies, &path_params).await;

    let mut response = Response::builder();

    let resp = match result {
        Ok(rsp) => match rsp {
            apis::subscribes::GetSubscribeResponse::Status200_SubscriptionState(body) => {
                let mut response = response.status(200);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
            apis::subscribes::GetSubscribeResponse::Status404_TheResourceWasNotFound(body) => {
                let mut response = response.status(404);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
            apis::subscribes::GetSubscribeResponse::Status500_InternalServerError(body) => {
                let mut response = response.status(500);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
        },
        Err(why) => {
            // Application code returned an error. This should not happen, as the implementation should
            // return a valid response.
            return api_impl.as_ref().handle_error(&method, &host, &cookies, why).await;
        }
    };

    resp.map_err(|e| {
        error!(error = ?e);
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

#[tracing::instrument(skip_all)]
fn list_subscribes_validation(query_params: models::ListSubscribesQueryParams) -> std::result::Result<(models::ListSubscribesQueryParams,), ValidationErrors> {
    query_params.validate()?;

    Ok((query_params,))
}
/// ListSubscribes - GET /api/v1/subscribes
#[tracing::instrument(skip_all)]
async fn list_subscribes<I, A, E>(
    method: Method,
    TypedHeader(host): TypedHeader<Host>,
    cookies: CookieJar,
    QueryExtra(query_params): QueryExtra<models::ListSubscribesQueryParams>,
    State(api_impl): State<I>,
) -> Result<Response, StatusCode>
where
    I: AsRef<A> + Send + Sync,
    A: apis::subscribes::Subscribes<E> + Send + Sync,
    E: std::fmt::Debug + Send + Sync + 'static,
{
    #[allow(clippy::redundant_closure)]
    let validation = tokio::task::spawn_blocking(move || list_subscribes_validation(query_params)).await.unwrap();

    let Ok((query_params,)) = validation else {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from(validation.unwrap_err().to_string()))
            .map_err(|_| StatusCode::BAD_REQUEST);
    };

    let result = api_impl.as_ref().list_subscribes(&method, &host, &cookies, &query_params).await;

    let mut response = Response::builder();

    let resp = match result {
        Ok(rsp) => match rsp {
            apis::subscribes::ListSubscribesResponse::Status200_KnownSubscriptions(body) => {
                let mut response = response.status(200);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
            apis::subscribes::ListSubscribesResponse::Status500_InternalServerError(body) => {
                let mut response = response.status(500);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
        },
        Err(why) => {
            // Application code returned an error. This should not happen, as the implementation should
            // return a valid response.
            return api_impl.as_ref().handle_error(&method, &host, &cookies, why).await;
        }
    };

    resp.map_err(|e| {
        error!(error = ?e);
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

#[tracing::instrument(skip_all)]
fn read_subscribe_content_validation(
    header_params: models::ReadSubscribeContentHeaderParams,
    path_params: models::ReadSubscribeContentPathParams,
) -> std::result::Result<(models::ReadSubscribeContentHeaderParams, models::ReadSubscribeContentPathParams), ValidationErrors> {
    header_params.validate()?;
    path_params.validate()?;

    Ok((header_params, path_params))
}
/// ReadSubscribeContent - GET /api/v1/subscribes/{subscribeId}/content
#[tracing::instrument(skip_all)]
async fn read_subscribe_content<I, A, E>(
    method: Method,
    TypedHeader(host): TypedHeader<Host>,
    cookies: CookieJar,
    headers: HeaderMap,
    Path(path_params): Path<models::ReadSubscribeContentPathParams>,
    State(api_impl): State<I>,
) -> Result<Response, StatusCode>
where
    I: AsRef<A> + Send + Sync,
    A: apis::subscribes::Subscribes<E> + Send + Sync,
    E: std::fmt::Debug + Send + Sync + 'static,
{
    // Header parameters
    let header_params = {
        let header_range = headers.get(HeaderName::from_static("range"));

        let header_range = match header_range {
            Some(v) => match header::IntoHeaderValue::<String>::try_from((*v).clone()) {
                Ok(result) => Some(result.0),
                Err(err) => {
                    return Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .body(Body::from(format!("Invalid header Range - {err}")))
                        .map_err(|e| {
                            error!(error = ?e);
                            StatusCode::INTERNAL_SERVER_ERROR
                        });
                }
            },
            None => None,
        };
        let header_if_none_match = headers.get(HeaderName::from_static("if-none-match"));

        let header_if_none_match = match header_if_none_match {
            Some(v) => match header::IntoHeaderValue::<String>::try_from((*v).clone()) {
                Ok(result) => Some(result.0),
                Err(err) => {
                    return Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .body(Body::from(format!("Invalid header If-None-Match - {err}")))
                        .map_err(|e| {
                            error!(error = ?e);
                            StatusCode::INTERNAL_SERVER_ERROR
                        });
                }
            },
            None => None,
        };

        models::ReadSubscribeContentHeaderParams {
            range: header_range,
            if_none_match: header_if_none_match,
        }
    };

    #[allow(clippy::redundant_closure)]
    let validation = tokio::task::spawn_blocking(move || read_subscribe_content_validation(header_params, path_params))
        .await
        .unwrap();

    let Ok((header_params, path_params)) = validation else {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from(validation.unwrap_err().to_string()))
            .map_err(|_| StatusCode::BAD_REQUEST);
    };

    let result = api_impl.as_ref().read_subscribe_content(&method, &host, &cookies, &header_params, &path_params).await;

    let mut response = Response::builder();

    let resp = match result {
        Ok(rsp) => match rsp {
            apis::subscribes::ReadSubscribeContentResponse::Status200_CompleteOutput {
                body,
                accept_ranges,
                content_range,
                e_tag,
            } => {
                if let Some(accept_ranges) = accept_ranges {
                    let accept_ranges = match header::IntoHeaderValue(accept_ranges).try_into() {
                        Ok(val) => val,
                        Err(e) => {
                            return Response::builder()
                                .status(StatusCode::INTERNAL_SERVER_ERROR)
                                .body(Body::from(format!("An internal server error occurred handling accept_ranges header - {e}")))
                                .map_err(|e| {
                                    error!(error = ?e);
                                    StatusCode::INTERNAL_SERVER_ERROR
                                });
                        }
                    };

                    {
                        let mut response_headers = response.headers_mut().unwrap();
                        response_headers.insert(HeaderName::from_static("accept-ranges"), accept_ranges);
                    }
                }
                if let Some(content_range) = content_range {
                    let content_range = match header::IntoHeaderValue(content_range).try_into() {
                        Ok(val) => val,
                        Err(e) => {
                            return Response::builder()
                                .status(StatusCode::INTERNAL_SERVER_ERROR)
                                .body(Body::from(format!("An internal server error occurred handling content_range header - {e}")))
                                .map_err(|e| {
                                    error!(error = ?e);
                                    StatusCode::INTERNAL_SERVER_ERROR
                                });
                        }
                    };

                    {
                        let mut response_headers = response.headers_mut().unwrap();
                        response_headers.insert(HeaderName::from_static("content-range"), content_range);
                    }
                }
                if let Some(e_tag) = e_tag {
                    let e_tag = match header::IntoHeaderValue(e_tag).try_into() {
                        Ok(val) => val,
                        Err(e) => {
                            return Response::builder()
                                .status(StatusCode::INTERNAL_SERVER_ERROR)
                                .body(Body::from(format!("An internal server error occurred handling e_tag header - {e}")))
                                .map_err(|e| {
                                    error!(error = ?e);
                                    StatusCode::INTERNAL_SERVER_ERROR
                                });
                        }
                    };

                    {
                        let mut response_headers = response.headers_mut().unwrap();
                        response_headers.insert(HeaderName::from_static("etag"), e_tag);
                    }
                }
                let mut response = response.status(200);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/octet-stream"));
                }

                let body_content = body.0;
                response.body(Body::from(body_content))
            }
            apis::subscribes::ReadSubscribeContentResponse::Status206_PartialOutput {
                body,
                accept_ranges,
                content_range,
                e_tag,
            } => {
                if let Some(accept_ranges) = accept_ranges {
                    let accept_ranges = match header::IntoHeaderValue(accept_ranges).try_into() {
                        Ok(val) => val,
                        Err(e) => {
                            return Response::builder()
                                .status(StatusCode::INTERNAL_SERVER_ERROR)
                                .body(Body::from(format!("An internal server error occurred handling accept_ranges header - {e}")))
                                .map_err(|e| {
                                    error!(error = ?e);
                                    StatusCode::INTERNAL_SERVER_ERROR
                                });
                        }
                    };

                    {
                        let mut response_headers = response.headers_mut().unwrap();
                        response_headers.insert(HeaderName::from_static("accept-ranges"), accept_ranges);
                    }
                }
                if let Some(content_range) = content_range {
                    let content_range = match header::IntoHeaderValue(content_range).try_into() {
                        Ok(val) => val,
                        Err(e) => {
                            return Response::builder()
                                .status(StatusCode::INTERNAL_SERVER_ERROR)
                                .body(Body::from(format!("An internal server error occurred handling content_range header - {e}")))
                                .map_err(|e| {
                                    error!(error = ?e);
                                    StatusCode::INTERNAL_SERVER_ERROR
                                });
                        }
                    };

                    {
                        let mut response_headers = response.headers_mut().unwrap();
                        response_headers.insert(HeaderName::from_static("content-range"), content_range);
                    }
                }
                if let Some(e_tag) = e_tag {
                    let e_tag = match header::IntoHeaderValue(e_tag).try_into() {
                        Ok(val) => val,
                        Err(e) => {
                            return Response::builder()
                                .status(StatusCode::INTERNAL_SERVER_ERROR)
                                .body(Body::from(format!("An internal server error occurred handling e_tag header - {e}")))
                                .map_err(|e| {
                                    error!(error = ?e);
                                    StatusCode::INTERNAL_SERVER_ERROR
                                });
                        }
                    };

                    {
                        let mut response_headers = response.headers_mut().unwrap();
                        response_headers.insert(HeaderName::from_static("etag"), e_tag);
                    }
                }
                let mut response = response.status(206);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/octet-stream"));
                }

                let body_content = body.0;
                response.body(Body::from(body_content))
            }
            apis::subscribes::ReadSubscribeContentResponse::Status304_OutputNotModified => {
                let mut response = response.status(304);
                response.body(Body::empty())
            }
            apis::subscribes::ReadSubscribeContentResponse::Status404_TheResourceWasNotFound(body) => {
                let mut response = response.status(404);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
            apis::subscribes::ReadSubscribeContentResponse::Status409_TheRequestConflictsWithTheCurrentState(body) => {
                let mut response = response.status(409);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
            apis::subscribes::ReadSubscribeContentResponse::Status410_TheResourceIsNoLongerAvailable(body) => {
                let mut response = response.status(410);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
            apis::subscribes::ReadSubscribeContentResponse::Status416_TheRequestedByteRangeCannotBeSatisfied(body) => {
                let mut response = response.status(416);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
            apis::subscribes::ReadSubscribeContentResponse::Status500_InternalServerError(body) => {
                let mut response = response.status(500);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
        },
        Err(why) => {
            // Application code returned an error. This should not happen, as the implementation should
            // return a valid response.
            return api_impl.as_ref().handle_error(&method, &host, &cookies, why).await;
        }
    };

    resp.map_err(|e| {
        error!(error = ?e);
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

#[tracing::instrument(skip_all)]
fn remove_subscribe_validation(path_params: models::RemoveSubscribePathParams) -> std::result::Result<(models::RemoveSubscribePathParams,), ValidationErrors> {
    path_params.validate()?;

    Ok((path_params,))
}
/// RemoveSubscribe - DELETE /api/v1/subscribes/{subscribeId}
#[tracing::instrument(skip_all)]
async fn remove_subscribe<I, A, E>(
    method: Method,
    TypedHeader(host): TypedHeader<Host>,
    cookies: CookieJar,
    Path(path_params): Path<models::RemoveSubscribePathParams>,
    State(api_impl): State<I>,
) -> Result<Response, StatusCode>
where
    I: AsRef<A> + Send + Sync,
    A: apis::subscribes::Subscribes<E> + Send + Sync,
    E: std::fmt::Debug + Send + Sync + 'static,
{
    #[allow(clippy::redundant_closure)]
    let validation = tokio::task::spawn_blocking(move || remove_subscribe_validation(path_params)).await.unwrap();

    let Ok((path_params,)) = validation else {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from(validation.unwrap_err().to_string()))
            .map_err(|_| StatusCode::BAD_REQUEST);
    };

    let result = api_impl.as_ref().remove_subscribe(&method, &host, &cookies, &path_params).await;

    let mut response = Response::builder();

    let resp = match result {
        Ok(rsp) => match rsp {
            apis::subscribes::RemoveSubscribeResponse::Status204_SubscriptionRemoved => {
                let mut response = response.status(204);
                response.body(Body::empty())
            }
            apis::subscribes::RemoveSubscribeResponse::Status404_TheResourceWasNotFound(body) => {
                let mut response = response.status(404);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
            apis::subscribes::RemoveSubscribeResponse::Status409_TheRequestConflictsWithTheCurrentState(body) => {
                let mut response = response.status(409);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
            apis::subscribes::RemoveSubscribeResponse::Status500_InternalServerError(body) => {
                let mut response = response.status(500);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let body_content = tokio::task::spawn_blocking(move || {
                    serde_json::to_vec(&body).map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
                })
                .await
                .unwrap()?;
                response.body(Body::from(body_content))
            }
        },
        Err(why) => {
            // Application code returned an error. This should not happen, as the implementation should
            // return a valid response.
            return api_impl.as_ref().handle_error(&method, &host, &cookies, why).await;
        }
    };

    resp.map_err(|e| {
        error!(error = ?e);
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

#[allow(dead_code)]
#[inline]
fn response_with_status_code_only(code: StatusCode) -> Result<Response, StatusCode> {
    Response::builder().status(code).body(Body::empty()).map_err(|_| code)
}
