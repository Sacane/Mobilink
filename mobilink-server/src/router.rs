use std::sync::Arc;

use mobilink_core::session::{Session, SessionId};

use crate::{HttpRouter, SessionRegistry};

/// Splits a public URL path into the session ID and the path intended for
/// the developer's local server.
///
/// `/s/{id}` and `/s/{id}/` map to a local path of `/`;
/// `/s/{id}/css/style.css` maps to `/css/style.css`.
pub fn parse_public_path(path: &str) -> Option<(SessionId, String)> {
    let rest = path.strip_prefix("/s/")?;
    let (id_part, local_path) = match rest.find('/') {
        Some(slash) => (&rest[..slash], &rest[slash..]),
        None => (rest, "/"),
    };
    let session_id = id_part.parse().ok()?;
    let local_path = if local_path.is_empty() || local_path == "/" {
        "/".to_string()
    } else {
        local_path.to_string()
    };
    Some((session_id, local_path))
}

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
        let (session_id, _) = parse_public_path(path)?;
        self.registry.get_session(&session_id)
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
    fn resolves_a_session_when_the_path_has_a_subpath() {
        let (registry, session) = registry_with_session();
        let router = SessionRouter::new(registry);

        let path = format!("/s/{}/css/style.css", session.id);
        let resolved = router.resolve_session(&path);

        assert_eq!(resolved.map(|s| s.id), Some(session.id));
    }

    #[test]
    fn parse_public_path_extracts_id_and_local_path() {
        let (_, session) = registry_with_session();

        let path = format!("/s/{}/api/items", session.id);
        let (id, local) = parse_public_path(&path).expect("path should parse");

        assert_eq!(id, session.id);
        assert_eq!(local, "/api/items");
    }

    #[test]
    fn parse_public_path_maps_bare_session_path_to_root() {
        let (_, session) = registry_with_session();

        let bare = format!("/s/{}", session.id);
        let trailing = format!("/s/{}/", session.id);

        assert_eq!(parse_public_path(&bare).expect("bare").1, "/");
        assert_eq!(parse_public_path(&trailing).expect("trailing").1, "/");
    }

    #[test]
    fn parse_public_path_rejects_foreign_paths() {
        assert!(parse_public_path("/health").is_none());
        assert!(parse_public_path("/s/not-a-uuid/whatever").is_none());
    }
}
