use serde::{Deserialize, Serialize};
use crate::session::SessionId;

/// Messages sent from the CLI to the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMessage {
    /// CLI announces itself and requests a session on a given local port.
    Hello { local_port: u16 },
    /// Raw HTTP response bytes forwarded back from localhost.
    HttpResponse { request_id: u32, payload: Vec<u8> },
}

/// Messages sent from the server to the CLI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMessage {
    /// Server confirms the session and provides the public URL.
    SessionCreated { session_id: SessionId, public_url: String },
    /// Server forwards an inbound HTTP request from mobile to the CLI.
    HttpRequest { request_id: u32, payload: Vec<u8> },
    /// Server notifies the CLI that the session was closed.
    SessionClosed,
}
