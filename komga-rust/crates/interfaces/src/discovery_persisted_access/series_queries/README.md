# discovery_persisted_access::series_queries

This subtree owns the crate-local series-side persisted discovery helpers used by the interfaces crate.
It exists to keep series filtering, grouping, payload generation, and runtime-owned response selection separate from the broader persisted-discovery transport shell.

## Files in this subtree

- `filtering.rs`: applies persisted series filters and paging over persisted summary rows.
- `groups.rs`: builds alphabetical group counts from filtered series results.
- `payload.rs`: converts persisted series pages into the HTTP JSON payload shape.
- `runtime.rs`: decides when the runtime-owned persisted series path applies and builds the final response.

## Keep outside this subtree

- Generic persisted discovery backend installation and shared helpers.
- Book-side persisted discovery handling.
- Server-side installation of concrete infrastructure backends.
