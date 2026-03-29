# server::composition

This subtree owns server-side composition for the Rust runtime.
It exists to install interfaces backends, translate `RuntimeConfig` into runtime state, and assemble the pieces needed before Axum routing starts serving requests.

## Files in this subtree

- `http_state.rs`: main composition entry point for HTTP runtime state and backend installation.
- `http_state_discovery.rs`: discovery backend composition.
- `http_state_media_assets.rs`: media-assets backend composition.
- `http_state_opds.rs`: OPDS backend composition.
- `http_state_operational_access.rs`: operational metrics and settings backend composition.
- `http_state_operational_state.rs`: operational state assembly.
- `http_state_runtime_config.rs`: `RuntimeConfig` to interface runtime-profile translation.
- `http_state_runtime_identity.rs`: runtime identity backend composition.
- `start_server.rs`: listener startup, graceful shutdown, shared-pool cleanup, and router assembly entry points.

## Keep outside this subtree

- Endpoint request handling and payload mapping.
- Concrete adapter implementations.
- Domain and application business rules.
