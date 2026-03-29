# komga-contract-testkit

`komga-contract-testkit` provides the shared harness used by workspace contract tests.
It normalizes HTTP responses, loads case definitions, runs setup requests, compares SSE logs, and writes human-readable diffs.

## Owned module groups

- `cases`: TOML-backed harness and case configuration loading.
- `contract_matrix`: required surface families and the mapping from review TODO markers to contract targets.
- `normalize`: shared response normalization helpers.
- `runtime`: request setup helpers, header templating, and setup-step execution.
- `diff_writer`: diff report generation and output formatting.
- `sse`: SSE log parsing, normalization, comparison, and report writing.

## Boundaries

- Keep reusable contract-harness utilities here.
- Do not move application or runtime behavior into this crate.
- Scenario-specific fixture setup and assertions should stay in the actual test targets that consume the harness.
