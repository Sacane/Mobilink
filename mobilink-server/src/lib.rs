use mobilink_core::session::{Session, SessionId};

pub mod registry;

/// Represents the server's ability to manage tunnel sessions.
/// Any component that opens, retrieves or closes sessions goes through this contract.
pub trait SessionRegistry: Send + Sync {
    /// Opens a new session for a developer exposing the given local port.
    /// Returns the created session, or None if the session could not be opened.
    fn open_session(&self, port: u16) -> Option<Session>;

    /// Retrieves an active session by its ID.
    /// Returns None if no session with that ID exists.
    fn get_session(&self, id: &SessionId) -> Option<Session>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use mobilink_core::session::SessionId;

    // --- Stub ---

    use std::collections::HashMap;
    use std::sync::Mutex;

    struct StubSessionRegistry {
        sessions: Mutex<HashMap<SessionId, Session>>,
    }

    impl StubSessionRegistry {
        fn new() -> Self {
            Self { sessions: Mutex::new(HashMap::new()) }
        }
    }

    impl SessionRegistry for StubSessionRegistry {
        fn open_session(&self, port: u16) -> Option<Session> {
            let session = Session::new(port, String::new());
            let session = Session {
                public_url: format!("http://localhost/s/{}", session.id),
                ..session
            };
            self.sessions.lock().ok()?.insert(session.id.clone(), session.clone());
            Some(session)
        }

        fn get_session(&self, id: &SessionId) -> Option<Session> {
            self.sessions.lock().ok()?.get(id).cloned()
        }
    }

    // --- Tests ---

    #[test]
    fn server_opens_a_session_when_developer_announces_a_port() {
        // Given: no active session exists (fresh registry)
        let registry = StubSessionRegistry::new();

        // When: developer connects and announces port 3000
        let session = registry.open_session(3000);

        // Then: a session is opened
        assert!(
            session.is_some(),
            "Expected a session to be opened, but got None"
        );
    }

    #[test]
    fn session_has_a_unique_id_and_a_public_url_containing_that_id() {
        // Given: a developer has just connected and announced port 3000
        let registry = StubSessionRegistry::new();

        // When: the server opens the session
        let session = registry.open_session(3000).expect("Expected a session");

        // Then: the session has a non-empty unique ID
        let id_str = session.id.to_string();
        assert!(!id_str.is_empty(), "Session ID should not be empty");

        // And: the public URL contains that session ID
        assert!(
            session.public_url.contains(&id_str),
            "Public URL '{}' should contain the session ID '{}'",
            session.public_url,
            id_str
        );
    }

    #[test]
    fn server_can_retrieve_an_active_session_by_its_id() {
        // Given: a session is active (it was opened)
        let registry = StubSessionRegistry::new();
        let session = registry.open_session(3000).expect("Expected a session");
        let session_id = session.id.clone();

        // When: the server looks up that session by ID
        let found = registry.get_session(&session_id);

        // Then: the session is found and belongs to the right tunnel
        assert!(found.is_some(), "Expected to find the session by ID");
        assert_eq!(
            found.unwrap().id,
            session_id,
            "Found session ID does not match the requested ID"
        );
    }

    #[test]
    fn server_returns_nothing_for_an_unknown_session_id() {
        // Given: no session with this ID exists
        let registry = StubSessionRegistry::new();
        let unknown_id = SessionId::new();

        // When: the server looks up that unknown ID
        let found = registry.get_session(&unknown_id);

        // Then: nothing is returned
        assert!(
            found.is_none(),
            "Expected no session for an unknown ID, but got one"
        );
    }
}
