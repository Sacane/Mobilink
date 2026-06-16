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
use axum::http::header::{HOST, ORIGIN};
use axum::http::{HeaderMap, HeaderValue, Method, Request, Response, StatusCode};
use axum::middleware::{self, Next};

use mobilink_core::auth::AuthMode;
use mobilink_core::http::{HttpRequestData, HttpResponseData};
use mobilink_core::wire;

use crate::{PipelineError, RequestPipeline, ResponseTransformer, SessionOptions};

/// Maximum accepted request body size (32 MiB).
const MAX_BODY_BYTES: usize = 32 * 1024 * 1024;

/// Request headers that must not travel through the tunnel.
/// `accept-encoding` is stripped so the local server replies uncompressed,
/// which keeps HTML bodies injectable.
const STRIPPED_REQUEST_HEADERS: [&str; 5] = [
    "host",
    "connection",
    "accept-encoding",
    "content-length",
    "transfer-encoding",
];

/// Response headers the proxy recomputes itself instead of passing through.
const STRIPPED_RESPONSE_HEADERS: [&str; 3] = ["content-length", "transfer-encoding", "connection"];

/// Everything the public HTTP layer needs to serve a request.
#[derive(Clone)]
pub struct ProxyState {
    pub pipeline: Arc<dyn RequestPipeline>,
    pub transformer: Arc<dyn ResponseTransformer>,
    pub options: Arc<dyn SessionOptions>,
    /// Public base URL (scheme + host) this server is reached at. Used to tell
    /// the local app the real scheme via `X-Forwarded-Proto` when no upstream
    /// proxy already set one.
    pub public_url: Arc<str>,
}

/// Scheme of the configured public URL ("https" / "http"), defaulting to
/// "http" when the URL has no scheme.
fn public_scheme(public_url: &str) -> &str {
    public_url
        .split_once("://")
        .map(|(scheme, _)| scheme)
        .unwrap_or("http")
}

/// Builds the public-facing router. Every path goes through the proxy
/// handler and is forwarded verbatim to the active tunnel; when no tunnel
/// is connected the proxy returns a 404.
/// A CORS middleware wraps all responses so browser fetch/XHR from any
/// origin can reach the developer's local server without being blocked.
pub fn public_router(state: ProxyState) -> Router {
    Router::new()
        .fallback(proxy_handler)
        .with_state(state)
        .layer(middleware::from_fn(cors_layer))
}

async fn cors_layer(request: Request<Body>, next: Next) -> Response<Body> {
    let origin = request.headers().get(ORIGIN).cloned();
    if request.method() == Method::OPTIONS {
        let requested_headers = request
            .headers()
            .get("access-control-request-headers")
            .cloned();
        return cors_preflight_response(origin.as_ref(), requested_headers.as_ref());
    }
    let mut response = next.run(request).await;
    add_cors_headers(response.headers_mut(), origin.as_ref());
    response
}

/// Sets the CORS origin headers. When the caller sent an `Origin` we reflect
/// it and allow credentials — a wildcard origin is illegal for credentialed
/// (cookie / `Authorization`) requests, so reflecting is what lets the local
/// app's authenticated fetch/XHR succeed cross-origin. With no `Origin`
/// (non-credentialed) we keep the permissive wildcard.
fn add_cors_headers(headers: &mut HeaderMap, origin: Option<&HeaderValue>) {
    match origin {
        Some(origin) => {
            headers.insert("access-control-allow-origin", origin.clone());
            headers.insert(
                "access-control-allow-credentials",
                HeaderValue::from_static("true"),
            );
            headers.insert("vary", HeaderValue::from_static("Origin"));
        }
        None => {
            headers.insert("access-control-allow-origin", HeaderValue::from_static("*"));
        }
    }
}

fn cors_preflight_response(
    origin: Option<&HeaderValue>,
    requested_headers: Option<&HeaderValue>,
) -> Response<Body> {
    let mut response = Response::builder()
        .status(StatusCode::NO_CONTENT)
        .body(Body::empty())
        .expect("preflight response must build");
    let headers = response.headers_mut();
    add_cors_headers(headers, origin);
    headers.insert(
        "access-control-allow-methods",
        HeaderValue::from_static("GET, POST, PUT, DELETE, PATCH, OPTIONS, HEAD"),
    );
    // A wildcard `Access-Control-Allow-Headers` is ignored once credentials
    // are allowed, so reflect exactly what the browser asked to send.
    headers.insert(
        "access-control-allow-headers",
        requested_headers
            .cloned()
            .unwrap_or_else(|| HeaderValue::from_static("*")),
    );
    headers.insert("access-control-max-age", HeaderValue::from_static("86400"));
    response
}

async fn proxy_handler(State(state): State<ProxyState>, request: Request<Body>) -> Response<Body> {
    let (parts, body) = request.into_parts();
    // Whole-host routing: the full request path is forwarded verbatim to the
    // developer's local server, so the dev server's absolute asset paths
    // (`/_nuxt/...`, `/@vite/client`) resolve straight back through the tunnel.
    // The active tunnel is resolved by the pipeline, not parsed from the path.
    let public_path = parts.uri.path().to_string();

    let Ok(body_bytes) = axum::body::to_bytes(body, MAX_BODY_BYTES).await else {
        return text_response(StatusCode::PAYLOAD_TOO_LARGE, "Request body too large.\n");
    };

    let target = match parts.uri.query() {
        Some(query) => format!("{public_path}?{query}"),
        None => public_path.clone(),
    };

    // Resolve forwarded metadata before the Host header is dropped, so the
    // local app (e.g. Spring with `forward-headers-strategy`) learns the real
    // scheme + host it is being served under — which is what makes secure
    // cookies, login redirects and Origin/CSRF checks line up. An upstream
    // proxy's values (nginx) win; otherwise we fill them in ourselves.
    let forwarded_proto = parts
        .headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .unwrap_or_else(|| public_scheme(&state.public_url).to_string());
    let forwarded_host = parts
        .headers
        .get("x-forwarded-host")
        .or_else(|| parts.headers.get(HOST))
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    let mut headers: Vec<(String, String)> = parts
        .headers
        .iter()
        .filter(|(name, _)| !STRIPPED_REQUEST_HEADERS.contains(&name.as_str()))
        // Re-emitted canonically below from the resolved values.
        .filter(|(name, _)| {
            !name.as_str().eq_ignore_ascii_case("x-forwarded-proto")
                && !name.as_str().eq_ignore_ascii_case("x-forwarded-host")
        })
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|v| (name.as_str().to_string(), v.to_string()))
        })
        .collect();

    headers.push(("X-Forwarded-Proto".to_string(), forwarded_proto));
    if let Some(host) = forwarded_host {
        headers.push(("X-Forwarded-Host".to_string(), host));
    }

    let request_data = HttpRequestData {
        method: parts.method.as_str().to_string(),
        target,
        headers,
        body: body_bytes.to_vec(),
    };

    let Ok(encoded_request) = wire::encode(&request_data) else {
        return text_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to encode request.\n",
        );
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
            let Ok(mut response_data) = wire::decode::<HttpResponseData>(&response_bytes) else {
                return text_response(
                    StatusCode::BAD_GATEWAY,
                    "The tunnel returned an unreadable response.\n",
                );
            };
            if state.options.auth_mode() == AuthMode::Cookie {
                rewrite_cookies_cross_site(&mut response_data);
            }
            let response_data = if state.options.eruda_disabled() {
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

/// Rewrites every `Set-Cookie` header so the session cookie survives the hop
/// through the public host (see [`rewrite_set_cookie_cross_site`]).
fn rewrite_cookies_cross_site(response: &mut HttpResponseData) {
    for (name, value) in response.headers.iter_mut() {
        if name.eq_ignore_ascii_case("set-cookie") {
            *value = rewrite_set_cookie_cross_site(value);
        }
    }
}

/// Makes a single `Set-Cookie` value usable from the public host: drops any
/// `Domain` (so the cookie binds to the host the browser is actually on, not
/// `localhost`), forces `Secure`, and sets `SameSite=None` so the browser
/// stores and resends it even on cross-site requests. The cookie name/value
/// and every other attribute (Path, HttpOnly, Max-Age, Expires…) are kept.
fn rewrite_set_cookie_cross_site(set_cookie: &str) -> String {
    let mut parts = set_cookie.split(';').map(str::trim);
    let mut out = parts.next().unwrap_or("").to_string();
    let mut has_secure = false;

    for attr in parts {
        if attr.is_empty() {
            continue;
        }
        let key = attr
            .split('=')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        match key.as_str() {
            "domain" => continue,   // bind to the public host instead
            "samesite" => continue, // replaced by SameSite=None below
            "secure" => has_secure = true,
            _ => {}
        }
        out.push_str("; ");
        out.push_str(attr);
    }

    if !has_secure {
        out.push_str("; Secure");
    }
    out.push_str("; SameSite=None");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ForwardError;
    use crate::transform::ErudaInjector;
    use http_body_util::BodyExt;
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
            Arc::new(Self {
                outcome,
                seen: Mutex::new(None),
            })
        }
    }

    impl RequestPipeline for SpyPipeline {
        fn handle(&self, path: &str, request: &[u8]) -> Result<Vec<u8>, PipelineError> {
            let decoded: HttpRequestData =
                wire::decode(request).expect("handler sends valid frames");
            *self.seen.lock().unwrap() = Some((path.to_string(), decoded));
            match &self.outcome {
                StubOutcome::Respond(data) => Ok(wire::encode(data).unwrap()),
                StubOutcome::SessionMissing => Err(PipelineError::SessionNotFound),
                StubOutcome::TunnelDown => Err(PipelineError::ForwardFailed(
                    ForwardError::TunnelDisconnected,
                )),
            }
        }
    }

    struct ErudaAlwaysOn;
    impl SessionOptions for ErudaAlwaysOn {
        fn eruda_disabled(&self) -> bool {
            false
        }
    }

    struct ErudaAlwaysOff;
    impl SessionOptions for ErudaAlwaysOff {
        fn eruda_disabled(&self) -> bool {
            true
        }
    }

    /// Options for an app whose auth relies on a session cookie.
    struct CookieAuthOptions;
    impl SessionOptions for CookieAuthOptions {
        fn eruda_disabled(&self) -> bool {
            true
        }
        fn auth_mode(&self) -> AuthMode {
            AuthMode::Cookie
        }
    }

    fn html_page() -> HttpResponseData {
        HttpResponseData {
            status: 200,
            headers: vec![("Content-Type".to_string(), "text/html".to_string())],
            body: b"<html><body><h1>Local app</h1></body></html>".to_vec(),
        }
    }

    fn proxy_state(pipeline: Arc<SpyPipeline>, options: Arc<dyn SessionOptions>) -> ProxyState {
        ProxyState {
            pipeline,
            transformer: Arc::new(ErudaInjector),
            options,
            public_url: "https://public.test".into(),
        }
    }

    fn app(pipeline: Arc<SpyPipeline>, eruda_off: bool) -> Router {
        let options: Arc<dyn SessionOptions> = if eruda_off {
            Arc::new(ErudaAlwaysOff)
        } else {
            Arc::new(ErudaAlwaysOn)
        };
        public_router(proxy_state(pipeline, options))
    }

    fn app_with_options(pipeline: Arc<SpyPipeline>, options: Arc<dyn SessionOptions>) -> Router {
        public_router(proxy_state(pipeline, options))
    }

    fn response_with_cookie() -> HttpResponseData {
        HttpResponseData {
            status: 200,
            headers: vec![
                ("Content-Type".to_string(), "application/json".to_string()),
                (
                    "Set-Cookie".to_string(),
                    "session=xyz; Path=/; Domain=localhost; HttpOnly".to_string(),
                ),
            ],
            body: b"{}".to_vec(),
        }
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

    // --- Scenario: the mobile browser gets the developer's response ---

    #[tokio::test]
    async fn returns_the_local_server_response_with_eruda_injected() {
        let pipeline = SpyPipeline::new(StubOutcome::Respond(html_page()));

        let (status, body) = send(app(Arc::clone(&pipeline), false), "/").await;

        assert_eq!(status, StatusCode::OK);
        assert!(
            body.contains("Local app"),
            "original content must be preserved"
        );
        assert!(body.contains("eruda"), "Eruda must be injected into HTML");
    }

    #[tokio::test]
    async fn forwards_the_full_request_path_and_query_verbatim() {
        let pipeline = SpyPipeline::new(StubOutcome::Respond(html_page()));

        // No /s/<id> prefix: the path reaches the local server untouched.
        send(app(Arc::clone(&pipeline), false), "/api/items?page=2").await;

        let seen = pipeline.seen.lock().unwrap();
        let (_, request) = seen.as_ref().expect("pipeline must be called");
        assert_eq!(request.target, "/api/items?page=2");
        assert_eq!(request.method, "GET");
    }

    #[tokio::test]
    async fn forwards_absolute_asset_paths_without_a_session_prefix() {
        let pipeline = SpyPipeline::new(StubOutcome::Respond(html_page()));

        // The kind of absolute asset path a Vite/Nuxt dev server emits.
        send(app(Arc::clone(&pipeline), false), "/_nuxt/@vite/client").await;

        let seen = pipeline.seen.lock().unwrap();
        let (_, request) = seen.as_ref().expect("pipeline must be called");
        assert_eq!(request.target, "/_nuxt/@vite/client");
    }

    // --- Scenario: 404 when no session matches ---

    #[tokio::test]
    async fn returns_404_when_no_tunnel_is_active() {
        let pipeline = SpyPipeline::new(StubOutcome::SessionMissing);

        let (status, _) = send(app(pipeline, false), "/").await;

        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    // --- Scenario: 502 when the tunnel is gone ---

    #[tokio::test]
    async fn returns_502_when_the_tunnel_is_unavailable() {
        let pipeline = SpyPipeline::new(StubOutcome::TunnelDown);

        let (status, _) = send(app(pipeline, false), "/").await;

        assert_eq!(status, StatusCode::BAD_GATEWAY);
    }

    // --- Scenario: --no-eruda disables injection ---

    #[tokio::test]
    async fn does_not_inject_eruda_when_the_session_disabled_it() {
        let pipeline = SpyPipeline::new(StubOutcome::Respond(html_page()));

        let (status, body) = send(app(pipeline, true), "/").await;

        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("Local app"));
        assert!(
            !body.contains("eruda"),
            "--no-eruda must keep the HTML untouched"
        );
    }

    // --- Scenario: CORS ---

    #[tokio::test]
    async fn cors_headers_are_present_on_successful_proxy_response() {
        let pipeline = SpyPipeline::new(StubOutcome::Respond(html_page()));

        let response = app(Arc::clone(&pipeline), false)
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get("access-control-allow-origin")
                .unwrap(),
            "*"
        );
    }

    #[tokio::test]
    async fn cors_headers_are_present_on_error_responses() {
        let pipeline = SpyPipeline::new(StubOutcome::SessionMissing);

        let response = app(pipeline, false)
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            response
                .headers()
                .get("access-control-allow-origin")
                .unwrap(),
            "*"
        );
    }

    #[tokio::test]
    async fn options_preflight_returns_no_content_with_cors_headers() {
        let pipeline = SpyPipeline::new(StubOutcome::SessionMissing);

        let response = app(pipeline, false)
            .oneshot(
                Request::builder()
                    .method("OPTIONS")
                    .uri("/api/data")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            response
                .headers()
                .get("access-control-allow-origin")
                .unwrap(),
            "*"
        );
        assert!(response.headers().get("access-control-max-age").is_some());
    }

    #[tokio::test]
    async fn cors_reflects_origin_and_allows_credentials_for_credentialed_requests() {
        let pipeline = SpyPipeline::new(StubOutcome::Respond(html_page()));

        let request = Request::builder()
            .uri("/")
            .header("origin", "https://app.example.com")
            .body(Body::empty())
            .unwrap();
        let response = app(Arc::clone(&pipeline), false)
            .oneshot(request)
            .await
            .unwrap();

        assert_eq!(
            response
                .headers()
                .get("access-control-allow-origin")
                .unwrap(),
            "https://app.example.com",
            "a wildcard origin is illegal with credentials; it must be reflected"
        );
        assert_eq!(
            response
                .headers()
                .get("access-control-allow-credentials")
                .unwrap(),
            "true"
        );
    }

    // --- Scenario: forwarded headers tell the local app the real scheme/host ---

    fn forwarded_header<'a>(request: &'a HttpRequestData, name: &str) -> Option<&'a str> {
        request
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    #[tokio::test]
    async fn injects_x_forwarded_proto_and_host_for_the_local_app() {
        let pipeline = SpyPipeline::new(StubOutcome::Respond(html_page()));

        let request = Request::builder()
            .uri("/")
            .header("host", "mobilink.sacane.fr")
            .body(Body::empty())
            .unwrap();
        app(Arc::clone(&pipeline), false)
            .oneshot(request)
            .await
            .unwrap();

        let seen = pipeline.seen.lock().unwrap();
        let (_, req) = seen.as_ref().expect("pipeline must be called");
        assert_eq!(
            forwarded_header(req, "x-forwarded-host"),
            Some("mobilink.sacane.fr")
        );
        assert_eq!(
            forwarded_header(req, "x-forwarded-proto"),
            Some("https"),
            "scheme is taken from the configured public URL"
        );
    }

    #[tokio::test]
    async fn an_upstream_x_forwarded_proto_is_preserved_and_not_duplicated() {
        let pipeline = SpyPipeline::new(StubOutcome::Respond(html_page()));

        let request = Request::builder()
            .uri("/")
            .header("host", "localhost")
            .header("x-forwarded-proto", "https")
            .body(Body::empty())
            .unwrap();
        app(Arc::clone(&pipeline), false)
            .oneshot(request)
            .await
            .unwrap();

        let seen = pipeline.seen.lock().unwrap();
        let (_, req) = seen.as_ref().unwrap();
        assert_eq!(forwarded_header(req, "x-forwarded-proto"), Some("https"));
        let count = req
            .headers
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case("x-forwarded-proto"))
            .count();
        assert_eq!(count, 1, "X-Forwarded-Proto must be emitted exactly once");
    }

    // --- Scenario: --auth cookie rewrites Set-Cookie to survive the tunnel ---

    #[test]
    fn rewriting_a_cookie_forces_secure_and_samesite_none() {
        let rewritten = rewrite_set_cookie_cross_site("session=abc; Path=/; HttpOnly");

        assert!(rewritten.starts_with("session=abc"));
        assert!(rewritten.contains("Path=/"));
        assert!(rewritten.contains("HttpOnly"));
        assert!(rewritten.contains("Secure"));
        assert!(rewritten.contains("SameSite=None"));
    }

    #[test]
    fn rewriting_a_cookie_strips_the_domain_attribute() {
        let rewritten = rewrite_set_cookie_cross_site("id=42; Domain=localhost; Path=/");

        assert!(
            !rewritten.to_ascii_lowercase().contains("domain"),
            "Domain must be dropped so the cookie binds to the public host, got: {rewritten}"
        );
    }

    #[test]
    fn rewriting_a_cookie_does_not_duplicate_secure_or_samesite() {
        let rewritten = rewrite_set_cookie_cross_site("a=b; Secure; SameSite=Lax");

        assert_eq!(rewritten.matches("Secure").count(), 1);
        assert_eq!(rewritten.matches("SameSite").count(), 1);
        assert!(rewritten.contains("SameSite=None"));
        assert!(!rewritten.contains("SameSite=Lax"));
    }

    #[tokio::test]
    async fn cookie_auth_mode_rewrites_set_cookie_on_the_response() {
        let pipeline = SpyPipeline::new(StubOutcome::Respond(response_with_cookie()));

        let response = app_with_options(pipeline, Arc::new(CookieAuthOptions))
            .oneshot(
                Request::builder()
                    .uri("/login")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let set_cookie = response
            .headers()
            .get("set-cookie")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(set_cookie.contains("SameSite=None"), "got: {set_cookie}");
        assert!(set_cookie.contains("Secure"), "got: {set_cookie}");
        assert!(!set_cookie.to_ascii_lowercase().contains("domain"));
    }

    #[tokio::test]
    async fn passthrough_mode_leaves_set_cookie_untouched() {
        let pipeline = SpyPipeline::new(StubOutcome::Respond(response_with_cookie()));

        // app(.., true) uses the default auth mode (passthrough).
        let response = app(pipeline, true)
            .oneshot(
                Request::builder()
                    .uri("/login")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let set_cookie = response
            .headers()
            .get("set-cookie")
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(
            set_cookie,
            "session=xyz; Path=/; Domain=localhost; HttpOnly"
        );
    }
}
