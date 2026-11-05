use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse as _, Response},
};

use omnius_axus_interface::models::WritePublishContentQueryParams;

use crate::state::DaemonState;

// omnius-lint:allow(free-fn) axum calls this route handler directly
pub async fn write_content(State(state): State<Arc<DaemonState>>, Path(publish_id): Path<String>, Query(query): Query<WritePublishContentQueryParams>, body: Body) -> Response {
    match state.publish.write(&publish_id, query.offset, body).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => error.into_response(),
    }
}
