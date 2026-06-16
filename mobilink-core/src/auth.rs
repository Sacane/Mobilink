//! How the tunnel adapts to the local app's authentication scheme.
//!
//! The developer declares it with `--auth`; the value is carried to the
//! server in the handshake. The server uses it to decide whether (and how)
//! to rewrite `Set-Cookie` headers so a session survives the hop through the
//! public host.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// The authentication strategy of the developer's local application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AuthMode {
    /// Touch nothing: cookies and headers pass through verbatim (default).
    #[default]
    Passthrough,
    /// Cookie/session based: rewrite `Set-Cookie` to `Secure; SameSite=None`
    /// and drop any `Domain`, so the browser stores and resends the session
    /// cookie across the public-host hop.
    Cookie,
    /// Token/bearer based: nothing to rewrite; relies on the always-on
    /// credential-safe CORS and verbatim `Authorization` forwarding.
    Bearer,
}

impl fmt::Display for AuthMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            AuthMode::Passthrough => "passthrough",
            AuthMode::Cookie => "cookie",
            AuthMode::Bearer => "bearer",
        })
    }
}

/// Returned when an unknown `--auth` value is supplied on the command line.
#[derive(Debug, PartialEq, Eq)]
pub struct ParseAuthModeError(pub String);

impl fmt::Display for ParseAuthModeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unknown auth mode '{}' (expected one of: passthrough, cookie, bearer)",
            self.0
        )
    }
}

impl std::error::Error for ParseAuthModeError {}

impl FromStr for AuthMode {
    type Err = ParseAuthModeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "passthrough" | "none" => Ok(AuthMode::Passthrough),
            "cookie" | "session" => Ok(AuthMode::Cookie),
            "bearer" | "token" => Ok(AuthMode::Bearer),
            other => Err(ParseAuthModeError(other.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_mode_defaults_to_passthrough() {
        assert_eq!(AuthMode::default(), AuthMode::Passthrough);
    }

    #[test]
    fn auth_mode_parses_each_known_value() {
        assert_eq!("passthrough".parse(), Ok(AuthMode::Passthrough));
        assert_eq!("cookie".parse(), Ok(AuthMode::Cookie));
        assert_eq!("bearer".parse(), Ok(AuthMode::Bearer));
    }

    #[test]
    fn auth_mode_parsing_is_case_insensitive_and_trims() {
        assert_eq!("  COOKIE ".parse(), Ok(AuthMode::Cookie));
    }

    #[test]
    fn auth_mode_rejects_an_unknown_value() {
        let err = "magic".parse::<AuthMode>().expect_err("must reject");
        assert_eq!(err, ParseAuthModeError("magic".to_string()));
    }

    #[test]
    fn auth_mode_display_round_trips_through_from_str() {
        for mode in [AuthMode::Passthrough, AuthMode::Cookie, AuthMode::Bearer] {
            assert_eq!(mode.to_string().parse(), Ok(mode));
        }
    }
}
