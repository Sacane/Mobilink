# Feature: Eruda automatic injection

The server is HTTP-aware. When an HTML response flows back through the tunnel,
the server silently injects the Eruda debug console script before the closing
`</body>` tag. The developer never has to configure anything — the debug
console is available the first time the mobile browser opens the URL.

---

## Scenario: The server injects Eruda into an HTML response

  Given the local server returned an HTTP response with Content-Type text/html
  When the server relays the response to the mobile browser
  Then the server inserts the Eruda script tag into the HTML body
  And the mobile browser receives a valid HTML page that loads Eruda

---

## Scenario: The server does not modify non-HTML responses

  Given the local server returned an HTTP response with Content-Type application/json
  When the server relays the response to the mobile browser
  Then the response body is passed through unchanged
  And no Eruda script is injected

---

## Scenario: Eruda injection can be disabled by the developer

  Given the developer started the CLI with the `--no-eruda` flag
  When an HTML response flows through the tunnel
  Then the server relays the HTML body unchanged
  And no Eruda script is injected
