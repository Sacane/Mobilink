/// Configuration for the server, loaded from environment variables or a config file.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub quic_bind: String,
    pub http_bind: String,
    pub domain: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            quic_bind: "0.0.0.0:4433".to_string(),
            http_bind: "0.0.0.0:8080".to_string(),
            domain: "localhost".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub local_port: u16,
    pub server_addr: String,
    pub no_eruda: bool,
}
