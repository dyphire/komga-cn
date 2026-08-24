# Persisted discovery

This subtree owns the persisted discovery queries served directly from the SQLite-backed runtime database.
It exists so discovery-specific read access, support models, and shared query helpers stay together behind one infrastructure module.

## Files in this subtree

- `models.rs`: persisted discovery DTOs such as author, book, poster, and series summaries.
- `authors.rs`: author list and scoped author query access.
- `books.rs`: persisted book summary, existence, and poster-summary queries.
- `facets.rs`: genre, tag, language, publisher, age-rating, sharing-label, and release-date facet queries.
- `library_mappings.rs`: collection, readlist, and library membership mapping queries.
- `runtime_queries.rs`: persisted runtime support queries such as on-deck books, duplicate books, tag lookup, series counts, and date helpers.
- `series.rs`: persisted series summary and existence queries.
- `common.rs`: shared SQL helpers used across the subtree.

## Keep outside this subtree

- HTTP route parsing and payload shaping.
- Discovery semantics and validation rules, which belong in `komga-domain` and `komga-application`.
- Detailed series or book transport read-model mapping, which belongs in interface or detail modules.
