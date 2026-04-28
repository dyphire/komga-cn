use super::*;

#[tokio::test]
async fn router_delete_syncpoints_me_without_key_id_deletes_all_syncpoints_for_current_user() {
    let paths = new_router_fixture("router-delete-syncpoints-me-all").await;
    seed_router_contract_data(&paths).await;
    seed_syncpoint_user(&paths, "other-user", "other@example.org").await;
    seed_syncpoints(
        &paths,
        &[
            ("sp-1", "admin-user", Some("key-1")),
            ("sp-2", "admin-user", Some("key-2")),
            ("sp-3", "admin-user", None),
            ("sp-4", "other-user", Some("key-1")),
        ],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/syncpoints/me")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("syncpoints delete-all request should build"),
        )
        .await
        .expect("syncpoints delete-all request should complete");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(load_syncpoint_ids(&paths).await, vec!["sp-4".to_string()]);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_delete_syncpoints_me_with_repeated_key_id_deletes_only_matching_keys() {
    let paths = new_router_fixture("router-delete-syncpoints-me-many-keys").await;
    seed_router_contract_data(&paths).await;
    seed_syncpoint_user(&paths, "other-user", "other@example.org").await;
    seed_syncpoints(
        &paths,
        &[
            ("sp-1", "admin-user", Some("key-1")),
            ("sp-2", "admin-user", Some("key-2")),
            ("sp-3", "admin-user", Some("key-3")),
            ("sp-4", "other-user", Some("key-1")),
        ],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/syncpoints/me?key_id=key-1&key_id=key-3")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("syncpoints delete-many request should build"),
        )
        .await
        .expect("syncpoints delete-many request should complete");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        load_syncpoint_ids(&paths).await,
        vec!["sp-2".to_string(), "sp-4".to_string()],
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_delete_syncpoints_me_with_comma_delimited_single_key_id_deletes_matching_keys() {
    let paths = new_router_fixture("router-delete-syncpoints-me-comma-key-id").await;
    seed_router_contract_data(&paths).await;
    seed_syncpoint_user(&paths, "other-user", "other@example.org").await;
    seed_syncpoints(
        &paths,
        &[
            ("sp-1", "admin-user", Some("key-1")),
            ("sp-2", "admin-user", Some("key-2")),
            ("sp-3", "admin-user", Some("key-3")),
            ("sp-4", "other-user", Some("key-1")),
        ],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/syncpoints/me?key_id=key-1,key-3")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("syncpoints delete comma-delimited request should build"),
        )
        .await
        .expect("syncpoints delete comma-delimited request should complete");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        load_syncpoint_ids(&paths).await,
        vec!["sp-2".to_string(), "sp-4".to_string()],
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_delete_syncpoints_me_with_whitespace_only_single_key_id_does_not_delete_anything() {
    let paths = new_router_fixture("router-delete-syncpoints-me-whitespace-key-id").await;
    seed_router_contract_data(&paths).await;
    seed_syncpoint_user(&paths, "other-user", "other@example.org").await;
    seed_syncpoints(
        &paths,
        &[
            ("sp-1", "admin-user", Some("key-1")),
            ("sp-2", "admin-user", Some("key-2")),
            ("sp-3", "admin-user", None),
            ("sp-4", "other-user", Some("key-1")),
        ],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/syncpoints/me?key_id=++")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("syncpoints delete whitespace key request should build"),
        )
        .await
        .expect("syncpoints delete whitespace key request should complete");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        load_syncpoint_ids(&paths).await,
        vec![
            "sp-1".to_string(),
            "sp-2".to_string(),
            "sp-3".to_string(),
            "sp-4".to_string()
        ],
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_delete_syncpoints_me_without_key_id_deletes_syncpoint_child_rows_for_current_user()
{
    let paths = new_router_fixture("router-delete-syncpoints-me-all-subentities").await;
    seed_router_contract_data(&paths).await;
    seed_syncpoint_user(&paths, "other-user", "other@example.org").await;
    seed_syncpoints(
        &paths,
        &[
            ("sp-1", "admin-user", Some("key-1")),
            ("sp-2", "other-user", Some("key-2")),
        ],
    )
    .await;
    seed_syncpoint_children(&paths, "sp-1").await;
    seed_syncpoint_children(&paths, "sp-2").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/syncpoints/me")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("syncpoints delete-all with subentities request should build"),
        )
        .await
        .expect("syncpoints delete-all with subentities request should complete");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(load_syncpoint_ids(&paths).await, vec!["sp-2".to_string()]);
    assert_eq!(
        load_syncpoint_child_counts(&paths, "sp-1").await,
        [0, 0, 0, 0, 0]
    );
    assert_eq!(
        load_syncpoint_child_counts(&paths, "sp-2").await,
        [1, 1, 1, 1, 1]
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_delete_syncpoints_me_with_key_id_deletes_syncpoint_child_rows_only_for_matching_keys()
 {
    let paths = new_router_fixture("router-delete-syncpoints-me-key-subentities").await;
    seed_router_contract_data(&paths).await;
    seed_syncpoint_user(&paths, "other-user", "other@example.org").await;
    seed_syncpoints(
        &paths,
        &[
            ("sp-1", "admin-user", Some("key-1")),
            ("sp-2", "admin-user", Some("key-2")),
            ("sp-3", "other-user", Some("key-1")),
        ],
    )
    .await;
    seed_syncpoint_children(&paths, "sp-1").await;
    seed_syncpoint_children(&paths, "sp-2").await;
    seed_syncpoint_children(&paths, "sp-3").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/syncpoints/me?key_id=key-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("syncpoints delete key-scoped with subentities request should build"),
        )
        .await
        .expect("syncpoints delete key-scoped with subentities request should complete");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        load_syncpoint_ids(&paths).await,
        vec!["sp-2".to_string(), "sp-3".to_string()],
    );
    assert_eq!(
        load_syncpoint_child_counts(&paths, "sp-1").await,
        [0, 0, 0, 0, 0]
    );
    assert_eq!(
        load_syncpoint_child_counts(&paths, "sp-2").await,
        [1, 1, 1, 1, 1]
    );
    assert_eq!(
        load_syncpoint_child_counts(&paths, "sp-3").await,
        [1, 1, 1, 1, 1]
    );

    cleanup_router_fixture(paths);
}
