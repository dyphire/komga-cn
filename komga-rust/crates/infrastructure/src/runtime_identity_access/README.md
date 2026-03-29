# runtime_identity_access

This subtree owns the concrete runtime-facing auth and device-access backend inside `komga-infrastructure`.
It exists to group token/session persistence, Kobo and KOReader lookups, backend installation state, and direct user mutations behind one stable module surface.

## Files in this subtree

- `backend_contract.rs`: backend types, DTOs, and closure-based contract shared by callers and installers.
- `backend_state.rs`: `OnceLock` installation and default test-backend resolution.
- `auth_access.rs`: token, remember-me, API-key, user listing, and authentication-activity delegation wrappers.
- `kobo_access.rs`: Kobo, KOReader, thumbnail, sync-point, and read-progress delegation wrappers.
- `user_mutation.rs`: direct SQLite-backed user create, update, and delete operations.
- `test_backend.rs`: test-only default backend composition used when no explicit runtime backend is installed.

## Keep outside this subtree

- HTTP header parsing and endpoint response shaping.
- Pure identity use-case logic, which belongs in `komga-application::identity_access`.
- Server bootstrap and backend installation call sites, which belong in `komga-server`.
