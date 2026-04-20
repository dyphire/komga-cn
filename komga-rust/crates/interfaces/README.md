# komga-interfaces

`komga-interfaces` owns transport-facing surfaces for the Rust runtime.
Its main job is to map HTTP, OPDS, Kobo, and KOReader requests onto application and runtime access contracts, then map results back into transport payloads.

## Owned module groups

- Transport modules: `discovery`, `discovery_auth`, `identity_access`, `library_catalog`, `media_assets`, `opds`, and `operational` own request parsing, auth extraction, route wiring, and response mapping.
- `state` owns transport-facing runtime contracts and DTOs that `komga-server` installs into handlers.
- Shared transport support lives in `access_log`, `cache`, `helpers`, `request_urls`, `router`, and crate-level header constants.

## Boundaries

- This crate maps transport <-> runtime contracts. It does not own SQLite, filesystem, or search implementations.
- Use-case orchestration belongs in `komga-application`.
- Concrete adapters belong in `komga-infrastructure`.
- Runtime composition belongs in `komga-server`.
