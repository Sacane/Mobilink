use std::sync::Arc;

use mobilink_core::session::Session;

use crate::{HttpRouter, SessionRegistry};

/// Routes incoming HTTP requests to the active tunnel session.
///
/// Under whole-host routing the public host is dedicated to a single tunnel,
/// so the session is resolved independently of the request path — every path
/// (including absolute asset paths like `/_nuxt/...`) reaches the same
/// developer's local server.
pub struct SessionRouter {
    registry: Arc<dyn SessionRegistry>,
}

impl SessionRouter {
    pub fn new(registry: Arc<dyn SessionRegistry>) -> Self {
        Self { registry }
    }
}

impl HttpRouter for SessionRouter {
    fn resolve_session(&self, _path: &str) -> Option<Session> {
        self.registry.active_session()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::InMemorySessionRegistry;

    fn registry_with_session() -> (Arc<InMemorySessionRegistry>, Session) {
        let registry = Arc::new(InMemorySessionRegistry::new("my-vps.com"));
        let session = registry.open_session(3000).expect("session should open");
        (registry, session)
    }

    #[test]
    fn resolves_the_active_session_for_any_path() {
        let (registry, session) = registry_with_session();
        let router = SessionRouter::new(registry);

        // An absolute asset path the dev server emits, with no /s/<id> prefix.
        let resolved = router.resolve_session("/_nuxt/@vite/client");

        assert_eq!(resolved.map(|s| s.id), Some(session.id));
    }

    #[test]
    fn resolves_the_active_session_for_the_root_path() {
        let (registry, session) = registry_with_session();
        let router = SessionRouter::new(registry);

        let resolved = router.resolve_session("/");

        assert_eq!(resolved.map(|s| s.id), Some(session.id));
    }

    #[test]
    fn resolves_nothing_when_no_tunnel_is_active() {
        let registry = Arc::new(InMemorySessionRegistry::new("my-vps.com"));
        let router = SessionRouter::new(registry);

        assert!(router.resolve_session("/anything").is_none());
    }
}
