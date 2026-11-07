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
    A: apis::authentication::Authentication<E>
        + apis::health::Health<E>
        + apis::library::Library<E>
        + apis::publications::Publications<E>
        + apis::subscriptions::Subscriptions<E>
        + Send
        + Sync
        + 'static,
    E: std::fmt::Debug + Send + Sync + 'static,
{
    // build our application with a route
    Router::new()
        .route("/api/v1/auth/session", delete(delete_session::<I, A, E>).post(create_session::<I, A, E>))
        .route("/api/v1/health", get(get_health::<I, A, E>))
        .route("/api/v1/library/roots", get(list_library_roots::<I, A, E>))
        .route("/api/v1/library/roots/{root_id}/entries", get(list_library_entries::<I, A, E>))
        .route("/api/v1/publications", get(list_publications::<I, A, E>).post(create_publication::<I, A, E>))
        .route(
            "/api/v1/publications/{publication_id}",
            delete(delete_publication::<I, A, E>).get(get_publication::<I, A, E>),
        )
        .route("/api/v1/publications/{publication_id}/cancel", post(cancel_publication::<I, A, E>))
        .route("/api/v1/publications/{publication_id}/content", put(write_publication_content::<I, A, E>))
        .route("/api/v1/setup", post(create_owner::<I, A, E>))
        .route("/api/v1/setup/status", get(get_setup_status::<I, A, E>))
        .route("/api/v1/subscriptions", get(list_subscriptions::<I, A, E>).post(create_subscription::<I, A, E>))
        .route(
            "/api/v1/subscriptions/{subscription_id}",
            delete(delete_subscription::<I, A, E>).get(get_subscription::<I, A, E>),
        )
        .route("/api/v1/subscriptions/{subscription_id}/cancel", post(cancel_subscription::<I, A, E>))
        .route("/api/v1/subscriptions/{subscription_id}/content", get(get_subscription_content::<I, A, E>))
        .with_state(api_impl)
}

#[derive(validator::Validate)]
#[allow(dead_code)]
struct CreateOwnerBodyValidator<'a> {
    #[validate(nested)]
    body: &'a models::Credentials,
}

#[tracing::instrument(skip_all)]
fn create_owner_validation(body: models::Credentials) -> std::result::Result<(models::Credentials,), ValidationErrors> {
    let b = CreateOwnerBodyValidator { body: &body };
    b.validate()?;

    Ok((body,))
}
/// CreateOwner - POST /api/v1/setup
#[tracing::instrument(skip_all)]
async fn create_owner<I, A, E>(
    method: Method,
    TypedHeader(host): TypedHeader<Host>,
    cookies: CookieJar,
    State(api_impl): State<I>,
    Json(body): Json<models::Credentials>,
) -> Result<Response, StatusCode>
where
    I: AsRef<A> + Send + Sync,
    A: apis::authentication::Authentication<E> + Send + Sync,
    E: std::fmt::Debug + Send + Sync + 'static,
{
    #[allow(clippy::redundant_closure)]
    let validation = tokio::task::spawn_blocking(move || create_owner_validation(body)).await.unwrap();

    let Ok((body,)) = validation else {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from(validation.unwrap_err().to_string()))
            .map_err(|_| StatusCode::BAD_REQUEST);
    };

    let result = api_impl.as_ref().create_owner(&method, &host, &cookies, &body).await;

    let mut response = Response::builder();

    let resp = match result {
        Ok(rsp) => match rsp {
            apis::authentication::CreateOwnerResponse::Status204_TheOwnerIsRegistered => {
                let mut response = response.status(204);
                response.body(Body::empty())
            }
            apis::authentication::CreateOwnerResponse::Status400_TheRequestIsMalformed(body) => {
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
            apis::authentication::CreateOwnerResponse::Status409_TheRequestConflictsWithTheCurrentState(body) => {
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
            apis::authentication::CreateOwnerResponse::Status500_InternalServerError(body) => {
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
struct CreateSessionBodyValidator<'a> {
    #[validate(nested)]
    body: &'a models::Credentials,
}

#[tracing::instrument(skip_all)]
fn create_session_validation(
    header_params: models::CreateSessionHeaderParams,
    body: models::Credentials,
) -> std::result::Result<(models::CreateSessionHeaderParams, models::Credentials), ValidationErrors> {
    header_params.validate()?;
    let b = CreateSessionBodyValidator { body: &body };
    b.validate()?;

    Ok((header_params, body))
}
/// CreateSession - POST /api/v1/auth/session
#[tracing::instrument(skip_all)]
async fn create_session<I, A, E>(
    method: Method,
    TypedHeader(host): TypedHeader<Host>,
    cookies: CookieJar,
    headers: HeaderMap,
    State(api_impl): State<I>,
    Json(body): Json<models::Credentials>,
) -> Result<Response, StatusCode>
where
    I: AsRef<A> + Send + Sync,
    A: apis::authentication::Authentication<E> + Send + Sync,
    E: std::fmt::Debug + Send + Sync + 'static,
{
    // Header parameters
    let header_params = {
        let header_x_axus_transport = headers.get(HeaderName::from_static("x-axus-transport"));

        let header_x_axus_transport = match header_x_axus_transport {
            Some(v) => match header::IntoHeaderValue::<String>::try_from((*v).clone()) {
                Ok(result) => Some(result.0),
                Err(err) => {
                    return Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .body(Body::from(format!("Invalid header X-Axus-Transport - {err}")))
                        .map_err(|e| {
                            error!(error = ?e);
                            StatusCode::INTERNAL_SERVER_ERROR
                        });
                }
            },
            None => None,
        };

        models::CreateSessionHeaderParams {
            x_axus_transport: header_x_axus_transport,
        }
    };

    #[allow(clippy::redundant_closure)]
    let validation = tokio::task::spawn_blocking(move || create_session_validation(header_params, body)).await.unwrap();

    let Ok((header_params, body)) = validation else {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from(validation.unwrap_err().to_string()))
            .map_err(|_| StatusCode::BAD_REQUEST);
    };

    let result = api_impl.as_ref().create_session(&method, &host, &cookies, &header_params, &body).await;

    let mut response = Response::builder();

    let resp = match result {
        Ok(rsp) => match rsp {
            apis::authentication::CreateSessionResponse::Status204_TheSessionCookieIsSet => {
                let mut response = response.status(204);
                response.body(Body::empty())
            }
            apis::authentication::CreateSessionResponse::Status200_TheSessionTokenForABearerClient(body) => {
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
            apis::authentication::CreateSessionResponse::Status401_ASessionIsRequired(body) => {
                let mut response = response.status(401);
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
            apis::authentication::CreateSessionResponse::Status500_InternalServerError(body) => {
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
fn delete_session_validation() -> std::result::Result<(), ValidationErrors> {
    Ok(())
}
/// DeleteSession - DELETE /api/v1/auth/session
#[tracing::instrument(skip_all)]
async fn delete_session<I, A, E>(method: Method, TypedHeader(host): TypedHeader<Host>, cookies: CookieJar, State(api_impl): State<I>) -> Result<Response, StatusCode>
where
    I: AsRef<A> + Send + Sync,
    A: apis::authentication::Authentication<E> + Send + Sync,
    E: std::fmt::Debug + Send + Sync + 'static,
{
    #[allow(clippy::redundant_closure)]
    let validation = tokio::task::spawn_blocking(move || delete_session_validation()).await.unwrap();

    let Ok(()) = validation else {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from(validation.unwrap_err().to_string()))
            .map_err(|_| StatusCode::BAD_REQUEST);
    };

    let result = api_impl.as_ref().delete_session(&method, &host, &cookies).await;

    let mut response = Response::builder();

    let resp = match result {
        Ok(rsp) => match rsp {
            apis::authentication::DeleteSessionResponse::Status204_TheSessionIsGone => {
                let mut response = response.status(204);
                response.body(Body::empty())
            }
            apis::authentication::DeleteSessionResponse::Status401_ASessionIsRequired(body) => {
                let mut response = response.status(401);
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
            apis::authentication::DeleteSessionResponse::Status500_InternalServerError(body) => {
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
fn get_setup_status_validation() -> std::result::Result<(), ValidationErrors> {
    Ok(())
}
/// GetSetupStatus - GET /api/v1/setup/status
#[tracing::instrument(skip_all)]
async fn get_setup_status<I, A, E>(method: Method, TypedHeader(host): TypedHeader<Host>, cookies: CookieJar, State(api_impl): State<I>) -> Result<Response, StatusCode>
where
    I: AsRef<A> + Send + Sync,
    A: apis::authentication::Authentication<E> + Send + Sync,
    E: std::fmt::Debug + Send + Sync + 'static,
{
    #[allow(clippy::redundant_closure)]
    let validation = tokio::task::spawn_blocking(move || get_setup_status_validation()).await.unwrap();

    let Ok(()) = validation else {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from(validation.unwrap_err().to_string()))
            .map_err(|_| StatusCode::BAD_REQUEST);
    };

    let result = api_impl.as_ref().get_setup_status(&method, &host, &cookies).await;

    let mut response = Response::builder();

    let resp = match result {
        Ok(rsp) => match rsp {
            apis::authentication::GetSetupStatusResponse::Status200_SetupState(body) => {
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
            apis::authentication::GetSetupStatusResponse::Status500_InternalServerError(body) => {
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
fn list_library_entries_validation(
    path_params: models::ListLibraryEntriesPathParams,
    query_params: models::ListLibraryEntriesQueryParams,
) -> std::result::Result<(models::ListLibraryEntriesPathParams, models::ListLibraryEntriesQueryParams), ValidationErrors> {
    path_params.validate()?;
    query_params.validate()?;

    Ok((path_params, query_params))
}
/// ListLibraryEntries - GET /api/v1/library/roots/{rootId}/entries
#[tracing::instrument(skip_all)]
async fn list_library_entries<I, A, E>(
    method: Method,
    TypedHeader(host): TypedHeader<Host>,
    cookies: CookieJar,
    Path(path_params): Path<models::ListLibraryEntriesPathParams>,
    QueryExtra(query_params): QueryExtra<models::ListLibraryEntriesQueryParams>,
    State(api_impl): State<I>,
) -> Result<Response, StatusCode>
where
    I: AsRef<A> + Send + Sync,
    A: apis::library::Library<E> + Send + Sync,
    E: std::fmt::Debug + Send + Sync + 'static,
{
    #[allow(clippy::redundant_closure)]
    let validation = tokio::task::spawn_blocking(move || list_library_entries_validation(path_params, query_params))
        .await
        .unwrap();

    let Ok((path_params, query_params)) = validation else {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from(validation.unwrap_err().to_string()))
            .map_err(|_| StatusCode::BAD_REQUEST);
    };

    let result = api_impl.as_ref().list_library_entries(&method, &host, &cookies, &path_params, &query_params).await;

    let mut response = Response::builder();

    let resp = match result {
        Ok(rsp) => match rsp {
            apis::library::ListLibraryEntriesResponse::Status200_EntriesOfTheDirectory(body) => {
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
            apis::library::ListLibraryEntriesResponse::Status400_TheRequestIsMalformed(body) => {
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
            apis::library::ListLibraryEntriesResponse::Status404_TheResourceDoesNotExist(body) => {
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
            apis::library::ListLibraryEntriesResponse::Status500_InternalServerError(body) => {
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
fn list_library_roots_validation() -> std::result::Result<(), ValidationErrors> {
    Ok(())
}
/// ListLibraryRoots - GET /api/v1/library/roots
#[tracing::instrument(skip_all)]
async fn list_library_roots<I, A, E>(method: Method, TypedHeader(host): TypedHeader<Host>, cookies: CookieJar, State(api_impl): State<I>) -> Result<Response, StatusCode>
where
    I: AsRef<A> + Send + Sync,
    A: apis::library::Library<E> + Send + Sync,
    E: std::fmt::Debug + Send + Sync + 'static,
{
    #[allow(clippy::redundant_closure)]
    let validation = tokio::task::spawn_blocking(move || list_library_roots_validation()).await.unwrap();

    let Ok(()) = validation else {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from(validation.unwrap_err().to_string()))
            .map_err(|_| StatusCode::BAD_REQUEST);
    };

    let result = api_impl.as_ref().list_library_roots(&method, &host, &cookies).await;

    let mut response = Response::builder();

    let resp = match result {
        Ok(rsp) => match rsp {
            apis::library::ListLibraryRootsResponse::Status200_LibraryRoots(body) => {
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
            apis::library::ListLibraryRootsResponse::Status500_InternalServerError(body) => {
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
fn cancel_publication_validation(path_params: models::CancelPublicationPathParams) -> std::result::Result<(models::CancelPublicationPathParams,), ValidationErrors> {
    path_params.validate()?;

    Ok((path_params,))
}
/// CancelPublication - POST /api/v1/publications/{publicationId}/cancel
#[tracing::instrument(skip_all)]
async fn cancel_publication<I, A, E>(
    method: Method,
    TypedHeader(host): TypedHeader<Host>,
    cookies: CookieJar,
    Path(path_params): Path<models::CancelPublicationPathParams>,
    State(api_impl): State<I>,
) -> Result<Response, StatusCode>
where
    I: AsRef<A> + Send + Sync,
    A: apis::publications::Publications<E> + Send + Sync,
    E: std::fmt::Debug + Send + Sync + 'static,
{
    #[allow(clippy::redundant_closure)]
    let validation = tokio::task::spawn_blocking(move || cancel_publication_validation(path_params)).await.unwrap();

    let Ok((path_params,)) = validation else {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from(validation.unwrap_err().to_string()))
            .map_err(|_| StatusCode::BAD_REQUEST);
    };

    let result = api_impl.as_ref().cancel_publication(&method, &host, &cookies, &path_params).await;

    let mut response = Response::builder();

    let resp = match result {
        Ok(rsp) => match rsp {
            apis::publications::CancelPublicationResponse::Status200_ThePublicationStateAfterTheRequest(body) => {
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
            apis::publications::CancelPublicationResponse::Status404_TheResourceDoesNotExist(body) => {
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
            apis::publications::CancelPublicationResponse::Status409_TheRequestConflictsWithTheCurrentState(body) => {
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
            apis::publications::CancelPublicationResponse::Status500_InternalServerError(body) => {
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
struct CreatePublicationBodyValidator<'a> {
    #[validate(nested)]
    body: &'a models::CreatePublicationRequest,
}

#[tracing::instrument(skip_all)]
fn create_publication_validation(body: models::CreatePublicationRequest) -> std::result::Result<(models::CreatePublicationRequest,), ValidationErrors> {
    let b = CreatePublicationBodyValidator { body: &body };
    b.validate()?;

    Ok((body,))
}
/// CreatePublication - POST /api/v1/publications
#[tracing::instrument(skip_all)]
async fn create_publication<I, A, E>(
    method: Method,
    TypedHeader(host): TypedHeader<Host>,
    cookies: CookieJar,
    State(api_impl): State<I>,
    Json(body): Json<models::CreatePublicationRequest>,
) -> Result<Response, StatusCode>
where
    I: AsRef<A> + Send + Sync,
    A: apis::publications::Publications<E> + Send + Sync,
    E: std::fmt::Debug + Send + Sync + 'static,
{
    #[allow(clippy::redundant_closure)]
    let validation = tokio::task::spawn_blocking(move || create_publication_validation(body)).await.unwrap();

    let Ok((body,)) = validation else {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from(validation.unwrap_err().to_string()))
            .map_err(|_| StatusCode::BAD_REQUEST);
    };

    let result = api_impl.as_ref().create_publication(&method, &host, &cookies, &body).await;

    let mut response = Response::builder();

    let resp = match result {
        Ok(rsp) => match rsp {
            apis::publications::CreatePublicationResponse::Status202_ThePublicationIsReadyToReceiveContent(body) => {
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
            apis::publications::CreatePublicationResponse::Status400_TheRequestIsMalformed(body) => {
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
            apis::publications::CreatePublicationResponse::Status409_TheRequestConflictsWithTheCurrentState(body) => {
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
            apis::publications::CreatePublicationResponse::Status413_TheContentIsOverTheDeclaredOrConfiguredSize(body) => {
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
            apis::publications::CreatePublicationResponse::Status507_TheDestinationHasInsufficientSpace(body) => {
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
            apis::publications::CreatePublicationResponse::Status500_InternalServerError(body) => {
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
fn delete_publication_validation(path_params: models::DeletePublicationPathParams) -> std::result::Result<(models::DeletePublicationPathParams,), ValidationErrors> {
    path_params.validate()?;

    Ok((path_params,))
}
/// DeletePublication - DELETE /api/v1/publications/{publicationId}
#[tracing::instrument(skip_all)]
async fn delete_publication<I, A, E>(
    method: Method,
    TypedHeader(host): TypedHeader<Host>,
    cookies: CookieJar,
    Path(path_params): Path<models::DeletePublicationPathParams>,
    State(api_impl): State<I>,
) -> Result<Response, StatusCode>
where
    I: AsRef<A> + Send + Sync,
    A: apis::publications::Publications<E> + Send + Sync,
    E: std::fmt::Debug + Send + Sync + 'static,
{
    #[allow(clippy::redundant_closure)]
    let validation = tokio::task::spawn_blocking(move || delete_publication_validation(path_params)).await.unwrap();

    let Ok((path_params,)) = validation else {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from(validation.unwrap_err().to_string()))
            .map_err(|_| StatusCode::BAD_REQUEST);
    };

    let result = api_impl.as_ref().delete_publication(&method, &host, &cookies, &path_params).await;

    let mut response = Response::builder();

    let resp = match result {
        Ok(rsp) => match rsp {
            apis::publications::DeletePublicationResponse::Status204_ThePublicationIsGone => {
                let mut response = response.status(204);
                response.body(Body::empty())
            }
            apis::publications::DeletePublicationResponse::Status404_TheResourceDoesNotExist(body) => {
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
            apis::publications::DeletePublicationResponse::Status409_TheRequestConflictsWithTheCurrentState(body) => {
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
            apis::publications::DeletePublicationResponse::Status500_InternalServerError(body) => {
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
fn get_publication_validation(path_params: models::GetPublicationPathParams) -> std::result::Result<(models::GetPublicationPathParams,), ValidationErrors> {
    path_params.validate()?;

    Ok((path_params,))
}
/// GetPublication - GET /api/v1/publications/{publicationId}
#[tracing::instrument(skip_all)]
async fn get_publication<I, A, E>(
    method: Method,
    TypedHeader(host): TypedHeader<Host>,
    cookies: CookieJar,
    Path(path_params): Path<models::GetPublicationPathParams>,
    State(api_impl): State<I>,
) -> Result<Response, StatusCode>
where
    I: AsRef<A> + Send + Sync,
    A: apis::publications::Publications<E> + Send + Sync,
    E: std::fmt::Debug + Send + Sync + 'static,
{
    #[allow(clippy::redundant_closure)]
    let validation = tokio::task::spawn_blocking(move || get_publication_validation(path_params)).await.unwrap();

    let Ok((path_params,)) = validation else {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from(validation.unwrap_err().to_string()))
            .map_err(|_| StatusCode::BAD_REQUEST);
    };

    let result = api_impl.as_ref().get_publication(&method, &host, &cookies, &path_params).await;

    let mut response = Response::builder();

    let resp = match result {
        Ok(rsp) => match rsp {
            apis::publications::GetPublicationResponse::Status200_PublicationState(body) => {
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
            apis::publications::GetPublicationResponse::Status404_TheResourceDoesNotExist(body) => {
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
            apis::publications::GetPublicationResponse::Status500_InternalServerError(body) => {
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
fn list_publications_validation() -> std::result::Result<(), ValidationErrors> {
    Ok(())
}
/// ListPublications - GET /api/v1/publications
#[tracing::instrument(skip_all)]
async fn list_publications<I, A, E>(method: Method, TypedHeader(host): TypedHeader<Host>, cookies: CookieJar, State(api_impl): State<I>) -> Result<Response, StatusCode>
where
    I: AsRef<A> + Send + Sync,
    A: apis::publications::Publications<E> + Send + Sync,
    E: std::fmt::Debug + Send + Sync + 'static,
{
    #[allow(clippy::redundant_closure)]
    let validation = tokio::task::spawn_blocking(move || list_publications_validation()).await.unwrap();

    let Ok(()) = validation else {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from(validation.unwrap_err().to_string()))
            .map_err(|_| StatusCode::BAD_REQUEST);
    };

    let result = api_impl.as_ref().list_publications(&method, &host, &cookies).await;

    let mut response = Response::builder();

    let resp = match result {
        Ok(rsp) => match rsp {
            apis::publications::ListPublicationsResponse::Status200_KnownPublications(body) => {
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
            apis::publications::ListPublicationsResponse::Status500_InternalServerError(body) => {
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
struct WritePublicationContentBodyValidator<'a> {
    body: &'a [u8],
}

#[tracing::instrument(skip_all)]
fn write_publication_content_validation(
    header_params: models::WritePublicationContentHeaderParams,
    path_params: models::WritePublicationContentPathParams,
    body: Bytes,
) -> std::result::Result<(models::WritePublicationContentHeaderParams, models::WritePublicationContentPathParams, Bytes), ValidationErrors> {
    header_params.validate()?;
    path_params.validate()?;

    Ok((header_params, path_params, body))
}
/// WritePublicationContent - PUT /api/v1/publications/{publicationId}/content
#[tracing::instrument(skip_all)]
async fn write_publication_content<I, A, E>(
    method: Method,
    TypedHeader(host): TypedHeader<Host>,
    cookies: CookieJar,
    headers: HeaderMap,
    Path(path_params): Path<models::WritePublicationContentPathParams>,
    State(api_impl): State<I>,
    body: Bytes,
) -> Result<Response, StatusCode>
where
    I: AsRef<A> + Send + Sync,
    A: apis::publications::Publications<E> + Send + Sync,
    E: std::fmt::Debug + Send + Sync + 'static,
{
    // Header parameters
    let header_params = {
        let header_content_range = headers.get(HeaderName::from_static("content-range"));

        let header_content_range = match header_content_range {
            Some(v) => match header::IntoHeaderValue::<String>::try_from((*v).clone()) {
                Ok(result) => result.0,
                Err(err) => {
                    return Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .body(Body::from(format!("Invalid header Content-Range - {err}")))
                        .map_err(|e| {
                            error!(error = ?e);
                            StatusCode::INTERNAL_SERVER_ERROR
                        });
                }
            },
            None => {
                return Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(Body::from("Missing required header Content-Range"))
                    .map_err(|e| {
                        error!(error = ?e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    });
            }
        };

        models::WritePublicationContentHeaderParams {
            content_range: header_content_range,
        }
    };

    #[allow(clippy::redundant_closure)]
    let validation = tokio::task::spawn_blocking(move || write_publication_content_validation(header_params, path_params, body))
        .await
        .unwrap();

    let Ok((header_params, path_params, body)) = validation else {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from(validation.unwrap_err().to_string()))
            .map_err(|_| StatusCode::BAD_REQUEST);
    };

    let result = api_impl
        .as_ref()
        .write_publication_content(&method, &host, &cookies, &header_params, &path_params, &body)
        .await;

    let mut response = Response::builder();

    let resp = match result {
        Ok(rsp) => match rsp {
            apis::publications::WritePublicationContentResponse::Status200_TheContentWasAccepted(body) => {
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
            apis::publications::WritePublicationContentResponse::Status400_TheRequestIsMalformed(body) => {
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
            apis::publications::WritePublicationContentResponse::Status404_TheResourceDoesNotExist(body) => {
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
            apis::publications::WritePublicationContentResponse::Status409_TheRequestConflictsWithTheCurrentState(body) => {
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
            apis::publications::WritePublicationContentResponse::Status413_TheContentIsOverTheDeclaredOrConfiguredSize(body) => {
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
            apis::publications::WritePublicationContentResponse::Status507_TheDestinationHasInsufficientSpace(body) => {
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
            apis::publications::WritePublicationContentResponse::Status500_InternalServerError(body) => {
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
fn cancel_subscription_validation(path_params: models::CancelSubscriptionPathParams) -> std::result::Result<(models::CancelSubscriptionPathParams,), ValidationErrors> {
    path_params.validate()?;

    Ok((path_params,))
}
/// CancelSubscription - POST /api/v1/subscriptions/{subscriptionId}/cancel
#[tracing::instrument(skip_all)]
async fn cancel_subscription<I, A, E>(
    method: Method,
    TypedHeader(host): TypedHeader<Host>,
    cookies: CookieJar,
    Path(path_params): Path<models::CancelSubscriptionPathParams>,
    State(api_impl): State<I>,
) -> Result<Response, StatusCode>
where
    I: AsRef<A> + Send + Sync,
    A: apis::subscriptions::Subscriptions<E> + Send + Sync,
    E: std::fmt::Debug + Send + Sync + 'static,
{
    #[allow(clippy::redundant_closure)]
    let validation = tokio::task::spawn_blocking(move || cancel_subscription_validation(path_params)).await.unwrap();

    let Ok((path_params,)) = validation else {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from(validation.unwrap_err().to_string()))
            .map_err(|_| StatusCode::BAD_REQUEST);
    };

    let result = api_impl.as_ref().cancel_subscription(&method, &host, &cookies, &path_params).await;

    let mut response = Response::builder();

    let resp = match result {
        Ok(rsp) => match rsp {
            apis::subscriptions::CancelSubscriptionResponse::Status200_TheSubscriptionStateAfterTheRequest(body) => {
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
            apis::subscriptions::CancelSubscriptionResponse::Status404_TheResourceDoesNotExist(body) => {
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
            apis::subscriptions::CancelSubscriptionResponse::Status409_TheRequestConflictsWithTheCurrentState(body) => {
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
            apis::subscriptions::CancelSubscriptionResponse::Status500_InternalServerError(body) => {
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
struct CreateSubscriptionBodyValidator<'a> {
    #[validate(nested)]
    body: &'a models::CreateSubscriptionRequest,
}

#[tracing::instrument(skip_all)]
fn create_subscription_validation(body: models::CreateSubscriptionRequest) -> std::result::Result<(models::CreateSubscriptionRequest,), ValidationErrors> {
    let b = CreateSubscriptionBodyValidator { body: &body };
    b.validate()?;

    Ok((body,))
}
/// CreateSubscription - POST /api/v1/subscriptions
#[tracing::instrument(skip_all)]
async fn create_subscription<I, A, E>(
    method: Method,
    TypedHeader(host): TypedHeader<Host>,
    cookies: CookieJar,
    State(api_impl): State<I>,
    Json(body): Json<models::CreateSubscriptionRequest>,
) -> Result<Response, StatusCode>
where
    I: AsRef<A> + Send + Sync,
    A: apis::subscriptions::Subscriptions<E> + Send + Sync,
    E: std::fmt::Debug + Send + Sync + 'static,
{
    #[allow(clippy::redundant_closure)]
    let validation = tokio::task::spawn_blocking(move || create_subscription_validation(body)).await.unwrap();

    let Ok((body,)) = validation else {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from(validation.unwrap_err().to_string()))
            .map_err(|_| StatusCode::BAD_REQUEST);
    };

    let result = api_impl.as_ref().create_subscription(&method, &host, &cookies, &body).await;

    let mut response = Response::builder();

    let resp = match result {
        Ok(rsp) => match rsp {
            apis::subscriptions::CreateSubscriptionResponse::Status202_TheSubscriptionIsFetching(body) => {
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
            apis::subscriptions::CreateSubscriptionResponse::Status400_TheRequestIsMalformed(body) => {
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
            apis::subscriptions::CreateSubscriptionResponse::Status409_TheRequestConflictsWithTheCurrentState(body) => {
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
            apis::subscriptions::CreateSubscriptionResponse::Status500_InternalServerError(body) => {
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
fn delete_subscription_validation(
    path_params: models::DeleteSubscriptionPathParams,
    query_params: models::DeleteSubscriptionQueryParams,
) -> std::result::Result<(models::DeleteSubscriptionPathParams, models::DeleteSubscriptionQueryParams), ValidationErrors> {
    path_params.validate()?;
    query_params.validate()?;

    Ok((path_params, query_params))
}
/// DeleteSubscription - DELETE /api/v1/subscriptions/{subscriptionId}
#[tracing::instrument(skip_all)]
async fn delete_subscription<I, A, E>(
    method: Method,
    TypedHeader(host): TypedHeader<Host>,
    cookies: CookieJar,
    Path(path_params): Path<models::DeleteSubscriptionPathParams>,
    QueryExtra(query_params): QueryExtra<models::DeleteSubscriptionQueryParams>,
    State(api_impl): State<I>,
) -> Result<Response, StatusCode>
where
    I: AsRef<A> + Send + Sync,
    A: apis::subscriptions::Subscriptions<E> + Send + Sync,
    E: std::fmt::Debug + Send + Sync + 'static,
{
    #[allow(clippy::redundant_closure)]
    let validation = tokio::task::spawn_blocking(move || delete_subscription_validation(path_params, query_params))
        .await
        .unwrap();

    let Ok((path_params, query_params)) = validation else {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from(validation.unwrap_err().to_string()))
            .map_err(|_| StatusCode::BAD_REQUEST);
    };

    let result = api_impl.as_ref().delete_subscription(&method, &host, &cookies, &path_params, &query_params).await;

    let mut response = Response::builder();

    let resp = match result {
        Ok(rsp) => match rsp {
            apis::subscriptions::DeleteSubscriptionResponse::Status204_TheSubscriptionIsGone => {
                let mut response = response.status(204);
                response.body(Body::empty())
            }
            apis::subscriptions::DeleteSubscriptionResponse::Status404_TheResourceDoesNotExist(body) => {
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
            apis::subscriptions::DeleteSubscriptionResponse::Status409_TheRequestConflictsWithTheCurrentState(body) => {
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
            apis::subscriptions::DeleteSubscriptionResponse::Status500_InternalServerError(body) => {
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
fn get_subscription_validation(path_params: models::GetSubscriptionPathParams) -> std::result::Result<(models::GetSubscriptionPathParams,), ValidationErrors> {
    path_params.validate()?;

    Ok((path_params,))
}
/// GetSubscription - GET /api/v1/subscriptions/{subscriptionId}
#[tracing::instrument(skip_all)]
async fn get_subscription<I, A, E>(
    method: Method,
    TypedHeader(host): TypedHeader<Host>,
    cookies: CookieJar,
    Path(path_params): Path<models::GetSubscriptionPathParams>,
    State(api_impl): State<I>,
) -> Result<Response, StatusCode>
where
    I: AsRef<A> + Send + Sync,
    A: apis::subscriptions::Subscriptions<E> + Send + Sync,
    E: std::fmt::Debug + Send + Sync + 'static,
{
    #[allow(clippy::redundant_closure)]
    let validation = tokio::task::spawn_blocking(move || get_subscription_validation(path_params)).await.unwrap();

    let Ok((path_params,)) = validation else {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from(validation.unwrap_err().to_string()))
            .map_err(|_| StatusCode::BAD_REQUEST);
    };

    let result = api_impl.as_ref().get_subscription(&method, &host, &cookies, &path_params).await;

    let mut response = Response::builder();

    let resp = match result {
        Ok(rsp) => match rsp {
            apis::subscriptions::GetSubscriptionResponse::Status200_SubscriptionState(body) => {
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
            apis::subscriptions::GetSubscriptionResponse::Status404_TheResourceDoesNotExist(body) => {
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
            apis::subscriptions::GetSubscriptionResponse::Status500_InternalServerError(body) => {
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
fn get_subscription_content_validation(
    header_params: models::GetSubscriptionContentHeaderParams,
    path_params: models::GetSubscriptionContentPathParams,
) -> std::result::Result<(models::GetSubscriptionContentHeaderParams, models::GetSubscriptionContentPathParams), ValidationErrors> {
    header_params.validate()?;
    path_params.validate()?;

    Ok((header_params, path_params))
}
/// GetSubscriptionContent - GET /api/v1/subscriptions/{subscriptionId}/content
#[tracing::instrument(skip_all)]
async fn get_subscription_content<I, A, E>(
    method: Method,
    TypedHeader(host): TypedHeader<Host>,
    cookies: CookieJar,
    headers: HeaderMap,
    Path(path_params): Path<models::GetSubscriptionContentPathParams>,
    State(api_impl): State<I>,
) -> Result<Response, StatusCode>
where
    I: AsRef<A> + Send + Sync,
    A: apis::subscriptions::Subscriptions<E> + Send + Sync,
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

        models::GetSubscriptionContentHeaderParams { range: header_range }
    };

    #[allow(clippy::redundant_closure)]
    let validation = tokio::task::spawn_blocking(move || get_subscription_content_validation(header_params, path_params))
        .await
        .unwrap();

    let Ok((header_params, path_params)) = validation else {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from(validation.unwrap_err().to_string()))
            .map_err(|_| StatusCode::BAD_REQUEST);
    };

    let result = api_impl.as_ref().get_subscription_content(&method, &host, &cookies, &header_params, &path_params).await;

    let mut response = Response::builder();

    let resp = match result {
        Ok(rsp) => match rsp {
            apis::subscriptions::GetSubscriptionContentResponse::Status200_TheWholeOutput(body) => {
                let mut response = response.status(200);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/octet-stream"));
                }

                let body_content = body.0;
                response.body(Body::from(body_content))
            }
            apis::subscriptions::GetSubscriptionContentResponse::Status206_ARangeOfTheOutput(body) => {
                let mut response = response.status(206);
                {
                    let mut response_headers = response.headers_mut().unwrap();
                    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/octet-stream"));
                }

                let body_content = body.0;
                response.body(Body::from(body_content))
            }
            apis::subscriptions::GetSubscriptionContentResponse::Status304_TheOutputMatchesTheRequest => {
                let mut response = response.status(304);
                response.body(Body::empty())
            }
            apis::subscriptions::GetSubscriptionContentResponse::Status404_TheResourceDoesNotExist(body) => {
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
            apis::subscriptions::GetSubscriptionContentResponse::Status409_TheRequestConflictsWithTheCurrentState(body) => {
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
            apis::subscriptions::GetSubscriptionContentResponse::Status416_TheRequestedRangeIsOutsideTheOutput(body) => {
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
            apis::subscriptions::GetSubscriptionContentResponse::Status500_InternalServerError(body) => {
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
fn list_subscriptions_validation() -> std::result::Result<(), ValidationErrors> {
    Ok(())
}
/// ListSubscriptions - GET /api/v1/subscriptions
#[tracing::instrument(skip_all)]
async fn list_subscriptions<I, A, E>(method: Method, TypedHeader(host): TypedHeader<Host>, cookies: CookieJar, State(api_impl): State<I>) -> Result<Response, StatusCode>
where
    I: AsRef<A> + Send + Sync,
    A: apis::subscriptions::Subscriptions<E> + Send + Sync,
    E: std::fmt::Debug + Send + Sync + 'static,
{
    #[allow(clippy::redundant_closure)]
    let validation = tokio::task::spawn_blocking(move || list_subscriptions_validation()).await.unwrap();

    let Ok(()) = validation else {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from(validation.unwrap_err().to_string()))
            .map_err(|_| StatusCode::BAD_REQUEST);
    };

    let result = api_impl.as_ref().list_subscriptions(&method, &host, &cookies).await;

    let mut response = Response::builder();

    let resp = match result {
        Ok(rsp) => match rsp {
            apis::subscriptions::ListSubscriptionsResponse::Status200_KnownSubscriptions(body) => {
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
            apis::subscriptions::ListSubscriptionsResponse::Status500_InternalServerError(body) => {
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
