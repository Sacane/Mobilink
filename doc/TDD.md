# TDD Rules — Mobilink

Every feature in this codebase must follow the strict Red → Green → Refactor cycle described below.
This file is a binding instruction for any Claude agent working in this repository.

---

## The cycle

### 🔴 Red — Write a failing test first

**Before writing any implementation code**, write a test that:

1. **Expresses business intent**, not implementation details.
   The test name and body must read like a specification, not like code.
   Prefer: `session_registry_rejects_duplicate_session_ids`
   Avoid: `test_hashmap_insert_returns_false`

2. **Fails for the right reason.**
   Run `cargo test` and confirm the test fails with a meaningful error — not a compile error caused by missing types, but a logical failure that proves the feature is not yet implemented.
   If the test cannot compile because an abstraction does not exist yet, create the minimal trait/struct signature (no logic) to make it compile, then confirm the test fails.

3. **Uses abstractions and mocks where needed.**
   Never couple a test to I/O, the network, the filesystem, or the clock.
   Define traits to represent dependencies (e.g. `SessionStore`, `UrlGenerator`) and inject mock implementations in tests.
   The real implementations come in the Green phase.

4. **Covers one behaviour at a time.**
   One test = one reason to fail. Do not bundle multiple assertions about different behaviours in a single test.

> ⛔ Do not write any implementation logic before this test exists and fails.

---

### 🟢 Green — Make the test pass without touching production code

The Green phase happens **exclusively in the test zone**.

- Make the test pass by writing the simplest possible code **inside the test file itself** (inline structs, hardcoded return values, stub implementations directly in the test module).
- **Do not write or modify any production code** in `src/`. Production code is the responsibility of the Refactor phase.
- Hardcoding a return value inside a test-local stub is expected and correct at this stage.
- Do not anticipate future requirements. Do not abstract prematurely.

Run `cargo test` and confirm **all tests pass** before moving on.

> ⛔ Do not move to Refactor if any test is red.
> ⛔ Do not write or modify any file outside of the test scope during this phase.

---

### 🔵 Refactor — Write the real implementation without touching the tests

With all tests green, write the actual production code in `src/`:

- Move the logic from the test stubs into proper production modules.
- Remove duplication, improve naming, extract functions or modules.
- Ensure the code matches the architecture described in `CLAUDE.md`.
- **Do not modify any test code.** Tests are frozen at this stage — they are the contract.

Run `cargo test` after every change. If a test turns red, undo the last change immediately.

> ⛔ Do not add new behaviour during Refactor. If you identify a missing case, write a new Red test first.
> ⛔ If you find yourself needing to modify the test code to make the refactor work, stop immediately. This means the Red/Green phase was poorly designed. Go back, identify what was wrong in the test specification or the stub, fix it, and restart the cycle cleanly.

---

## Traits and mocks — Rust conventions

Dependencies that involve I/O or external state must be expressed as traits:

```rust
// Define the abstraction
pub trait SessionStore: Send + Sync {
    fn insert(&self, session: Session) -> Result<(), SessionError>;
    fn get(&self, id: &SessionId) -> Option<Session>;
}

// Mock for tests
pub struct InMemorySessionStore {
    sessions: std::sync::Mutex<std::collections::HashMap<SessionId, Session>>,
}

impl SessionStore for InMemorySessionStore { ... }
```

Real implementations (backed by a database, network, etc.) implement the same trait and are only introduced in the **Refactor phase**.

---

## Test naming convention

```
<unit>_<behaviour>_<expected_outcome>
```

Examples:
- `session_registry_assigns_unique_id_per_connection`
- `url_generator_builds_url_from_domain_and_session_id`
- `http_router_returns_404_when_session_not_found`

---

## Where tests live

- **Unit tests**: in the same file as the code under test, inside a `#[cfg(test)] mod tests { }` block.
- **Integration tests**: in a `tests/` directory at the root of the crate.
- Tests that cross crate boundaries go in `mobilink-core/tests/` or the relevant crate's `tests/` folder.

---

## Checklist before any commit

- [ ] Every new behaviour has a corresponding test written before the implementation
- [ ] `cargo test --workspace` passes with no failures
- [ ] `cargo clippy --all-targets --all-features` reports no warnings
- [ ] No test is marked `#[ignore]` without a written justification in a comment
