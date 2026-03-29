# komga-kepubify

`komga-kepubify` is a focused EPUB-to-KEPUB conversion utility crate.
It reads EPUB archives, inspects the package document, injects Kobo span markers into eligible spine resources, and writes a converted archive back to bytes.

## Owned surface

- `convert_epub_file_to_bytes`: convert an EPUB file from disk.
- `convert_epub_bytes`: convert an EPUB archive already loaded in memory.
- Internal helpers in `src/lib.rs` handle container parsing, spine/resource discovery, HTML marker injection, and zip rewrite logic.

## Boundaries

- Keep this crate focused on EPUB and KEPUB conversion mechanics.
- Database access, runtime HTTP concerns, and broader media orchestration belong in other crates.
