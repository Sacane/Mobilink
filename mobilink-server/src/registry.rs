use std::collections::HashMap;
use std::sync::Mutex;

use mobilink_core::session::{Session, SessionId};

use crate::SessionRegistry;

/// In-memory implementation of SessionRegistry.
/// Holds all active sessions in a thread-safe map.
pub struct InMemorySessionRegistry {
    sessions: Mutex<HashMap<SessionId, Session>>,
    domain: String,
}

impl InMemorySessionRegistry {
    pub fn new(domain: impl Into<String>) -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            domain: domain.into(),
        }
    }
}

impl SessionRegistry for InMemorySessionRegistry {
    fn open_session(&self, port: u16) -> Option<Session> {
        let mut sessions = self.sessions.lock().ok()?;
        let session = Session::new(port, String::new());
        let session = Session {
            public_url: format!("https://{}/s/{}", self.domain, session.id),
            ..session
        };
        sessions.insert(session.id.clone(), session.clone());
        Some(session)
    }

    fn get_session(&self, id: &SessionId) -> Option<Session> {
        self.sessions.lock().ok()?.get(id).cloned()
    }
}
