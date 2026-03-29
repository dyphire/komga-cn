# komga-infrastructure

`komga-infrastructure` owns the concrete adapters behind the Rust runtime.
It is where SQLite, filesystem, auth persistence, search, OPDS backing queries, and task worker implementation details live.

## Owned module groups

- Persistence and storage: `sqlite`, `sql`, `read_models`, `library_catalog`, `announcements_access`, `claims_access`, `operational_settings_access`, `operational_metrics_access`, and `page_hashes_access`.
- Auth and identity backing services: `auth` and `runtime_identity_access`.
- Discovery backing services: `discovery_persisted_access` and `discovery_detail_access`.
- Media and file access: `filesystem` and `metadata`.
- Search and background execution: `search`, `tasks`, and `task_queue`.
- Crate-level shared persistence context: `SqlitePersistenceConnection`, `SqlitePersistenceContext`, `SqliteUnitOfWork`, search lifecycle exports, and `ServerSettingsStore`.

## Boundaries

- Keep concrete adapter code here, even when it is only used by one runtime path.
- Do not move HTTP request parsing, response shaping, or route ownership into this crate.
- Do not move application-level orchestration or pure domain modeling into this crate.
