# komga-infrastructure

`komga-infrastructure` owns the concrete adapters behind the Rust runtime. Its public API is
grouped by the capability modules exposed from `src/lib.rs`; implementation belongs to the same
capability that owns the behavior and its persisted data.

## Module ownership

- `persistence`: SQLite pool topology, schema bootstrap, transactions, codecs, and stored-path
  primitives. It must not own entity-specific queries.
- `file_io`: file lifecycle primitives shared across capabilities.
- `identity`: users, authentication, sessions, claims, device authentication, and Kobo sync.
- `discovery`: books, series, collections, readlists, libraries, visibility, and deletion.
- `media`: content delivery, format primitives, analysis, metadata, progress, import,
  maintenance, transient books, and library scanning.
- `opds`: OPDS catalog adapters, persisted feed queries, and record mapping.
- `operational`: settings, announcements, metrics, history, filesystem browsing, fonts, page
  hashes, remote feeds, and sync-point administration.
- `search`: analyzers, documents, index lifecycle, the search engine, and synchronization.
- `tasks`: queue persistence, scheduling, worker runtime, dispatch, and thin job orchestration.
- `shared`: small crate-wide primitives with no capability ownership, currently random token and
  identifier generation.

## Dependency rules

- Capability modules may depend on `persistence` and `shared` primitives.
- `tasks` may invoke capability entry points, but queue and dispatch modules must not own media,
  discovery, identity, or search business SQL.
- Capability-specific SQLite rows and queries stay private to their owning module.
- Cross-capability behavior uses typed entry points or existing application ports, not shared raw
  rows or generic access/helper modules.
- HTTP parsing, response shaping, and route ownership remain outside this crate.
- Pure domain rules and application use-case contracts remain in `komga-domain` and
  `komga-application`.

Consumers should import through the owning module, for example `persistence::DatabaseHandle`,
`media::ContentResolver`, or `tasks::TaskRuntimeContext`. When adding behavior, extend the deepest
existing owner before creating another adapter or top-level module. Keep `src/lib.rs` limited to
module declarations and crate-private shared primitives.
