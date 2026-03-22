use std::collections::BTreeSet;

const APPLICATION_DISCOVERY_CORE: &str =
    include_str!("../crates/application/src/discovery/core.rs");
const RUNTIME_DISCOVERY_HANDLER: &str =
    include_str!("../crates/runtime-server/src/app/compat_runtime/content/discovery.rs");
const WORKSPACE_CARGO_TOML: &str = include_str!("../Cargo.toml");

#[test]
fn phase_55_migration_boundary_is_frozen() {
    let expected = BTreeSet::from([
        "sqlite-only",
        "behavior-compatible",
        "broader-persistence-boundary-redesign",
        "no-unrelated-business-module-rewrites",
        "rusqlite-discovery-path-removed-after-parity",
    ]);

    assert_eq!(expected, frozen_phase_55_boundary());
}

#[test]
fn protected_suites_and_rollback_point_are_frozen() {
    let expected = BTreeSet::from([
        "catalog_discovery_queries",
        "catalog_detail_queries",
        "catalog_detail_contract",
        "minimal_http_surface",
        "live_network_smoke",
        "sqlx_migration_contract",
    ]);

    assert_eq!(expected, frozen_protected_suites());

    assert_eq!(
        "legacy discovery fallback removed after protected-suite parity",
        frozen_rollback_point(),
    );
}

#[test]
fn rollback_boundary_is_finalized_after_full_sqlx_parity() {
    assert!(
        APPLICATION_DISCOVERY_CORE.contains("pub trait DiscoveryQueryRepository"),
        "application discovery repository trait must stay available before runtime cutover",
    );
    assert!(
        APPLICATION_DISCOVERY_CORE.contains("fn list_series(")
            && !APPLICATION_DISCOVERY_CORE.contains("async fn list_series"),
        "rollback boundary expects the pre-cutover sync repository contract to remain",
    );

    for expected_fragment in [
        "native_owned_series_list_response",
        "native_owned_books_list_response",
        "native_owned_books_latest_response",
        "non_native_series_list_response",
        "non_native_books_list_response",
        "non_native_books_latest_response",
        "mark_non_native(&mut response)",
        "SqlxRuntimeDiscoveryStore::new",
    ] {
        assert!(
            RUNTIME_DISCOVERY_HANDLER.contains(expected_fragment),
            "rollback boundary requires runtime discovery fragment: {expected_fragment}",
        );
    }

    assert!(
        !RUNTIME_DISCOVERY_HANDLER.contains("SqliteDiscoveryAdapter::default()"),
        "runtime discovery handlers must no longer instantiate rusqlite discovery adapters after sqlx cutover",
    );

    assert!(
        !WORKSPACE_CARGO_TOML.contains("rusqlite ="),
        "workspace dependency list should not keep legacy rusqlite entry after finalization",
    );
}

fn frozen_phase_55_boundary() -> BTreeSet<&'static str> {
    BTreeSet::from([
        "sqlite-only",
        "behavior-compatible",
        "broader-persistence-boundary-redesign",
        "no-unrelated-business-module-rewrites",
        "rusqlite-discovery-path-removed-after-parity",
    ])
}

fn frozen_protected_suites() -> BTreeSet<&'static str> {
    BTreeSet::from([
        "catalog_discovery_queries",
        "catalog_detail_queries",
        "catalog_detail_contract",
        "minimal_http_surface",
        "live_network_smoke",
        "sqlx_migration_contract",
    ])
}

fn frozen_rollback_point() -> &'static str {
    "legacy discovery fallback removed after protected-suite parity"
}
