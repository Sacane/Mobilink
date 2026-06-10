use std::sync::Arc;

use mobilink_core::message::ServerMessage;

use crate::{HandshakeError, SessionRegistry, TunnelHandshakeHandler};

/// Handles the QUIC handshake when a CLI connects.
/// Delegates session creation to the registry and builds the `SessionCreated` response.
pub struct SessionHandshakeHandler {
    registry: Arc<dyn SessionRegistry>,
}

impl SessionHandshakeHandler {
    pub fn new(registry: Arc<dyn SessionRegistry>) -> Self {
        Self { registry }
    }
}

impl TunnelHandshakeHandler for SessionHandshakeHandler {
    fn handle_hello(&self, local_port: u16) -> Result<ServerMessage, HandshakeError> {
        let session = self
            .registry
            .open_session(local_port)
            .ok_or(HandshakeError::SessionCreationFailed)?;

        Ok(ServerMessage::SessionCreated {
            session_id: session.id,
            public_url: session.public_url,
        })
    }
}
