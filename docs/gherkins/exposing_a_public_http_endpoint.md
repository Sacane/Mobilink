# Feature: Exposing a public HTTP endpoint

The server listens on a public HTTPS port. Any mobile browser can reach it.
The server must route the request to the right tunnel, handle errors gracefully
when no session matches, and return a proper HTTP response in every case.

---

## Scenario: The server returns the developer's response to the mobile browser

  Given an active session exists with a connected QUIC tunnel
  When a mobile browser sends a GET request to the session's public URL
  Then the server resolves the session from the URL path
  And forwards the request through the QUIC tunnel
  And returns the HTTP response from the local server to the mobile browser
  And the response status and body match what the local server sent

---

## Scenario: The server returns 404 when the URL path matches no active session

  Given no session is active for path `/s/unknownid`
  When a mobile browser sends a request to that path
  Then the server responds with HTTP 404
  And the body indicates that the session was not found

---

## Scenario: The server returns 502 when the tunnel is unavailable

  Given a session exists but its QUIC tunnel has disconnected
  When a mobile browser sends a request to that session's public URL
  Then the server attempts to forward the request
  And the forwarding fails because the tunnel is gone
  And the server responds with HTTP 502 to the mobile browser
