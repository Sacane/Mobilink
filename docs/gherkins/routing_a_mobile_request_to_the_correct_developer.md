# Routing a mobile request to the correct developer

```gherkin
Feature: Routing a mobile request to the correct developer
  The server forwards the mobile browser's request through the tunnel
  to the developer whose session owns that URL.

  Scenario: A mobile request is forwarded to the right developer
    Given a session with ID "abc123" is active and belongs to developer A
    And developer A is listening on port 3000
    When a mobile browser sends a GET request to the public URL of session "abc123"
    Then the server forwards the request to developer A's tunnel
    And developer A's local server on port 3000 receives the request
```
