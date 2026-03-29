# komga-domain

`komga-domain` owns the pure domain concepts for the Rust workspace.
It defines business-facing types and write-side ports without taking dependencies on HTTP,
SQLite, filesystem, search, or runtime bootstrap code.

## Owned module groups

- `common_ids`: shared identifiers used across bounded contexts.
- `discovery`: discovery semantics such as filters, sorts, paging envelopes, errors, and write-side ports for saved-search style persistence.
- `identity_access`: access principals, roles, device sessions, library sharing rules, and write ports for identity-side state changes.
- `library_catalog`: library, series, book, collection, and read list entities, plus catalog events and write ports.
- `media_assets`: media asset and thumbnail domain models with write ports.
- `task_processing`: task payload, command, and state types with the write port for persisted task mutation.

## Boundaries

- Keep this crate focused on domain meaning and persistence-agnostic contracts.
- Application use cases, read-model query contracts, and orchestration helpers belong in `komga-application`.
- Concrete adapters for SQLite, filesystem, auth storage, search, and task workers belong in `komga-infrastructure`.
- HTTP request and response mapping belongs in `komga-interfaces`.
