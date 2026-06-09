# Cleanly closing a session

```gherkin
Feature: Cleanly closing a session
  When a developer stops their tunnel, the session is released
  and the public URL becomes unreachable.

  Scenario: The session is removed when the developer disconnects
    Given an active session with ID "abc123" exists
    When the developer stops the tunnel
    Then the session "abc123" is removed from the active registry

  Scenario: A mobile browser is informed when a session is no longer active
    Given the session with ID "abc123" has been closed
    When a mobile browser sends a request to the former public URL
    Then the browser receives a response indicating the session is no longer available
```
