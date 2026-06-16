# Mobilink

Expose your localhost to your phone in one command.

Mobilink is a **self-hosted**, **mobile-first** network tunnel. It lets you access a local server from any mobile device via a public URL — no deployment, no account, no third-party service involved.

---

## The problem

Testing a web app on a real phone is painful. DevTools and emulators don't replace a real device. Existing tools (ngrok, expose) are generic tunnels: they're not designed for mobile development, they require an account, and your traffic passes through servers you don't control.

---

## What Mobilink does

```bash
mobilink start --port 3000 --server my-vps.com
```

```
✔ Tunnel active
  URL  → https://my-vps.com/s/abc123
  QR   → [████████████]

  GET  /          200  12ms
  GET  /style.css 200   3ms
  POST /api/login 201  45ms
```

- The public URL opens directly on your phone
- The Eruda debug console is automatically injected into every page
- The terminal displays requests in real time
- If your phone switches from WiFi to 4G, the tunnel survives

---

## Why self-hosted

- **Privacy** — your traffic never leaves your own infrastructure
- **No firewall blocking** — no third-party service involved
- **No account, no limits** — you operate your own server
- **Single binary** — zero dependencies, configurable via environment variables

---

## Architecture

```
[Dev machine]                    [Your VPS]                   [Phone]
      |                               |                           |
 localhost:3000                       |                           |
      |                               |                           |
 mobilink-cli  <--- QUIC tunnel --->  mobilink-server  <-- HTTPS -- Browser
```

The tunnel uses **QUIC** (the protocol behind HTTP/3), built for mobile networks: instant reconnection, packet loss resistance, transparent WiFi → 4G connection migration.

The server is an **intelligent HTTP proxy**: it understands the traffic passing through and can inspect or modify it (Eruda injection, real-time logs).

---

## Components

| Component | Role |
|---|---|
| `mobilink-cli` | Local agent installed on the dev's machine |
| `mobilink-server` | Relay server installed on the dev's VPS |
| `mobilink-core` | Shared library (types, protocol) |

---

## Quick start

### 1. Build the binaries

```bash
cargo build --release
# → target/release/mobilink-server  and  target/release/mobilink
```

### 2. Start the server on your VPS

```bash
MOBILINK_PUBLIC_URL=http://my-vps.com:8060 ./mobilink-server
```

### 3. Start a tunnel from your machine

```bash
mobilink start --port 3000 --server my-vps.com
```

Scan the QR code displayed in the terminal from your phone.

---

## Documentation

- **[User guide](docs/USER_GUIDE.md)** — install, run, flags, troubleshooting
- **[Technical guide](docs/TECHNICAL_GUIDE.md)** — architecture, wire protocol, design choices
- **[Roadmap](docs/PLAN.md)** · **[TDD rules](docs/TDD.md)** · **[Gherkin scenarios](docs/gherkins/)**

---

## Features

| Feature | Status |
|---|---|
| Mobile-first QUIC tunnel | MVP |
| QR code in the terminal | MVP |
| Real-time request inspector | MVP |
| Automatic Eruda injection | MVP |
| Auth-aware proxying (`X-Forwarded-*`, credential-safe CORS, `--auth cookie`) | MVP |
| Multi-port sessions | Post-MVP |
| Authentication / access control | Post-MVP |
| Web dashboard | Post-MVP |

---

## Tech stack

- **Rust** — CLI and server
- **QUIC via `quinn`** — tunnel protocol
- **`axum`** — public HTTP endpoint on the server side
- **`tokio`** — async runtime

---

## License

MIT
