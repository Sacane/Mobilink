# Returning the response to the mobile browser

```gherkin
Feature: Returning the response to the mobile browser
  The response produced by the developer's local server
  travels back through the tunnel and reaches the mobile browser.

  Scenario: The mobile browser receives the response from the developer's local server
    Given a mobile request has been forwarded to developer A's local server
    When developer A's local server responds with status 200 and an HTML body
    Then the server relays that response back through the tunnel
    And the mobile browser receives the status 200 and the HTML body
```
