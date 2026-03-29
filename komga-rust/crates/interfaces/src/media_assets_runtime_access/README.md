# media_assets_runtime_access

This subtree owns the interfaces crate's media-assets runtime bridge.
It exists so HTTP media handlers can call one crate-local backend contract for imports, metadata updates, page access, read progress, EPUB helpers, and thumbnail access without owning the concrete adapter wiring themselves.

## Files in this subtree

- `mod.rs`: shell that re-exports the backend contract and internal facade.
- `backend.rs`: installable backend contract and DTO/service types used by callers and server composition.
- `facade.rs`: small crate-local forwarding functions used by transport code.
- `test_backend.rs`: test-only fallback backend composition.

## Keep outside this subtree

- HTTP route parsing and response shaping.
- Concrete filesystem and SQLite adapter logic, which belongs in `komga-infrastructure`.
- Server composition and backend installation decisions, which belong in `komga-server`.
