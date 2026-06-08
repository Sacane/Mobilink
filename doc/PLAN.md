# Mobilink — Implementation Plan

## Vision: Intelligent mobile-first proxy

Mobilink is not a simple pass-through tunnel. It is an HTTP-aware proxy that understands the traffic passing through it, optimized for mobile networks, with built-in debugging tools.

---

## Deployment model: Self-hosted

Mobilink is **self-hosted by default**. The developer installs and operates the server on their own infrastructure (VPS, personal server). There is no dependency on any third-party cloud service.

- Traffic never leaves the developer's own infrastructure
- No account, no subscription
- Works in corporate environments (no DSI/firewall blocking)
- Single binary, zero dependencies, configurable via environment variables

The CLI can point to any server:
```bash
mobilink start --port 3000 --server my-vps.com
```

A Mobilink-operated cloud mode may be added later as a layer on top of the same architecture — not a concern for the MVP.

---

## Key differentiators

### 1. QUIC as the tunnel protocol
- The protocol behind HTTP/3, designed for degraded mobile networks
- **Connection migration**: if the mobile switches from WiFi to 4G, the tunnel survives
- **0-RTT reconnection**: near-instant reconnection after a drop
- **Multiplexing** without head-of-line blocking (better performance under packet loss)
- Implemented via the `quinn` Rust crate

### 2. Automatic Eruda injection
- The proxy intercepts HTML responses passing through the tunnel
- It automatically injects the Eruda debug script
- Zero configuration for the developer: the debug console is available as soon as the URL is opened

### 3. Native QR code in the terminal
- As soon as a session is opened, the QR code of the public URL is displayed directly in the terminal
- Instant "wow" moment: just point your phone at the terminal

### 4. Real-time request inspector
- The terminal displays live HTTP requests transiting from the mobile
- Method, path, status code, latency — like a lightweight built-in proxy inspector

---

## Architecture

```
[Dev machine]                        [Dev's VPS]                    [Mobile device]
      |                                    |                               |
 localhost:3000                            |                               |
      |                                    |                               |
 mobilink-cli  <==== QUIC tunnel ====>  mobilink-server  <==== HTTPS ==== Browser
  (Rust)          persistent connection    (Rust)            public URL
                                           |
                                    HTTP-aware proxy
                                    - injects Eruda
                                    - logs requests
                                    - manages sessions
```

**3 components:**

| Component | Role |
|---|---|
| `mobilink-cli` | Local agent: connects to the server via QUIC, forwards traffic to/from localhost |
| `mobilink-server` | Relay hosted by the dev: manages QUIC sessions, exposes public URLs, HTTP proxy |
| Public URL | Generated per session, based on the dev's server domain |

---

## Session flow

1. Dev runs `mobilink start --port 3000 --server my-vps.com`
2. CLI opens a QUIC connection to their own Mobilink server
3. Server generates a session ID and public URL (e.g. `https://my-vps.com/s/abc123`)
4. CLI displays the URL and QR code in the terminal
5. Mobile opens the URL → server proxies the request to the CLI via QUIC → CLI forwards to `localhost:3000`
6. HTML response is intercepted by the proxy, Eruda is injected
7. Dev's terminal displays the request in real time

---

## MVP Roadmap

### Phase 1 — Foundations
- [ ] Initialize the Cargo workspace (`mobilink-server`, `mobilink-cli`, `mobilink-core`)
- [ ] Define shared types in `mobilink-core` (messages, session, config)
- [ ] Set up CI (build + tests)

### Phase 2 — QUIC tunnel
- [ ] Integrate `quinn` server-side: accept QUIC connections
- [ ] Integrate `quinn` CLI-side: establish a persistent QUIC connection
- [ ] Handshake protocol: CLI announces itself, server assigns a session ID
- [ ] Basic TCP forwarding through the QUIC tunnel (not yet HTTP-aware)

### Phase 3 — Public HTTP exposure
- [ ] Server-side: listen on a public HTTP port per session (or reverse proxy by subdomain)
- [ ] Route incoming HTTP requests to the correct QUIC tunnel (by session ID)
- [ ] Generate and display the public URL on the CLI side

### Phase 4 — QR code & terminal UX
- [ ] Display the URL QR code in the terminal at session start
- [ ] Display incoming requests in real time (method, path, status, latency)
- [ ] Clean Ctrl+C handling (session teardown, resource cleanup)

### Phase 5 — Intelligent HTTP proxy
- [ ] Switch the proxy to HTTP-aware mode (parse requests/responses)
- [ ] Automatically inject Eruda into `text/html` responses
- [ ] CLI option to disable injection (`--no-eruda`)

### Phase 6 — Packaging & distribution
- [ ] Multi-platform release builds (Linux, macOS, Windows) via `cross`
- [ ] One-liner install script
- [ ] First tagged release

---

## Key Rust crates

| Crate | Usage |
|---|---|
| `quinn` | QUIC client/server |
| `tokio` | Async runtime |
| `hyper` | HTTP proxy on the server side |
| `axum` | Internal server API (session management) |
| `qrcode` | Terminal QR code generation |
| `ratatui` | Advanced terminal UI (optional, phase 4+) |
| `clap` | CLI argument parsing |
| `tracing` | Structured logging |
