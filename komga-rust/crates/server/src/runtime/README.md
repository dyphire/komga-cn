# server::runtime

This subtree owns the task-runtime lifecycle glue that remains in `komga-server`.
It turns `RuntimeConfig` into HTTP runtime parts plus a router-attached lifecycle guard without re-owning concrete queue worker behavior.

## Files in this subtree

- `background_workers.rs`: owns the `TaskRuntime` facade that turns `RuntimeConfig` into HTTP runtime parts plus a router-attached lifecycle guard.

## Keep outside this subtree

- Concrete queue worker logic, scheduling internals, and persisted task execution.
- HTTP runtime composition.
- Application task contracts and scan-planning rules.
