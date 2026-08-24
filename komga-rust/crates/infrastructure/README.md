# Infrastructure crates

The infrastructure layer is split into focused crates so consumers compile only the adapters they
use. Each crate owns the persisted data and application ports for one capability; consumers import
that capability directly instead of through a facade crate.

## Crates

- `base`: SQLite topology, schema bootstrap, transactions, file I/O, and stored-path primitives.
- `discovery`: books, series, collections, readlists, libraries, visibility, and discovery queries.
- `identity`: users, authentication, sessions, device authentication, and Kobo sync.
- `jobs`: concrete task dispatch, job implementations, runtime context, and worker orchestration.
- `media-access`: import, reading progress, transient media, and media access events.
- `media-core`: content resolution and archive/format primitives.
- `media-library`: analysis, library scans, and library maintenance.
- `media-metadata`: metadata refresh, artwork, thumbnails, and read-progress persistence.
- `operational`: settings, announcements, metrics, history, filesystem browsing, fonts, and
  synchronization administration.
- `opds`: OPDS catalog adapters, persisted feed queries, and record mapping.
- `search`: analyzers, documents, index lifecycle, the search engine, and synchronization.
- `tasks`: generic queue persistence, scheduling, execution pools, and execution loops.
- `test-support`: shared fixtures for infrastructure crate tests.

`tasks` contains no concrete domain job implementations. `jobs` depends on `tasks` and the
capability crates to connect queue records to application task requests. `base` remains below the
capability crates and must not depend on application or domain code.
