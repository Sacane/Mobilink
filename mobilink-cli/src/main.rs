//! `mobilink` binary — parses the command line and drives the tunnel.
//!
//! Gherkin: cli_starting_a_tunnel.md + terminal_developer_experience.md.

use std::process::ExitCode;

use clap::Parser;

use mobilink_cli::args::{Cli, Command, StartArgs};
use mobilink_cli::{tls, tunnel, ui};

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
    // Resolve the server address (DNS or literal IP).
    let server_addr = tokio::net::lookup_host((args.server.as_str(), args.server_port))
        .await?
        .next()
        .ok_or_else(|| format!("could not resolve host '{}'", args.server))?;

    let endpoint = tls::insecure_client_endpoint()?;

    let session = tunnel::connect_and_handshake(
        &endpoint,
        server_addr,
        &args.server,
        args.port,
        args.no_eruda,
    )
    .await
    .map_err(|error| {
        format!(
            "could not reach the Mobilink server at {}:{} — {error}",
            args.server, args.server_port
        )
    })?;

    println!();
    println!("  Tunnel ready!  localhost:{}  \u{21d2}  {}", args.port, session.public_url);
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
