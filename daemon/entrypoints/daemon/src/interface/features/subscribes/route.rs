use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::Response,
};
use tokio::io::{AsyncReadExt as _, AsyncSeekExt as _, SeekFrom};
use tokio_util::io::ReaderStream;

use crate::{
    error::ApiError,
    service::download::{ByteRange, content_disposition, parse_range},
    state::DaemonState,
};

const CHUNK_SIZE: u64 = 64 * 1024;

// omnius-lint:allow(free-fn) axum calls this route handler directly
pub async fn read_content(State(state): State<Arc<DaemonState>>, Path(subscribe_id): Path<String>, headers: HeaderMap) -> Response {
    let download = match state.subscribe.download(&subscribe_id).await {
        Ok(download) => download,
        Err(error) => return error.into_response(),
    };

    if headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == download.etag)
    {
        return Response::builder()
            .status(StatusCode::NOT_MODIFIED)
            .header(header::ETAG, download.etag)
            .body(Body::empty())
            .unwrap_or_else(|_| ApiError::internal("cannot build the response").into_response());
    }

    let range_header = headers.get(header::RANGE).and_then(|value| value.to_str().ok());
    let range = parse_range(range_header, download.size);
    let (status, start, length, content_range) = match range {
        ByteRange::Full => (StatusCode::OK, 0, download.size, None),
        ByteRange::Partial { start, end } => (StatusCode::PARTIAL_CONTENT, start, end - start + 1, Some(format!("bytes {start}-{end}/{}", download.size))),
        ByteRange::Unsatisfiable => {
            return Response::builder()
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .header(header::CONTENT_RANGE, format!("bytes */{}", download.size))
                .body(Body::empty())
                .unwrap_or_else(|_| ApiError::internal("cannot build the response").into_response());
        }
    };

    let mut file = match tokio::fs::File::open(&download.path).await {
        Ok(file) => file,
        Err(_) => return ApiError::gone("the subscription output is no longer available").into_response(),
    };
    if start != 0 && file.seek(SeekFrom::Start(start)).await.is_err() {
        return ApiError::internal("cannot seek the subscription output").into_response();
    }
    let body = Body::from_stream(ReaderStream::with_capacity(file.take(length), CHUNK_SIZE as usize));

    let mut response = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_LENGTH, length)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::ETAG, download.etag)
        .header(header::CONTENT_DISPOSITION, content_disposition(&download.file_name));
    if let Some(content_range) = content_range {
        response = response.header(header::CONTENT_RANGE, content_range);
    }
    match response.body(body) {
        Ok(response) => response,
        Err(_) => ApiError::internal("cannot build the response").into_response(),
    }
}
