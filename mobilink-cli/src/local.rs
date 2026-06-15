//! Replaying tunnel frames against the developer's local server.
//!
//! Gherkin: cli_serving_local_traffic.md — requests are replayed with their
//! headers and body; when nothing listens on the local port, the mobile
//! browser gets an explicit 502 instead of a hang.

use mobilink_core::http::{HttpRequestData, HttpResponseData};

/// Replays a request frame against `http://127.0.0.1:{port}` and captures
/// the response. Never fails: any transport error becomes a 502 frame so
/// the mobile browser always gets an answer.
pub async fn replay_locally(
    client: &reqwest::Client,
    port: u16,
    request: HttpRequestData,
) -> HttpResponseData {
    match try_replay(client, port, request).await {
        Ok(response) => response,
        Err(error) => {
            let mut chain = format!("{error}");
            let mut src = std::error::Error::source(error.as_ref());
            while let Some(e) = src {
                chain.push_str(&format!("\n  caused by: {e}"));
                src = e.source();
            }
            HttpResponseData {
                status: 502,
                headers: vec![(
                    "Content-Type".to_string(),
                    "text/plain; charset=utf-8".to_string(),
                )],
                body: format!(
                    "Mobilink: the local server on port {port} is unreachable.\n{chain}\n"
                )
                .into_bytes(),
            }
        }
    }
}

async fn try_replay(
    client: &reqwest::Client,
    port: u16,
    request: HttpRequestData,
) -> Result<HttpResponseData, Box<dyn std::error::Error + Send + Sync>> {
    let url = format!("http://localhost:{port}{}", request.target);
    let method = reqwest::Method::from_bytes(request.method.as_bytes())?;

    let mut builder = client.request(method, url);
    for (name, value) in &request.headers {
        // reqwest computes these itself from the actual local connection.
        if name.eq_ignore_ascii_case("host") || name.eq_ignore_ascii_case("content-length") {
            continue;
        }
        builder = builder.header(name, value);
    }
    if !request.body.is_empty() {
        builder = builder.body(request.body);
    }

    let response = builder.send().await?;
    let status = response.status().as_u16();
    let headers = response
        .headers()
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|v| (name.as_str().to_string(), v.to_string()))
        })
        .collect();
    let body = response.bytes().await?.to_vec();

    Ok(HttpResponseData {
        status,
        headers,
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get(target: &str) -> HttpRequestData {
        HttpRequestData {
            method: "GET".to_string(),
            target: target.to_string(),
            headers: vec![("X-Test".to_string(), "1".to_string())],
            body: Vec::new(),
        }
    }

    /// Finds a port that nothing listens on.
    fn free_port() -> u16 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    }

    #[tokio::test]
    async fn local_server_down_yields_an_explicit_502() {
        let client = reqwest::Client::new();
        let dead_port = free_port();

        let response = replay_locally(&client, dead_port, get("/")).await;

        assert_eq!(response.status, 502);
        let body = String::from_utf8_lossy(&response.body).to_string();
        assert!(
            body.contains(&dead_port.to_string()),
            "the error must name the unreachable port, got: {body}"
        );
    }

    #[tokio::test]
    async fn request_is_replayed_with_method_path_and_response_captured() {
        // A tiny real local server, like the developer's app.
        let app = axum::Router::new().route(
            "/api/items",
            axum::routing::get(|| async {
                ([("x-powered-by", "local-app")], "from the local app")
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let client = reqwest::Client::new();
        let response = replay_locally(&client, port, get("/api/items")).await;

        assert_eq!(response.status, 200);
        assert_eq!(response.body, b"from the local app".to_vec());
        let powered = response
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("x-powered-by"))
            .map(|(_, v)| v.as_str());
        assert_eq!(
            powered,
            Some("local-app"),
            "response headers must be captured"
        );
    }
}
