# Session identity and public URL assignment

```gherkin
Feature: Session identity and public URL assignment
  Every active session has a unique identity and a public URL
  that the developer can share or scan from their phone.

  Scenario: A developer receives a unique ID and a public URL upon connection
    Given a developer has just connected and announced port 3000
    When the server opens the session
    Then the developer receives a unique session ID
    And the developer receives a public URL tied to that session ID
```
