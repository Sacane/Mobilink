# Session uniqueness

```gherkin
Feature: Session uniqueness
  The system guarantees that no two active tunnels share the same identity,
  preventing routing conflicts.

  Scenario: The server rejects a session that duplicates an existing ID
    Given a session with ID "abc123" is already active
    When the server attempts to register a new session with the same ID "abc123"
    Then the registration is rejected
    And the existing session with ID "abc123" remains active and unaffected
```
