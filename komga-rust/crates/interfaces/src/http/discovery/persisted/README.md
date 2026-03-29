# http::discovery::persisted

This subtree owns the transport shell for persisted discovery endpoints.
It exists to keep the main `persisted.rs` route module thin by splitting backend installation, delegator helpers, transport-only models, and small response-shaping utilities into adjacent files.

## Files in this subtree

- `backend.rs`: the installable persisted discovery backend contract used by server composition.
- `delegates.rs`: crate-local forwarding helpers that call the non-HTTP persisted-access modules.
- `helpers.rs`: transport-side paging, regex extraction, media-profile, and response helpers.
- `models.rs`: persisted discovery DTOs and request-filter structs owned by the transport shell.

## Keep outside this subtree

- Concrete SQLite query ownership, which belongs in non-HTTP access modules and `komga-infrastructure`.
- Cross-route discovery authentication logic, which stays in `http::discovery_auth`.
- General route registration, which stays in `http::discovery::persisted.rs` and the parent discovery router wiring.
