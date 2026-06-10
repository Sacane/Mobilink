//! Public HTTP entry point: this is what the mobile browser talks to.
//!
//! Every inbound request is converted into a transport-agnostic
//! [`HttpRequestData`], pushed through the domain [`RequestPipeline`]
//! (which forwards it through the QUIC tunnel), and the resulting
//! [`HttpResponseData`] is turned back into a real HTTP response —
//! after the [`ResponseTransformer`] had a chance to inject Eruda.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, Response, StatusCode};

use mobilink_core::http::{HttpRequestData, HttpResponseData};
use mobilink_core::wire;

use crate::router::parse_public_path;
use crate::{PipelineError, RequestPipeline, ResponseTransformer, SessionOptions};

/// Maximum accepted request body size (32 MiB).
const MAX_BODY_BYTES: usize = 32 * 1024 * 1024;

/// Request headers that must not travel through the tunnel.
/// `accept-encoding` is stripped so the local server replies uncompressed,
/// which keeps HTML bodies injectable.
const STRIPPED_REQUEST_HEADERS: [&str; 5] =
    ["host", "connection", "accept-encoding", "content-length", "transfer-encoding"];

/// Response headers the proxy recomputes itself instead of passing through.
const STRIPPED_RESPONSE_HEADERS: [&str; 3] = ["content-length", "transfer-encoding", "connection"];

/// Everything the public HTTP layer needs to serve a request.
#[derive(Clone)]
pub struct ProxyState {
    pub pipeline: Arc<dyn RequestPipeline>,
    pub transformer: Arc<dyn ResponseTransformer>,
    pub options: Arc<dyn SessionOptions>,
}

/// Builds the public-facing router. Every path goes through the proxy
/// handler; anything that doesn't look like `/s/{id}...` is a 404.
pub fn public_router(state: ProxyState) -> Router {
    Router::new().fallback(proxy_handler).with_state(state)
}

async fn proxy_handler(State(state): State<ProxyState>, request: Request<Body>) -> Response<Body> {
    let (parts, body) = request.into_parts();
    let public_path = parts.uri.path().to_string();

    let Some((session_id, local_path)) = parse_public_path(&public_path) else {
        return text_response(StatusCode::NOT_FOUND, "No active session for this URL.\n");
    };

    let Ok(body_bytes) = axum::body::to_bytes(body, MAX_BODY_BYTES).await else {
        return text_response(StatusCode::PAYLOAD_TOO_LARGE, "Request body too large.\n");
    };

    let target = match parts.uri.query() {
        Some(query) => format!("{local_path}?{query}"),
        None => local_path,
    };

    let headers = parts
        .headers
        .iter()
        .filter(|(name, _)| !STRIPPED_REQUEST_HEADERS.contains(&name.as_str()))
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|v| (name.as_str().to_string(), v.to_string()))
        })
        .collect();

    let request_data = HttpRequestData {
        method: parts.method.as_str().to_string(),
        target,
        headers,
        body: body_bytes.to_vec(),
    };

    let Ok(encoded_request) = wire::encode(&request_data) else {
        return text_response(StatusCode::INTERNAL_SERVER_ERROR, "Failed to encode request.\n");
    };

    // The domain pipeline is synchronous by design (the contract tests are
    // sync); run it on a blocking thread so the async runtime stays free.
    let pipeline = Arc::clone(&state.pipeline);
    let outcome =
        tokio::task::spawn_blocking(move || pipeline.handle(&public_path, &encoded_request)).await;

    let forwarded = match outcome {
        Ok(result) => result,
        Err(_) => {
            return text_response(StatusCode::INTERNAL_SERVER_ERROR, "Proxy task failed.\n");
        }
    };

    match forwarded {
        Ok(response_bytes) => {
            let Ok(response_data) = wire::decode::<HttpResponseData>(&response_bytes) else {
                return text_response(
                    StatusCode::BAD_GATEWAY,
                    "The tunnel returned an unreadable response.\n",
                );
            };
            let response_data = if state.options.eruda_disabled(&session_id) {
                response_data
            } else {
                state.transformer.transform(response_data)
            };
            build_response(response_data)
        }
        Err(PipelineError::SessionNotFound) => {
            text_response(StatusCode::NOT_FOUND, "No active session for this URL.\n")
        }
        Err(PipelineError::ForwardFailed(_)) => text_response(
            StatusCode::BAD_GATEWAY,
            "The developer's tunnel is not reachable.\n",
        ),
    }
}

fn build_response(data: HttpResponseData) -> Response<Body> {
    let status = StatusCode::from_u16(data.status).unwrap_or(StatusCode::BAD_GATEWAY);
    let mut builder = Response::builder().status(status);
    for (name, value) in &data.headers {
        if STRIPPED_RESPONSE_HEADERS.contains(&name.to_ascii_lowercase().as_str()) {
            continue;
        }
        builder = builder.header(name, value);
    }
    builder
        .body(Body::from(data.body))
        .unwrap_or_else(|_| text_response(StatusCode::BAD_GATEWAY, "Invalid tunnel response.\n"))
}

fn text_response(status: StatusCode, message: &'static str) -> Response<Body> {
    Response::builder()
        .status(status)
        .header("content-type", "text/plain; charset=utf-8")
        .body(Body::from(message))
        .expect("static response must build")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ForwardError;
    use crate::transform::ErudaInjector;
    use http_body_util::BodyExt;
    use mobilink_core::session::SessionId;
    use std::sync::Mutex;
    use tower::ServiceExt;

    // --- Test doubles ---

    enum StubOutcome {
        Respond(HttpResponseData),
        SessionMissing,
        TunnelDown,
    }

    /// Pipeline double: returns a fixed outcome and records what it received.
    struct SpyPipeline {
        outcome: StubOutcome,
        seen: Mutex<Option<(String, HttpRequestData)>>,
    }

    impl SpyPipeline {
        fn new(outcome: StubOutcome) -> Arc<Self> {
            Arc::new(Self { outcome, seen: Mutex::new(None) })
        }
    }

    impl RequestPipeline for SpyPipeline {
        fn handle(&self, path: &str, request: &[u8]) -> Result<Vec<u8>, PipelineError> {
            let decoded: HttpRequestData = wire::decode(request).expect("handler sends valid frames");
            *self.seen.lock().unwrap() = Some((path.to_string(), decoded));
            match &self.outcome {
                StubOutcome::Respond(data) => Ok(wire::encode(data).unwrap()),
                StubOutcome::SessionMissing => Err(PipelineError::SessionNotFound),
                StubOutcome::TunnelDown => {
                    Err(PipelineError::ForwardFailed(ForwardError::TunnelDisconnected))
                }
            }
        }
    }

    struct ErudaAlwaysOn;
    impl SessionOptions for ErudaAlwaysOn {
        fn eruda_disabled(&self, _id: &SessionId) -> bool {
            false
        }
    }

    struct ErudaAlwaysOff;
    impl SessionOptions for ErudaAlwaysOff {
        fn eruda_disabled(&self, _id: &SessionId) -> bool {
            true
        }
    }

    fn html_page() -> HttpResponseData {
        HttpResponseData {
            status: 200,
            headers: vec![("Content-Type".to_string(), "text/html".to_string())],
            body: b"<html><body><h1>Local app</h1></body></html>".to_vec(),
        }
    }

    fn app(pipeline: Arc<SpyPipeline>, eruda_off: bool) -> Router {
        let options: Arc<dyn SessionOptions> =
            if eruda_off { Arc::new(ErudaAlwaysOff) } else { Arc::new(ErudaAlwaysOn) };
        public_router(ProxyState {
            pipeline,
            transformer: Arc::new(ErudaInjector),
            options,
        })
    }

    async fn send(app: Router, uri: &str) -> (StatusCode, String) {
        let response = app
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        (status, String::from_utf8_lossy(&body).to_string())
    }

    fn session_uri(suffix: &str) -> (SessionId, String) {
        let id = SessionId::new();
        let uri = format!("/s/{id}{suffix}");
        (id, uri)
    }

    // --- Scenario: the mobile browser gets the developer's response ---

    #[tokio::test]
    async fn returns_the_local_server_response_with_eruda_injected() {
        let pipeline = SpyPipeline::new(StubOutcome::Respond(html_page()));
        let (_, uri) = session_uri("");

        let (status, body) = send(app(Arc::clone(&pipeline), false), &uri).await;

        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("Local app"), "original content must be preserved");
        assert!(body.contains("eruda"), "Eruda must be injected into HTML");
    }

    #[tokio::test]
    async fn rewrites_the_target_path_and_query_for_the_local_server() {
        let pipeline = SpyPipeline::new(StubOutcome::Respond(html_page()));
        let (_, uri) = session_uri("/api/items?page=2");

        send(app(Arc::clone(&pipeline), false), &uri).await;

        let seen = pipeline.seen.lock().unwrap();
        let (_, request) = seen.as_ref().expect("pipeline must be called");
        assert_eq!(request.target, "/api/items?page=2");
        assert_eq!(request.method, "GET");
    }

    // --- Scenario: 404 when no session matches ---

    #[tokio::test]
    async fn returns_404_when_the_path_matches_no_session() {
        let pipeline = SpyPipeline::new(StubOutcome::SessionMissing);
        let (_, uri) = session_uri("");

        let (status, _) = send(app(pipeline, false), &uri).await;

        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn returns_404_for_paths_outside_the_session_namespace() {
        let pipeline = SpyPipeline::new(StubOutcome::Respond(html_page()));

        let (status, _) = send(app(pipeline, false), "/health").await;

        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    // --- Scenario: 502 when the tunnel is gone ---

    #[tokio::test]
    async fn returns_502_when_the_tunnel_is_unavailable() {
        let pipeline = SpyPipeline::new(StubOutcome::TunnelDown);
        let (_, uri) = session_uri("");

        let (status, _) = send(app(pipeline, false), &uri).await;

        assert_eq!(status, StatusCode::BAD_GATEWAY);
    }

    // --- Scenario: --no-eruda disables injection ---

    #[tokio::test]
    async fn does_not_inject_eruda_when_the_session_disabled_it() {
        let pipeline = SpyPipeline::new(StubOutcome::Respond(html_page()));
        let (_, uri) = session_uri("");

        let (status, body) = send(app(pipeline, true), &uri).await;

        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("Local app"));
        assert!(!body.contains("eruda"), "--no-eruda must keep the HTML untouched");
    }
}
