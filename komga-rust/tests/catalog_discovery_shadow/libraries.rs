use super::*;

#[tokio::test]
async fn libraries_admin_user_limited_parity() {
    let app = komga_rust::app::build_router();

    let admin_token = session_token_for_basic_auth(&app, "YWRtaW5AZXhhbXBsZS5vcmc6YWRtaW4=").await;
    let user_token = session_token_for_basic_auth(&app, "dXNlckBleGFtcGxlLm9yZzp1c2Vy").await;
    let limited_token =
        session_token_for_basic_auth(&app, "bGltaXRlZEBleGFtcGxlLm9yZzpsaW1pdGVk").await;

    let admin_json = libraries_json_for_token(&app, &admin_token).await;
    let user_json = libraries_json_for_token(&app, &user_token).await;
    let limited_json = libraries_json_for_token(&app, &limited_token).await;

    let admin_ids = ids(&admin_json);
    let user_ids = ids(&user_json);
    let limited_ids = ids(&limited_json);

    assert_eq!(admin_ids, vec!["1"]);
    assert_eq!(user_ids, vec!["1"]);
    assert_eq!(limited_ids, vec!["1"]);
}

#[tokio::test]
async fn libraries_non_admin_root_is_sanitized() {
    let app = komga_rust::app::build_router();

    let admin_token = session_token_for_basic_auth(&app, "YWRtaW5AZXhhbXBsZS5vcmc6YWRtaW4=").await;
    let user_token = session_token_for_basic_auth(&app, "dXNlckBleGFtcGxlLm9yZzp1c2Vy").await;
    let limited_token =
        session_token_for_basic_auth(&app, "bGltaXRlZEBleGFtcGxlLm9yZzpsaW1pdGVk").await;

    let admin_json = libraries_json_for_token(&app, &admin_token).await;
    let user_json = libraries_json_for_token(&app, &user_token).await;
    let limited_json = libraries_json_for_token(&app, &limited_token).await;

    assert_eq!(admin_json[0]["root"], "/library1");

    for library in user_json
        .as_array()
        .expect("user libraries must be an array")
    {
        assert_eq!(library["root"], "");
    }
    for library in limited_json
        .as_array()
        .expect("limited libraries must be an array")
    {
        assert_eq!(library["root"], "");
    }
}
