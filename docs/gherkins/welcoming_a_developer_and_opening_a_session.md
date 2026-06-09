# Welcoming a developer and opening a session

```gherkin
Feature: Welcoming a developer and opening a session
  A developer can register their local server with Mobilink
  so it becomes reachable from any mobile device.

  Scenario: The server opens a session when a developer connects
    Given no active session exists for this developer
    When the developer connects to the Mobilink server and announces port 3000
    Then the server accepts the connection
    And a new session is opened for that developer
```
