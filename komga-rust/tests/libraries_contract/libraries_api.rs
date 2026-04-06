use super::*;

async fn count_query_rows(paths: &RuntimeDbPaths, sql: &str, bind: &str) -> i64 {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("count query db should open");
    let count = sqlx::query(sql)
        .bind(bind)
        .fetch_one(&pool)
        .await
        .expect("count query should succeed")
        .get::<i64, _>("COUNT");
    pool.close().await;
    count
}

#[tokio::test]
async fn router_api_libraries_route_matches_kotlin_etag_without_extra_cache_headers() {
    let paths = new_router_fixture("router-api-libraries-kotlin-cache-headers").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let first_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/libraries")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("libraries cache request should build"),
        )
        .await
        .expect("libraries cache request should complete");

    assert_eq!(first_response.status(), StatusCode::OK);
    assert!(
        first_response
            .headers()
            .get(header::CACHE_CONTROL)
            .is_none(),
        "Kotlin libraries list does not emit Cache-Control on 200"
    );

    let etag = first_response
        .headers()
        .get(header::ETAG)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .expect("libraries response should include etag");

    let second_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/libraries")
                .header("x-auth-token", &auth_token)
                .header(header::IF_NONE_MATCH, etag)
                .body(Body::empty())
                .expect("conditional libraries request should build"),
        )
        .await
        .expect("conditional libraries request should complete");

    assert_eq!(second_response.status(), StatusCode::NOT_MODIFIED);
    assert!(
        second_response
            .headers()
            .get(header::CACHE_CONTROL)
            .is_none(),
        "Kotlin conditional libraries list does not emit Cache-Control on 304"
    );
    assert!(second_response.headers().contains_key(header::ETAG));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_api_library_patch_accepts_null_scan_directory_exclusions_as_clear() {
    let paths = new_router_fixture("router-api-library-patch-null-exclusions").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract db should open for library exclusions seed");
    sqlx::query("INSERT INTO LIBRARY_EXCLUSIONS (LIBRARY_ID, EXCLUSION) VALUES (?, ?), (?, ?)")
        .bind("library-1")
        .bind("folder-a")
        .bind("library-1")
        .bind("folder-b")
        .execute(&pool)
        .await
        .expect("library exclusions should be seeded");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let patch_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/libraries/library-1")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "scanDirectoryExclusions": null }).to_string(),
                ))
                .expect("library patch request should build"),
        )
        .await
        .expect("library patch request should complete");
    assert_eq!(patch_response.status(), StatusCode::NO_CONTENT);

    let get_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/libraries/library-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("library detail request should build"),
        )
        .await
        .expect("library detail request should complete");
    assert_eq!(get_response.status(), StatusCode::OK);
    let payload = response_json(get_response).await;
    assert_eq!(
        payload.get("scanDirectoryExclusions"),
        Some(&json!([])),
        "PATCH null scanDirectoryExclusions should clear exclusions like Kotlin"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_api_library_delete_requires_authentication() {
    let paths = new_router_fixture("router-api-library-delete-requires-auth").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/libraries/library-1")
                .body(Body::empty())
                .expect("library delete unauthenticated request should build"),
        )
        .await
        .expect("library delete unauthenticated request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_api_library_put_route_is_removed() {
    let paths = new_router_fixture("router-api-library-put-route-removed").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/libraries/library-1")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "name": "Library 1" }).to_string()))
                .expect("removed library put request should build"),
        )
        .await
        .expect("removed library put request should complete");

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_api_library_delete_forbids_non_admin_user() {
    let paths = new_router_fixture("router-api-library-delete-forbidden-non-admin").await;
    seed_router_contract_data(&paths).await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "non-admin-user",
        "non-admin@example.org",
        "router-contract-non-admin-123",
        18,
        &["USER"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "non-admin@example.org",
        "router-contract-non-admin-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/libraries/library-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("library delete forbidden request should build"),
        )
        .await
        .expect("library delete forbidden request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_api_library_delete_returns_not_found_for_missing_library() {
    let paths = new_router_fixture("router-api-library-delete-missing").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/libraries/missing-library")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("library delete missing request should build"),
        )
        .await
        .expect("library delete missing request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_api_library_delete_cascades_library_rows_like_kotlin() {
    let paths = new_router_fixture("router-api-library-delete-cascade").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("library delete cascade seed db should open");
    sqlx::query("INSERT INTO LIBRARY_EXCLUSIONS (LIBRARY_ID, EXCLUSION) VALUES (?, ?)")
        .bind("library-1")
        .bind("excluded-dir")
        .execute(&pool)
        .await
        .expect("library exclusion should be seeded");
    sqlx::query(
        "INSERT INTO SIDECAR (URL, PARENT_URL, LAST_MODIFIED_TIME, LIBRARY_ID) VALUES (?, ?, ?, ?)",
    )
    .bind("books/book-1.xml")
    .bind("books/book-1.epub")
    .bind(1_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("library sidecar should be seeded");
    sqlx::query("INSERT INTO USER_LIBRARY_SHARING (USER_ID, LIBRARY_ID) VALUES (?, ?)")
        .bind("admin-user")
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("library sharing should be seeded");
    sqlx::query("INSERT INTO BOOK_METADATA_AGGREGATION_TAG (TAG, SERIES_ID) VALUES (?, ?)")
        .bind("agg-tag")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("aggregation tag should be seeded");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/libraries/library-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("library delete cascade request should build"),
        )
        .await
        .expect("library delete cascade request should complete");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        count_query_rows(
            &paths,
            "SELECT COUNT(*) AS COUNT FROM LIBRARY WHERE ID = ?",
            "library-1",
        )
        .await,
        0
    );
    assert_eq!(
        count_query_rows(
            &paths,
            "SELECT COUNT(*) AS COUNT FROM SERIES WHERE LIBRARY_ID = ?",
            "library-1",
        )
        .await,
        0
    );
    assert_eq!(
        count_query_rows(
            &paths,
            "SELECT COUNT(*) AS COUNT FROM BOOK WHERE LIBRARY_ID = ?",
            "library-1",
        )
        .await,
        0
    );
    assert_eq!(
        count_query_rows(
            &paths,
            "SELECT COUNT(*) AS COUNT FROM LIBRARY_EXCLUSIONS WHERE LIBRARY_ID = ?",
            "library-1",
        )
        .await,
        0
    );
    assert_eq!(
        count_query_rows(
            &paths,
            "SELECT COUNT(*) AS COUNT FROM SIDECAR WHERE LIBRARY_ID = ?",
            "library-1",
        )
        .await,
        0
    );
    assert_eq!(
        count_query_rows(
            &paths,
            "SELECT COUNT(*) AS COUNT FROM USER_LIBRARY_SHARING WHERE LIBRARY_ID = ?",
            "library-1",
        )
        .await,
        0
    );
    assert_eq!(
        count_query_rows(
            &paths,
            "SELECT COUNT(*) AS COUNT FROM BOOK_METADATA_AGGREGATION WHERE SERIES_ID = ?",
            "series-1",
        )
        .await,
        0
    );
    assert_eq!(
        count_query_rows(
            &paths,
            "SELECT COUNT(*) AS COUNT FROM BOOK_METADATA_AGGREGATION_AUTHOR WHERE SERIES_ID = ?",
            "series-1",
        )
        .await,
        0
    );
    assert_eq!(
        count_query_rows(
            &paths,
            "SELECT COUNT(*) AS COUNT FROM BOOK_METADATA_AGGREGATION_TAG WHERE SERIES_ID = ?",
            "series-1",
        )
        .await,
        0
    );
    assert_eq!(
        count_query_rows(
            &paths,
            "SELECT COUNT(*) AS COUNT FROM THUMBNAIL_BOOK WHERE BOOK_ID = ?",
            "book-1",
        )
        .await,
        0
    );
    assert_eq!(
        count_query_rows(
            &paths,
            "SELECT COUNT(*) AS COUNT FROM READLIST_BOOK WHERE BOOK_ID = ?",
            "book-1",
        )
        .await,
        0
    );
    assert_eq!(
        count_query_rows(
            &paths,
            "SELECT COUNT(*) AS COUNT FROM COLLECTION_SERIES WHERE SERIES_ID = ?",
            "series-1",
        )
        .await,
        0
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_api_library_patch_rejects_blank_name() {
    let paths = new_router_fixture("router-api-library-patch-blank-name").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let patch_response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/libraries/library-1")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "name": "   " }).to_string()))
                .expect("library patch blank-name request should build"),
        )
        .await
        .expect("library patch blank-name request should complete");

    assert_eq!(patch_response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(patch_response).await;
    assert_eq!(
        payload,
        json!({
            "violations": [
                {
                    "fieldName": "name",
                    "message": "must not be blank"
                }
            ]
        })
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_api_library_patch_rejects_blank_root() {
    let paths = new_router_fixture("router-api-library-patch-blank-root").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let patch_response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/libraries/library-1")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "root": "   " }).to_string()))
                .expect("library patch blank-root request should build"),
        )
        .await
        .expect("library patch blank-root request should complete");

    assert_eq!(patch_response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(patch_response).await;
    assert_eq!(
        payload,
        json!({
            "violations": [
                {
                    "fieldName": "root",
                    "message": "must not be blank"
                }
            ]
        })
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_api_library_patch_rejects_multiple_blank_fields_with_kotlin_validation_payload() {
    let paths = new_router_fixture("router-api-library-patch-multiple-blank-fields").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let patch_response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/libraries/library-1")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "name": "   ", "root": "   " }).to_string(),
                ))
                .expect("library patch multiple-blank-fields request should build"),
        )
        .await
        .expect("library patch multiple-blank-fields request should complete");

    assert_eq!(patch_response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(patch_response).await;
    assert_eq!(
        payload,
        json!({
            "violations": [
                {
                    "fieldName": "root",
                    "message": "must not be blank"
                },
                {
                    "fieldName": "name",
                    "message": "must not be blank"
                }
            ]
        })
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_api_libraries_head_reuses_get_etag_for_conditional_requests() {
    let paths = new_router_fixture("router-api-libraries-head-etag").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let get_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/libraries")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("libraries get request should build"),
        )
        .await
        .expect("libraries get request should complete");
    assert_eq!(get_response.status(), StatusCode::OK);
    let etag = get_response
        .headers()
        .get(header::ETAG)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .expect("libraries get response should include etag");

    let head_response = app
        .oneshot(
            Request::builder()
                .method("HEAD")
                .uri("/api/v1/libraries")
                .header("x-auth-token", &auth_token)
                .header(header::IF_NONE_MATCH, etag)
                .body(Body::empty())
                .expect("libraries head request should build"),
        )
        .await
        .expect("libraries head request should complete");

    assert_eq!(head_response.status(), StatusCode::NOT_MODIFIED);

    cleanup_router_fixture(paths);
}
