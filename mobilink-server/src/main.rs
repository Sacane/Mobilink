//! Mobilink server binary: wires the domain components to the real
//! network layers (QUIC tunnel endpoint + public HTTP endpoint).
//!
//! Configuration via environment variables:
//! - `MOBILINK_QUIC_BIND`  (default 0.0.0.0:4433) — UDP, tunnel side
//! - `MOBILINK_HTTP_BIND`  (default 0.0.0.0:8080) — TCP, public side
//! - `MOBILINK_PUBLIC_URL` (default http://localhost:8080) — base of public URLs

use std::sync::Arc;

use mobilink_core::config::ServerConfig;
use mobilink_server::dispatcher::RequestDispatcher;
use mobilink_server::handshake::SessionHandshakeHandler;
use mobilink_server::http::{ProxyState, public_router};
use mobilink_server::quic::{
    QuicTunnelForwarder, TunnelMap, make_server_endpoint, run_tunnel_endpoint,
};
use mobilink_server::registry::InMemorySessionRegistry;
use mobilink_server::router::SessionRouter;
use mobilink_server::transform::ErudaInjector;

fn config_from_env() -> ServerConfig {
    let defaults = ServerConfig::default();
    ServerConfig {
        quic_bind: std::env::var("MOBILINK_QUIC_BIND").unwrap_or(defaults.quic_bind),
        http_bind: std::env::var("MOBILINK_HTTP_BIND").unwrap_or(defaults.http_bind),
        public_url: std::env::var("MOBILINK_PUBLIC_URL").unwrap_or(defaults.public_url),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let config = config_from_env();

    // Domain components, assembled exactly like in the tests.
    let registry = Arc::new(InMemorySessionRegistry::new(config.public_url.clone()));
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

    // QUIC tunnel endpoint (CLI side).
    let endpoint = make_server_endpoint(config.quic_bind.parse()?)?;
    tracing::info!(addr = %config.quic_bind, "QUIC tunnel endpoint listening");
    tokio::spawn(run_tunnel_endpoint(
        endpoint,
        registry.clone(),
        handshake,
        tunnels.clone(),
    ));

    // Public HTTP endpoint (mobile side).
    let app = public_router(ProxyState {
        pipeline,
        transformer: Arc::new(ErudaInjector),
        options: tunnels,
    });
    let listener = tokio::net::TcpListener::bind(&config.http_bind).await?;
    tracing::info!(addr = %config.http_bind, public_url = %config.public_url, "public HTTP endpoint listening");
    axum::serve(listener, app).await?;

    Ok(())
}
