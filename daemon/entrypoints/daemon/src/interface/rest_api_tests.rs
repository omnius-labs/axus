use std::{path::Path, sync::Arc, time::Duration};

use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Method, Request, StatusCode, header},
};
use bytes::Bytes;
use omnius_axus_engine::{FileExchangerIntervals, NodeFinderIntervals, SubscribedFileStatus, service::AxusServiceOption};
use omnius_axus_interface::{models, types::Nullable};
use omnius_core_omnikit::generated::omni_hash::{OmniHash, OmniHashAlgorithmType};
use testresult::TestResult;
use tower::ServiceExt as _;

use crate::{
    config::{ApiConfig, CoreConfig, DaemonConfig, LoggingConfig, P2pConfig},
    state::DaemonState,
};

// omnius-lint:allow(free-fn) shared helper for the REST API tests
fn config(state_dir: &Path, max_upload_size: u64, bootstrap_nodes: Vec<String>) -> DaemonConfig {
    DaemonConfig {
        core: CoreConfig {
            state_dir: state_dir.to_path_buf(),
        },
        api: ApiConfig {
            listen_addr: "127.0.0.1:0".to_string(),
            max_upload_size,
        },
        p2p: P2pConfig {
            listen_addr: "127.0.0.1:0".to_string(),
            advertise_addrs: vec![],
            use_upnp: false,
            bootstrap_nodes,
        },
        logging: LoggingConfig {
            level: "error".to_string(),
            json: false,
        },
    }
}

// omnius-lint:allow(free-fn) shared helper for the REST API tests
fn fast_option() -> AxusServiceOption {
    AxusServiceOption {
        node_finder_intervals: NodeFinderIntervals {
            connect: Duration::from_millis(100),
            compute: Duration::from_millis(100),
            communicate: Duration::from_millis(200),
            lookup: Duration::from_millis(100),
        },
        file_exchanger_intervals: FileExchangerIntervals {
            status_cycle: Duration::from_millis(200),
            request_timeout: Duration::from_secs(5),
            request_retract_after: Duration::from_secs(1),
            unwanted_close: Duration::from_secs(2),
            unwanted_cooldown: Duration::from_secs(1),
            reject_retry: Duration::from_secs(1),
            rank_absent_retry: Duration::from_secs(1),
            cancel_grace: Duration::from_secs(1),
            tail_remaining: 32,
            tail_stall_remaining: 256,
            tail_stall: Duration::from_secs(1),
            tail_copies: 2,
            fetch_window: Duration::from_secs(10),
            fetch_min_blocks: 4,
            pending_min: 4,
            ranking_window: Duration::from_secs(10),
            unchoke_rotation: Duration::from_secs(1),
            violation_exclusion: Duration::from_secs(5),
            timeout_exclusion: Duration::from_secs(5),
            poll: Duration::from_millis(20),
            connect: Duration::from_millis(100),
            wanted_refresh: Duration::from_millis(50),
        },
        ..Default::default()
    }
}

// omnius-lint:allow(free-fn) shared helper for the REST API tests
async fn send(app: &Router, method: Method, uri: &str, content_type: Option<&str>, body: Option<Vec<u8>>) -> (StatusCode, HeaderMap, Bytes) {
    let mut request = Request::builder().method(method).uri(uri).header(header::HOST, "localhost");
    if let Some(content_type) = content_type {
        request = request.header(header::CONTENT_TYPE, content_type);
    }
    let request = request.body(body.map(Body::from).unwrap_or_else(Body::empty)).unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, headers, bytes)
}

// omnius-lint:allow(free-fn) shared helper for the REST API tests
fn json<T: serde::de::DeserializeOwned>(body: &[u8]) -> T {
    serde_json::from_slice(body).unwrap()
}

// omnius-lint:allow(free-fn) shared helper for the REST API tests
async fn create_publish(app: &Router, file_name: &str, size: u64, block_size: u32) -> models::Publish {
    let body = serde_json::to_vec(&serde_json::json!({
        "fileName": file_name,
        "size": size,
        "blockSize": block_size,
    }))
    .unwrap();
    let (status, _, body) = send(app, Method::POST, "/api/v1/publishes", Some("application/json"), Some(body)).await;
    assert_eq!(status, StatusCode::CREATED);
    json::<models::PublishResponse>(&body).publish
}

// omnius-lint:allow(free-fn) shared helper for the REST API tests
async fn get_publish(app: &Router, id: &str) -> models::Publish {
    let (status, _, body) = send(app, Method::GET, &format!("/api/v1/publishes/{id}"), None, None).await;
    assert_eq!(status, StatusCode::OK);
    json::<models::PublishResponse>(&body).publish
}

// omnius-lint:allow(free-fn) shared helper for the REST API tests
async fn wait_publish(app: &Router, id: &str, expected: models::PublishStatus) -> models::Publish {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
    loop {
        let publish = get_publish(app, id).await;
        if publish.status == expected {
            return publish;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!("publish {id} did not reach {expected:?}: {:?}", publish.status);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

// omnius-lint:allow(free-fn) shared helper for the REST API tests
async fn create_subscribe(app: &Router, root_hash: &str, output_path: &Path) -> models::Subscribe {
    let body = serde_json::to_vec(&serde_json::json!({
        "rootHash": root_hash,
        "outputPath": output_path,
    }))
    .unwrap();
    let (status, _, body) = send(app, Method::POST, "/api/v1/subscribes", Some("application/json"), Some(body)).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    json::<models::SubscribeResponse>(&body).subscribe
}

// omnius-lint:allow(free-fn) shared helper for the REST API tests
async fn get_subscribe(app: &Router, id: &str) -> models::Subscribe {
    let (status, _, body) = send(app, Method::GET, &format!("/api/v1/subscribes/{id}"), None, None).await;
    assert_eq!(status, StatusCode::OK);
    json::<models::SubscribeResponse>(&body).subscribe
}

// omnius-lint:allow(free-fn) shared helper for the REST API tests
async fn wait_subscribe(app: &Router, id: &str, expected: models::SubscribeStatus) -> models::Subscribe {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
    loop {
        let subscribe = get_subscribe(app, id).await;
        if subscribe.status == expected {
            return subscribe;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!("subscription {id} did not reach {expected:?}: {:?}", subscribe.status);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

// omnius-lint:allow(free-fn) shared helper for the REST API tests
fn present(value: &Option<Nullable<String>>) -> Option<String> {
    match value {
        Some(Nullable::Present(value)) => Some(value.clone()),
        _ => None,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn rest_api_publish_subscribe_download_flow_test() -> TestResult {
    let publisher_dir = tempfile::tempdir()?;
    let subscriber_dir = tempfile::tempdir()?;

    let publisher = Arc::new(DaemonState::new_for_test(config(publisher_dir.path(), 0, vec![]), fast_option()).await?);
    let publisher_uri = publisher.node_profile().to_uri()?;
    let subscriber = Arc::new(DaemonState::new_for_test(config(subscriber_dir.path(), 0, vec![publisher_uri]), fast_option()).await?);
    let publisher_api = super::ApiServer::router(publisher.clone());
    let subscriber_api = super::ApiServer::router(subscriber.clone());

    let content: Vec<u8> = (0..4096).map(|index| (index % 251) as u8).collect();
    let publish = create_publish(&publisher_api, "payload.bin", content.len() as u64, 256).await;
    let (status, _, _) = send(
        &publisher_api,
        Method::PUT,
        &format!("/api/v1/publishes/{}/content?offset=0", publish.id),
        Some("application/octet-stream"),
        Some(content.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _, _) = send(&publisher_api, Method::POST, &format!("/api/v1/publishes/{}/commit", publish.id), None, None).await;
    assert_eq!(status, StatusCode::ACCEPTED);

    let publish = wait_publish(&publisher_api, &publish.id, models::PublishStatus::Succeeded).await;
    let root_hash = present(&publish.root_hash).expect("a succeeded publish has a root hash");
    assert!(publish.encoded_block_count >= 17);

    let output_path = subscriber_dir.path().join("downloaded.bin");
    let subscribe = create_subscribe(&subscriber_api, &root_hash, &output_path).await;
    let subscribe = wait_subscribe(&subscriber_api, &subscribe.id, models::SubscribeStatus::Completed).await;
    assert!(output_path.exists());

    let (status, headers, body) = send(&subscriber_api, Method::GET, &format!("/api/v1/subscribes/{}/content", subscribe.id), None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_ref(), content.as_slice());
    let etag = headers.get(header::ETAG).unwrap().to_str()?.to_string();

    let range_request = {
        let mut request = Request::builder()
            .method(Method::GET)
            .uri(format!("/api/v1/subscribes/{}/content", subscribe.id))
            .header(header::HOST, "localhost")
            .header(header::RANGE, "bytes=10-19")
            .body(Body::empty())
            .unwrap();
        request.headers_mut().remove(header::CONTENT_TYPE);
        request
    };
    let response = subscriber_api.clone().oneshot(range_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(response.headers().get(header::CONTENT_RANGE).unwrap(), "bytes 10-19/4096");
    let range_body = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
    assert_eq!(range_body.as_ref(), &content[10..=19]);

    let not_modified_request = Request::builder()
        .method(Method::GET)
        .uri(format!("/api/v1/subscribes/{}/content", subscribe.id))
        .header(header::HOST, "localhost")
        .header(header::IF_NONE_MATCH, etag)
        .body(Body::empty())
        .unwrap();
    let response = subscriber_api.clone().oneshot(not_modified_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_MODIFIED);

    let (status, _, body) = send(&subscriber_api, Method::GET, "/api/v1/subscribes?limit=1&offset=0", None, None).await;
    assert_eq!(status, StatusCode::OK);
    let list = json::<models::SubscribeList>(&body);
    assert_eq!(list.total, 1);
    assert_eq!(list.subscribes.len(), 1);

    std::fs::remove_file(&output_path)?;
    let (status, _, _) = send(&subscriber_api, Method::GET, &format!("/api/v1/subscribes/{}/content", subscribe.id), None, None).await;
    assert_eq!(status, StatusCode::GONE);

    let (status, _, _) = send(&subscriber_api, Method::DELETE, &format!("/api/v1/subscribes/{}", subscribe.id), None, None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _, _) = send(&subscriber_api, Method::GET, &format!("/api/v1/subscribes/{}", subscribe.id), None, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _, _) = send(&publisher_api, Method::DELETE, &format!("/api/v1/publishes/{}", publish.id), None, None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _, _) = send(&publisher_api, Method::GET, &format!("/api/v1/publishes/{}", publish.id), None, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    publisher.shutdown().await;
    subscriber.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rest_api_upload_offset_and_commit_size_errors_test() -> TestResult {
    let dir = tempfile::tempdir()?;
    let state = Arc::new(DaemonState::new_for_test(config(dir.path(), 0, vec![]), fast_option()).await?);
    let app = super::ApiServer::router(state.clone());

    let publish = create_publish(&app, "payload.bin", 10, 256).await;
    let (status, _, _) = send(
        &app,
        Method::PUT,
        &format!("/api/v1/publishes/{}/content?offset=1", publish.id),
        Some("application/octet-stream"),
        Some(vec![0u8; 5]),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(get_publish(&app, &publish.id).await.uploaded_size, 0);

    let (status, _, _) = send(
        &app,
        Method::PUT,
        &format!("/api/v1/publishes/{}/content?offset=0", publish.id),
        Some("application/octet-stream"),
        Some(vec![0u8; 5]),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _, _) = send(&app, Method::POST, &format!("/api/v1/publishes/{}/commit", publish.id), None, None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(get_publish(&app, &publish.id).await.uploaded_size, 5);

    let (status, _, _) = send(&app, Method::POST, &format!("/api/v1/publishes/{}/cancel", publish.id), None, None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _, _) = send(&app, Method::GET, &format!("/api/v1/publishes/{}", publish.id), None, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    state.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rest_api_max_upload_size_test() -> TestResult {
    let dir = tempfile::tempdir()?;
    let state = Arc::new(DaemonState::new_for_test(config(dir.path(), 4, vec![]), fast_option()).await?);
    let app = super::ApiServer::router(state.clone());

    let body = serde_json::to_vec(&serde_json::json!({"fileName": "too-large.bin", "size": 5}))?;
    let (status, _, body) = send(&app, Method::POST, "/api/v1/publishes", Some("application/json"), Some(body)).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    let error = json::<models::ErrorResponse>(&body);
    assert_eq!(error.kind, models::ErrorKind::PayloadTooLarge);

    let publish = create_publish(&app, "fits.bin", 4, 256).await;
    let (status, _, _) = send(&app, Method::POST, &format!("/api/v1/publishes/{}/cancel", publish.id), None, None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    state.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rest_api_cancel_and_remove_test() -> TestResult {
    let dir = tempfile::tempdir()?;
    let state = Arc::new(DaemonState::new_for_test(config(dir.path(), 0, vec![]), fast_option()).await?);
    let app = super::ApiServer::router(state.clone());

    let publish = create_publish(&app, "cancel.bin", 4, 256).await;
    let (status, _, _) = send(&app, Method::POST, &format!("/api/v1/publishes/{}/cancel", publish.id), None, None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let publish = create_publish(&app, "remove.bin", 4, 256).await;
    let (status, _, _) = send(&app, Method::DELETE, &format!("/api/v1/publishes/{}", publish.id), None, None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let root_hash = OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, b"missing").to_string();
    let subscribe = create_subscribe(&app, &root_hash, &dir.path().join("missing.bin")).await;
    let (status, _, _) = send(&app, Method::GET, &format!("/api/v1/subscribes/{}/content", subscribe.id), None, None).await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _, body) = send(&app, Method::POST, &format!("/api/v1/subscribes/{}/cancel", subscribe.id), None, None).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(json::<models::SubscribeResponse>(&body).subscribe.status, models::SubscribeStatus::Canceled);
    let (status, _, _) = send(&app, Method::DELETE, &format!("/api/v1/subscribes/{}", subscribe.id), None, None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let subscribe = create_subscribe(&app, &root_hash, &dir.path().join("remove.bin")).await;
    let (status, _, _) = send(&app, Method::DELETE, &format!("/api/v1/subscribes/{}", subscribe.id), None, None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    state.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rest_api_restart_test() -> TestResult {
    let dir = tempfile::tempdir()?;
    let conf = config(dir.path(), 0, vec![]);

    let first = Arc::new(DaemonState::new_for_test(conf.clone(), fast_option()).await?);
    let app = super::ApiServer::router(first.clone());
    let content = vec![7u8; 1024];
    let publish = create_publish(&app, "restart.bin", content.len() as u64, 256).await;
    let (status, _, _) = send(
        &app,
        Method::PUT,
        &format!("/api/v1/publishes/{}/content?offset=0", publish.id),
        Some("application/octet-stream"),
        Some(content),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _, _) = send(&app, Method::POST, &format!("/api/v1/publishes/{}/commit", publish.id), None, None).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    wait_publish(&app, &publish.id, models::PublishStatus::Succeeded).await;

    let pending = create_publish(&app, "lost.bin", 4, 256).await;
    first.shutdown().await;
    drop(app);
    drop(first);

    let second = Arc::new(DaemonState::new_for_test(conf, fast_option()).await?);
    let app = super::ApiServer::router(second.clone());
    let (status, _, _) = send(&app, Method::GET, &format!("/api/v1/publishes/{}", publish.id), None, None).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _, _) = send(&app, Method::GET, &format!("/api/v1/publishes/{}", pending.id), None, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(std::fs::read_dir(dir.path().join("uploads"))?.count(), 0);

    second.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn rest_api_publish_list_and_terminal_rules_test() -> TestResult {
    let dir = tempfile::tempdir()?;
    let state = Arc::new(DaemonState::new_for_test(config(dir.path(), 0, vec![]), fast_option()).await?);
    let app = super::ApiServer::router(state.clone());

    let first = create_publish(&app, "first.bin", 4, 256).await;
    let second = create_publish(&app, "second.bin", 4, 256).await;

    let (status, _, body) = send(&app, Method::GET, "/api/v1/publishes?limit=1&offset=0", None, None).await;
    assert_eq!(status, StatusCode::OK);
    let page = json::<models::PublishList>(&body);
    assert_eq!(page.total, 2);
    assert_eq!(page.publishes.len(), 1);

    let (status, _, body) = send(&app, Method::GET, "/api/v1/publishes?limit=1&offset=1", None, None).await;
    assert_eq!(status, StatusCode::OK);
    let next = json::<models::PublishList>(&body);
    assert_eq!(next.total, 2);
    assert_eq!(next.publishes.len(), 1);
    assert_ne!(page.publishes[0].id, next.publishes[0].id);

    for publish in [first, second] {
        let (status, _, _) = send(&app, Method::POST, &format!("/api/v1/publishes/{}/cancel", publish.id), None, None).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    let content = vec![3u8; 4];
    let publish = create_publish(&app, "done.bin", content.len() as u64, 256).await;
    let (status, _, _) = send(
        &app,
        Method::PUT,
        &format!("/api/v1/publishes/{}/content?offset=0", publish.id),
        Some("application/octet-stream"),
        Some(content.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _, _) = send(&app, Method::POST, &format!("/api/v1/publishes/{}/commit", publish.id), None, None).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    wait_publish(&app, &publish.id, models::PublishStatus::Succeeded).await;

    let (status, _, _) = send(&app, Method::POST, &format!("/api/v1/publishes/{}/cancel", publish.id), None, None).await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _, _) = send(&app, Method::DELETE, &format!("/api/v1/publishes/{}", publish.id), None, None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let duplicate_content = vec![9u8; 4];
    let original = create_publish(&app, "duplicate.bin", duplicate_content.len() as u64, 256).await;
    let (status, _, _) = send(
        &app,
        Method::PUT,
        &format!("/api/v1/publishes/{}/content?offset=0", original.id),
        Some("application/octet-stream"),
        Some(duplicate_content.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _, _) = send(&app, Method::POST, &format!("/api/v1/publishes/{}/commit", original.id), None, None).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    wait_publish(&app, &original.id, models::PublishStatus::Succeeded).await;

    let copy = create_publish(&app, "duplicate.bin", duplicate_content.len() as u64, 256).await;
    let (status, _, _) = send(
        &app,
        Method::PUT,
        &format!("/api/v1/publishes/{}/content?offset=0", copy.id),
        Some("application/octet-stream"),
        Some(duplicate_content),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _, _) = send(&app, Method::POST, &format!("/api/v1/publishes/{}/commit", copy.id), None, None).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let copy = wait_publish(&app, &copy.id, models::PublishStatus::Succeeded).await;
    assert!(copy.root_hash.is_some());

    let (status, _, _) = send(&app, Method::GET, &format!("/api/v1/publishes/{}", original.id), None, None).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _, _) = send(&app, Method::GET, &format!("/api/v1/publishes/{}", copy.id), None, None).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _, _) = send(&app, Method::DELETE, &format!("/api/v1/publishes/{}", original.id), None, None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _, _) = send(&app, Method::GET, &format!("/api/v1/publishes/{}", original.id), None, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = send(&app, Method::GET, &format!("/api/v1/publishes/{}", copy.id), None, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let root_hash = OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, b"terminal").to_string();
    let subscribe = create_subscribe(&app, &root_hash, &dir.path().join("terminal.bin")).await;
    let (status, _, body) = send(&app, Method::POST, &format!("/api/v1/subscribes/{}/cancel", subscribe.id), None, None).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(json::<models::SubscribeResponse>(&body).subscribe.status, models::SubscribeStatus::Canceled);
    let (status, _, body) = send(&app, Method::POST, &format!("/api/v1/subscribes/{}/cancel", subscribe.id), None, None).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(json::<models::SubscribeResponse>(&body).subscribe.status, models::SubscribeStatus::Canceled);
    let (status, _, _) = send(&app, Method::DELETE, &format!("/api/v1/subscribes/{}", subscribe.id), None, None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    state.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rest_api_finalizing_cancel_and_remove_conflict_test() -> TestResult {
    let dir = tempfile::tempdir()?;
    let state = Arc::new(DaemonState::new_for_test(config(dir.path(), 0, vec![]), fast_option()).await?);
    let app = super::ApiServer::router(state.clone());

    let root_hash = OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, b"finalizing").to_string();
    let subscribe = create_subscribe(&app, &root_hash, &dir.path().join("finalizing.bin")).await;
    state
        .file_service()
        .set_subscribe_status_for_test(&subscribe.id, &SubscribedFileStatus::Downloading, &SubscribedFileStatus::Finalizing)
        .await?;

    let (status, _, _) = send(&app, Method::POST, &format!("/api/v1/subscribes/{}/cancel", subscribe.id), None, None).await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _, _) = send(&app, Method::DELETE, &format!("/api/v1/subscribes/{}", subscribe.id), None, None).await;
    assert_eq!(status, StatusCode::CONFLICT);

    state
        .file_service()
        .set_subscribe_status_for_test(&subscribe.id, &SubscribedFileStatus::Finalizing, &SubscribedFileStatus::Canceled)
        .await?;
    let (status, _, _) = send(&app, Method::DELETE, &format!("/api/v1/subscribes/{}", subscribe.id), None, None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    state.shutdown().await;
    Ok(())
}
