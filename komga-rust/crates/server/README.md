# komga-server

`komga-server` owns bootstrap, composition, and runtime startup for the Rust workspace.
It wires configuration, installs interface backends, prepares background workers, and starts the Axum server.

## Owned module groups

- `app`: public entry points used to build routers, serve listeners, configure remember-me storage, and expose runtime-task context from `RuntimeConfig`.
- `bootstrap`: startup flows for admin and noclaim initialization.
- `composition`: HTTP/runtime backend installation and server assembly.
- `config`: runtime configuration loading, path resolution, CLI parsing, and writer/runtime profile decisions.
- `runtime`: task queue preparation, startup scan handling, background worker startup, and router lifecycle guards.

## Boundaries

- This crate should stay focused on wiring and lifecycle concerns.
- Business logic, query semantics, and domain models belong in `komga-application` and `komga-domain`.
- Concrete persistence and worker implementations belong in `komga-infrastructure`.
- HTTP transport mapping belongs in `komga-interfaces`.
