use async_trait::async_trait;
use axum_extra::extract::CookieJar;
use headers::Host;
use http::Method;

use omnius_axus_interface::{
    apis::health::{GetHealthResponse, Health},
    models::HealthResponse,
};

use crate::interface::ApiServer;

#[allow(unused_variables)]
#[async_trait]
impl Health for ApiServer {
    async fn get_health(&self, method: &Method, host: &Host, cookies: &CookieJar) -> std::result::Result<GetHealthResponse, ()> {
        Ok(GetHealthResponse::Status200_TheDaemonIsRunning(HealthResponse::new(true)))
    }
}
