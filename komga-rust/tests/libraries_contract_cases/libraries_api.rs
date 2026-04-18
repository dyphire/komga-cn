use super::*;

fn build_library_task_contract_router(paths: &RuntimeDbPaths) -> axum::Router {
    // These contracts assert queued TASK rows themselves, so runtime workers must stay off or the
    // background consumer can claim and delete the rows before the assertions inspect them.
    komga_server::app::build_router_without_runtime_workers_for_contract(&runtime_config_for_paths(
        paths,
    ))
}

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

async fn load_task_rows(paths: &RuntimeDbPaths, sql: &str) -> Vec<sqlx::sqlite::SqliteRow> {
    let pool = connect_pool(paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open");
    let rows = sqlx::query(sql)
        .fetch_all(&pool)
        .await
        .expect("task rows should be queryable");
    pool.close().await;
    rows
}

async fn assert_single_scan_task(
    paths: &RuntimeDbPaths,
    expected_id: String,
    expected_library_id: &str,
    expected_priority: i32,
    expected_deep: bool,
) {
    let rows = load_task_rows(
        paths,
        "SELECT ID, SIMPLE_TYPE, GROUP_ID, PRIORITY, PAYLOAD FROM TASK ORDER BY ID ASC",
    )
    .await;

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<String, _>("ID"), expected_id);
    assert_eq!(rows[0].get::<String, _>("SIMPLE_TYPE"), "ScanLibrary");
    assert_eq!(rows[0].get::<Option<String>, _>("GROUP_ID"), None);
    assert_eq!(rows[0].get::<i32, _>("PRIORITY"), expected_priority);
    assert_eq!(
        serde_json::from_str::<Value>(
            &rows[0]
                .get::<Option<String>, _>("PAYLOAD")
                .expect("scan task should persist payload metadata"),
        )
        .expect("scan task payload should be valid json"),
        json!({
            "libraryId": expected_library_id,
            "scanDeep": expected_deep,
            "priority": expected_priority,
            "groupId": Value::Null,
            "uniqueId": expected_id,
        })
    );
}

#[tokio::test]
async fn router_api_libraries_accepts_basic_auth_like_kotlin_clients() {
    let paths = new_router_fixture("router-api-libraries-basic-auth-compat").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/libraries")
                .header(
                    header::AUTHORIZATION,
                    basic_authorization_header_value(
                        "admin@example.org",
                        "router-contract-admin-123",
                    ),
                )
                .body(Body::empty())
                .expect("libraries basic-auth request should build"),
        )
        .await
        .expect("libraries basic-auth request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let ids = payload
        .as_array()
        .expect("libraries payload should be an array")
        .iter()
        .filter_map(|entry| entry.get("id").and_then(Value::as_str))
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["library-1"]);

    cleanup_router_fixture(paths);
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
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/libraries")
                .header("x-auth-token", &auth_token)
                .header(header::IF_NONE_MATCH, etag.as_str())
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

    let head_response = app
        .oneshot(
            Request::builder()
                .method("HEAD")
                .uri("/api/v1/libraries")
                .header("x-auth-token", &auth_token)
                .header(header::IF_NONE_MATCH, etag.as_str())
                .body(Body::empty())
                .expect("conditional libraries head request should build"),
        )
        .await
        .expect("conditional libraries head request should complete");

    assert_eq!(head_response.status(), StatusCode::NOT_MODIFIED);
    assert!(
        head_response.headers().get(header::CACHE_CONTROL).is_none(),
        "Kotlin conditional libraries head does not emit Cache-Control on 304"
    );
    assert!(head_response.headers().contains_key(header::ETAG));

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
async fn router_api_library_create_and_scan_enqueue_expected_scan_tasks() {
    let paths = new_router_fixture("router-api-library-create-enqueues-scan").await;
    seed_router_contract_data(&paths).await;

    let new_root = paths
        .config_dir
        .parent()
        .expect("fixture config dir should have a parent")
        .join("created-library-root");
    std::fs::create_dir_all(&new_root).expect("created library root should be creatable");

    let app = build_library_task_contract_router(&paths);
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/libraries")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "name": "Created Library",
                        "root": new_root.to_string_lossy(),
                    })
                    .to_string(),
                ))
                .expect("library create request should build"),
        )
        .await
        .expect("library create request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let library_id = payload
        .get("id")
        .and_then(Value::as_str)
        .expect("created library response should include id");

    assert_single_scan_task(
        &paths,
        format!("SCAN_LIBRARY_{library_id}_DEEP_false"),
        library_id,
        4,
        false,
    )
    .await;

    cleanup_router_fixture(paths);

    let paths = new_router_fixture("router-api-library-scan-task-shape").await;
    seed_router_contract_data(&paths).await;

    let app = build_library_task_contract_router(&paths);
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/libraries/library-1/scan?deep=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("library scan request should build"),
        )
        .await
        .expect("library scan request should complete");

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    assert_single_scan_task(
        &paths,
        "SCAN_LIBRARY_library-1_DEEP_true".to_string(),
        "library-1",
        8,
        true,
    )
    .await;

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_api_library_scan_returns_not_found_for_missing_library() {
    let paths = new_router_fixture("router-api-library-scan-missing-library").await;
    seed_router_contract_data(&paths).await;

    let app = build_library_task_contract_router(&paths);
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/libraries/missing-library/scan")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("missing library scan request should build"),
        )
        .await
        .expect("missing library scan request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let rows = load_task_rows(&paths, "SELECT COUNT(*) AS COUNT FROM TASK").await;
    assert_eq!(rows[0].get::<i64, _>("COUNT"), 0);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_api_library_analyze_enqueues_analyze_book_tasks_grouped_by_series_id() {
    let paths = new_router_fixture("router-api-library-analyze-task-groups").await;
    seed_router_contract_data(&paths).await;

    let app = build_library_task_contract_router(&paths);
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/libraries/library-1/analyze")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("library analyze request should build"),
        )
        .await
        .expect("library analyze request should complete");

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let rows = load_task_rows(
        &paths,
        "SELECT ID, SIMPLE_TYPE, GROUP_ID, PRIORITY FROM TASK ORDER BY ID ASC",
    )
    .await;

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<String, _>("ID"), "ANALYZE_BOOK_book-1");
    assert_eq!(rows[0].get::<String, _>("SIMPLE_TYPE"), "AnalyzeBook");
    assert_eq!(
        rows[0].get::<Option<String>, _>("GROUP_ID"),
        Some("series-1".to_string())
    );
    assert_eq!(rows[0].get::<i32, _>("PRIORITY"), 6);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_api_library_metadata_refresh_leaves_series_local_artwork_ungrouped() {
    let paths = new_router_fixture("router-api-library-metadata-refresh-task-shape").await;
    seed_router_contract_data(&paths).await;

    let app = build_library_task_contract_router(&paths);
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/libraries/library-1/metadata/refresh")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("library metadata refresh request should build"),
        )
        .await
        .expect("library metadata refresh request should complete");

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let rows = load_task_rows(
        &paths,
        "SELECT ID, SIMPLE_TYPE, GROUP_ID, PRIORITY, PAYLOAD FROM TASK ORDER BY ID ASC",
    )
    .await;

    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows[0].get::<String, _>("ID"),
        "REFRESH_BOOK_LOCAL_ARTWORK_book-1"
    );
    assert_eq!(rows[0].get::<Option<String>, _>("GROUP_ID"), None);
    assert_eq!(rows[0].get::<i32, _>("PRIORITY"), 6);

    assert_eq!(
        rows[1].get::<String, _>("ID"),
        "REFRESH_BOOK_METADATA_book-1"
    );
    assert_eq!(
        rows[1].get::<Option<String>, _>("GROUP_ID"),
        Some("series-1".to_string())
    );
    assert_eq!(rows[1].get::<i32, _>("PRIORITY"), 6);

    assert_eq!(
        rows[2].get::<String, _>("ID"),
        "REFRESH_SERIES_LOCAL_ARTWORK_series-1"
    );
    assert_eq!(rows[2].get::<Option<String>, _>("GROUP_ID"), None);
    assert_eq!(rows[2].get::<i32, _>("PRIORITY"), 6);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_api_library_empty_trash_enqueues_ungrouped_task() {
    let paths = new_router_fixture("router-api-library-empty-trash-task-shape").await;
    seed_router_contract_data(&paths).await;

    let app = build_library_task_contract_router(&paths);
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/libraries/library-1/empty-trash")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("library empty-trash request should build"),
        )
        .await
        .expect("library empty-trash request should complete");

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let rows = load_task_rows(
        &paths,
        "SELECT ID, SIMPLE_TYPE, GROUP_ID, PRIORITY, PAYLOAD FROM TASK ORDER BY ID ASC",
    )
    .await;

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<String, _>("ID"), "EMPTY_TRASH_library-1");
    assert_eq!(rows[0].get::<String, _>("SIMPLE_TYPE"), "EmptyTrash");
    assert_eq!(rows[0].get::<Option<String>, _>("GROUP_ID"), None);
    assert_eq!(rows[0].get::<i32, _>("PRIORITY"), 6);
    assert_eq!(
        serde_json::from_str::<Value>(&rows[0].get::<String, _>("PAYLOAD"))
            .expect("empty-trash payload should be valid json"),
        json!({
            "libraryId": "library-1",
            "priority": 6,
            "groupId": Value::Null,
            "uniqueId": "EMPTY_TRASH_library-1"
        }),
        "library empty-trash route should persist the Kotlin-compatible payload shape consumed by legacy readers",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_api_library_delete_rejects_invalid_access_paths() {
    #[derive(Clone, Copy)]
    enum DeleteAuth {
        None,
        Admin,
        NonAdmin,
    }

    let cases = [
        (
            "router-api-library-delete-requires-auth",
            "/api/v1/libraries/library-1",
            DeleteAuth::None,
            StatusCode::UNAUTHORIZED,
        ),
        (
            "router-api-library-delete-forbidden-non-admin",
            "/api/v1/libraries/library-1",
            DeleteAuth::NonAdmin,
            StatusCode::FORBIDDEN,
        ),
        (
            "router-api-library-delete-missing",
            "/api/v1/libraries/missing-library",
            DeleteAuth::Admin,
            StatusCode::NOT_FOUND,
        ),
    ];

    for (fixture_name, uri, auth_mode, expected_status) in cases {
        let paths = new_router_fixture(fixture_name).await;
        seed_router_contract_data(&paths).await;

        if matches!(auth_mode, DeleteAuth::NonAdmin) {
            seed_router_age_exclude_user_with_roles(
                &paths,
                "non-admin-user",
                "non-admin@example.org",
                "router-contract-non-admin-123",
                18,
                &["USER"],
            )
            .await;
        }

        let app = build_router_with_config(&runtime_config_for_paths(&paths));
        let auth_token = match auth_mode {
            DeleteAuth::None => None,
            DeleteAuth::Admin => Some(login_with_basic_and_get_token(app.clone()).await),
            DeleteAuth::NonAdmin => Some(
                login_with_basic_credentials_and_get_token(
                    app.clone(),
                    "non-admin@example.org",
                    "router-contract-non-admin-123",
                )
                .await,
            ),
        };

        let mut request = Request::builder().method("DELETE").uri(uri);
        if let Some(auth_token) = auth_token.as_deref() {
            request = request.header("x-auth-token", auth_token);
        }

        let response = app
            .oneshot(
                request
                    .body(Body::empty())
                    .expect("library delete request should build"),
            )
            .await
            .expect("library delete request should complete");

        assert_eq!(
            response.status(),
            expected_status,
            "unexpected status for {fixture_name}"
        );

        cleanup_router_fixture(paths);
    }
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
async fn router_api_library_patch_rejects_blank_fields_with_kotlin_validation_payload() {
    for (fixture_name, body, expected_payload) in [
        (
            "router-api-library-patch-blank-name",
            json!({ "name": "   " }),
            json!({
                "violations": [
                    {
                        "fieldName": "name",
                        "message": "must not be blank"
                    }
                ]
            }),
        ),
        (
            "router-api-library-patch-blank-root",
            json!({ "root": "   " }),
            json!({
                "violations": [
                    {
                        "fieldName": "root",
                        "message": "must not be blank"
                    }
                ]
            }),
        ),
        (
            "router-api-library-patch-multiple-blank-fields",
            json!({ "name": "   ", "root": "   " }),
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
            }),
        ),
    ] {
        let paths = new_router_fixture(fixture_name).await;
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
                    .body(Body::from(body.to_string()))
                    .expect("library patch blank-field request should build"),
            )
            .await
            .expect("library patch blank-field request should complete");

        assert_eq!(
            patch_response.status(),
            StatusCode::BAD_REQUEST,
            "case: {fixture_name}"
        );
        let payload = response_json(patch_response).await;
        assert_eq!(payload, expected_payload, "case: {fixture_name}");

        cleanup_router_fixture(paths);
    }
}
