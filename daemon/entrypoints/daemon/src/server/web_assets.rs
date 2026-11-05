use std::{convert::Infallible, path::Path};

use axum::{
    extract::Request,
    response::{IntoResponse as _, Response},
};
use tower::ServiceExt as _;
use tower_http::services::{ServeDir, ServeFile};

/// Serves the browser client's build output on the API's port, so the client and the API share an origin.
#[derive(Clone)]
pub struct WebAssets {
    files: ServeDir<ServeFile>,
}

impl WebAssets {
    /// A path without a file returns `index.html`, so reloading a client route reaches the same screen.
    /// `ServeDir` treats a `..` component as such a path, so it never reaches above `dir`.
    pub fn new(dir: &Path) -> Self {
        Self {
            files: ServeDir::new(dir).fallback(ServeFile::new(dir.join("index.html"))),
        }
    }

    /// Passes a request under `/api` to `api`, and gives any other request a file.
    pub fn router(self, api: axum::Router) -> axum::Router {
        axum::Router::new().fallback(move |request: Request| self.clone().dispatch(api.clone(), request))
    }

    /// A wrong API call answers the API's 404, so it does not look like a success with HTML.
    async fn dispatch(self, api: axum::Router, request: Request) -> Response {
        let response: std::result::Result<Response, Infallible> = if Self::is_api_path(request.uri().path()) {
            api.oneshot(request).await
        } else {
            self.files.oneshot(request).await.map(|response| response.into_response())
        };
        match response {
            Ok(response) => response,
            Err(never) => match never {},
        }
    }

    fn is_api_path(path: &str) -> bool {
        path == "/api" || path.starts_with("/api/")
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use http::StatusCode;
    use testresult::TestResult;

    use crate::server::tests::TestDaemon;

    #[tokio::test]
    async fn a_path_outside_the_api_returns_a_file_or_the_index() -> TestResult {
        let web = tempfile::tempdir()?;
        let root = web.path().join("web");
        std::fs::create_dir(&root)?;
        std::fs::write(root.join("index.html"), "index")?;
        std::fs::write(root.join("app.js"), "app")?;
        std::fs::write(web.path().join("secret.txt"), "secret")?;
        let daemon = TestDaemon::start_with(vec![], Some(&root)).await?;

        let (status, body) = daemon.request("GET", "/app.js", Body::empty()).await?;
        assert_eq!((status, body.as_ref()), (StatusCode::OK, b"app".as_ref()));
        let (status, body) = daemon.request("GET", "/", Body::empty()).await?;
        assert_eq!((status, body.as_ref()), (StatusCode::OK, b"index".as_ref()));
        let (status, body) = daemon.request("GET", "/some/client/route", Body::empty()).await?;
        assert_eq!((status, body.as_ref()), (StatusCode::OK, b"index".as_ref()));

        for uri in ["/../secret.txt", "/%2e%2e/secret.txt", "/a/../../secret.txt", "/..%2fsecret.txt"] {
            let (_, body) = daemon.request("GET", uri, Body::empty()).await?;
            assert_ne!(body.as_ref(), b"secret".as_ref(), "{uri}");
        }

        let (status, value) = daemon.get_json("/api/v1/unknown").await?;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(value["kind"], "NotFound");
        let (status, value) = daemon.get_json("/api/v1/health").await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value["result"], true);

        daemon.stop().await;
        Ok(())
    }
}
