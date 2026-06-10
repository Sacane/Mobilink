//! The CLI side of the QUIC tunnel: handshake, then a serve loop that
//! answers every incoming stream with a replay against the local server.
//!
//! Wire convention (mobilink-core::wire): one message per stream direction.
//! The CLI opens the first bidirectional stream for the handshake; the
//! server then opens one bidirectional stream per proxied HTTP request.

use std::net::SocketAddr;
use std::time::Instant;

use mobilink_core::http::HttpRequestData;
use mobilink_core::message::{ClientMessage, ServerMessage};
use mobilink_core::session::SessionId;
use mobilink_core::wire::{self, MAX_FRAME_BYTES};

use crate::{local, ui};

/// A successfully established tunnel session.
pub struct TunnelSession {
    pub session_id: SessionId,
    pub public_url: String,
    pub connection: quinn::Connection,
}

/// Connects to the server and performs the Hello → SessionCreated handshake.
pub async fn connect_and_handshake(
    endpoint: &quinn::Endpoint,
    server_addr: SocketAddr,
    server_name: &str,
    local_port: u16,
    no_eruda: bool,
) -> Result<TunnelSession, Box<dyn std::error::Error>> {
    let connection = endpoint.connect(server_addr, server_name)?.await?;

    let (mut send, mut recv) = connection.open_bi().await?;
    let hello = ClientMessage::Hello { local_port, no_eruda };
    send.write_all(&wire::encode(&hello)?).await?;
    send.finish()?;

    let bytes = recv.read_to_end(MAX_FRAME_BYTES).await?;
    let ServerMessage::SessionCreated { session_id, public_url } = wire::decode(&bytes)?;

    Ok(TunnelSession { session_id, public_url, connection })
}

/// Serve loop: accepts request streams until the connection closes.
/// Each stream is handled on its own task, so a slow page never blocks
/// the rest of the session (Gherkin: cli_serving_local_traffic.md).
pub async fn serve(connection: quinn::Connection, local_port: u16) {
    let client = reqwest::Client::new();
    while let Ok((send, recv)) = connection.accept_bi().await {
        let client = client.clone();
        tokio::spawn(async move {
            if let Err(error) = handle_stream(send, recv, client, local_port).await {
                tracing::warn!(%error, "request stream failed");
            }
        });
    }
}

async fn handle_stream(
    mut send: quinn::SendStream,
    mut recv: quinn::RecvStream,
    client: reqwest::Client,
    local_port: u16,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let started = Instant::now();

    let bytes = recv.read_to_end(MAX_FRAME_BYTES).await?;
    let request: HttpRequestData = wire::decode(&bytes)?;
    let method = request.method.clone();
    let target = request.target.clone();

    let response = local::replay_locally(&client, local_port, request).await;
    let status = response.status;

    send.write_all(&wire::encode(&response)?).await?;
    send.finish()?;

    println!(
        "{}",
        ui::format_log_line(&method, &target, status, started.elapsed().as_millis())
    );
    Ok(())
}
