# server::runtime

This subtree owns the task-runtime lifecycle glue that remains in `komga-server`.
It starts task runtime resources from `RuntimeConfig` and returns HTTP runtime parts plus a router-attached lifecycle guard without re-owning concrete queue worker behavior.

## Files in this subtree

- `background_workers.rs`: prepares task queues, optional workers, HTTP runtime parts, and the router lifecycle guard.

## Keep outside this subtree

- Concrete queue worker logic, scheduling internals, and persisted task execution.
- HTTP runtime composition.
- Application task contracts and scan-planning rules.
