# komga-benchmark-wpd3

`komga-benchmark-wpd3` exposes the shared WPD3 benchmark module used by the Rust workspace.
Its crate surface is intentionally small: it re-exports `src/wpd3.rs` from the workspace root through `pub mod wpd3`.

## Boundaries

- Keep benchmark-specific code and helpers in the shared `wpd3` module path that this crate forwards.
- Do not turn this crate into a general runtime or application dependency surface.
