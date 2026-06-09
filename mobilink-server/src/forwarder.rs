use mobilink_core::session::Session;

use crate::{ForwardError, RequestForwarder};

/// Forwards HTTP requests to the developer's local server via a QUIC tunnel.
/// The QUIC connection management will be implemented in Phase 2.
pub struct QuicForwarder;

impl RequestForwarder for QuicForwarder {
    fn forward(&self, _session: &Session, _request: &[u8]) -> Result<Vec<u8>, ForwardError> {
        // Will be implemented when the QUIC tunnel layer is built (Phase 2).
        Err(ForwardError::TunnelNotFound)
    }
}
