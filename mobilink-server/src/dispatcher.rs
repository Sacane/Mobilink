use std::sync::Arc;

use crate::{HttpRouter, PipelineError, RequestForwarder, RequestPipeline};

/// Orchestrates the full request lifecycle: resolves the session from the URL path,
/// forwards the raw HTTP request through the tunnel, and returns the raw HTTP response.
pub struct RequestDispatcher {
    router: Arc<dyn HttpRouter>,
    forwarder: Arc<dyn RequestForwarder>,
}

impl RequestDispatcher {
    pub fn new(router: Arc<dyn HttpRouter>, forwarder: Arc<dyn RequestForwarder>) -> Self {
        Self { router, forwarder }
    }
}

impl RequestPipeline for RequestDispatcher {
    fn handle(&self, path: &str, request: &[u8]) -> Result<Vec<u8>, PipelineError> {
        let session = self
            .router
            .resolve_session(path)
            .ok_or(PipelineError::SessionNotFound)?;
        self.forwarder
            .forward(&session, request)
            .map_err(PipelineError::ForwardFailed)
    }
}
