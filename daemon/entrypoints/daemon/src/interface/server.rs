use std::sync::Arc;

use axum::{
    extract::DefaultBodyLimit,
    routing::{get, put},
};
use tokio_util::sync::CancellationToken;

use crate::{
    interface::features::{publishes::route as publish_route, subscribes::route as subscribe_route},
    prelude::*,
    state::DaemonState,
};

pub struct ApiServer {
    pub(crate) state: Arc<DaemonState>,
}

impl omnius_axus_interface::apis::ErrorHandler for ApiServer {}

impl ApiServer {
    pub fn router(state: Arc<DaemonState>) -> axum::Router {
        let generated = omnius_axus_interface::server::new(Arc::new(Self { state: state.clone() }));
        axum::Router::new()
            .route(
                "/api/v1/publishes/{publishId}/content",
                put(publish_route::write_content).layer(DefaultBodyLimit::disable()),
            )
            .route("/api/v1/subscribes/{subscribeId}/content", get(subscribe_route::read_content))
            .with_state(state)
            .fallback_service(generated)
    }

    pub async fn serve(state: Arc<DaemonState>, token: CancellationToken) -> Result<()> {
        let app = Self::router(state.clone());

        let listener = tokio::net::TcpListener::bind(state.conf.api.listen_addr.clone()).await?;
        info!(addr = state.conf.api.listen_addr, "listen start");
        axum::serve(listener, app).with_graceful_shutdown(token.cancelled_owned()).await?;

        Ok(())
    }
}
