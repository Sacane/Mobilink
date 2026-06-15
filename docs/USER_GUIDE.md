# Mobilink — User Guide

Mobilink exposes a port of your development machine through your **own**
server, behind a public URL you can open on your phone. No account, no
third-party cloud: one binary on your VPS, one binary on your laptop.

```
your laptop                 your VPS                    your phone
mobilink  <== QUIC ==>  mobilink-server  <== HTTP ==>  browser
localhost:3000          https://my-vps.com
```

---

## 1. Requirements

- A server you control (VPS, home server…) with a public IP or DNS name.
- One **UDP** port open for the QUIC tunnel (default **4433**).
- One **TCP** port open for the public HTTP endpoint (default **8060**).
- The two release binaries: `mobilink-server` (server) and `mobilink` (CLI).

Build them from source with:

```bash
cargo build --release
# → target/release/mobilink-server  and  target/release/mobilink
```

---

## 2. Start the server (once, on your VPS)

```bash
MOBILINK_PUBLIC_URL=http://my-vps.com:8060 ./mobilink-server
```

Configuration is done entirely through environment variables:

| Variable | Default | Meaning |
|---|---|---|
| `MOBILINK_QUIC_BIND` | `0.0.0.0:4433` | UDP address the tunnel listens on |
| `MOBILINK_HTTP_BIND` | `0.0.0.0:8060` | TCP address the public endpoint listens on |
| `MOBILINK_PUBLIC_URL` | `http://localhost:8060` | Base of the URLs given to developers |

`MOBILINK_PUBLIC_URL` is what your phone will use — set it to whatever your
server is reachable as from the outside (scheme included), with no path.

The server keeps running; the most recently connected tunnel owns the public
host (see the one-active-tunnel note in section 6).

---

## 3. Open a tunnel (on your laptop)

```bash
./mobilink start --port 3000 --server my-vps.com
```

| Flag | Required | Meaning |
|---|---|---|
| `--port` | yes | Local port to expose (your dev server) |
| `--server` | yes | Host name or IP of **your** Mobilink server |
| `--server-port` | no (4433) | QUIC port of the server |
| `--no-eruda` | no | Disable the mobile debug console injection |

The terminal then shows:

- the **public URL** of your session,
- a **QR code** — point your phone's camera at the terminal, tap, done,
- a **live request log**: every request your phone makes, with method,
  path, status code and latency:

```
  Tunnel ready!  localhost:3000  ⇒  http://my-vps.com:8060

  █▀▀▀▀▀█ ▀▄█▀▄▀▄ █▀▀▀▀▀█
  …

      GET /            → 200  (4 ms)
      GET /style.css   → 200  (2 ms)
      GET /api/items   → 200  (11 ms)
```

Press **Ctrl+C** to close the session cleanly; the public URL stops working
immediately.

---

## 4. The mobile debug console (Eruda)

Every HTML page going through the tunnel gets the
[Eruda](https://github.com/liriliri/eruda) debug console injected
automatically. On your phone, tap the floating gear button to inspect the
console, network calls, DOM and more — like desktop DevTools, on mobile.

Don't want it? Start the tunnel with `--no-eruda`.

Note: Eruda loads from the jsDelivr CDN, so the phone needs internet access
beyond your server.

---

## 5. Single-page apps (SPA) and dev servers

Mobilink dedicates the whole public host to your active tunnel
(`https://my-vps.com/`) and forwards **every** request path verbatim to your
local server. A client-side router in **history mode** (`/dashboard`) and
absolute asset paths (`/_nuxt/...`, `/assets/app.js`) all resolve correctly
through the tunnel.

**SPAs in history mode and framework dev servers (Vite, Nuxt, Next, …) work
out of the box — no hash routing, no relative-base config needed.**

> The one thing that does **not** travel through the tunnel yet is the dev
> server's hot-reload **WebSocket** (Vite/Nuxt HMR). Your page loads and runs,
> but live reload is disabled — refresh manually after a change. WebSocket
> tunneling is on the roadmap.

---

## 6. What you should know (MVP limitations)

- **One active tunnel per server.** The public host is dedicated to the most
  recently connected tunnel (whole-host routing), so one server proxies one
  developer at a time. Per-session subdomain routing is on the roadmap.
- **Hot reload (HMR) is not tunneled yet.** Dev-server WebSockets don't pass
  through, so live reload is disabled — the page loads, but you refresh
  manually. Plain WebSocket tunneling is on the roadmap.
- **TLS between CLI and server**: the tunnel is encrypted, but the server's
  certificate is self-signed and not verified by the CLI (MVP). Certificate
  pinning is planned.
- **Public HTTPS** is not terminated by Mobilink itself; put a reverse
  proxy (Caddy, nginx) in front of the HTTP endpoint if you need
  `https://` for your phone.
- **Sessions live in memory**: restarting the server forgets the session
  (the CLI notices and exits; just restart it).
- Request and response bodies are capped at **32 MiB**.

---

## 7. Troubleshooting

| Symptom | Likely cause |
|---|---|
| `could not reach the Mobilink server` | Wrong host/port, or UDP 4433 blocked by a firewall |
| Public URL returns **404** | No tunnel is connected — the CLI was stopped or the server restarted |
| Public URL returns **502** | The tunnel is up but nothing answers on your local port |
| Page loads but **no live reload** | HMR WebSocket isn't tunneled yet — refresh manually (section 5) |
