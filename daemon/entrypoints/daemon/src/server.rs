use std::sync::Arc;

use async_trait::async_trait;
use axum_extra::extract::CookieJar;
use headers::Host;
use http::Method;
use tokio_util::sync::CancellationToken;

use crate::{prelude::*, state::DaemonState};

pub struct ApiServer {
    state: Arc<DaemonState>,
}

#[allow(unused_variables)]
#[async_trait]
impl omnius_axus_interface::apis::health::Health for ApiServer {
    async fn get_health(&self, method: &Method, host: &Host, cookies: &CookieJar) -> std::result::Result<omnius_axus_interface::apis::health::GetHealthResponse, ()> {
        todo!()
    }
}

impl omnius_axus_interface::apis::ErrorHandler for ApiServer {}

impl ApiServer {
    pub async fn serve(state: Arc<DaemonState>, token: CancellationToken) -> Result<()> {
        let server = Self { state: state.clone() };

        let app = omnius_axus_interface::server::new(Arc::new(server));

        let listener = tokio::net::TcpListener::bind(state.conf.core.listen_addr.clone()).await?;
        info!(addr = state.conf.core.listen_addr, "listen start");
        axum::serve(listener, app).with_graceful_shutdown(token.cancelled_owned()).await?;

        Ok(())
    }
}
