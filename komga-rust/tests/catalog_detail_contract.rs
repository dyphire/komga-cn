use komga_compat_testkit::cases::HarnessConfig;
use std::collections::BTreeSet;

#[path = "catalog_detail_contract/direct_browse.rs"]
mod direct_browse;
#[path = "catalog_detail_contract/excluded_non_native.rs"]
mod excluded_non_native;
#[path = "catalog_detail_contract/helpers.rs"]
mod helpers;
#[path = "catalog_detail_contract/oneshot.rs"]
mod oneshot;

use helpers::{
    frozen_in_scope_direct_browse_shapes, frozen_named_exclusion_proofs,
    frozen_non_native_detail_shapes, frozen_oneshot_direct_route_shapes,
    frozen_oneshot_named_exclusion_proofs,
};

#[test]
fn phase_55_rollback_boundary_keeps_detail_owned_and_fallback_cases_available() {
    let config = HarnessConfig::load_default().expect("default compat cases should load");

    for id in [
        "P3-DETAIL-SERIES-DETAIL-OWNED",
        "P3-DETAIL-BOOK-DETAIL-OWNED",
        "P3-DETAIL-BOOK-READLISTS-OWNED",
        "P3-DETAIL-EXCLUDED-BOOK-PAGES",
        "P3-DETAIL-EXCLUDED-BOOK-FILE",
        "P3-DETAIL-EXCLUDED-READ-PROGRESS-PATCH",
        "P3-DETAIL-EXCLUDED-READLIST-CONTEXT-BOOKS",
    ] {
        assert!(
            config.cases.iter().any(|it| it.id == id),
            "rollback boundary requires compat case to remain available: {id}",
        );
    }
}

#[test]
fn phase7_series_oneshot_exact_route_shape_is_frozen() {
    oneshot::phase7_series_oneshot_exact_route_shape_is_frozen();
}

#[test]
fn phase7_adjacent_oneshot_query_variants_remain_explicitly_non_native() {
    oneshot::phase7_adjacent_oneshot_query_variants_remain_explicitly_non_native();
}
