use mobilink_core::session::{Session, SessionId};

pub mod dispatcher;
pub mod forwarder;
pub mod registry;
pub mod router;

/// Reason why a session operation failed.
#[derive(Debug, PartialEq)]
pub enum SessionError {
    /// A session with this ID is already active.
    DuplicateId,
    /// No session with this ID was found.
    NotFound,
}

/// Reason why a request could not be forwarded through the tunnel.
#[derive(Debug, PartialEq)]
pub enum ForwardError {
    /// No tunnel connection is registered for this session.
    TunnelNotFound,
    /// The tunnel connection was lost mid-transfer.
    TunnelDisconnected,
}

/// Forwards an HTTP request through the tunnel to the developer's local server.
pub trait RequestForwarder: Send + Sync {
    /// Sends raw HTTP request bytes to the developer's local server via the tunnel.
    /// Returns the raw HTTP response bytes on success.
    fn forward(&self, session: &Session, request: &[u8]) -> Result<Vec<u8>, ForwardError>;
}

/// Resolves an incoming HTTP request to the correct active session.
pub trait HttpRouter: Send + Sync {
    /// Given a URL path (e.g. "/s/abc123"), returns the matching active session.
    /// Returns None if the path doesn't match or the session doesn't exist.
    fn resolve_session(&self, path: &str) -> Option<Session>;
}

/// Handles a single end-to-end request: resolves the session from the URL path,
/// forwards the raw HTTP request through the tunnel, and returns the raw HTTP response.
pub trait RequestPipeline: Send + Sync {
    /// Processes an inbound HTTP request for the given URL path.
    /// Returns the raw HTTP response bytes, or an error if routing or forwarding failed.
    fn handle(&self, path: &str, request: &[u8]) -> Result<Vec<u8>, PipelineError>;
}

/// Reason why the request pipeline failed to process a request.
#[derive(Debug, PartialEq)]
pub enum PipelineError {
    /// No active session matched the URL path.
    SessionNotFound,
    /// The tunnel for the matched session is unavailable.
    ForwardFailed(ForwardError),
}

/// Represents the server's ability to manage tunnel sessions.
/// Any component that opens, retrieves or closes sessions goes through this contract.
pub trait SessionRegistry: Send + Sync {
    /// Opens a new session for a developer exposing the given local port.
    /// Generates a fresh session ID internally and delegates to register_session.
    /// Returns the created session, or None if an internal error occurred.
    fn open_session(&self, port: u16) -> Option<Session>;

    /// Retrieves an active session by its ID.
    /// Returns None if no session with that ID exists.
    fn get_session(&self, id: &SessionId) -> Option<Session>;

    /// Attempts to register a session that already has an ID.
    /// Returns Err(DuplicateId) if a session with the same ID is already active.
    fn register_session(&self, session: Session) -> Result<Session, SessionError>;

    /// Closes an active session and removes it from the registry.
    /// Returns Err(NotFound) if no session with this ID exists.
    fn close_session(&self, id: &SessionId) -> Result<(), SessionError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use mobilink_core::session::SessionId;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    // --- Stub ---

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
            self.register_session(session).ok()
        }

        fn get_session(&self, id: &SessionId) -> Option<Session> {
            self.sessions.lock().ok()?.get(id).cloned()
        }

        fn register_session(&self, session: Session) -> Result<Session, SessionError> {
            let mut sessions = self.sessions.lock().map_err(|_| SessionError::DuplicateId)?;
            if sessions.contains_key(&session.id) {
                return Err(SessionError::DuplicateId);
            }
            sessions.insert(session.id.clone(), session.clone());
            Ok(session)
        }

        fn close_session(&self, id: &SessionId) -> Result<(), SessionError> {
            let mut sessions = self.sessions.lock().map_err(|_| SessionError::NotFound)?;
            match sessions.remove(id) {
                Some(_) => Ok(()),
                None => Err(SessionError::NotFound),
            }
        }
    }

    // --- Spy forwarder ---

    struct SpyForwarder {
        known_sessions: Mutex<Vec<SessionId>>,
        last_session_id: Mutex<Option<SessionId>>,
    }

    impl SpyForwarder {
        fn new() -> Self {
            Self {
                known_sessions: Mutex::new(Vec::new()),
                last_session_id: Mutex::new(None),
            }
        }

        fn register_tunnel(&self, session_id: SessionId) {
            self.known_sessions.lock().unwrap().push(session_id);
        }

        fn last_session_id(&self) -> Option<SessionId> {
            self.last_session_id.lock().unwrap().clone()
        }
    }

    impl RequestForwarder for SpyForwarder {
        fn forward(&self, session: &Session, _request: &[u8]) -> Result<Vec<u8>, ForwardError> {
            let known = self.known_sessions.lock().unwrap();
            if !known.contains(&session.id) {
                return Err(ForwardError::TunnelNotFound);
            }
            *self.last_session_id.lock().unwrap() = Some(session.id.clone());
            Ok(b"HTTP/1.1 200 OK\r\n\r\n".to_vec())
        }
    }

    // --- Stub router ---

    struct StubHttpRouter {
        registry: Arc<StubSessionRegistry>,
    }

    impl HttpRouter for StubHttpRouter {
        fn resolve_session(&self, path: &str) -> Option<Session> {
            let id_str = path.strip_prefix("/s/")?;
            let session_id: SessionId = id_str.parse().ok()?;
            self.registry.get_session(&session_id)
        }
    }

    // --- Tests ---

    #[test]
    fn server_opens_a_session_when_developer_announces_a_port() {
        let registry = StubSessionRegistry::new();
        let session = registry.open_session(3000);
        assert!(session.is_some(), "Expected a session to be opened, but got None");
    }

    #[test]
    fn session_has_a_unique_id_and_a_public_url_containing_that_id() {
        let registry = StubSessionRegistry::new();
        let session = registry.open_session(3000).expect("Expected a session");
        let id_str = session.id.to_string();
        assert!(!id_str.is_empty(), "Session ID should not be empty");
        assert!(
            session.public_url.contains(&id_str),
            "Public URL '{}' should contain the session ID '{}'",
            session.public_url, id_str
        );
    }

    #[test]
    fn server_can_retrieve_an_active_session_by_its_id() {
        let registry = StubSessionRegistry::new();
        let session = registry.open_session(3000).expect("Expected a session");
        let session_id = session.id.clone();
        let found = registry.get_session(&session_id);
        assert!(found.is_some(), "Expected to find the session by ID");
        assert_eq!(found.unwrap().id, session_id);
    }

    #[test]
    fn server_returns_nothing_for_an_unknown_session_id() {
        let registry = StubSessionRegistry::new();
        let unknown_id = SessionId::new();
        let found = registry.get_session(&unknown_id);
        assert!(found.is_none(), "Expected no session for an unknown ID");
    }

    #[test]
    fn server_rejects_a_session_with_a_duplicate_id() {
        let registry = StubSessionRegistry::new();
        let existing = registry.open_session(3000).expect("Expected a session");
        let duplicate = Session {
            id: existing.id.clone(),
            local_port: 8080,
            public_url: "http://localhost/s/duplicate".to_string(),
        };
        let result = registry.register_session(duplicate);
        assert_eq!(result, Err(SessionError::DuplicateId));
        let still_there = registry.get_session(&existing.id).expect("Original session should still exist");
        assert_eq!(still_there.local_port, 3000, "Original session should be unchanged");
    }

    #[test]
    fn session_is_removed_from_registry_when_developer_disconnects() {
        // Given: an active session exists
        let registry = StubSessionRegistry::new();
        let session = registry.open_session(3000).expect("Expected a session");
        let session_id = session.id.clone();

        // When: the developer stops the tunnel
        let result = registry.close_session(&session_id);

        // Then: the operation succeeds
        assert!(result.is_ok(), "Expected close_session to succeed");

        // And: the session is no longer in the registry
        let found = registry.get_session(&session_id);
        assert!(found.is_none(), "Session should be gone after closing");
    }

    #[test]
    fn server_forwards_request_to_the_correct_developer_tunnel() {
        // Given: a session is active for developer A on port 3000
        let registry = Arc::new(StubSessionRegistry::new());
        let session = registry.open_session(3000).expect("Expected a session");
        let forwarder = SpyForwarder::new();
        forwarder.register_tunnel(session.id.clone()); // developer A has an active tunnel

        // When: a mobile browser sends a GET request for that session
        let request = b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n";
        let result = forwarder.forward(&session, request);

        // Then: the request was forwarded successfully
        assert!(result.is_ok(), "Expected forwarding to succeed");

        // And: the forwarder targeted the correct session
        assert_eq!(
            forwarder.last_session_id(),
            Some(session.id),
            "Forwarder should have targeted the correct session"
        );
    }

    #[test]
    fn server_returns_error_when_no_tunnel_exists_for_session() {
        // Given: a session with no active tunnel connection
        let session = Session::new(3000, "http://localhost/s/ghost".to_string());
        let forwarder = SpyForwarder::new();

        // When: a request is forwarded for that session
        let request = b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n";
        let result = forwarder.forward(&session, request);

        // Then: forwarding fails with TunnelNotFound
        assert_eq!(result, Err(ForwardError::TunnelNotFound));
    }

    #[test]
    fn server_identifies_session_from_public_url_path() {
        // Given: a session is active with a known public URL
        let registry = Arc::new(StubSessionRegistry::new());
        let session = registry.open_session(3000).expect("Expected a session");
        let path = format!("/s/{}", session.id);

        // When: a mobile browser sends a request to that URL path
        let router = StubHttpRouter { registry: Arc::clone(&registry) };
        let resolved = router.resolve_session(&path);

        // Then: the server identifies the correct session
        assert!(resolved.is_some(), "Expected a session to be resolved from the URL");
        assert_eq!(
            resolved.unwrap().id,
            session.id,
            "Resolved session ID should match the one in the URL"
        );
    }

    #[test]
    fn server_returns_nothing_for_unknown_url_path() {
        // Given: no session exists for this path
        let registry = Arc::new(StubSessionRegistry::new());
        let router = StubHttpRouter { registry: Arc::clone(&registry) };

        // When: a mobile browser sends a request to an unknown path
        let resolved = router.resolve_session("/s/unknownid");

        // Then: nothing is resolved
        assert!(resolved.is_none(), "Expected no session for an unknown path");
    }

    #[test]
    fn closing_a_nonexistent_session_returns_not_found() {
        // Given: no session with this ID exists
        let registry = StubSessionRegistry::new();
        let unknown_id = SessionId::new();

        // When: the server tries to close it anyway
        let result = registry.close_session(&unknown_id);

        // Then: it returns NotFound
        assert_eq!(result, Err(SessionError::NotFound));
    }

    // --- Stub pipeline ---

    struct StubRequestPipeline {
        router: StubHttpRouter,
        forwarder: SpyForwarder,
    }

    impl RequestPipeline for StubRequestPipeline {
        fn handle(&self, path: &str, request: &[u8]) -> Result<Vec<u8>, PipelineError> {
            let session = self.router.resolve_session(path)
                .ok_or(PipelineError::SessionNotFound)?;
            self.forwarder.forward(&session, request)
                .map_err(PipelineError::ForwardFailed)
        }
    }

    #[test]
    fn server_relays_response_from_developer_server_to_mobile_browser() {
        // Given: a session is active and its tunnel is connected
        let registry = Arc::new(StubSessionRegistry::new());
        let session = registry.open_session(3000).expect("Expected a session");
        let path = format!("/s/{}", session.id);

        let forwarder = SpyForwarder::new();
        forwarder.register_tunnel(session.id.clone()); // tunnel is live

        let pipeline = StubRequestPipeline {
            router: StubHttpRouter { registry: Arc::clone(&registry) },
            forwarder,
        };

        // When: the mobile browser sends a GET request
        let request = b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n";
        let result = pipeline.handle(&path, request);

        // Then: the pipeline returns the response from the developer's local server
        assert!(result.is_ok(), "Expected a response, got: {:?}", result);
        assert_eq!(
            result.unwrap(),
            b"HTTP/1.1 200 OK\r\n\r\n".to_vec(),
            "Response body should match what the developer server sent"
        );
    }

    #[test]
    fn server_returns_session_not_found_when_path_is_unknown() {
        // Given: no session exists for this path
        let registry = Arc::new(StubSessionRegistry::new());
        let pipeline = StubRequestPipeline {
            router: StubHttpRouter { registry },
            forwarder: SpyForwarder::new(),
        };

        // When: a request arrives for an unknown path
        let request = b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n";
        let result = pipeline.handle("/s/doesnotexist", request);

        // Then: the pipeline signals that no session matched
        assert_eq!(result, Err(PipelineError::SessionNotFound));
    }
}
