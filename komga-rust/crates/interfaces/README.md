# komga-interfaces

`komga-interfaces` owns transport-facing surfaces for the Rust runtime.
Its main job is to map HTTP, OPDS, Kobo, and KOReader requests onto application and runtime access contracts, then map results back into transport payloads.

## Owned module groups

- `http`: the route tree, request parsing, auth extraction, and response mapping for discovery, identity access, library catalog, media assets, OPDS, and operational endpoints.
- Crate-local runtime access bridges such as `runtime_identity_access`, `media_assets_runtime_access`, `opds_*_access`, and `operational_*_access`: backend contracts and DTOs installed by `komga-server` so transport code can stay focused on mapping.
- Crate constants re-exported into transport code, such as cache-control and ownership marker headers.

## Boundaries

- This crate is transport-focused. It should not become the home for SQLite ownership, filesystem ownership, or search implementation.
- Business rules and use-case orchestration belong in `komga-application`.
- Concrete adapters belong in `komga-infrastructure`.
- Bootstrap, backend installation, and runtime composition belong in `komga-server`.
