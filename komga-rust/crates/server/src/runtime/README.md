# server::runtime

This subtree owns the thin runtime wrappers that remain in `komga-server`.
It exists so `server` can expose bootstrap and task-runtime lifecycle entry points without re-owning the concrete worker implementation.

## Files in this subtree

- `background_workers.rs`: owns the `StartedTaskRuntime` facade that turns `RuntimeConfig` into HTTP runtime parts plus a router-attached lifecycle guard.
- `startup_scan.rs`: forwards startup library-scan bootstrap into the infrastructure task queue runtime.

## Keep outside this subtree

- Concrete queue worker logic, scheduling internals, and persisted task execution.
- HTTP runtime composition.
- Application task contracts and scan-planning rules.
