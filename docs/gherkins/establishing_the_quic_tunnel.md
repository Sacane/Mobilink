# Feature: Establishing the QUIC tunnel

A developer uses the CLI to expose a local port. The CLI connects to the
developer's own server via QUIC and negotiates a session. From that moment,
the tunnel is live and requests can flow through it.

---

## Scenario: The CLI connects to the server and receives a session

  Given a developer runs `mobilink start --port 3000 --server my-vps.com`
  When the CLI establishes a QUIC connection to the server
  Then the server accepts the connection
  And the server assigns a session ID and a public URL to the CLI
  And the CLI receives and stores that session information

---

## Scenario: The server registers the tunnel as active for that session

  Given the CLI has connected via QUIC and announced port 3000
  When the server processes the handshake
  Then the session is marked as having an active tunnel
  And subsequent requests to the public URL can be forwarded through it

---

## Scenario: The tunnel is torn down when the CLI disconnects

  Given a QUIC tunnel is established for an active session
  When the developer stops the CLI (Ctrl+C or network loss)
  Then the server detects the disconnection
  And the session is closed and removed from the registry
  And subsequent requests to the public URL return a "session not found" error
