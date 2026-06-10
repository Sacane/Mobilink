//! End-to-end test: real Mobilink server + real CLI code + real local app,
//! all on localhost. A "mobile browser" (reqwest) hits the public URL and
//! must receive the local app's response — with Eruda injected into HTML.
//!
//! Full path under test:
//! browser → axum public endpoint → pipeline → QUIC stream → CLI serve loop
//!         → local axum app → back through the same stream → browser.

use std::sync::Arc;

use mobilink_server::dispatcher::RequestDispatcher;
use mobilink_server::handshake::SessionHandshakeHandler;
use mobilink_server::http::{ProxyState, public_router};
use mobilink_server::quic::{
    QuicTunnelForwarder, TunnelMap, make_server_endpoint, run_tunnel_endpoint,
};
use mobilink_server::registry::InMemorySessionRegistry;
use mobilink_server::router::SessionRouter;
use mobilink_server::transform::ErudaInjector;

use mobilink_cli::{tls, tunnel};

/// A stand-in for the developer's app: one HTML page, one JSON route.
async fn start_local_app() -> u16 {
    let app = axum::Router::new()
        .route(
            "/",
            axum::routing::get(|| async {
                axum::response::Html("<html><body><h1>Hello Mobilink</h1></body></html>")
            }),
        )
        .route(
            "/api/ping",
            axum::routing::get(|| async {
                ([("content-type", "application/json")], r#"{"pong":true}"#)
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    port
}

struct Mobilink {
    quic_addr: std::net::SocketAddr,
    public_base: String,
}

/// Boots a complete Mobilink server (QUIC + public HTTP) on random ports.
async fn start_mobilink_server() -> Mobilink {
    // Bind the public HTTP side first so the registry can build real URLs.
    let http_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let public_base = format!("http://{}", http_listener.local_addr().unwrap());

    let registry = Arc::new(InMemorySessionRegistry::new(public_base.clone()));
    let tunnels = Arc::new(TunnelMap::new());
    let handshake = Arc::new(SessionHandshakeHandler::new(registry.clone()));
    let forwarder = Arc::new(QuicTunnelForwarder::new(
        tunnels.clone(),
        tokio::runtime::Handle::current(),
    ));
    let pipeline = Arc::new(RequestDispatcher::new(
        Arc::new(SessionRouter::new(registry.clone())),
        forwarder,
    ));

    let endpoint = make_server_endpoint("127.0.0.1:0".parse().unwrap()).unwrap();
    let quic_addr = endpoint.local_addr().unwrap();
    tokio::spawn(run_tunnel_endpoint(endpoint, registry, handshake, tunnels.clone()));

    let app = public_router(ProxyState {
        pipeline,
        transformer: Arc::new(ErudaInjector),
        options: tunnels,
    });
    tokio::spawn(async move { axum::serve(http_listener, app).await.unwrap() });

    Mobilink { quic_addr, public_base }
}

/// Runs the real CLI flow (handshake + serve loop) and returns the session.
async fn start_cli(server: &Mobilink, local_port: u16, no_eruda: bool) -> tunnel::TunnelSession {
    let endpoint = tls::insecure_client_endpoint().unwrap();
    let session =
        tunnel::connect_and_handshake(&endpoint, server.quic_addr, "localhost", local_port, no_eruda)
            .await
            .unwrap();
    tokio::spawn(tunnel::serve(session.connection.clone(), local_port));
    // Keep the endpoint alive for the whole test.
    std::mem::forget(endpoint);
    session
}

#[tokio::test]
async fn the_mobile_browser_sees_the_local_app_with_eruda_injected() {
    let local_port = start_local_app().await;
    let server = start_mobilink_server().await;
    let session = start_cli(&server, local_port, false).await;

    assert!(
        session.public_url.starts_with(&server.public_base),
        "the public URL must live under the server's base"
    );

    let browser = reqwest::Client::new();
    let response = browser.get(&session.public_url).send().await.unwrap();
    assert_eq!(response.status().as_u16(), 200);
    let html = response.text().await.unwrap();
    assert!(html.contains("Hello Mobilink"), "local content must be relayed");
    assert!(html.contains("eruda"), "Eruda must be injected into HTML");
}

#[tokio::test]
async fn json_routes_pass_through_untouched_and_subpaths_are_rewritten() {
    let local_port = start_local_app().await;
    let server = start_mobilink_server().await;
    let session = start_cli(&server, local_port, false).await;

    let browser = reqwest::Client::new();
    let url = format!("{}/api/ping", session.public_url);
    let response = browser.get(&url).send().await.unwrap();

    assert_eq!(response.status().as_u16(), 200);
    let body = response.text().await.unwrap();
    assert_eq!(body, r#"{"pong":true}"#, "JSON must not be modified");
}

#[tokio::test]
async fn no_eruda_keeps_html_untouched_end_to_end() {
    let local_port = start_local_app().await;
    let server = start_mobilink_server().await;
    let session = start_cli(&server, local_port, true).await;

    let browser = reqwest::Client::new();
    let html = browser
        .get(&session.public_url)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    assert!(html.contains("Hello Mobilink"));
    assert!(!html.contains("eruda"), "--no-eruda must disable injection");
}

#[tokio::test]
async fn a_dead_local_server_yields_a_502_for_the_mobile_browser() {
    // Nothing listens on this port.
    let dead_port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let server = start_mobilink_server().await;
    let session = start_cli(&server, dead_port, false).await;

    let browser = reqwest::Client::new();
    let response = browser.get(&session.public_url).send().await.unwrap();

    assert_eq!(response.status().as_u16(), 502);
}

#[tokio::test]
async fn closing_the_cli_makes_the_public_url_a_404() {
    let local_port = start_local_app().await;
    let server = start_mobilink_server().await;
    let session = start_cli(&server, local_port, false).await;

    session.connection.close(0u32.into(), b"bye");

    // Wait for the server-side cleanup, then the URL must be gone.
    let browser = reqwest::Client::new();
    for _ in 0..30 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let status = browser
            .get(&session.public_url)
            .send()
            .await
            .unwrap()
            .status()
            .as_u16();
        if status == 404 {
            return;
        }
    }
    panic!("the public URL must return 404 once the CLI is gone");
}
