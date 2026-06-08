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

/// Configuration for the CLI, provided via command-line arguments.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Local port to expose through the tunnel.
    pub local_port: u16,
    /// Address of the Mobilink server to connect to.
    pub server_addr: String,
    /// Whether to disable automatic Eruda injection.
    pub no_eruda: bool,
}
