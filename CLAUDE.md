# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Mobilink is a self-hosted, mobile-first network tunnel that exposes localhost via a public URL. Unlike simple TCP tunnels (ngrok-style), it is an **HTTP-aware proxy** that understands the traffic passing through it.

See [doc/PLAN.md](doc/PLAN.md) for the full architecture and roadmap.
See [doc/README.md](doc/README.md) for the official project documentation.

## Workspace structure

Cargo workspace with three crates:

- `mobilink-core` — shared types (session, messages, config). No I/O, no async.
- `mobilink-server` — relay server: accepts QUIC connections, manages sessions, HTTP reverse proxy.
- `mobilink-cli` — local agent: connects to server via QUIC, forwards traffic to/from localhost, renders terminal UI.

## Commands

```bash
# Build all crates
cargo build

# Build release binaries
cargo build --release

# Run tests (all workspace)
cargo test

# Run a single test
cargo test -p mobilink-core <test_name>

# Run the server (dev)
cargo run -p mobilink-server

# Run the CLI
cargo run -p mobilink-cli -- start --port 3000 --server my-vps.com

# Lint
cargo clippy --all-targets --all-features

# Format
cargo fmt --all
```

## Architecture

```
[Dev machine]                    [Dev's VPS]                    [Mobile device]
      |                               |                               |
 localhost:PORT                       |                               |
      |                               |                               |
 mobilink-cli  <=== QUIC tunnel ===>  mobilink-server  <=== HTTPS === Browser
```

**Request flow:**
1. CLI opens a persistent QUIC connection to the developer's own server (`quinn`)
2. Server assigns a session ID and public URL (e.g. `https://my-vps.com/s/abc123`)
3. Inbound HTTP request from mobile → server routes it to the correct QUIC stream → CLI forwards to localhost
4. Response travels back; server intercepts `text/html` responses to inject Eruda debug script
5. CLI displays the QR code and live request log in the terminal

## Key technical choices

- **QUIC via `quinn`** for the tunnel — connection migration survives WiFi→4G switches on mobile
- **`hyper`** for HTTP proxy logic on the server side
- **`axum`** for the server's internal session management API
- **`clap`** for CLI argument parsing
- **`tracing`** for structured logs throughout

## Deployment model

Mobilink is **self-hosted by default**. The developer runs `mobilink-server` on their own VPS or server. There is no dependency on any Mobilink-operated cloud infrastructure. The CLI takes a `--server` flag to point at any host.

A Mobilink-operated cloud mode may be added later as a layer on top of the same architecture — do not design around it now.

## Scope constraints (MVP)

No authentication, no user accounts, no monetization, no native mobile app, no web dashboard. Keep features to the roadmap in `doc/PLAN.md`.
