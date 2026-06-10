use std::collections::HashMap;
use std::sync::Mutex;

use mobilink_core::session::{Session, SessionId};

use crate::{SessionError, SessionRegistry};

/// In-memory implementation of SessionRegistry.
/// Holds all active sessions in a thread-safe map.
pub struct InMemorySessionRegistry {
    sessions: Mutex<HashMap<SessionId, Session>>,
    /// Base of every public URL, scheme included (e.g. "https://my-vps.com").
    public_base_url: String,
}

impl InMemorySessionRegistry {
    pub fn new(public_base_url: impl Into<String>) -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            public_base_url: public_base_url.into().trim_end_matches('/').to_string(),
        }
    }
}

impl SessionRegistry for InMemorySessionRegistry {
    fn open_session(&self, port: u16) -> Option<Session> {
        let session = Session::new(port, String::new());
        let session = Session {
            public_url: format!("{}/s/{}", self.public_base_url, session.id),
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
