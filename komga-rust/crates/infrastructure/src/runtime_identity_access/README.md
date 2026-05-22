# runtime_identity_access

This subtree owns the concrete runtime-facing auth and device-access backend inside `komga-infrastructure`.
It adapts the lower-level auth persistence, session store, Kobo, and KOReader persistence helpers to the `komga-application::identity_access` ports used by HTTP state composition.

## Files in this subtree

- `../runtime_identity_access.rs`: public module surface and compatibility exports for direct infrastructure tests.
- `access.rs`: `IdentityAccess`, the concrete application-port adapter.
- `user_mutation.rs`: direct SQLite-backed user create, update, and delete operations.

## Keep outside this subtree

- HTTP header parsing and endpoint response shaping.
- Pure identity use-case logic, which belongs in `komga-application::identity_access`.
- Server bootstrap and backend installation call sites, which belong in `komga-server`.
