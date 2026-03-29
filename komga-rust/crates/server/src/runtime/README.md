# server::runtime

This subtree owns the thin runtime wrappers that remain in `komga-server`.
It exists so `server` can expose bootstrap and background-worker entry points without re-owning the concrete worker implementation.

## Files in this subtree

- `background_workers.rs`: forwards task-queue preparation and worker spawning into `komga_infrastructure::task_queue`, while keeping `RuntimeConfig` to `TaskRuntimeContext` wiring at the server boundary.
- `startup_scan.rs`: forwards startup library-scan bootstrap into the infrastructure task queue runtime.

## Keep outside this subtree

- Concrete queue worker logic, scheduling internals, and persisted task execution.
- HTTP runtime composition.
- Application task contracts and scan-planning rules.
