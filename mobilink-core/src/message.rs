use crate::auth::AuthMode;
use crate::session::SessionId;
use serde::{Deserialize, Serialize};

/// Control messages sent from the CLI to the server during the handshake.
///
/// HTTP traffic itself does not travel through these variants: each proxied
/// request flows on its own QUIC stream as [`crate::http::HttpRequestData`] /
/// [`crate::http::HttpResponseData`] frames.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMessage {
    /// CLI announces itself and requests a session on a given local port.
    Hello {
        /// Local port the developer wants to expose.
        local_port: u16,
        /// When true, the server must not inject the Eruda debug script.
        no_eruda: bool,
        /// How the server should adapt cookies/headers to the local app's
        /// authentication scheme (declared with `--auth`).
        auth: AuthMode,
    },
}

/// Control messages sent from the server to the CLI during the handshake.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMessage {
    /// Server confirms the session and provides the public URL.
    SessionCreated {
        session_id: SessionId,
        public_url: String,
    },
}
