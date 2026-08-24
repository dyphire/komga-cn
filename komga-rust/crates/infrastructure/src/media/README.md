# Media infrastructure

This subtree owns persisted media lookup plus filesystem and archive access used to serve,
analyze, import, scan, describe, or maintain book media content.

## Files in this subtree

- `content/`: persisted records, page rendering, EPUB resources, and content resolution.
- `formats/`: Pdfium loading plus RAR and ZIP format primitives.
- `analysis/`: media detection, analysis, and analysis persistence.
- `metadata/`: metadata refresh, aggregation, artwork, and thumbnail persistence.
- `library_scan/`: filesystem discovery, diffing, persistence, events, and follow-up planning.
- `import/`: import destination and released-file handling.
- `maintenance/`: media-derived maintenance operations such as page hashing.
- `progress/`: read-progress persistence and aggregation.
- `transient/`: analysis and content access for books not yet imported.
- `reader.rs`: application media read-port adapter.

## Keep outside this subtree

- HTTP request and response mapping.
- Unrelated filesystem concerns such as fonts.
- Task queue persistence, scheduling, dispatch, and worker lifecycle.
