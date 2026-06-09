use std::sync::Arc;

use mobilink_core::session::Session;

use crate::{HttpRouter, SessionRegistry};

/// Routes incoming HTTP requests to the correct active session
/// by parsing the session ID from the URL path.
pub struct SessionRouter {
    registry: Arc<dyn SessionRegistry>,
}

impl SessionRouter {
    pub fn new(registry: Arc<dyn SessionRegistry>) -> Self {
        Self { registry }
    }
}

impl HttpRouter for SessionRouter {
    fn resolve_session(&self, path: &str) -> Option<Session> {
        let id_str = path.strip_prefix("/s/")?;
        let session_id = id_str.parse().ok()?;
        self.registry.get_session(&session_id)
    }
}
