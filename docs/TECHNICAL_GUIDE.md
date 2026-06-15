# Mobilink — Technical Guide

This document explains how Mobilink is built: architecture, domain
contracts, wire protocol, and the reasoning behind the technical choices.
Read [USER_GUIDE.md](USER_GUIDE.md) first for what Mobilink does.

---

## 1. Workspace layout

```
mobilink-core/     shared types — no I/O, no async, no network crates
mobilink-server/   relay: QUIC tunnel endpoint + public HTTP endpoint
mobilink-cli/      local agent: tunnel client + localhost replay + terminal UI
```

| Crate | Key modules |
|---|---|
| `mobilink-core` | `session` (Session, SessionId), `message` (handshake enums), `http` (HttpRequestData/HttpResponseData), `wire` (encode/decode), `config` |
| `mobilink-server` | `lib.rs` (domain traits + contract tests), `registry`, `router`, `dispatcher`, `handshake`, `transform`, `http`, `quic`, `main.rs` |
| `mobilink-cli` | `args`, `tls`, `tunnel`, `local`, `ui`, `main.rs` |

---

## 2. Domain layer (mobilink-server/src/lib.rs)

The server's behaviour is specified by **synchronous traits**, each backed
by Gherkin scenarios in `docs/gherkins/` and locked by contract tests in
`lib.rs`. Production implementations live in their own modules:

| Trait | Production impl | Role |
|---|---|---|
| `SessionRegistry` | `InMemorySessionRegistry` | open / get / register / close sessions |
| `HttpRouter` | `SessionRouter` | any request path → the active tunnel session (whole-host routing) |
| `RequestForwarder` | `QuicTunnelForwarder` | request bytes → through the tunnel → response bytes |
| `RequestPipeline` | `RequestDispatcher` | router + forwarder = full request lifecycle |
| `TunnelHandshakeHandler` | `SessionHandshakeHandler` | Hello(port) → SessionCreated |
| `SessionOptions` | `TunnelMap` | per-session flags (`--no-eruda`) |
| `ResponseTransformer` | `ErudaInjector` | HTML rewriting on the way back |

Two deliberate design points:

- **The domain is sync.** Network layers bridge into it:
  the axum handler calls the pipeline inside `spawn_blocking`, and
  `QuicTunnelForwarder` re-enters tokio with `Handle::block_on` from that
  blocking thread. The domain tests stay simple and transport-free.
- **Tests talk to traits, not implementations.** The contract tests in
  `lib.rs` use stubs/spies; swapping `InMemorySessionRegistry` for, say, a
  Redis-backed registry would not touch a single test.

---

## 3. Wire protocol

Everything that crosses the tunnel is serialized with **bincode** through
the pure helpers in `mobilink-core::wire` (`encode`/`decode`,
`MAX_FRAME_BYTES` = 32 MiB).

**Framing rule: one message per stream direction.** The sender writes its
bytes and `finish()`es; the receiver `read_to_end()`s. The QUIC stream
boundary *is* the message boundary — no length prefixes, no framing bugs.

| Stream | Opened by | Client → Server | Server → Client |
|---|---|---|---|
| Handshake (first bi) | CLI | `ClientMessage::Hello { local_port, no_eruda }` | `ServerMessage::SessionCreated { session_id, public_url }` |
| Request (one per HTTP request) | server | `HttpResponseData` (the reply) | `HttpRequestData` |

Session teardown needs no message: when the CLI closes the **connection**
(Ctrl+C, crash, network loss), the server's `connection.closed().await`
fires, the tunnel is dropped from `TunnelMap` and the session is closed in
the registry. The public URL turns 404 within milliseconds.

---

## 4. Request lifecycle (server)

```
mobile browser
   │  GET /api/items?page=2
   ▼
axum fallback handler (http.rs)
   │  path forwarded verbatim → target "/api/items?page=2"
   │  Request → HttpRequestData   (strip host/connection/accept-encoding/…)
   │  wire::encode
   ▼
RequestDispatcher::handle          (spawn_blocking)
   │  SessionRouter::resolve_session → the active tunnel session
   ▼
QuicTunnelForwarder::forward       (Handle::block_on)
   │  open_bi → write frame → finish → read_to_end
   ▼
CLI serve loop (tunnel.rs)         one tokio task per stream
   │  wire::decode → replay_locally (reqwest → http://127.0.0.1:{port})
   │  response → wire::encode → same stream back
   ▼
axum handler
   │  wire::decode → ErudaInjector::transform (unless --no-eruda)
   │  HttpResponseData → Response  (recompute content-length)
   ▼
mobile browser
```

Error mapping: no session → **404** · tunnel gone → **502** ·
local server down → CLI itself answers a **502 frame** (the tunnel never
hangs).

`accept-encoding` is stripped from forwarded requests so local responses
arrive uncompressed — that is what keeps HTML injectable. Compressed or
non-UTF-8 HTML passes through untouched rather than corrupted.

---

## 5. QUIC & TLS choices

- **quinn 0.11 / rustls 0.23 (ring)**. QUIC gives connection migration
  (WiFi → 4G survives), 0-RTT reconnection and per-stream multiplexing —
  one slow request never blocks the others, which maps exactly to our
  one-stream-per-request design.
- **Server certificate**: generated at startup with `rcgen`
  (self-signed). The CLI installs a `ServerCertVerifier` that accepts any
  certificate: traffic is encrypted but the server is **not
  authenticated** (MVP). The follow-up is certificate pinning: print the
  cert fingerprint server-side, pass `--server-fingerprint` to the CLI.
- **Keep-alive 5 s / idle timeout 10 s** on the client: keeps NAT mappings
  warm during quiet periods, and makes "server unreachable" fail in
  seconds instead of hanging.

---

## 6. Testing strategy

| Level | Where | What it proves |
|---|---|---|
| Contract tests (stubs/spies) | `mobilink-server/src/lib.rs` | the domain scenarios, frozen — 14 tests |
| Module unit tests | `router`, `transform`, `http`, `core::*`, `cli::*` | production implementations, incl. axum layer via `tower::oneshot` |
| QUIC integration | `mobilink-server/tests/quic_tunnel.rs` | real handshake, frame round-trip, teardown over real sockets |
| End-to-end | `mobilink-cli/tests/e2e.rs` | browser → server → tunnel → local app → back, Eruda included |

Every feature followed the Red → Green → Refactor cycle of
[TDD.md](TDD.md): contract tests are written against traits and never
modified afterwards; production code only appears at refactor time.

Run everything with `cargo test --workspace`.

---

## 7. Operational notes

- The server is a single stateless-on-disk process; sessions are an
  in-memory `HashMap` behind a `Mutex`. Horizontal scaling or persistence
  would slot in behind `SessionRegistry` without touching anything else.
- Logs are `tracing`-structured; set `RUST_LOG=debug` for verbose output.
- The public endpoint speaks plain HTTP; terminate TLS in front of it
  (Caddy/nginx) and set `MOBILINK_PUBLIC_URL=https://…` accordingly.
- Known MVP limits: whole-host routing dedicates the public host to a
  single active tunnel (the most recent one wins; per-session subdomain
  routing is the planned multi-tunnel fix); dev-server hot-reload
  WebSockets (Vite/Nuxt HMR) are not tunneled yet; 32 MiB body cap; no
  authentication of CLIs (anyone who can reach UDP 4433 can open a
  session — firewall accordingly).
