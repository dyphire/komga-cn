use komga_compat_testkit::contract_matrix::assert_required_target_declared;

#[test]
fn books_media_contract_target_is_registered() {
    assert_required_target_declared("books/media", "books_media_contract");
}
