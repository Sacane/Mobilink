# Feature: CLI — starting a tunnel

The developer's entire experience starts with one command:
`mobilink start --port 3000 --server my-vps.com`. The CLI validates the
arguments, reaches the server over QUIC, performs the handshake and keeps
the session information at hand for the rest of the run.

---

## Scenario: The developer starts a tunnel with valid arguments

  Given a Mobilink server is reachable at my-vps.com
  When the developer runs `mobilink start --port 3000 --server my-vps.com`
  Then the CLI connects to the server over QUIC
  And sends a Hello announcing port 3000
  And receives the session ID and the public URL
  And keeps them for display and logging

---

## Scenario: The developer disables Eruda injection

  Given a Mobilink server is reachable
  When the developer adds the `--no-eruda` flag
  Then the Hello message carries no_eruda = true
  And the server will relay HTML untouched for this session

---

## Scenario: The server is unreachable

  Given no Mobilink server is listening at the given address
  When the developer runs `mobilink start --port 3000 --server wrong-host.com`
  Then the CLI stops with a clear error message
  And the exit code is non-zero

---

## Scenario: The CLI rejects invalid arguments

  Given the developer omits the required `--port` argument
  When they run `mobilink start --server my-vps.com`
  Then the CLI prints a usage message explaining the required arguments
  And exits without attempting any connection
