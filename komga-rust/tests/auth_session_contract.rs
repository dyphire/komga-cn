use komga_compat_testkit::contract_matrix::assert_required_target_declared;

#[test]
fn auth_session_contract_target_is_registered() {
    assert_required_target_declared("auth/session", "auth_session_contract");
}
