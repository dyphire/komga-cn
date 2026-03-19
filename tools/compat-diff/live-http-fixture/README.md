# live-http-fixture

Deterministic filesystem + localdb fixture for the first live data-route diff step.

## What it creates

The prep script builds this tree:

```text
<output>/
  library1/
    series/
      book.cbr
```

`book.cbr` is copied from the checked-in sample archive at `komga/src/test/resources/archives/zip.zip`.
The file bytes stay untouched; the script only creates the directory layout and normalizes mtimes/permissions.

## Localdb seeding

`seed-live-http-fixture.py` seeds a migrated Komga SQLite localdb directly.
It replaces the fixed `1` / `series-1` / `book-1` fixture rows deterministically, clears the related
companion tables for those IDs, and leaves any unrelated data alone.
Pass `--fixture-root` to seed the binary-capable ready fixture with absolute `file:` URLs from a prepared
`library1/series/book.cbr` tree.

## Usage

Prepare a fresh fixture:

```bash
python tools/compat-diff/live-http-fixture/prepare-live-http-fixture.py prepare /tmp/live-http-fixture
```

Seed a migrated localdb after booting Komga with `dev,localdb,noclaim`:

```bash
python tools/compat-diff/live-http-fixture/seed-live-http-fixture.py \
  --database ./localdb/localdb.sqlite \
  --fixture-root /tmp/live-http-fixture
```

Validate an existing fixture quickly:

```bash
python tools/compat-diff/live-http-fixture/prepare-live-http-fixture.py verify /tmp/live-http-fixture
```

## Intended use with dev/localdb bootRun

1. Start Komga with `SPRING_PROFILES_ACTIVE=dev,localdb,noclaim` (or `./gradlew bootRun` with the same profiles).
2. Let Komga create and migrate `./localdb/localdb.sqlite` and `./localdb/localdb-tasks.sqlite`.
3. Prepare the filesystem fixture at `<output>/library1` for any later scan/diff work.
4. Run the seed script against `./localdb/localdb.sqlite` with `--fixture-root <output>`.

This seed path now covers the ready book/media/page rows used by the live binary routes.
