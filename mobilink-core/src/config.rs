/// Configuration for the server, loaded from environment variables or a config file.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// UDP address the QUIC tunnel endpoint listens on.
    pub quic_bind: String,
    /// TCP address the public HTTP endpoint listens on.
    pub http_bind: String,
    /// Base of every public URL, scheme included (e.g. "https://my-vps.com").
    pub public_url: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            quic_bind: "0.0.0.0:4433".to_string(),
            http_bind: "0.0.0.0:8080".to_string(),
            public_url: "http://localhost:8080".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub local_port: u16,
    pub server_addr: String,
    pub no_eruda: bool,
}
