# legacy-db-smoke

Tracked assets for the WP-E1 legacy SQLite smoke path.

## Fixture requirements

- The fixture directory must contain both SQLite files:
  - `database.sqlite`
  - `tasks.sqlite`
- If the source database has WAL sidecars, copy the full set together: `*.sqlite`, `*.sqlite-wal`, and `*.sqlite-shm`.
- Do not vacuum, migrate, or rewrite the fixture in place.
- Prefer a read-only mount for the fixture root. If a runner needs writes, copy both DBs into a scratch directory first.

## Java startup / mount

- Mount the legacy fixture root at the Java config directory.
- Set `LEGACY_DB_SMOKE_JAVA_CONFIG_DIR` to that mount path.
- Start Komga with the local DB profile set, for example:
  - `SPRING_PROFILES_ACTIVE=localdb,noclaim`
  - `KOMGA_CONFIG_DIR=$LEGACY_DB_SMOKE_JAVA_CONFIG_DIR`
- The mounted directory must expose both `database.sqlite` and `tasks.sqlite`.

## Rust startup / mount

- Mount the same legacy fixture root for the Rust runner.
- Set `LEGACY_DB_SMOKE_RUST_CONFIG_DIR` to that mount path.
- Use the equivalent Rust process/container config so it reads the same fixture root and can reach the same pair of DB files.
- The mounted directory must expose both `database.sqlite` and `tasks.sqlite`.

## WAL / writer / RO-RW notes

- WAL fixtures are expected.
- Keep the fixture read-only for smoke reads.
- Treat the DBs as single-writer artifacts even when the runtime opens separate read-only/read-write pools.
- Do not run concurrent writers against the same fixture copy.
- If a write step is unavoidable, isolate it to a scratch copy and keep the RO copy untouched.

## Smoke case inputs

- Base URLs:
  - `LEGACY_DB_SMOKE_JAVA_BASE_URL`
  - `LEGACY_DB_SMOKE_RUST_BASE_URL`
- Credentials:
  - `LEGACY_DB_SMOKE_USERNAME`
  - `LEGACY_DB_SMOKE_PASSWORD`
  - `LEGACY_DB_SMOKE_SESSION_TOKEN`

## Result archive

- Write smoke output and archived diffs under:
  - `target/compat-diff/legacy-db-smoke/results/`
