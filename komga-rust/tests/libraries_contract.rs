use komga_compat_testkit::contract_matrix::assert_required_target_declared;

#[test]
fn libraries_contract_target_is_registered() {
    assert_required_target_declared("libraries", "libraries_contract");
}
