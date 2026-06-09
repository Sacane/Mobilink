# Active session registry

```gherkin
Feature: Active session registry
  The server maintains a live registry of all open sessions
  so it can route incoming traffic to the right developer at any time.

  Scenario: The server can retrieve an active session by its ID
    Given a session with ID "abc123" is active
    When the server looks up session "abc123"
    Then the session is found
    And it points to the correct developer's tunnel

  Scenario: The server finds nothing for an unknown session ID
    Given no session with ID "xyz999" exists
    When the server looks up session "xyz999"
    Then no session is returned
```
