use super::*;

#[tokio::test]
async fn router_opds_v1_collection_detail_returns_empty_feed_when_visible_page_is_empty() {
    let paths = new_router_fixture("router-opds-v1-collection-detail-empty-feed").await;
    seed_router_contract_data(&paths).await;
    update_router_series_age_rating(&paths, "series-1", 21).await;
    seed_router_age_exclude_user(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        12,
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted@example.org",
        "router-contract-restricted-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/collections/collection-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 collection detail empty-feed request should build"),
        )
        .await
        .expect("opds v1 collection detail empty-feed request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains("<id>collection-1</id>"));
    assert!(body.contains("<title>Collection 1</title>"));
    assert!(body.contains("rel=\"self\""));
    assert!(body.contains("rel=\"start\""));
    assert!(!body.contains("/opds/v1.2/series/series-1"));
    assert!(!body.contains("<entry>"));
    assert!(!body.contains("rel=\"previous\""));
    assert!(!body.contains("rel=\"next\""));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_collection_detail_uses_collection_and_series_last_modified_timestamps() {
    let paths = new_router_fixture("router-opds-v1-collection-detail-updated").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 collection detail updated db should open");
    sqlx::query("UPDATE COLLECTION SET LAST_MODIFIED_DATE = ? WHERE ID = ?")
        .bind("2024-01-20T01:02:03Z")
        .bind("collection-1")
        .execute(&pool)
        .await
        .expect("collection last modified should update");
    sqlx::query("UPDATE SERIES SET LAST_MODIFIED_DATE = ? WHERE ID = ?")
        .bind("2024-01-21T02:03:04Z")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series last modified should update");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/collections/collection-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 collection detail updated request should build"),
        )
        .await
        .expect("opds v1 collection detail updated request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains("<updated>2024-01-20T01:02:03Z</updated>"));
    assert!(body.contains("<updated>2024-01-21T02:03:04Z</updated>"));
    assert!(body.contains("/opds/v1.2/series/series-1"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_collection_detail_orders_unordered_entries_by_title_sort() {
    let paths = new_router_fixture("router-opds-v1-collection-detail-unordered-title-sort").await;
    seed_router_contract_data(&paths).await;
    seed_router_custom_series(&paths, "series-2", "Alpha Display", "library-1").await;
    update_router_series_metadata_titles(&paths, "series-1", "Zeta Display", "Zulu Sort").await;
    update_router_series_metadata_titles(&paths, "series-2", "Alpha Display", "Alpha Sort").await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 collection detail order db should open");
    sqlx::query("UPDATE COLLECTION SET ORDERED = ? WHERE ID = ?")
        .bind(false)
        .bind("collection-1")
        .execute(&pool)
        .await
        .expect("collection ordered flag should update");
    sqlx::query(
        "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?)",
    )
    .bind("collection-1")
    .bind("series-2")
    .bind(99_i64)
    .execute(&pool)
    .await
    .expect("second series should be attached to collection");
    sqlx::query(
        "UPDATE COLLECTION_SERIES SET NUMBER = ? WHERE COLLECTION_ID = ? AND SERIES_ID = ?",
    )
    .bind(0_i64)
    .bind("collection-1")
    .bind("series-1")
    .execute(&pool)
    .await
    .expect("first series collection number should update");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/collections/collection-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 collection detail unordered request should build"),
        )
        .await
        .expect("opds v1 collection detail unordered request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    let alpha_pos = body
        .find("/opds/v1.2/series/series-2")
        .expect("series-2 entry should be present");
    let zeta_pos = body
        .find("/opds/v1.2/series/series-1")
        .expect("series-1 entry should be present");
    assert!(
        alpha_pos < zeta_pos,
        "unordered OPDS v1 collection detail must order by Kotlin titleSort semantics, body={body}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_collection_detail_returns_not_found_when_collection_is_outside_shared_libraries()
 {
    let paths =
        new_router_fixture("router-opds-v1-collection-detail-library-scope-not-found").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "library-user",
        "library-user@example.org",
        "router-contract-library-123",
        &["library-1"],
    )
    .await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 collection detail library-scope db should open");
    sqlx::query("INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT) VALUES (?, ?, ?, ?)")
        .bind("collection-2")
        .bind("Collection 2")
        .bind(false)
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("library-scoped collection should be inserted");
    sqlx::query(
        "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?)",
    )
    .bind("collection-2")
    .bind("series-3")
    .bind(0_i64)
    .execute(&pool)
    .await
    .expect("library-scoped collection series should be inserted");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "library-user@example.org",
        "router-contract-library-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/collections/collection-2")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 collection detail library-scope request should build"),
        )
        .await
        .expect("opds v1 collection detail library-scope request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_readlist_detail_returns_empty_feed_when_visible_page_is_empty() {
    let paths = new_router_fixture("router-opds-v1-readlist-detail-empty-feed").await;
    seed_router_contract_data(&paths).await;
    update_router_series_age_rating(&paths, "series-1", 21).await;
    seed_router_age_exclude_user(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        12,
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted@example.org",
        "router-contract-restricted-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/readlists/readlist-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 readlist detail empty-feed request should build"),
        )
        .await
        .expect("opds v1 readlist detail empty-feed request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains("<id>readlist-1</id>"));
    assert!(body.contains("<title>ReadList 1</title>"));
    assert!(body.contains("rel=\"self\""));
    assert!(body.contains("rel=\"start\""));
    assert!(!body.contains("<entry>"));
    assert!(!body.contains("rel=\"previous\""));
    assert!(!body.contains("rel=\"next\""));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_readlist_detail_returns_not_found_when_readlist_is_outside_shared_libraries()
 {
    let paths = new_router_fixture("router-opds-v1-readlist-detail-library-scope-not-found").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "library-user",
        "library-user@example.org",
        "router-contract-library-123",
        &["library-1"],
    )
    .await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 readlist detail library-scope db should open");
    sqlx::query("INSERT INTO READLIST (ID, NAME, BOOK_COUNT, ORDERED) VALUES (?, ?, ?, ?)")
        .bind("readlist-2")
        .bind("ReadList 2")
        .bind(1_i64)
        .bind(false)
        .execute(&pool)
        .await
        .expect("library-scoped readlist should be inserted");
    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind("readlist-2")
        .bind("book-3")
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("library-scoped readlist book should be inserted");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "library-user@example.org",
        "router-contract-library-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/readlists/readlist-2")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 readlist detail library-scope request should build"),
        )
        .await
        .expect("opds v1 readlist detail library-scope request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_readlist_detail_orders_unordered_entries_by_release_date() {
    let paths = new_router_fixture("router-opds-v1-readlist-detail-unordered-release-date").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 readlist detail order db should open");
    sqlx::query("UPDATE READLIST SET ORDERED = ?, BOOK_COUNT = ? WHERE ID = ?")
        .bind(false)
        .bind(2_i64)
        .bind("readlist-1")
        .execute(&pool)
        .await
        .expect("readlist ordered flag should update");
    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind("readlist-1")
        .bind("book-2")
        .bind(99_i64)
        .execute(&pool)
        .await
        .expect("second readlist book should be inserted");
    sqlx::query("INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("application/epub+zip")
        .bind("READY")
        .bind("book-2")
        .bind(10_i64)
        .execute(&pool)
        .await
        .expect("book-2 ready media row should be inserted for unordered readlist test");
    sqlx::query("UPDATE BOOK_METADATA SET RELEASE_DATE = ? WHERE BOOK_ID = ?")
        .bind("2024-01-16")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book-1 release date should update");
    sqlx::query("UPDATE BOOK_METADATA SET RELEASE_DATE = ? WHERE BOOK_ID = ?")
        .bind("2024-01-14")
        .bind("book-2")
        .execute(&pool)
        .await
        .expect("book-2 release date should update");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/readlists/readlist-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 readlist detail unordered request should build"),
        )
        .await
        .expect("opds v1 readlist detail unordered request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    let earlier_pos = body
        .find("<id>book-2</id>")
        .expect("book-2 entry should be present");
    let later_pos = body
        .find("<id>book-1</id>")
        .expect("book-1 entry should be present");
    assert!(
        earlier_pos < later_pos,
        "unordered OPDS v1 readlist detail must order by Kotlin releaseDate semantics, body={body}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_readlist_detail_uses_readlist_and_book_last_modified_timestamps() {
    let paths = new_router_fixture("router-opds-v1-readlist-detail-updated").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 readlist detail updated db should open");
    sqlx::query("UPDATE READLIST SET LAST_MODIFIED_DATE = ? WHERE ID = ?")
        .bind("2024-01-22T01:02:03Z")
        .bind("readlist-1")
        .execute(&pool)
        .await
        .expect("readlist last modified should update");
    sqlx::query("UPDATE BOOK SET LAST_MODIFIED_DATE = ? WHERE ID = ?")
        .bind("2024-01-23T02:03:04Z")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book last modified should update");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/readlists/readlist-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 readlist detail updated request should build"),
        )
        .await
        .expect("opds v1 readlist detail updated request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains("<updated>2024-01-22T01:02:03Z</updated>"));
    assert!(body.contains("<updated>2024-01-23T02:03:04Z</updated>"));
    assert!(body.contains("<id>book-1</id>"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_readlist_detail_uses_kotlin_acquisition_entry_shape() {
    let paths = new_router_fixture("router-opds-v1-readlist-detail-entry-shape").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/readlists/readlist-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 readlist detail entry-shape request should build"),
        )
        .await
        .expect("opds v1 readlist detail entry-shape request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains("<title>Series 1 1: Book 1</title>"));
    assert!(body.contains("<content>epub - 1024</content>"));
    assert!(body.contains("<author><name>Jane Writer</name></author>"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_readlist_detail_filters_non_ready_books_without_turning_empty_scope_into_not_found()
 {
    let paths = new_router_fixture("router-opds-v1-readlist-detail-ready-only").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 readlist detail ready-only db should open");
    sqlx::query("UPDATE READLIST SET ORDERED = ?, BOOK_COUNT = ? WHERE ID = ?")
        .bind(true)
        .bind(2_i64)
        .bind("readlist-1")
        .execute(&pool)
        .await
        .expect("readlist ordered flag should update for ready-only test");
    sqlx::query("UPDATE MEDIA SET STATUS = ? WHERE BOOK_ID = ?")
        .bind("ERROR")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book-1 media status should update to non-ready");
    sqlx::query("INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("application/epub+zip")
        .bind("READY")
        .bind("book-2")
        .bind(10_i64)
        .execute(&pool)
        .await
        .expect("book-2 ready media row should be inserted");
    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind("readlist-1")
        .bind("book-2")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("book-2 should be attached to readlist-1");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/readlists/readlist-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 readlist detail ready-only request should build"),
        )
        .await
        .expect("opds v1 readlist detail ready-only request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains("<id>readlist-1</id>"));
    assert!(body.contains("<id>book-2</id>"));
    assert!(!body.contains("<id>book-1</id>"));

    cleanup_router_fixture(paths);
}
