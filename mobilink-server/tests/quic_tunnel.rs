//! Integration tests for the QUIC tunnel layer, played against a real
//! quinn client on localhost. They cover the Gherkin scenarios of
//! `establishing_the_quic_tunnel.md` and the transport half of
//! `forwarding_a_request_through_the_tunnel.md`.

use std::sync::Arc;
use std::time::Duration;

use mobilink_core::http::HttpResponseData;
use mobilink_core::message::{ClientMessage, ServerMessage};
use mobilink_core::wire;

use mobilink_server::handshake::SessionHandshakeHandler;
use mobilink_server::quic::{
    MAX_FRAME_BYTES, QuicTunnelForwarder, TunnelMap, make_server_endpoint, run_tunnel_endpoint,
};
use mobilink_server::registry::InMemorySessionRegistry;
use mobilink_server::{RequestForwarder, SessionOptions, SessionRegistry};

mod common;
use common::connect_insecure;

struct Harness {
    registry: Arc<InMemorySessionRegistry>,
    tunnels: Arc<TunnelMap>,
    server_addr: std::net::SocketAddr,
    _endpoint: quinn::Endpoint,
}

/// Starts a real tunnel endpoint on a random localhost port.
fn start_server() -> Harness {
    let registry = Arc::new(InMemorySessionRegistry::new("http://127.0.0.1:8060"));
    let tunnels = Arc::new(TunnelMap::new());
    let handler = Arc::new(SessionHandshakeHandler::new(registry.clone()));

    let endpoint = make_server_endpoint("127.0.0.1:0".parse().unwrap()).expect("server endpoint");
    let server_addr = endpoint.local_addr().expect("local addr");

    tokio::spawn(run_tunnel_endpoint(
        endpoint.clone(),
        registry.clone(),
        handler,
        tunnels.clone(),
    ));

    Harness {
        registry,
        tunnels,
        server_addr,
        _endpoint: endpoint,
    }
}

/// Plays the CLI side of the handshake and returns the created session info.
async fn handshake(connection: &quinn::Connection, no_eruda: bool) -> ServerMessage {
    let (mut send, mut recv) = connection.open_bi().await.expect("open handshake stream");
    let hello = ClientMessage::Hello {
        local_port: 3000,
        no_eruda,
        auth: mobilink_core::auth::AuthMode::Passthrough,
    };
    send.write_all(&wire::encode(&hello).unwrap())
        .await
        .expect("send hello");
    send.finish().expect("finish hello");
    let bytes = recv
        .read_to_end(MAX_FRAME_BYTES)
        .await
        .expect("read session created");
    wire::decode(&bytes).expect("decode session created")
}

#[tokio::test]
async fn cli_handshake_creates_a_session_with_an_active_tunnel() {
    let server = start_server();

    let (connection, _client) = connect_insecure(server.server_addr).await;
    let ServerMessage::SessionCreated {
        session_id,
        public_url,
    } = handshake(&connection, false).await;

    // The session exists in the registry…
    let session = server.registry.get_session(&session_id);
    assert!(session.is_some(), "handshake must create a session");
    assert_eq!(
        public_url, "http://127.0.0.1:8060",
        "whole-host routing exposes the host root as the public URL"
    );
    // …and Eruda is enabled by default.
    assert!(!server.tunnels.eruda_disabled());
}

#[tokio::test]
async fn no_eruda_flag_is_remembered_for_the_session() {
    let server = start_server();

    let (connection, _client) = connect_insecure(server.server_addr).await;
    let ServerMessage::SessionCreated { .. } = handshake(&connection, true).await;

    assert!(
        server.tunnels.eruda_disabled(),
        "--no-eruda must be remembered server-side"
    );
}

#[tokio::test]
async fn a_request_frame_travels_through_the_tunnel_and_back() {
    let server = start_server();

    let (connection, _client) = connect_insecure(server.server_addr).await;
    let ServerMessage::SessionCreated { session_id, .. } = handshake(&connection, false).await;

    // CLI side: answer every incoming stream with a fixed response frame.
    let cli_connection = connection.clone();
    tokio::spawn(async move {
        while let Ok((mut send, mut recv)) = cli_connection.accept_bi().await {
            let _request = recv.read_to_end(MAX_FRAME_BYTES).await.unwrap();
            let response = HttpResponseData {
                status: 200,
                headers: vec![("Content-Type".to_string(), "text/plain".to_string())],
                body: b"hello from localhost".to_vec(),
            };
            send.write_all(&wire::encode(&response).unwrap())
                .await
                .unwrap();
            send.finish().unwrap();
        }
    });

    let session = server
        .registry
        .get_session(&session_id)
        .expect("session exists");
    let forwarder = Arc::new(QuicTunnelForwarder::new(
        server.tunnels.clone(),
        tokio::runtime::Handle::current(),
    ));

    // The domain trait is sync: call it from a blocking thread like the HTTP layer does.
    let result = tokio::task::spawn_blocking(move || forwarder.forward(&session, b"frame"))
        .await
        .expect("blocking task");

    let bytes = result.expect("forwarding should succeed");
    let response: HttpResponseData = wire::decode(&bytes).expect("valid response frame");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"hello from localhost".to_vec());
}

#[tokio::test]
async fn closing_the_cli_connection_closes_the_session() {
    let server = start_server();

    let (connection, _client) = connect_insecure(server.server_addr).await;
    let ServerMessage::SessionCreated { session_id, .. } = handshake(&connection, false).await;
    assert!(server.registry.get_session(&session_id).is_some());

    // The developer stops the CLI.
    connection.close(0u32.into(), b"bye");

    // The server must notice and clean up (poll up to 3s).
    for _ in 0..30 {
        if server.registry.get_session(&session_id).is_none() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("session should be closed after the CLI disconnects");
}
