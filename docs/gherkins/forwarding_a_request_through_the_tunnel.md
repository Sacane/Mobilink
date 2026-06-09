# Feature: Forwarding a request through the QUIC tunnel

Once the tunnel is established, the server acts as a relay: it receives raw
HTTP bytes from the mobile browser, opens a QUIC stream to the CLI, sends the
bytes, and waits for the response to flow back.

---

## Scenario: The server sends a mobile request to the CLI through the tunnel

  Given a QUIC tunnel is active for a session
  When the server receives a raw HTTP request for that session's public URL
  Then the server opens a QUIC stream to the CLI
  And sends the raw HTTP request bytes over that stream

---

## Scenario: The CLI forwards the request to the local server and collects the response

  Given the CLI received raw HTTP request bytes from the server via the tunnel
  When it forwards those bytes to localhost on the configured port
  Then it receives the raw HTTP response from the local server
  And sends those response bytes back to the server over the same QUIC stream

---

## Scenario: The server receives the response and closes the stream

  Given the CLI sent back a raw HTTP response over the QUIC stream
  When the server reads those bytes
  Then it closes the QUIC stream
  And returns the raw HTTP response to the mobile browser
