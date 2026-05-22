# http::discovery::persisted

This subtree owns the transport shell for persisted discovery endpoints.
It keeps the discovery route modules thin by grouping persisted-discovery request helpers, transport-only models, and response-shaping utilities next to the routes that use them.

## Files in this subtree

- `authors_queries.rs`: author facet payload shaping.
- `common_helpers.rs`: transport-side query decoding, filtering, paging, and error responses.
- `library_mappings.rs`: compatibility mapping for legacy numeric library query values.
- `models.rs`: persisted discovery DTOs and request-filter structs owned by the transport shell.
- `series_queries.rs`: series payload exports.
- `series_queries/payload.rs`: series page response shaping.

## Keep outside this subtree

- Concrete SQLite query ownership, which belongs in non-HTTP access modules and `komga-infrastructure`.
- Cross-route discovery authentication logic, which stays in `http::discovery_auth`.
- General route registration, which stays in the parent discovery router wiring.
