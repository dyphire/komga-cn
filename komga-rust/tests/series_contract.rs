use komga_compat_testkit::contract_matrix::assert_required_target_declared;

#[test]
fn series_contract_target_is_registered() {
    assert_required_target_declared("series", "series_contract");
}
