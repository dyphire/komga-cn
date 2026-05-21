# server::composition

This subtree owns server-side composition for the Rust runtime.
It exists to install interfaces backends, translate `RuntimeConfig` into runtime state, and assemble the pieces needed before Axum routing starts serving requests.

## Files in this subtree

- `compose_http_runtime.rs`: HTTP runtime state assembly. Constructs concrete infrastructure access types and threads them into `HttpAppState` for the Axum router.
- `start_server.rs`: listener startup, graceful shutdown, shared-pool cleanup, and router assembly entry points.

## Keep outside this subtree

- Endpoint request handling and payload mapping.
- Concrete adapter implementations.
- Domain and application business rules.
