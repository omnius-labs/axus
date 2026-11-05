use axum::{
    extract::Request,
    middleware::Next,
    response::{IntoResponse as _, Response},
};
use http::{
    HeaderMap,
    header::{HOST, ORIGIN},
};

use super::api_error::ApiError;

/// Rejects a request whose `Origin` differs from `Host`, so another site's page cannot drive the API.
pub struct OriginCheck;

impl OriginCheck {
    pub async fn middleware(request: Request, next: Next) -> Response {
        if Self::is_allowed(request.headers()) {
            next.run(request).await
        } else {
            ApiError::forbidden("origin does not match host").into_response()
        }
    }

    /// A request without `Origin` does not come from another site's script, so curl and native clients pass.
    fn is_allowed(headers: &HeaderMap) -> bool {
        let Some(origin) = headers.get(ORIGIN) else {
            return true;
        };
        let origin = origin.to_str().ok().and_then(Self::authority);
        let host = headers.get(HOST).and_then(|value| value.to_str().ok());
        matches!((origin, host), (Some(origin), Some(host)) if origin.eq_ignore_ascii_case(host))
    }

    /// Returns `host[:port]` of `scheme://host[:port]`. A value without a scheme, such as `null`, has none.
    fn authority(origin: &str) -> Option<&str> {
        let (_, authority) = origin.split_once("://")?;
        (!authority.is_empty() && !authority.contains('/')).then_some(authority)
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use http::{Request, StatusCode};
    use testresult::TestResult;

    use crate::server::tests::TestDaemon;

    async fn status(daemon: &TestDaemon, method: &str, uri: &str, origin: Option<&str>) -> TestResult<StatusCode> {
        let mut request = Request::builder().method(method).uri(uri).header("host", "node:5050");
        if let Some(origin) = origin {
            request = request.header("origin", origin);
        }
        Ok(daemon.send(request.body(Body::empty())?).await?.status())
    }

    #[tokio::test]
    async fn every_path_rejects_a_foreign_origin() -> TestResult {
        let daemon = TestDaemon::start().await?;
        for (method, uri) in [
            ("GET", "/api/v1/health"),
            ("GET", "/api/v1/publications"),
            ("POST", "/api/v1/publications?fileName=a"),
            ("PUT", "/api/v1/uploads/a/content?offset=0"),
            ("GET", "/api/v1/subscriptions/a/content"),
            ("GET", "/api/v1/unknown"),
            ("GET", "/"),
        ] {
            for origin in ["http://evil.example", "http://node:6060", "null", "node:5050"] {
                assert_eq!(status(&daemon, method, uri, Some(origin)).await?, StatusCode::FORBIDDEN, "{method} {uri} {origin}");
            }
        }
        daemon.stop().await;
        Ok(())
    }

    #[tokio::test]
    async fn a_matching_origin_and_no_origin_pass() -> TestResult {
        let daemon = TestDaemon::start().await?;
        for origin in [None, Some("http://node:5050"), Some("https://NODE:5050")] {
            assert_eq!(status(&daemon, "GET", "/api/v1/health", origin).await?, StatusCode::OK, "{origin:?}");
        }
        daemon.stop().await;
        Ok(())
    }
}
