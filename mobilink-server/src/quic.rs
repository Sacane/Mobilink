//! QUIC tunnel endpoint: accepts CLI connections, performs the handshake,
//! keeps track of live tunnels and forwards HTTP frames through them.
//!
//! Wire convention (see mobilink-core::wire): one message per stream
//! direction. The CLI opens the first bidirectional stream for the
//! handshake; afterwards the server opens one bidirectional stream per
//! proxied HTTP request.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use quinn::Endpoint;
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};

use mobilink_core::message::{ClientMessage, ServerMessage};
use mobilink_core::session::SessionId;
use mobilink_core::wire;

use crate::{
    ForwardError, RequestForwarder, SessionOptions, SessionRegistry, TunnelHandshakeHandler,
};

/// Maximum size of a single tunnel frame (32 MiB), matching the HTTP layer.
pub const MAX_FRAME_BYTES: usize = 32 * 1024 * 1024;

/// A live QUIC connection to a developer's CLI, plus its session options.
struct TunnelEntry {
    connection: quinn::Connection,
    no_eruda: bool,
}

/// Thread-safe map of active tunnels, keyed by session.
#[derive(Default)]
pub struct TunnelMap {
    inner: Mutex<HashMap<SessionId, TunnelEntry>>,
}

impl TunnelMap {
    pub fn new() -> Self {
        Self::default()
    }

    fn insert(&self, id: SessionId, connection: quinn::Connection, no_eruda: bool) {
        let mut inner = self.inner.lock().expect("tunnel map lock");
        inner.insert(
            id,
            TunnelEntry {
                connection,
                no_eruda,
            },
        );
    }

    fn remove(&self, id: &SessionId) {
        let mut inner = self.inner.lock().expect("tunnel map lock");
        inner.remove(id);
    }

    fn connection(&self, id: &SessionId) -> Option<quinn::Connection> {
        let inner = self.inner.lock().expect("tunnel map lock");
        inner.get(id).map(|entry| entry.connection.clone())
    }
}

impl SessionOptions for TunnelMap {
    fn eruda_disabled(&self, id: &SessionId) -> bool {
        let inner = self.inner.lock().expect("tunnel map lock");
        inner.get(id).is_some_and(|entry| entry.no_eruda)
    }
}

/// Real [`RequestForwarder`]: sends the request frame over a fresh QUIC
/// stream of the session's tunnel and waits for the response frame.
pub struct QuicTunnelForwarder {
    tunnels: Arc<TunnelMap>,
    runtime: tokio::runtime::Handle,
}

impl QuicTunnelForwarder {
    pub fn new(tunnels: Arc<TunnelMap>, runtime: tokio::runtime::Handle) -> Self {
        Self { tunnels, runtime }
    }
}

impl RequestForwarder for QuicTunnelForwarder {
    fn forward(
        &self,
        session: &mobilink_core::session::Session,
        request: &[u8],
    ) -> Result<Vec<u8>, ForwardError> {
        let connection = self
            .tunnels
            .connection(&session.id)
            .ok_or(ForwardError::TunnelNotFound)?;
        let request = request.to_vec();

        // This trait is sync (domain contract); we are called from a
        // blocking thread, so re-entering the runtime here is safe.
        self.runtime.block_on(async move {
            let (mut send, mut recv) = connection
                .open_bi()
                .await
                .map_err(|_| ForwardError::TunnelDisconnected)?;
            send.write_all(&request)
                .await
                .map_err(|_| ForwardError::TunnelDisconnected)?;
            send.finish()
                .map_err(|_| ForwardError::TunnelDisconnected)?;
            recv.read_to_end(MAX_FRAME_BYTES)
                .await
                .map_err(|_| ForwardError::TunnelDisconnected)
        })
    }
}

/// Creates the server-side QUIC endpoint with a freshly generated
/// self-signed certificate (MVP: the CLI skips verification).
pub fn make_server_endpoint(bind: SocketAddr) -> Result<Endpoint, Box<dyn std::error::Error>> {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])?;
    let cert_der = CertificateDer::from(cert.cert);
    let key_der = PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der());

    let server_config = quinn::ServerConfig::with_single_cert(vec![cert_der], key_der.into())?;
    let endpoint = Endpoint::server(server_config, bind)?;
    Ok(endpoint)
}

/// Accept loop: each incoming CLI connection is handled on its own task.
pub async fn run_tunnel_endpoint(
    endpoint: Endpoint,
    registry: Arc<dyn SessionRegistry>,
    handshake: Arc<dyn TunnelHandshakeHandler>,
    tunnels: Arc<TunnelMap>,
) {
    while let Some(incoming) = endpoint.accept().await {
        let registry = Arc::clone(&registry);
        let handshake = Arc::clone(&handshake);
        let tunnels = Arc::clone(&tunnels);
        tokio::spawn(async move {
            if let Err(error) = handle_connection(incoming, registry, handshake, tunnels).await {
                tracing::warn!(%error, "tunnel connection ended with an error");
            }
        });
    }
}

/// Performs the handshake, registers the tunnel, then waits for the
/// connection to close to clean everything up.
async fn handle_connection(
    incoming: quinn::Incoming,
    registry: Arc<dyn SessionRegistry>,
    handshake: Arc<dyn TunnelHandshakeHandler>,
    tunnels: Arc<TunnelMap>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let connection = incoming.await?;

    // The CLI opens the first bidirectional stream and sends Hello.
    let (mut send, mut recv) = connection.accept_bi().await?;
    let hello_bytes = recv.read_to_end(MAX_FRAME_BYTES).await?;
    let ClientMessage::Hello {
        local_port,
        no_eruda,
    } = wire::decode(&hello_bytes)?;

    let response = handshake
        .handle_hello(local_port)
        .map_err(|e| format!("handshake refused: {e:?}"))?;
    let ServerMessage::SessionCreated {
        session_id,
        public_url,
    } = &response;
    let session_id = session_id.clone();

    tunnels.insert(session_id.clone(), connection.clone(), no_eruda);
    send.write_all(&wire::encode(&response)?).await?;
    send.finish()?;

    tracing::info!(%session_id, %public_url, local_port, "tunnel established");

    // Block until the CLI goes away (Ctrl+C, crash, network loss)…
    let reason = connection.closed().await;
    tracing::info!(%session_id, ?reason, "tunnel closed");

    // …then forget both the tunnel and the session.
    tunnels.remove(&session_id);
    let _ = registry.close_session(&session_id);
    Ok(())
}
