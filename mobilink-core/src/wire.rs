//! Pure serialization helpers for everything that crosses the QUIC tunnel.
//!
//! Wire convention: one message per stream direction. The sender writes the
//! encoded bytes then finishes its side of the stream; the receiver reads to
//! end and decodes. No length prefix is needed because the stream boundary
//! IS the message boundary. Core stays free of any I/O: these helpers only
//! transform values to/from bytes.

use serde::de::DeserializeOwned;
use serde::Serialize;

/// Failure while encoding or decoding a wire message.
#[derive(Debug)]
pub struct WireError(pub String);

impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "wire error: {}", self.0)
    }
}

impl std::error::Error for WireError {}

/// Encodes any serializable value into compact binary bytes.
pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, WireError> {
    bincode::serialize(value).map_err(|e| WireError(e.to_string()))
}

/// Decodes bytes produced by [`encode`] back into a value.
pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, WireError> {
    bincode::deserialize(bytes).map_err(|e| WireError(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{HttpRequestData, HttpResponseData};
    use crate::message::{ClientMessage, ServerMessage};
    use crate::session::SessionId;

    #[test]
    fn hello_message_survives_a_wire_roundtrip() {
        let hello = ClientMessage::Hello { local_port: 3000, no_eruda: true };

        let bytes = encode(&hello).expect("encoding should succeed");
        let decoded: ClientMessage = decode(&bytes).expect("decoding should succeed");

        match decoded {
            ClientMessage::Hello { local_port, no_eruda } => {
                assert_eq!(local_port, 3000);
                assert!(no_eruda);
            }
        }
    }

    #[test]
    fn session_created_message_survives_a_wire_roundtrip() {
        let id = SessionId::new();
        let msg = ServerMessage::SessionCreated {
            session_id: id.clone(),
            public_url: format!("https://my-vps.com/s/{id}"),
        };

        let bytes = encode(&msg).expect("encoding should succeed");
        let decoded: ServerMessage = decode(&bytes).expect("decoding should succeed");

        match decoded {
            ServerMessage::SessionCreated { session_id, public_url } => {
                assert_eq!(session_id, id);
                assert!(public_url.contains(&id.to_string()));
            }
        }
    }

    #[test]
    fn http_request_and_response_survive_a_wire_roundtrip() {
        let request = HttpRequestData {
            method: "POST".to_string(),
            target: "/api/items?page=2".to_string(),
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
            body: br#"{"name":"test"}"#.to_vec(),
        };
        let response = HttpResponseData {
            status: 201,
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
            body: br#"{"id":42}"#.to_vec(),
        };

        let req_back: HttpRequestData =
            decode(&encode(&request).unwrap()).expect("request roundtrip");
        let resp_back: HttpResponseData =
            decode(&encode(&response).unwrap()).expect("response roundtrip");

        assert_eq!(req_back, request);
        assert_eq!(resp_back, response);
    }

    #[test]
    fn decoding_garbage_bytes_fails_cleanly() {
        let result: Result<HttpRequestData, WireError> = decode(&[0xFF, 0x00, 0x12]);
        assert!(result.is_err(), "garbage bytes must not decode into a request");
    }
}
