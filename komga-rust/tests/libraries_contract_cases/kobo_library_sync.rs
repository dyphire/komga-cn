use super::*;

async fn load_first_kobo_sync_point_row(paths: &RuntimeDbPaths) -> (String, Option<String>) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("kobo sync point db should open");

    let row = sqlx::query("SELECT USER_ID, API_KEY_ID FROM SYNC_POINT ORDER BY ID LIMIT 1")
        .fetch_one(&pool)
        .await
        .expect("kobo sync point row should load");

    pool.close().await;
    (
        row.get::<String, _>("USER_ID"),
        row.get::<Option<String>, _>("API_KEY_ID"),
    )
}

async fn seed_second_book_in_primary_series(paths: &RuntimeDbPaths) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("second primary-series book db should open");

    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-2")
    .bind(0_i64)
    .bind("book-2.epub")
    .bind("books/book-2.epub")
    .bind("series-1")
    .bind(2_048_i64)
    .bind(2_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("second primary-series book should be inserted");

    sqlx::query("INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("application/epub+zip")
        .bind("READY")
        .bind("book-2")
        .bind(12_i64)
        .execute(&pool)
        .await
        .expect("second primary-series media should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("2")
    .bind(2.0_f64)
    .bind("Book 2")
    .bind("2024-01-16")
    .bind("book-2")
    .execute(&pool)
    .await
    .expect("second primary-series metadata should be inserted");

    sqlx::query("INSERT INTO BOOK_METADATA_AUTHOR (BOOK_ID, NAME, ROLE) VALUES (?, ?, ?)")
        .bind("book-2")
        .bind("Jane Writer")
        .bind("writer")
        .execute(&pool)
        .await
        .expect("second primary-series author should be inserted");

    pool.close().await;
}

async fn update_series_read_progress_date(
    paths: &RuntimeDbPaths,
    series_id: &str,
    user_id: &str,
    most_recent_read_date: &str,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("series read progress db should open");

    sqlx::query(
        "UPDATE READ_PROGRESS_SERIES SET MOST_RECENT_READ_DATE = ? WHERE SERIES_ID = ? AND USER_ID = ?",
    )
    .bind(most_recent_read_date)
    .bind(series_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .expect("series read progress date should be updated");

    pool.close().await;
}

#[tokio::test]
async fn router_kobo_library_sync_returns_nested_dto_shape_and_sync_token() {
    let paths = new_router_fixture("router-kobo-library-sync-shape").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let first_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/sync")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo library sync request should build"),
        )
        .await
        .expect("kobo library sync request should complete");
    assert_eq!(first_response.status(), StatusCode::OK);

    let sync_token_header = first_response
        .headers()
        .get("x-kobo-synctoken")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .expect("kobo sync response should include x-kobo-synctoken header");
    assert!(sync_token_header.starts_with("KOMGA."));

    let first_payload = response_json(first_response).await;
    let first_events = first_payload
        .as_array()
        .expect("kobo sync response should be a JSON array");
    assert!(!first_events.is_empty());

    let entitlement = first_events
        .iter()
        .find_map(|event| event.get("NewEntitlement"))
        .expect("kobo sync payload should contain a NewEntitlement event");
    assert!(entitlement.get("BookEntitlement").is_some());
    assert!(entitlement.get("BookMetadata").is_some());
    assert!(entitlement.get("ReadingState").is_some());

    let second_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/sync")
                .header("x-auth-token", &auth_token)
                .header("x-kobo-synctoken", sync_token_header)
                .body(Body::empty())
                .expect("kobo library sync continuation request should build"),
        )
        .await
        .expect("kobo library sync continuation request should complete");
    assert_eq!(second_response.status(), StatusCode::OK);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_library_sync_respects_shared_library_scope() {
    let paths = new_router_fixture("router-kobo-library-sync-library-scope").await;
    seed_router_contract_data(&paths).await;
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("secondary kobo library db should open");
    sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES (?, ?, ?)")
        .bind("library-2")
        .bind("Library 2")
        .bind(paths.config_dir.to_string_lossy().to_string())
        .execute(&pool)
        .await
        .expect("secondary library should be inserted");
    sqlx::query(
        "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("series-2")
    .bind(0_i64)
    .bind("Series 2")
    .bind("series/series-2")
    .bind("library-2")
    .execute(&pool)
    .await
    .expect("secondary series should be inserted");
    sqlx::query(
        "INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, PUBLISHER, LANGUAGE, AGE_RATING, SERIES_ID) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("ONGOING")
    .bind("Series 2")
    .bind("Series 2")
    .bind("AltPub")
    .bind("EN")
    .bind(12_i64)
    .bind("series-2")
    .execute(&pool)
    .await
    .expect("secondary series metadata should be inserted");
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-2")
    .bind(0_i64)
    .bind("book-2.epub")
    .bind("books/book-2.epub")
    .bind("series-2")
    .bind(2_048_i64)
    .bind(1_i64)
    .bind("library-2")
    .execute(&pool)
    .await
    .expect("secondary book should be inserted");
    sqlx::query("INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("application/epub+zip")
        .bind("READY")
        .bind("book-2")
        .bind(12_i64)
        .execute(&pool)
        .await
        .expect("secondary media should be inserted");
    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("1")
    .bind(1.0_f64)
    .bind("Book 2")
    .bind("2024-01-16")
    .bind("book-2")
    .execute(&pool)
    .await
    .expect("secondary book metadata should be inserted");
    sqlx::query("INSERT INTO BOOK_METADATA_AUTHOR (BOOK_ID, NAME, ROLE) VALUES (?, ?, ?)")
        .bind("book-2")
        .bind("Morgan Else")
        .bind("writer")
        .execute(&pool)
        .await
        .expect("secondary book author should be inserted");
    pool.close().await;

    seed_router_library_restricted_user(
        &paths,
        "kobo-library-user",
        "kobo-library@example.org",
        "router-contract-kobo-library-123",
        &["library-1"],
    )
    .await;
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("user role seed db should open");
    sqlx::query("INSERT INTO USER_ROLE (USER_ID, ROLE) VALUES (?, ?)")
        .bind("kobo-library-user")
        .bind("KOBO_SYNC")
        .execute(&pool)
        .await
        .expect("user role should be inserted");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "kobo-library@example.org",
        "router-contract-kobo-library-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/sync")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo library sync request should build"),
        )
        .await
        .expect("kobo library sync request should complete");

    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    let entitlement_ids = payload
        .as_array()
        .expect("kobo sync response should be a JSON array")
        .iter()
        .filter_map(|event| event.get("NewEntitlement"))
        .filter_map(|event| event.get("BookEntitlement"))
        .filter_map(|event| event.get("Id"))
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();

    assert!(entitlement_ids.contains(&"book-1"));
    assert!(
        !entitlement_ids.contains(&"book-2"),
        "kobo sync should exclude books outside the user's shared libraries"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_library_sync_respects_age_restrictions() {
    let paths = new_router_fixture("router-kobo-library-sync-age-restriction").await;
    seed_router_contract_data(&paths).await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "kobo-age-user",
        "kobo-age@example.org",
        "router-contract-kobo-age-123",
        13,
        &["USER", "KOBO_SYNC"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "kobo-age@example.org",
        "router-contract-kobo-age-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/sync")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo library sync request should build"),
        )
        .await
        .expect("kobo library sync request should complete");

    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    let has_entitlements = payload
        .as_array()
        .expect("kobo sync response should be a JSON array")
        .iter()
        .any(|event| event.get("NewEntitlement").is_some());

    assert!(
        !has_entitlements,
        "kobo sync should exclude books blocked by the user's age restriction"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_library_sync_excludes_regular_readlist_tags_without_ondeck() {
    let paths = new_router_fixture("router-kobo-library-sync-excludes-regular-readlist-tags").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/sync")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo library sync request should build"),
        )
        .await
        .expect("kobo library sync request should complete");

    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    let has_regular_readlist_tag = payload
        .as_array()
        .expect("kobo sync response should be a JSON array")
        .iter()
        .filter_map(|event| event.get("NewTag"))
        .any(|tag| {
            tag.get("Tag")
                .and_then(|value| value.get("Id"))
                .and_then(Value::as_str)
                == Some("readlist-1")
        });

    assert!(
        !has_regular_readlist_tag,
        "kobo sync should not expose persisted readlists; Kotlin only seeds synthetic On Deck"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_library_sync_uses_series_read_date_for_ondeck_last_modified() {
    let paths = new_router_fixture("router-kobo-library-sync-ondeck-last-modified").await;
    seed_router_contract_data(&paths).await;
    seed_second_book_in_primary_series(&paths).await;
    seed_router_read_progress(&paths, true).await;
    seed_router_series_read_progress(&paths, 1, 0).await;
    update_series_read_progress_date(&paths, "series-1", "admin-user", "2026-01-05T00:00:00Z")
        .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/sync")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo library sync request should build"),
        )
        .await
        .expect("kobo library sync request should complete");

    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    let ondeck_tag = payload
        .as_array()
        .expect("kobo sync response should be a JSON array")
        .iter()
        .filter_map(|event| event.get("NewTag"))
        .find(|tag| {
            tag.get("Tag")
                .and_then(|value| value.get("Id"))
                .and_then(Value::as_str)
                == Some("KOMGA-ONDECK")
        })
        .expect("kobo sync should include a synthetic On Deck tag");

    assert_eq!(
        ondeck_tag
            .get("Tag")
            .and_then(|value| value.get("Items"))
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(|item| item.get("RevisionId"))
            .and_then(Value::as_str),
        Some("book-2")
    );
    assert_eq!(
        ondeck_tag
            .get("Tag")
            .and_then(|value| value.get("LastModified"))
            .and_then(Value::as_str),
        Some("2026-01-05T00:00:00Z")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_library_sync_persists_api_key_id_in_sync_point_state() {
    let paths = new_router_fixture("router-kobo-library-sync-api-key-ownership").await;
    seed_router_contract_data(&paths).await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "kobo-sync-user",
        "kobo-sync@example.org",
        "router-contract-kobo-sync-123",
        0,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(&paths, "validkobotoken", "kobo-sync-user").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/validkobotoken/v1/library/sync")
                .body(Body::empty())
                .expect("kobo library sync path-token request should build"),
        )
        .await
        .expect("kobo library sync path-token request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let (user_id, api_key_id) = load_first_kobo_sync_point_row(&paths).await;
    assert_eq!(user_id, "kobo-sync-user");
    assert_eq!(api_key_id.as_deref(), Some("api-key-validkobotoken"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_library_sync_uses_request_api_key_identity_when_session_authenticated() {
    let paths = new_router_fixture("router-kobo-library-sync-session-api-key-identity").await;
    seed_router_contract_data(&paths).await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "kobo-sync-user",
        "kobo-sync@example.org",
        "router-contract-kobo-sync-123",
        0,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(&paths, "validkobotoken", "kobo-sync-user").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "kobo-sync@example.org",
        "router-contract-kobo-sync-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/sync")
                .header("x-auth-token", &auth_token)
                .header("x-api-key", "validkobotoken")
                .body(Body::empty())
                .expect("session-authenticated kobo sync request should build"),
        )
        .await
        .expect("session-authenticated kobo sync request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let (user_id, api_key_id) = load_first_kobo_sync_point_row(&paths).await;
    assert_eq!(user_id, "kobo-sync-user");
    assert_eq!(api_key_id.as_deref(), Some("api-key-validkobotoken"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_library_sync_rejects_bare_base64_komga_tokens_as_invalid() {
    let paths = new_router_fixture("router-kobo-library-sync-bare-base64-komga-token").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let first_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/sync")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("initial kobo library sync request should build"),
        )
        .await
        .expect("initial kobo library sync request should complete");

    assert_eq!(first_response.status(), StatusCode::OK);
    let sync_token_header = first_response
        .headers()
        .get("x-kobo-synctoken")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .expect("initial sync response should include x-kobo-synctoken");
    let first_payload = response_json(first_response).await;
    assert!(
        first_payload
            .as_array()
            .expect("initial sync response should be a JSON array")
            .iter()
            .any(|event| event.get("NewEntitlement").is_some())
    );
    let bare_sync_token = sync_token_header
        .strip_prefix("KOMGA.")
        .expect("initial sync token should use KOMGA prefix");

    let second_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/sync")
                .header("x-auth-token", &auth_token)
                .header("x-kobo-synctoken", bare_sync_token)
                .body(Body::empty())
                .expect("bare-base64 kobo library sync request should build"),
        )
        .await
        .expect("bare-base64 kobo library sync request should complete");

    assert_eq!(second_response.status(), StatusCode::OK);
    let second_payload = response_json(second_response).await;
    assert!(
        second_payload
            .as_array()
            .expect("bare-base64 sync response should be a JSON array")
            .iter()
            .any(|event| event.get("NewEntitlement").is_some())
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_library_sync_removed_book_matches_kotlin_placeholder_metadata() {
    let paths = new_router_fixture("router-kobo-library-sync-removed-book-metadata").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let initial_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/sync")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("initial kobo library sync request should build"),
        )
        .await
        .expect("initial kobo library sync request should complete");

    assert_eq!(initial_response.status(), StatusCode::OK);
    let sync_token_header = initial_response
        .headers()
        .get("x-kobo-synctoken")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .expect("initial sync response should include x-kobo-synctoken");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("kobo removed metadata db should open");
    sqlx::query("UPDATE BOOK SET DELETED_DATE = ? WHERE ID = ?")
        .bind("2026-04-15 10:00:00")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book should be soft deleted");
    pool.close().await;

    let removed_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/sync")
                .header("x-auth-token", &auth_token)
                .header("x-kobo-synctoken", sync_token_header)
                .body(Body::empty())
                .expect("removed-book kobo library sync request should build"),
        )
        .await
        .expect("removed-book kobo library sync request should complete");

    assert_eq!(removed_response.status(), StatusCode::OK);
    let payload = response_json(removed_response).await;
    let removed = payload
        .as_array()
        .and_then(|events| {
            events
                .iter()
                .find_map(|event| event.get("ChangedEntitlement"))
        })
        .expect("removed-book sync response should contain ChangedEntitlement");

    assert_eq!(
        removed.pointer("/BookEntitlement/Id"),
        Some(&Value::String("book-1".to_string()))
    );
    assert_eq!(
        removed.pointer("/BookEntitlement/IsRemoved"),
        Some(&Value::Bool(true))
    );
    assert_eq!(
        removed.pointer("/BookMetadata/CoverImageId"),
        Some(&Value::String("book-1".to_string()))
    );
    assert_eq!(
        removed.pointer("/BookMetadata/Title"),
        Some(&Value::String("book-1".to_string()))
    );
    assert_eq!(
        removed.pointer("/BookMetadata/Language"),
        Some(&Value::String("en".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_library_sync_does_not_backfill_missing_api_key_id_on_existing_state() {
    let paths = new_router_fixture("router-kobo-library-sync-no-api-key-backfill").await;
    seed_router_contract_data(&paths).await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "kobo-sync-user",
        "kobo-sync@example.org",
        "router-contract-kobo-sync-123",
        0,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(&paths, "validkobotoken", "kobo-sync-user").await;

    let ongoing_sync_point_id = "existing-sync-point";
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("kobo sync point seed db should open");
    sqlx::query("INSERT INTO SYNC_POINT (ID, USER_ID, API_KEY_ID) VALUES (?, ?, ?)")
        .bind(ongoing_sync_point_id)
        .bind("kobo-sync-user")
        .bind(Option::<String>::None)
        .execute(&pool)
        .await
        .expect("kobo sync point row should be inserted");
    pool.close().await;

    let sync_token_payload = json!({
        "version": 1,
        "rawKoboSyncToken": "",
        "ongoingSyncPointId": ongoing_sync_point_id,
        "lastSuccessfulSyncPointId": null
    })
    .to_string();
    let sync_token_header = format!(
        "KOMGA.{}",
        STANDARD_NO_PAD.encode(sync_token_payload.as_bytes())
    );

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/validkobotoken/v1/library/sync")
                .header("x-kobo-synctoken", sync_token_header)
                .body(Body::empty())
                .expect("existing-state kobo library sync request should build"),
        )
        .await
        .expect("existing-state kobo library sync request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("kobo sync point db should open");
    let row = sqlx::query("SELECT API_KEY_ID FROM SYNC_POINT WHERE ID = ? LIMIT 1")
        .bind(ongoing_sync_point_id)
        .fetch_one(&pool)
        .await
        .expect("kobo sync point row should load");
    let api_key_id = row.get::<Option<String>, _>("API_KEY_ID");
    pool.close().await;

    assert_eq!(api_key_id, None);

    cleanup_router_fixture(paths);
}
