# Feature: CLI — serving local traffic

Once the tunnel is up, the CLI is a bridge: every frame arriving from the
server is an HTTP request from a mobile browser. The CLI replays it against
the local server and ships the response back on the same stream.

---

## Scenario: A mobile request is replayed against the local server

  Given the tunnel is established for local port 3000
  And a local server is running on localhost:3000
  When a request frame arrives for GET /api/items
  Then the CLI sends GET http://localhost:3000/api/items with the same headers and body
  And encodes the local response (status, headers, body) into a response frame
  And sends that frame back on the same stream

---

## Scenario: The local server is down

  Given the tunnel is established for local port 3000
  And nothing is listening on localhost:3000
  When a request frame arrives
  Then the CLI answers with a 502 response frame
  And the body explains that the local server is unreachable
  So the mobile browser sees a clear error instead of a hang

---

## Scenario: Requests keep flowing while one is in flight

  Given the tunnel is established
  And the local server takes time to answer a slow request
  When a second request frame arrives meanwhile
  Then the CLI handles it on its own stream without waiting for the first
  So one slow page never blocks the rest of the session
