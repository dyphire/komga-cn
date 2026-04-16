use super::*;
use icu::collator::{
    Collator,
    options::{CollatorOptions, Strength},
};
use icu::locale::locale;
use time::format_description::well_known::Rfc3339;
use time::macros::format_description;
use time::{PrimitiveDateTime, UtcOffset};

fn kotlin_unicode_3_collator() -> icu::collator::CollatorBorrowed<'static> {
    let mut options = CollatorOptions::default();
    options.strength = Some(Strength::Tertiary);
    Collator::try_new(locale!("und").into(), options)
        .expect("ICU collator for OPDS contract ordering should construct")
}

mod book_feeds;

mod series_detail;

mod collection_readlist_details;

#[tokio::test]
async fn router_opds_v1_publishers_returns_atom_feed() {
    let paths = new_router_fixture("router-opds-v1-publishers-feed").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/publishers")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 publishers request should build"),
        )
        .await
        .expect("opds v1 publishers request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    assert!(content_type.contains("application/atom+xml"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_readlists_filters_out_of_scope_entries_sorts_by_name_and_uses_persisted_entry_updated()
 {
    let paths = new_router_fixture("router-opds-v1-readlists-visible-order-updated").await;
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

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 readlists db should open");
    sqlx::query(
        "INSERT INTO READLIST (ID, NAME, BOOK_COUNT, ORDERED, LAST_MODIFIED_DATE) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("readlist-alpha")
    .bind("Alpha ReadList")
    .bind(1_i64)
    .bind(true)
    .bind("2024-01-24T01:02:03Z")
    .execute(&pool)
    .await
    .expect("visible readlist should be inserted");
    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind("readlist-alpha")
        .bind("book-2")
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("visible readlist book should be inserted");
    sqlx::query(
        "INSERT INTO READLIST (ID, NAME, BOOK_COUNT, ORDERED, LAST_MODIFIED_DATE) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("readlist-zulu")
    .bind("Zulu ReadList")
    .bind(1_i64)
    .bind(true)
    .bind("2024-01-25T01:02:03Z")
    .execute(&pool)
    .await
    .expect("out-of-scope readlist should be inserted");
    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind("readlist-zulu")
        .bind("book-3")
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("out-of-scope readlist book should be inserted");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
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
                .uri("/opds/v1.2/readlists")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 readlists request should build"),
        )
        .await
        .expect("opds v1 readlists request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains("<id>allReadLists</id>"));
    assert!(body.contains("<title>All read lists</title>"));
    assert!(body.contains("<updated>2024-01-24T01:02:03Z</updated>"));
    assert!(body.contains("/opds/v1.2/readlists/readlist-alpha"));
    assert!(body.contains("/opds/v1.2/readlists/readlist-1"));
    assert!(!body.contains("/opds/v1.2/readlists/readlist-zulu"));
    let alpha_pos = body
        .find("/opds/v1.2/readlists/readlist-alpha")
        .expect("alpha readlist entry should be present");
    let default_pos = body
        .find("/opds/v1.2/readlists/readlist-1")
        .expect("default readlist entry should be present");
    assert!(
        alpha_pos < default_pos,
        "readlists list must preserve Kotlin name ordering, body={body}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_collections_filters_out_of_scope_entries_sorts_by_name_and_uses_persisted_entry_updated()
 {
    let paths = new_router_fixture("router-opds-v1-collections-visible-order-updated").await;
    seed_router_contract_data(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "library-user",
        "library-user@example.org",
        "router-contract-library-123",
        &["library-1"],
    )
    .await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 collections db should open");
    sqlx::query(
        "INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT, LAST_MODIFIED_DATE) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("collection-alpha")
    .bind("Alpha Collection")
    .bind(false)
    .bind(1_i64)
    .bind("2024-01-26T01:02:03Z")
    .execute(&pool)
    .await
    .expect("visible collection should be inserted");
    sqlx::query(
        "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?)",
    )
    .bind("collection-alpha")
    .bind("series-1")
    .bind(0_i64)
    .execute(&pool)
    .await
    .expect("visible collection series should be inserted");
    sqlx::query(
        "INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT, LAST_MODIFIED_DATE) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("collection-zulu")
    .bind("Zulu Collection")
    .bind(false)
    .bind(1_i64)
    .bind("2024-01-27T01:02:03Z")
    .execute(&pool)
    .await
    .expect("out-of-scope collection should be inserted");
    sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES (?, ?, ?)")
        .bind("library-2")
        .bind("Library 2")
        .bind(paths.config_dir.to_string_lossy().to_string())
        .execute(&pool)
        .await
        .expect("library-2 should be inserted for out-of-scope collection test");
    sqlx::query(
        "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("series-zulu")
    .bind(0_i64)
    .bind("Series Zulu")
    .bind("series/series-zulu")
    .bind("library-2")
    .execute(&pool)
    .await
    .expect("series-zulu should be inserted for out-of-scope collection test");
    sqlx::query(
        "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?)",
    )
    .bind("collection-zulu")
    .bind("series-zulu")
    .bind(0_i64)
    .execute(&pool)
    .await
    .expect("out-of-scope collection series should be inserted");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
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
                .uri("/opds/v1.2/collections")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 collections request should build"),
        )
        .await
        .expect("opds v1 collections request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains("<id>allCollections</id>"));
    assert!(body.contains("<title>All collections</title>"));
    assert!(
        body.contains("<entry><title>Alpha Collection</title><updated>2024-01-26T01:02:03Z</updated><id>collection-alpha</id>"),
        "collections list should preserve persisted entry updated, body={body}"
    );
    assert!(body.contains("/opds/v1.2/collections/collection-alpha"));
    assert!(body.contains("/opds/v1.2/collections/collection-1"));
    assert!(!body.contains("/opds/v1.2/collections/collection-zulu"));
    let alpha_pos = body
        .find("/opds/v1.2/collections/collection-alpha")
        .expect("alpha collection entry should be present");
    let default_pos = body
        .find("/opds/v1.2/collections/collection-1")
        .expect("default collection entry should be present");
    assert!(
        alpha_pos < default_pos,
        "collections list must preserve Kotlin name ordering, body={body}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_collections_keeps_empty_collection_for_all_library_user() {
    let paths = new_router_fixture("router-opds-v1-collections-empty-visible").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 collections empty-visible db should open");
    sqlx::query(
        "INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT, LAST_MODIFIED_DATE) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("collection-empty")
    .bind("Empty Collection")
    .bind(false)
    .bind(0_i64)
    .bind("2024-02-01T01:02:03Z")
    .execute(&pool)
    .await
    .expect("empty collection should be inserted");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/collections")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 collections empty-visible request should build"),
        )
        .await
        .expect("opds v1 collections empty-visible request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(
        body.contains("<entry><title>Empty Collection</title><updated>2024-02-01T01:02:03Z</updated><id>collection-empty</id>"),
        "all-libraries OPDS collections list should keep empty collections like Kotlin, body={body}"
    );
    assert!(body.contains("/opds/v1.2/collections/collection-empty"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_collections_formats_sqlite_naive_updated_like_kotlin() {
    let paths = new_router_fixture("router-opds-v1-collections-naive-updated").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 collections naive-updated db should open");
    sqlx::query(
        "INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT, LAST_MODIFIED_DATE) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("collection-naive-updated")
    .bind("Naive Updated Collection")
    .bind(false)
    .bind(0_i64)
    .bind("2024-01-26 01:02:03")
    .execute(&pool)
    .await
    .expect("naive-updated collection should be inserted");
    pool.close().await;

    let parsed = PrimitiveDateTime::parse(
        "2024-01-26 01:02:03",
        format_description!("[year]-[month]-[day] [hour]:[minute]:[second]"),
    )
    .expect("test naive timestamp should parse");
    let expected_updated = parsed
        .assume_utc()
        .to_offset(UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC))
        .format(&Rfc3339)
        .expect("expected OPDS updated timestamp should format");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/collections")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 collections naive-updated request should build"),
        )
        .await
        .expect("opds v1 collections naive-updated request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(
        body.contains(
            format!(
                "<entry><title>Naive Updated Collection</title><updated>{expected_updated}</updated><id>collection-naive-updated</id>"
            )
            .as_str()
        ),
        "OPDS v1 collections should format SQLite naive updated like Kotlin, body={body}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_collections_preserves_unicode_collation_order() {
    let paths = new_router_fixture("router-opds-v1-collections-unicode-order").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 collections unicode-order db should open");
    sqlx::query("UPDATE COLLECTION SET NAME = ? WHERE ID = ?")
        .bind("Éclair Collection")
        .bind("collection-1")
        .execute(&pool)
        .await
        .expect("default collection name should update for Unicode ordering test");
    sqlx::query("INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT) VALUES (?, ?, ?, ?)")
        .bind("collection-alpha")
        .bind("Alpha Collection")
        .bind(false)
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("alpha collection should insert for Unicode ordering test");
    sqlx::query(
        "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?)",
    )
    .bind("collection-alpha")
    .bind("series-1")
    .bind(0_i64)
    .execute(&pool)
    .await
    .expect("alpha collection series should insert for Unicode ordering test");
    sqlx::query("INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT) VALUES (?, ?, ?, ?)")
        .bind("collection-zulu")
        .bind("Zulu Collection")
        .bind(false)
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("zulu collection should insert for Unicode ordering test");
    sqlx::query(
        "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?)",
    )
    .bind("collection-zulu")
    .bind("series-1")
    .bind(0_i64)
    .execute(&pool)
    .await
    .expect("zulu collection series should insert for Unicode ordering test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/collections")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 collections Unicode ordering request should build"),
        )
        .await
        .expect("opds v1 collections Unicode ordering request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    let alpha_pos = body
        .find("/opds/v1.2/collections/collection-alpha")
        .expect("alpha collection entry should be present");
    let eclair_pos = body
        .find("/opds/v1.2/collections/collection-1")
        .expect("Éclair collection entry should be present");
    let zulu_pos = body
        .find("/opds/v1.2/collections/collection-zulu")
        .expect("zulu collection entry should be present");
    assert!(
        alpha_pos < eclair_pos && eclair_pos < zulu_pos,
        "OPDS v1 collections should keep Kotlin Unicode collation order, body={body}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_collections_preserves_kotlin_tertiary_case_order() {
    let paths = new_router_fixture("router-opds-v1-collections-tertiary-case-order").await;
    seed_router_contract_data(&paths).await;

    let collator = kotlin_unicode_3_collator();
    let mut names = [
        "eclair Collection".to_string(),
        "Eclair Collection".to_string(),
        "ECLAIR Collection".to_string(),
    ];
    names.sort_by(|left_name, right_name| collator.compare(left_name, right_name));

    let assigned = [
        ("collection-1", names[2].clone()),
        ("collection-a", names[1].clone()),
        ("collection-b", names[0].clone()),
    ];

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 collections tertiary-case-order db should open");
    sqlx::query("UPDATE COLLECTION SET NAME = ? WHERE ID = ?")
        .bind(&assigned[0].1)
        .bind(assigned[0].0)
        .execute(&pool)
        .await
        .expect("default collection name should update for tertiary case-order test");
    sqlx::query("INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT) VALUES (?, ?, ?, ?)")
        .bind(assigned[1].0)
        .bind(&assigned[1].1)
        .bind(false)
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("second collection should insert for tertiary case-order test");
    sqlx::query(
        "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?)",
    )
    .bind(assigned[1].0)
    .bind("series-1")
    .bind(0_i64)
    .execute(&pool)
    .await
    .expect("second collection series should insert for tertiary case-order test");
    sqlx::query("INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT) VALUES (?, ?, ?, ?)")
        .bind(assigned[2].0)
        .bind(&assigned[2].1)
        .bind(false)
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("third collection should insert for tertiary case-order test");
    sqlx::query(
        "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?)",
    )
    .bind(assigned[2].0)
    .bind("series-1")
    .bind(0_i64)
    .execute(&pool)
    .await
    .expect("third collection series should insert for tertiary case-order test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/collections")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 collections tertiary case-order request should build"),
        )
        .await
        .expect("opds v1 collections tertiary case-order request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;

    let expected_ids = [
        "/opds/v1.2/collections/collection-b",
        "/opds/v1.2/collections/collection-a",
        "/opds/v1.2/collections/collection-1",
    ];
    for pair in expected_ids.windows(2) {
        let left_pos = body
            .find(pair[0])
            .expect("expected left collection entry should be present");
        let right_pos = body
            .find(pair[1])
            .expect("expected right collection entry should be present");
        assert!(
            left_pos < right_pos,
            "OPDS v1 collections should keep Kotlin tertiary case order, body={body}"
        );
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_library_detail_orders_series_by_title_sort() {
    let paths = new_router_fixture("router-opds-v1-library-detail-title-sort").await;
    seed_router_contract_data(&paths).await;
    seed_router_custom_series(&paths, "series-2", "Alpha Display", "library-1").await;
    update_router_series_metadata_titles(&paths, "series-1", "Zeta Display", "Alpha Sort").await;
    update_router_series_metadata_titles(&paths, "series-2", "Alpha Display", "Zeta Sort").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/libraries/library-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 library detail request should build"),
        )
        .await
        .expect("opds v1 library detail request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    let series_1_pos = body
        .find("/opds/v1.2/series/series-1")
        .expect("series-1 entry should be present");
    let series_2_pos = body
        .find("/opds/v1.2/series/series-2")
        .expect("series-2 entry should be present");
    assert!(
        series_1_pos < series_2_pos,
        "OPDS v1 library detail should order by Kotlin titleSort semantics, body={body}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_library_detail_hides_age_restricted_series() {
    let paths = new_router_fixture("router-opds-v1-library-detail-age-restricted").await;
    seed_router_contract_data(&paths).await;
    seed_router_custom_series(&paths, "series-0", "Visible Series", "library-1").await;
    update_router_series_age_rating(&paths, "series-0", 0).await;
    seed_router_age_exclude_user(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        12,
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
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
                .uri("/opds/v1.2/libraries/library-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 restricted library detail request should build"),
        )
        .await
        .expect("opds v1 restricted library detail request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(!body.contains("/opds/v1.2/series/series-1"));
    assert!(body.contains("/opds/v1.2/series/series-0"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_library_detail_paginates_after_restrictions_filtering() {
    let paths = new_router_fixture("router-opds-v1-library-detail-filtered-pagination").await;
    seed_router_contract_data(&paths).await;
    seed_router_custom_series(&paths, "series-0", "Visible Series", "library-1").await;
    seed_router_custom_series(&paths, "series-2", "Restricted Series 2", "library-1").await;
    update_router_series_metadata_titles(&paths, "series-2", "Restricted Series 2", "Alpha Sort")
        .await;
    update_router_series_metadata_titles(&paths, "series-1", "Restricted Series 1", "Beta Sort")
        .await;
    update_router_series_metadata_titles(&paths, "series-0", "Visible Series", "Gamma Sort").await;
    update_router_series_age_rating(&paths, "series-2", 18).await;
    update_router_series_age_rating(&paths, "series-1", 18).await;
    update_router_series_age_rating(&paths, "series-0", 0).await;
    seed_router_age_exclude_user(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        12,
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
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
                .uri("/opds/v1.2/libraries/library-1?page=0&size=1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 restricted library detail paged request should build"),
        )
        .await
        .expect("opds v1 restricted library detail paged request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains("/opds/v1.2/series/series-0"));
    assert!(!body.contains("/opds/v1.2/series/series-1"));
    assert!(!body.contains("/opds/v1.2/series/series-2"));
    assert!(
        !body.contains("rel=\"next\""),
        "OPDS v1 library detail must paginate after restrictions filtering, body={body}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_publishers_preserves_unicode_collation_order() {
    let paths = new_router_fixture("router-opds-v1-publishers-unicode-order").await;
    seed_router_contract_data(&paths).await;
    update_router_series_publisher(&paths, "series-1", "Zulu House").await;
    seed_router_custom_series(&paths, "series-ang", "Series Å", "library-1").await;
    update_router_series_publisher(&paths, "series-ang", "Ångström Press").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/publishers")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 publishers request should build"),
        )
        .await
        .expect("opds v1 publishers request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    let angstrom_pos = body
        .find("publisher:%C3%85ngstr%C3%B6m%20Press")
        .expect("Ångström publisher entry should be present");
    let zulu_pos = body
        .find("publisher:Zulu%20House")
        .expect("Zulu publisher entry should be present");
    assert!(
        angstrom_pos < zulu_pos,
        "OPDS v1 publishers should keep Kotlin Unicode collation order, body={body}"
    );

    cleanup_router_fixture(paths);
}
