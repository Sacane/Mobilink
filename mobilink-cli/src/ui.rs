//! Terminal output: QR code rendering and the live request log.
//!
//! Gherkin: terminal_developer_experience.md — the developer can point
//! their phone at the terminal, and follows requests as they happen.

/// Renders a URL as a scannable QR code made of Unicode half-blocks.
/// Returns None only if the URL cannot fit in a QR code at all.
pub fn qr_string(url: &str) -> Option<String> {
    let code = qrcode::QrCode::new(url.as_bytes()).ok()?;
    Some(
        code.render::<qrcode::render::unicode::Dense1x2>()
            .quiet_zone(true)
            .build(),
    )
}

/// One log line per proxied request: method, path, status, latency.
pub fn format_log_line(method: &str, path: &str, status: u16, latency_ms: u128) -> String {
    format!("  {method:>7} {path}  \u{2192} {status}  ({latency_ms} ms)")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qr_code_is_rendered_as_multiline_unicode_blocks() {
        let qr = qr_string("https://my-vps.com/s/abc123").expect("URL fits in a QR code");

        assert!(
            qr.lines().count() > 10,
            "a QR code spans many terminal lines"
        );
        assert!(
            qr.contains('█') || qr.contains('▀') || qr.contains('▄'),
            "the QR code is drawn with block characters"
        );
    }

    #[test]
    fn log_line_shows_method_path_status_and_latency() {
        let line = format_log_line("GET", "/api/items", 200, 42);

        assert!(line.contains("GET"));
        assert!(line.contains("/api/items"));
        assert!(line.contains("200"));
        assert!(line.contains("42"), "latency must be visible");
    }
}
