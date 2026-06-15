use std::collections::HashMap;
use std::sync::Mutex;

use mobilink_core::session::{Session, SessionId};

use crate::{SessionError, SessionRegistry};

/// In-memory implementation of SessionRegistry.
/// Holds all active sessions in a thread-safe map and remembers which one
/// currently owns the public host (whole-host routing: the most recently
/// opened tunnel wins).
pub struct InMemorySessionRegistry {
    sessions: Mutex<HashMap<SessionId, Session>>,
    /// Session that currently owns the public host, if any.
    current: Mutex<Option<SessionId>>,
    /// Base of every public URL, scheme included (e.g. "https://my-vps.com").
    public_base_url: String,
}

impl InMemorySessionRegistry {
    pub fn new(public_base_url: impl Into<String>) -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            current: Mutex::new(None),
            public_base_url: public_base_url.into().trim_end_matches('/').to_string(),
        }
    }
}

impl SessionRegistry for InMemorySessionRegistry {
    fn open_session(&self, port: u16) -> Option<Session> {
        // Whole-host routing: the public URL is the host root itself, so the
        // developer's app keeps serving absolute asset paths (`/_nuxt/...`)
        // that resolve straight back through the tunnel.
        let session = Session::new(port, self.public_base_url.clone());
        self.register_session(session).ok()
    }

    fn get_session(&self, id: &SessionId) -> Option<Session> {
        self.sessions.lock().ok()?.get(id).cloned()
    }

    fn active_session(&self) -> Option<Session> {
        let current = self.current.lock().ok()?.clone()?;
        self.get_session(&current)
    }

    fn register_session(&self, session: Session) -> Result<Session, SessionError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| SessionError::DuplicateId)?;
        if sessions.contains_key(&session.id) {
            return Err(SessionError::DuplicateId);
        }
        sessions.insert(session.id.clone(), session.clone());
        *self.current.lock().map_err(|_| SessionError::DuplicateId)? = Some(session.id.clone());
        Ok(session)
    }

    fn close_session(&self, id: &SessionId) -> Result<(), SessionError> {
        let mut sessions = self.sessions.lock().map_err(|_| SessionError::NotFound)?;
        match sessions.remove(id) {
            Some(_) => {
                let mut current = self.current.lock().map_err(|_| SessionError::NotFound)?;
                if current.as_ref() == Some(id) {
                    *current = None;
                }
                Ok(())
            }
            None => Err(SessionError::NotFound),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_session_sets_public_url_to_the_host_root() {
        let registry = InMemorySessionRegistry::new("https://mobilink.sacane.fr");

        let session = registry.open_session(3000).expect("session should open");

        assert_eq!(
            session.public_url, "https://mobilink.sacane.fr",
            "whole-host routing exposes the host root, not a /s/<id> sub-path"
        );
    }

    #[test]
    fn active_session_returns_the_most_recently_opened_session() {
        let registry = InMemorySessionRegistry::new("https://mobilink.sacane.fr");
        let _first = registry.open_session(3000).expect("first session");
        let second = registry.open_session(4000).expect("second session");

        let active = registry.active_session().expect("a session is active");

        assert_eq!(
            active.id, second.id,
            "the latest tunnel takes ownership of the public host"
        );
    }

    #[test]
    fn active_session_is_none_when_no_tunnel_is_connected() {
        let registry = InMemorySessionRegistry::new("https://mobilink.sacane.fr");

        assert!(registry.active_session().is_none());
    }

    #[test]
    fn active_session_is_cleared_after_the_session_is_closed() {
        let registry = InMemorySessionRegistry::new("https://mobilink.sacane.fr");
        let session = registry.open_session(3000).expect("session should open");

        registry.close_session(&session.id).expect("close succeeds");

        assert!(
            registry.active_session().is_none(),
            "closing the active session releases the public host"
        );
    }
}
