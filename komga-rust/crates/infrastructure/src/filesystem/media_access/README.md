# filesystem::media_access

This subtree owns persisted media lookup plus filesystem and archive access used to serve or derive book media content.
It exists so the surrounding `filesystem` module can expose one focused surface for media file reads, page extraction, EPUB position decoding, page-hash generation, and read-progress side effects.

## Files in this subtree

- `mod.rs`: shell that re-exports the subtree surface.
- `db_queries.rs`: loads persisted `BookMediaRecord` and `BookPageRecord` data from SQLite.
- `page_content.rs`: reads page bytes from directories, zip or rar archives, PDFs, and single-image media.
- `read_progress.rs`: maintains series read-progress aggregates and Tachiyomi-oriented progression data.
- `hashes.rs`: computes and persists page hashes from resolved media bytes.
- `epub.rs`: reads EPUB resources and decodes or derives EPUB position data.

## Keep outside this subtree

- HTTP request and response mapping.
- Application-level media import or metadata orchestration.
- Unrelated filesystem concerns such as fonts or transient-books handling.
