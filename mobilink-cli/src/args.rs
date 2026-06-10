//! Command-line interface definition.
//!
//! Gherkin: cli_starting_a_tunnel.md — valid arguments are accepted,
//! missing required arguments produce a usage error before any connection
//! attempt.

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "mobilink", version, about = "Expose a local port through your own Mobilink server")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Open a tunnel: localhost:PORT becomes reachable through your server
    Start(StartArgs),
}

#[derive(Debug, Args, PartialEq)]
pub struct StartArgs {
    /// Local port to expose
    #[arg(long)]
    pub port: u16,

    /// Host name or IP of your Mobilink server
    #[arg(long)]
    pub server: String,

    /// QUIC port the server's tunnel endpoint listens on
    #[arg(long, default_value_t = 4433)]
    pub server_port: u16,

    /// Do not inject the Eruda debug console into HTML pages
    #[arg(long)]
    pub no_eruda: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn parse(argv: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(argv)
    }

    #[test]
    fn start_with_port_and_server_is_accepted() {
        let cli = parse(&["mobilink", "start", "--port", "3000", "--server", "my-vps.com"])
            .expect("valid arguments must parse");
        let Command::Start(args) = cli.command;
        assert_eq!(args.port, 3000);
        assert_eq!(args.server, "my-vps.com");
        assert_eq!(args.server_port, 4433, "QUIC port defaults to 4433");
        assert!(!args.no_eruda, "Eruda is enabled by default");
    }

    #[test]
    fn no_eruda_flag_is_recorded() {
        let cli = parse(&[
            "mobilink", "start", "--port", "3000", "--server", "my-vps.com", "--no-eruda",
        ])
        .expect("valid arguments must parse");
        let Command::Start(args) = cli.command;
        assert!(args.no_eruda);
    }

    #[test]
    fn missing_port_is_rejected_with_usage_error() {
        let error = parse(&["mobilink", "start", "--server", "my-vps.com"])
            .expect_err("--port is required");
        let rendered = error.to_string();
        assert!(
            rendered.contains("--port"),
            "the error must point at the missing argument, got: {rendered}"
        );
    }

    #[test]
    fn missing_server_is_rejected_with_usage_error() {
        let error =
            parse(&["mobilink", "start", "--port", "3000"]).expect_err("--server is required");
        assert!(error.to_string().contains("--server"));
    }
}
