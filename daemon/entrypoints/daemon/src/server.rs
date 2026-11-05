use std::sync::Arc;

use async_trait::async_trait;
use axum_extra::extract::CookieJar;
use headers::Host;
use http::Method;
use tokio_util::sync::CancellationToken;

use omnius_axus_interface::{
    apis::health::{GetHealthResponse, Health},
    models::HealthResponse,
};

use crate::{prelude::*, state::DaemonState};

use self::{
    api_error::ApiError, origin_check::OriginCheck, publications::PublicationRoute, subscriptions::SubscriptionContentRoute, uploads::UploadContentRoute, web_assets::WebAssets,
};

mod api_error;
mod file_name;
mod origin_check;
mod publications;
mod subscriptions;
pub mod upload_session;
mod uploads;
mod web_assets;

pub struct ApiServer {
    state: Arc<DaemonState>,
}

#[allow(unused_variables)]
#[async_trait]
impl Health for ApiServer {
    async fn get_health(&self, method: &Method, host: &Host, cookies: &CookieJar) -> std::result::Result<GetHealthResponse, ()> {
        Ok(GetHealthResponse::Status200_TheDaemonIsRunning(HealthResponse::new(true)))
    }
}

impl omnius_axus_interface::apis::ErrorHandler for ApiServer {}

impl ApiServer {
    /// Places the routes that carry file bytes in front of the generated router, then static serving, and wraps
    /// everything in the `Origin` check.
    fn router(state: Arc<DaemonState>) -> axum::Router {
        let generated = omnius_axus_interface::server::new(Arc::new(Self { state: state.clone() })).fallback(ApiError::no_route);
        let api = axum::Router::new()
            .route(PublicationRoute::PATH, axum::routing::post(PublicationRoute::post).fallback_service(generated.clone()))
            .route(UploadContentRoute::PATH, axum::routing::put(UploadContentRoute::put))
            .route(SubscriptionContentRoute::PATH, axum::routing::get(SubscriptionContentRoute::get))
            .with_state(state.clone())
            .fallback_service(generated);
        let app = match &state.conf.web.dir {
            Some(dir) => WebAssets::new(dir).router(api),
            None => api,
        };
        app.layer(axum::middleware::from_fn(OriginCheck::middleware))
    }

    pub async fn serve(state: Arc<DaemonState>, token: CancellationToken) -> Result<()> {
        if !state.conf.api.listens_only_on_loopback() {
            warn!(
                addr = state.conf.api.listen_addr,
                "the REST API has no authentication and listens beyond loopback; limit its reach outside the daemon"
            );
        }

        let app = Self::router(state.clone());

        let listener = tokio::net::TcpListener::bind(state.conf.api.listen_addr.clone()).await?;
        info!(addr = state.conf.api.listen_addr, "listen start");
        axum::serve(listener, app).with_graceful_shutdown(token.cancelled_owned()).await?;

        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::{path::Path, time::Duration};

    use axum::body::{Body, Bytes, to_bytes};
    use http::{Request, Response, StatusCode, header::CONTENT_TYPE};
    use omnius_axus_engine::{
        model::NodeProfile,
        service::{AxusServiceOption, FileExchangerIntervals, NodeFinderIntervals},
    };
    use serde_json::{Value, json};
    use tempfile::TempDir;
    use testresult::TestResult;
    use tower::ServiceExt as _;

    use crate::config::{ApiConfig, CoreConfig, DaemonConfig, LoggingConfig, P2pConfig, StorageConfig, WebConfig};

    use super::*;

    const WATCHDOG: Duration = Duration::from_secs(120);

    pub(crate) struct TestDaemon {
        pub dir: TempDir,
        pub state: Arc<DaemonState>,
        pub router: axum::Router,
    }

    impl TestDaemon {
        pub async fn start() -> TestResult<Self> {
            Self::start_with(vec![], None).await
        }

        pub async fn start_with(bootstrap_nodes: Vec<NodeProfile>, web_dir: Option<&Path>) -> TestResult<Self> {
            let dir = tempfile::tempdir()?;
            let port = std::net::TcpListener::bind("127.0.0.1:0")?.local_addr()?.port();
            let conf = DaemonConfig {
                core: CoreConfig {
                    state_dir: dir.path().join("state"),
                },
                api: ApiConfig {
                    listen_addr: "127.0.0.1:0".to_string(),
                },
                p2p: P2pConfig {
                    listen_addr: format!("127.0.0.1:{port}"),
                    advertise_addrs: vec![],
                    use_upnp: false,
                    bootstrap_nodes: bootstrap_nodes.iter().map(|profile| profile.to_uri()).collect::<std::result::Result<_, _>>()?,
                },
                logging: LoggingConfig {
                    level: "info".to_string(),
                    json: false,
                },
                storage: StorageConfig {
                    download_dir: dir.path().join("downloads"),
                },
                web: WebConfig {
                    dir: web_dir.map(Path::to_path_buf),
                },
            };
            let option = AxusServiceOption {
                node_finder_intervals: NodeFinderIntervals {
                    connect: Duration::from_millis(100),
                    compute: Duration::from_millis(100),
                    communicate: Duration::from_millis(500),
                    lookup: Duration::from_millis(100),
                },
                file_exchanger_intervals: FileExchangerIntervals {
                    status_cycle: Duration::from_millis(200),
                    // A loaded machine can miss the short receive deadline, so the only peer must return quickly.
                    timeout_exclusion: Duration::from_secs(2),
                    unwanted_cooldown: Duration::from_secs(2),
                    reject_retry: Duration::from_millis(300),
                    rank_absent_retry: Duration::from_millis(200),
                    poll: Duration::from_millis(20),
                    connect: Duration::from_millis(50),
                    wanted_refresh: Duration::from_millis(20),
                    ..Default::default()
                },
                ..Default::default()
            };
            let state = Arc::new(DaemonState::for_test(conf, option).await?);
            let router = ApiServer::router(state.clone());
            Ok(Self { dir, state, router })
        }

        pub async fn stop(self) {
            self.state.shutdown().await;
        }

        /// Stops the daemon so that the caller can change its state, and returns what a restart needs.
        pub async fn stop_for_restart(self) -> (TempDir, DaemonConfig) {
            self.state.shutdown().await;
            let Self { dir, state, router } = self;
            let conf = state.conf.clone();
            // Dropping the router and the state releases the P2P port and the stores, as a process exit does.
            drop(router);
            drop(state);
            (dir, conf)
        }

        pub async fn restart(dir: TempDir, conf: DaemonConfig) -> TestResult<Self> {
            let state = Arc::new(DaemonState::for_test(conf, AxusServiceOption::default()).await?);
            let router = ApiServer::router(state.clone());
            Ok(Self { dir, state, router })
        }

        pub async fn send(&self, request: Request<Body>) -> TestResult<Response<Body>> {
            Ok(self.router.clone().oneshot(request).await?)
        }

        pub async fn request(&self, method: &str, uri: &str, body: Body) -> TestResult<(StatusCode, Bytes)> {
            let request = Request::builder().method(method).uri(uri).header("host", "localhost").body(body)?;
            let response = self.send(request).await?;
            let status = response.status();
            Ok((status, to_bytes(response.into_body(), usize::MAX).await?))
        }

        pub async fn request_json(&self, method: &str, uri: &str, body: &Value) -> TestResult<(StatusCode, Value)> {
            let request = Request::builder()
                .method(method)
                .uri(uri)
                .header("host", "localhost")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(body)?))?;
            let response = self.send(request).await?;
            let status = response.status();
            let bytes = to_bytes(response.into_body(), usize::MAX).await?;
            Ok((status, serde_json::from_slice(&bytes).unwrap_or(Value::Null)))
        }

        pub async fn get_json(&self, uri: &str) -> TestResult<(StatusCode, Value)> {
            let (status, bytes) = self.request("GET", uri, Body::empty()).await?;
            Ok((status, serde_json::from_slice(&bytes).unwrap_or(Value::Null)))
        }

        pub async fn publish(&self, name: &str, content: &[u8]) -> TestResult<Value> {
            let (status, bytes) = self.request("POST", &format!("/api/v1/publications?fileName={name}"), Body::from(content.to_vec())).await?;
            assert_eq!(status, StatusCode::ACCEPTED);
            Ok(serde_json::from_slice(&bytes)?)
        }

        /// Polls a resource until `status` under a limit that catches a hang rather than asserting speed.
        pub async fn wait_status(&self, uri: &str, status: &str) -> TestResult<Value> {
            let polled = tokio::time::timeout(WATCHDOG, async {
                loop {
                    let (_, value) = self.get_json(uri).await?;
                    if value["status"] == status {
                        return TestResult::Ok(value);
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await??;
            Ok(polled)
        }
    }

    #[tokio::test]
    async fn health_still_answers() -> TestResult {
        let daemon = TestDaemon::start().await?;
        let (status, value) = daemon.get_json("/api/v1/health").await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value["result"], true);
        daemon.stop().await;
        Ok(())
    }

    #[tokio::test]
    async fn an_unknown_api_path_returns_the_api_404() -> TestResult {
        let daemon = TestDaemon::start().await?;
        let (status, value) = daemon.get_json("/api/v1/unknown").await?;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(value["kind"], "NotFound");
        daemon.stop().await;
        Ok(())
    }

    #[tokio::test]
    async fn two_daemons_transfer_a_file_through_the_api() -> TestResult {
        let publisher = TestDaemon::start().await?;
        let subscriber = TestDaemon::start_with(vec![publisher.state.service().my_node_profile()], None).await?;
        let content: Vec<u8> = (0..3 * 1024 * 1024 + 17).map(|i: u32| (i % 251) as u8).collect();

        let publication = publisher.publish("movie.bin", &content).await?;
        let uri = format!("/api/v1/publications/{}", publication["id"].as_str().unwrap());
        let committed = publisher.wait_status(&uri, "Committed").await?;
        let root_hash = committed["rootHash"].as_str().unwrap();

        let (status, subscription) = subscriber
            .request_json("POST", "/api/v1/subscriptions", &json!({"rootHash": root_hash, "fileName": "movie.bin"}))
            .await?;
        assert_eq!(status, StatusCode::ACCEPTED);
        assert_eq!(subscription["status"], "Downloading");
        assert!(subscription.get("rank").is_none());
        assert_eq!(subscription["blockCountDownloaded"], 0);
        assert_eq!(subscription["blockCountTotal"], 1);

        let id = subscription["id"].as_str().unwrap();
        let uri = format!("/api/v1/subscriptions/{id}");
        let completed = subscriber.wait_status(&uri, "Completed").await?;
        assert_eq!(completed["rank"], 0);
        assert_eq!(completed["blockCountDownloaded"], completed["blockCountTotal"]);

        let (status, downloaded) = subscriber.request("GET", &format!("{uri}/content"), Body::empty()).await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(downloaded.as_ref(), content.as_slice());

        let request = Request::builder()
            .uri(format!("{uri}/content"))
            .header("host", "localhost")
            .header("range", "bytes=10-19")
            .body(Body::empty())?;
        let response = subscriber.send(request).await?;
        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(response.headers()["content-type"], "application/octet-stream");
        assert_eq!(response.headers()["x-content-type-options"], "nosniff");
        assert!(response.headers()["content-disposition"].to_str()?.starts_with("attachment"));
        let range = to_bytes(response.into_body(), usize::MAX).await?;
        assert_eq!(range.as_ref(), &content[10..20]);

        subscriber.stop().await;
        publisher.stop().await;
        Ok(())
    }
}
