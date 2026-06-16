//! `mobilink` binary — parses the command line and drives the tunnel.
//!
//! Gherkin: cli_starting_a_tunnel.md + terminal_developer_experience.md.

use std::net::SocketAddr;
use std::process::ExitCode;

use clap::Parser;

use mobilink_cli::args::{Cli, Command, StartArgs};
use mobilink_cli::{tls, tunnel, ui};

/// Picks the first IPv4 address from a resolved list, falling back to the
/// first address of any family when no IPv4 is present.
///
/// Windows resolves `localhost` to `[::1]` before `127.0.0.1`; quinn's
/// client endpoint is bound to `0.0.0.0` (IPv4), so connecting to an IPv6
/// address fails immediately. Preferring IPv4 fixes the local dev workflow
/// without affecting production use (VPS hostnames resolve to IPv4 anyway).
fn prefer_ipv4(addrs: &[SocketAddr]) -> Option<SocketAddr> {
    addrs
        .iter()
        .find(|a| a.is_ipv4())
        .or_else(|| addrs.first())
        .copied()
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let Command::Start(args) = cli.command;

    match run(args).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run(args: StartArgs) -> Result<(), Box<dyn std::error::Error>> {
    // Resolve the server address (DNS or literal IP), preferring IPv4.
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host((args.server.as_str(), args.server_port))
        .await?
        .collect();
    let server_addr =
        prefer_ipv4(&addrs).ok_or_else(|| format!("could not resolve host '{}'", args.server))?;

    let endpoint = tls::insecure_client_endpoint()?;

    let session = tunnel::connect_and_handshake(
        &endpoint,
        server_addr,
        &args.server,
        args.port,
        args.no_eruda,
        args.auth,
    )
    .await
    .map_err(|error| {
        format!(
            "could not reach the Mobilink server at {}:{} — {error}",
            args.server, args.server_port
        )
    })?;

    println!();
    println!(
        "  Tunnel ready!  localhost:{}  \u{21d2}  {}",
        args.port, session.public_url
    );
    println!();
    if let Some(qr) = ui::qr_string(&session.public_url) {
        println!("{qr}");
    }
    println!("  Scan the QR code with your phone, or open the URL directly.");
    if args.no_eruda {
        println!("  Eruda injection is disabled (--no-eruda).");
    } else {
        println!("  The Eruda debug console is injected into every HTML page.");
    }
    println!("  Press Ctrl+C to stop the tunnel.");
    println!();

    let connection = session.connection.clone();
    tokio::select! {
        _ = tunnel::serve(connection, args.port) => {
            eprintln!("the tunnel was closed by the server");
        }
        _ = tokio::signal::ctrl_c() => {
            println!("\n  Closing the tunnel\u{2026}");
            session.connection.close(0u32.into(), b"client shutdown");
        }
    }

    // Let QUIC flush the close frame before the process exits.
    endpoint.wait_idle().await;
    println!("  Session closed. Bye!");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolver_prefers_ipv4_when_both_families_are_present() {
        let addrs: Vec<SocketAddr> = vec![
            "[::1]:4433".parse().unwrap(),
            "127.0.0.1:4433".parse().unwrap(),
        ];
        assert_eq!(prefer_ipv4(&addrs), Some("127.0.0.1:4433".parse().unwrap()));
    }

    #[test]
    fn resolver_falls_back_to_ipv6_when_no_ipv4_is_available() {
        let addrs: Vec<SocketAddr> = vec!["[::1]:4433".parse().unwrap()];
        assert_eq!(prefer_ipv4(&addrs), Some("[::1]:4433".parse().unwrap()));
    }

    #[test]
    fn resolver_returns_none_for_an_empty_list() {
        assert_eq!(prefer_ipv4(&[]), None);
    }
}
