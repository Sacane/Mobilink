# Feature: Terminal developer experience

The moment a session is opened, the developer should be able to point their
phone at the terminal without looking up any URL. As requests flow in, the
terminal shows a live log so the developer can follow what the mobile is doing.

---

## Scenario: The CLI displays a QR code when the session is opened

  Given the CLI has connected to the server and received a public URL
  When the session is ready
  Then the CLI prints the public URL in the terminal
  And renders a scannable QR code for that URL
  So the developer can point their phone at the screen immediately

---

## Scenario: The CLI logs each incoming request in real time

  Given a session is active and the tunnel is connected
  When the mobile browser sends an HTTP request
  And the server forwards it through the tunnel
  Then the CLI displays one log line for that request
  And the log line includes the HTTP method, the path, and the response status
  And the log line includes the round-trip latency

---

## Scenario: The CLI shuts down cleanly on Ctrl+C

  Given a session is active with a connected tunnel
  When the developer presses Ctrl+C
  Then the CLI sends a teardown message to the server
  And the server closes and removes the session
  And the CLI exits with a zero status code
