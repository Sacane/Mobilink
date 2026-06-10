use serde::{Deserialize, Serialize};

/// A full HTTP request captured by the server, ready to travel through the tunnel.
/// This is a transport-agnostic representation: no hyper/axum types leak into core.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HttpRequestData {
    /// HTTP method (GET, POST, ...).
    pub method: String,
    /// Path + query as seen by the local server (e.g. "/css/style.css?v=2").
    pub target: String,
    /// Header pairs, in order. Names are kept as received.
    pub headers: Vec<(String, String)>,
    /// Raw request body.
    pub body: Vec<u8>,
}

/// A full HTTP response produced by the developer's local server,
/// ready to travel back through the tunnel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HttpResponseData {
    /// HTTP status code (200, 404, ...).
    pub status: u16,
    /// Header pairs, in order.
    pub headers: Vec<(String, String)>,
    /// Raw response body.
    pub body: Vec<u8>,
}

impl HttpResponseData {
    /// Returns the value of the first header matching `name` (case-insensitive).
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// Removes every header matching `name` (case-insensitive).
    pub fn remove_header(&mut self, name: &str) {
        self.headers.retain(|(k, _)| !k.eq_ignore_ascii_case(name));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_lookup_is_case_insensitive() {
        let response = HttpResponseData {
            status: 200,
            headers: vec![("Content-Type".to_string(), "text/html".to_string())],
            body: Vec::new(),
        };
        assert_eq!(response.header("content-type"), Some("text/html"));
        assert_eq!(response.header("CONTENT-TYPE"), Some("text/html"));
        assert_eq!(response.header("x-missing"), None);
    }

    #[test]
    fn removing_a_header_removes_all_case_variants() {
        let mut response = HttpResponseData {
            status: 200,
            headers: vec![
                ("Content-Length".to_string(), "10".to_string()),
                ("content-length".to_string(), "10".to_string()),
                ("Content-Type".to_string(), "text/html".to_string()),
            ],
            body: Vec::new(),
        };
        response.remove_header("content-length");
        assert_eq!(response.header("content-length"), None);
        assert_eq!(response.header("content-type"), Some("text/html"));
    }
}
