use axum::{
    Json,
    response::{IntoResponse, Response},
};
use http::StatusCode;

use omnius_axus_engine::ErrorKind as EngineErrorKind;
use omnius_axus_interface::models;

use crate::prelude::*;

/// What a request failed with, in the terms of the REST contract.
#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub body: models::ErrorResponse,
}

impl ApiError {
    fn new(status: StatusCode, kind: models::ErrorKind, message: impl Into<String>) -> Self {
        let mut body = models::ErrorResponse::new(kind);
        body.message = Some(message.into());
        Self { status, body }
    }

    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, models::ErrorKind::InvalidRequest, message)
    }

    pub fn limit_exceeded(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, models::ErrorKind::LimitExceeded, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, models::ErrorKind::NotFound, message)
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, models::ErrorKind::Forbidden, message)
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, models::ErrorKind::Conflict, message)
    }

    pub fn payload_too_large(message: impl Into<String>) -> Self {
        Self::new(StatusCode::PAYLOAD_TOO_LARGE, models::ErrorKind::PayloadTooLarge, message)
    }

    fn insufficient_storage() -> Self {
        Self::new(StatusCode::INSUFFICIENT_STORAGE, models::ErrorKind::InsufficientStorage, "no space left on the volume")
    }

    /// The detail goes to the log, because a client on another host has no use for host paths or internals.
    fn internal(error: &dyn std::fmt::Debug) -> Self {
        error!(?error, "request failed");
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, models::ErrorKind::InternalServerError, "unexpected server error")
    }

    pub async fn no_route() -> Self {
        Self::not_found("no such API path")
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.status, self.body.message.as_deref().unwrap_or_default())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(self.body)).into_response()
    }
}

impl From<std::io::Error> for ApiError {
    fn from(error: std::io::Error) -> Self {
        if error.kind() == std::io::ErrorKind::StorageFull {
            return Self::insufficient_storage();
        }
        Self::internal(&error)
    }
}

impl From<omnius_axus_engine::Error> for ApiError {
    fn from(error: omnius_axus_engine::Error) -> Self {
        let message = error.message().unwrap_or("rejected").to_string();
        match error.kind() {
            EngineErrorKind::NotFound => Self::not_found(message),
            EngineErrorKind::AlreadyExists | EngineErrorKind::Reject => Self::conflict(message),
            EngineErrorKind::InvalidFormat => Self::invalid_request(message),
            EngineErrorKind::IoError if Self::is_storage_full(&error) => Self::insufficient_storage(),
            _ => Self::internal(&error),
        }
    }
}

impl ApiError {
    fn is_storage_full(error: &omnius_axus_engine::Error) -> bool {
        std::error::Error::source(error)
            .and_then(|source| source.downcast_ref::<std::io::Error>())
            .is_some_and(|source| source.kind() == std::io::ErrorKind::StorageFull)
    }
}

/// Maps an [`ApiError`] onto a generated response enum, whose variants differ per operation.
macro_rules! impl_from_api_error {
    ($response:ty { $($status:path => $variant:ident),* $(,)? }) => {
        impl From<$crate::server::api_error::ApiError> for $response {
            fn from(error: $crate::server::api_error::ApiError) -> Self {
                match error.status {
                    $($status => Self::$variant(error.body),)*
                    _ => Self::Status500_InternalServerError(error.body),
                }
            }
        }
    };
}

pub(crate) use impl_from_api_error;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_kinds_map_to_the_contract() {
        for (kind, status) in [
            (EngineErrorKind::NotFound, StatusCode::NOT_FOUND),
            (EngineErrorKind::AlreadyExists, StatusCode::CONFLICT),
            (EngineErrorKind::Reject, StatusCode::CONFLICT),
            (EngineErrorKind::InvalidFormat, StatusCode::BAD_REQUEST),
            (EngineErrorKind::DatabaseError, StatusCode::INTERNAL_SERVER_ERROR),
        ] {
            assert_eq!(ApiError::from(omnius_axus_engine::Error::new(kind)).status, status);
        }
    }

    #[test]
    fn a_full_volume_maps_to_insufficient_storage() {
        let io_error = || std::io::Error::from(std::io::ErrorKind::StorageFull);
        assert_eq!(ApiError::from(io_error()).status, StatusCode::INSUFFICIENT_STORAGE);
        let engine_error: omnius_axus_engine::Error = io_error().into();
        assert_eq!(ApiError::from(engine_error).status, StatusCode::INSUFFICIENT_STORAGE);
    }

    #[test]
    fn an_internal_error_hides_its_detail() {
        let error = ApiError::from(std::io::Error::other("/secret/path"));
        assert_eq!(error.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert!(!error.body.message.unwrap().contains("secret"));
    }
}
