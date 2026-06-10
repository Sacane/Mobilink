use mobilink_core::http::HttpResponseData;

use crate::ResponseTransformer;

/// Injects the Eruda mobile debug console into HTML responses.
///
/// The script is inserted just before `</body>` (or appended if the tag is
/// missing). Non-HTML and compressed responses pass through untouched.
pub struct ErudaInjector;

/// The snippet injected into every HTML page flowing through the tunnel.
pub const ERUDA_SNIPPET: &str =
    r#"<script src="https://cdn.jsdelivr.net/npm/eruda"></script><script>eruda.init();</script>"#;

impl ResponseTransformer for ErudaInjector {
    fn transform(&self, mut response: HttpResponseData) -> HttpResponseData {
        let is_html = response
            .header("content-type")
            .is_some_and(|ct| ct.to_ascii_lowercase().contains("text/html"));
        if !is_html {
            return response;
        }

        // A compressed body can't be edited as text; rather than corrupt it,
        // let it through without the debug console.
        let is_compressed = response
            .header("content-encoding")
            .is_some_and(|enc| !enc.eq_ignore_ascii_case("identity"));
        if is_compressed {
            return response;
        }

        let body = match String::from_utf8(std::mem::take(&mut response.body)) {
            Ok(text) => text,
            Err(err) => {
                // Not valid UTF-8: hand the bytes back untouched.
                response.body = err.into_bytes();
                return response;
            }
        };

        let injected = match body.to_ascii_lowercase().rfind("</body>") {
            Some(pos) => {
                let mut s = String::with_capacity(body.len() + ERUDA_SNIPPET.len());
                s.push_str(&body[..pos]);
                s.push_str(ERUDA_SNIPPET);
                s.push_str(&body[pos..]);
                s
            }
            None => format!("{body}{ERUDA_SNIPPET}"),
        };

        response.body = injected.into_bytes();
        // The original Content-Length no longer matches; the HTTP layer will
        // recompute it from the new body.
        response.remove_header("content-length");
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn html_response(body: &str) -> HttpResponseData {
        HttpResponseData {
            status: 200,
            headers: vec![
                ("Content-Type".to_string(), "text/html; charset=utf-8".to_string()),
                ("Content-Length".to_string(), body.len().to_string()),
            ],
            body: body.as_bytes().to_vec(),
        }
    }

    #[test]
    fn injects_eruda_before_the_closing_body_tag() {
        let response = html_response("<html><body><h1>Hi</h1></body></html>");

        let transformed = ErudaInjector.transform(response);

        let body = String::from_utf8(transformed.body).expect("body stays valid UTF-8");
        assert!(body.contains("eruda"), "Eruda script should be present");
        let eruda_pos = body.find("eruda").unwrap();
        let close_body_pos = body.find("</body>").unwrap();
        assert!(
            eruda_pos < close_body_pos,
            "Eruda must be injected before </body>"
        );
    }

    #[test]
    fn stale_content_length_is_dropped_after_injection() {
        let response = html_response("<html><body></body></html>");

        let transformed = ErudaInjector.transform(response);

        assert_eq!(
            transformed.header("content-length"),
            None,
            "Content-Length would be wrong after injection; it must be removed"
        );
    }

    #[test]
    fn appends_eruda_when_html_has_no_closing_body_tag() {
        let response = html_response("<h1>Fragment without body tag</h1>");

        let transformed = ErudaInjector.transform(response);

        let body = String::from_utf8(transformed.body).unwrap();
        assert!(body.contains("eruda"), "Eruda should still be injected");
        assert!(body.starts_with("<h1>"), "Original content must be preserved");
    }

    #[test]
    fn leaves_non_html_responses_untouched() {
        let response = HttpResponseData {
            status: 200,
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
            body: br#"{"ok":true}"#.to_vec(),
        };

        let transformed = ErudaInjector.transform(response.clone());

        assert_eq!(transformed, response, "JSON must pass through unchanged");
    }

    #[test]
    fn leaves_compressed_html_untouched() {
        let mut response = html_response("<html><body></body></html>");
        response.headers.push(("Content-Encoding".to_string(), "gzip".to_string()));

        let transformed = ErudaInjector.transform(response.clone());

        assert_eq!(
            transformed, response,
            "Compressed bodies cannot be safely modified and must pass through"
        );
    }
}
