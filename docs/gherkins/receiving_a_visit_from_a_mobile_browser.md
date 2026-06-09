# Receiving a visit from a mobile browser

```gherkin
Feature: Receiving a visit from a mobile browser
  When a mobile browser opens a public session URL,
  the server recognises which session it belongs to.

  Scenario: The server identifies the session behind a public URL
    Given a session with ID "abc123" is active and has a public URL
    When a mobile browser sends a request to that public URL
    Then the server identifies the request as belonging to session "abc123"
```
