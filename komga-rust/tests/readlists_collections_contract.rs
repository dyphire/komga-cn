use komga_compat_testkit::contract_matrix::assert_required_target_declared;

#[test]
fn readlists_collections_contract_target_is_registered() {
    assert_required_target_declared("readlists/collections", "readlists_collections_contract");
}
