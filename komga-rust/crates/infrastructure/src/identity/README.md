# Identity infrastructure

This subtree owns the concrete runtime-facing identity backend inside `komga-infrastructure`.
It adapts auth persistence, session storage, Kobo, and KOReader persistence to the
`komga-application::identity_access` ports used by HTTP state composition.

## Files in this subtree

- `mod.rs`: public identity facade and private subtree wiring.
- `adapter.rs`: `IdentityAccess`, the concrete application-port adapter.
- `session_store.rs`: in-memory session state and persisted remember-me tokens.
- `users/`: authentication and SQLite-backed user mutation.
- `kobo/`: Kobo sync state, diffing, seeding, and proxy transport.

## Keep outside this subtree

- HTTP header parsing and endpoint response shaping.
- Pure identity use-case logic, which belongs in `komga-application::identity_access`.
- Server bootstrap and backend installation call sites, which belong in `komga-server`.
