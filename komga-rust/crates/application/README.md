# komga-application

`komga-application` owns use cases, query contracts, and orchestration contracts for the Rust runtime.
It sits between pure domain concepts and concrete adapters.

## Owned module groups

- `discovery`: query services, request-shape validation, runtime-vs-persisted query decisions, index maintenance contracts, and discovery read models.
- `identity_access`: auth user/session models, principal resolution, device token helpers, remember-me and session token contracts, and Kobo sync logic.
- `library_catalog`: create, update, delete, and query services for libraries, plus task request and mutation port contracts.
- `media_assets`: media import, metadata updates, page retrieval contracts, read-progress use cases, and thumbnail-facing read models.
- `platform_runtime`: runtime lifecycle policies such as startup search recovery contracts.
- `task_processing`: queue orchestration contracts, runtime task configuration/context, and scan scheduling helpers.

## Boundaries

- Keep this crate on application rules and contracts, not transport concerns or concrete storage.
- Domain entities and write-side ports stay in `komga-domain`.
- SQLite, filesystem, auth persistence, search, and worker implementations stay in `komga-infrastructure`.
- Route handlers, request parsing, and response payload mapping stay in `komga-interfaces`.
