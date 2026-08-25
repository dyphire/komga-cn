use super::*;

async fn soft_delete_series(paths: &RuntimeDbPaths, series_ids: &[&str]) {
    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("series deleted fixture db should open");

    for series_id in series_ids {
        sqlx::query("UPDATE SERIES SET DELETED_DATE = ? WHERE ID = ?")
            .bind("2025-01-01 00:00:00")
            .bind(series_id)
            .execute(&pool)
            .await
            .expect("series deleted date should update");
    }

    pool.close().await;
}

async fn update_series_metadata_title(paths: &RuntimeDbPaths, series_id: &str, title: &str) {
    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("series metadata title fixture db should open");

    sqlx::query("UPDATE SERIES_METADATA SET TITLE = ?, TITLE_SORT = ? WHERE SERIES_ID = ?")
        .bind(title)
        .bind(title)
        .bind(series_id)
        .execute(&pool)
        .await
        .expect("series metadata title should update");

    pool.close().await;
}

async fn update_collection_series_number(
    paths: &RuntimeDbPaths,
    collection_id: &str,
    series_id: &str,
    number: i64,
) {
    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("collection number fixture db should open");

    sqlx::query(
        "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?) \
         ON CONFLICT (COLLECTION_ID, SERIES_ID) DO UPDATE SET NUMBER = excluded.NUMBER",
    )
    .bind(collection_id)
    .bind(series_id)
    .bind(number)
    .execute(&pool)
    .await
    .expect("collection membership number should upsert");

    pool.close().await;
}

async fn update_series_read_date(paths: &RuntimeDbPaths, series_id: &str, read_date: &str) {
    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("series read date fixture db should open");

    sqlx::query(
        "INSERT INTO READ_PROGRESS_SERIES (SERIES_ID, USER_ID, READ_COUNT, IN_PROGRESS_COUNT, MOST_RECENT_READ_DATE) \
         VALUES (?, ?, ?, ?, ?) \
         ON CONFLICT (SERIES_ID, USER_ID) DO UPDATE \
         SET MOST_RECENT_READ_DATE = excluded.MOST_RECENT_READ_DATE, \
             READ_COUNT = excluded.READ_COUNT, \
             IN_PROGRESS_COUNT = excluded.IN_PROGRESS_COUNT",
    )
    .bind(series_id)
    .bind("admin-user")
    .bind(1_i64)
    .bind(0_i64)
    .bind(read_date)
    .execute(&pool)
    .await
    .expect("series read date should upsert");

    pool.close().await;
}

fn series_page_ids(payload: &Value) -> Vec<String> {
    payload
        .get("content")
        .and_then(Value::as_array)
        .expect("series page payload should expose content array")
        .iter()
        .filter_map(|entry| entry.get("id").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

fn series_page_names(payload: &Value) -> Vec<String> {
    payload
        .get("content")
        .and_then(Value::as_array)
        .expect("series page payload should expose content array")
        .iter()
        .filter_map(|entry| entry.get("name").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

fn series_page_sorted(payload: &Value) -> bool {
    payload
        .get("sort")
        .and_then(|value| value.get("sorted"))
        .and_then(Value::as_bool)
        .expect("series page payload should expose sort.sorted")
}

fn book_page_ids(payload: &Value) -> Vec<String> {
    payload
        .get("content")
        .and_then(Value::as_array)
        .expect("book page payload should expose content array")
        .iter()
        .filter_map(|entry| entry.get("id").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

struct LegacySeriesFixture<'a> {
    series_id: &'a str,
    title: &'a str,
    age_rating: Option<i64>,
    release_date: Option<&'a str>,
    sharing_label: Option<&'a str>,
    author: Option<(&'a str, &'a str)>,
    collection_id: Option<&'a str>,
}

async fn seed_legacy_series_fixture(paths: &RuntimeDbPaths, fixture: LegacySeriesFixture<'_>) {
    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("legacy series fixture db should open");

    let book_id = format!("book-for-{}", fixture.series_id);
    sqlx::query(
        "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID, BOOK_COUNT) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(fixture.series_id)
    .bind(0_i64)
    .bind(fixture.title)
    .bind(format!("series/{}", fixture.series_id))
    .bind("library-1")
    .bind(1_i64)
    .execute(&pool)
    .await
    .expect("legacy series fixture row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, PUBLISHER, LANGUAGE, AGE_RATING, TOTAL_BOOK_COUNT, SERIES_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("ONGOING")
    .bind(fixture.title)
    .bind(fixture.title)
    .bind("PubHouse")
    .bind("EN")
    .bind(fixture.age_rating)
    .bind(Some(1_i64))
    .bind(fixture.series_id)
    .execute(&pool)
    .await
    .expect("legacy series fixture metadata should be inserted");

    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&book_id)
    .bind(0_i64)
    .bind(format!("{book_id}.epub"))
    .bind(format!("books/{book_id}.epub"))
    .bind(fixture.series_id)
    .bind(1_024_i64)
    .bind(1_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("legacy series fixture book should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("1")
    .bind(1.0_f64)
    .bind(fixture.title)
    .bind(fixture.release_date)
    .bind(&book_id)
    .execute(&pool)
    .await
    .expect("legacy series fixture book metadata should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA_AGGREGATION (RELEASE_DATE, SUMMARY, SUMMARY_NUMBER, SERIES_ID) \
         VALUES (?, ?, ?, ?)",
    )
    .bind(fixture.release_date)
    .bind("")
    .bind("")
    .bind(fixture.series_id)
    .execute(&pool)
    .await
    .expect("legacy series fixture aggregation should be inserted");

    if let Some(label) = fixture.sharing_label {
        sqlx::query("INSERT INTO SERIES_METADATA_SHARING (SERIES_ID, LABEL) VALUES (?, ?)")
            .bind(fixture.series_id)
            .bind(label)
            .execute(&pool)
            .await
            .expect("legacy series fixture sharing label should be inserted");
    }

    if let Some((name, role)) = fixture.author {
        sqlx::query(
            "INSERT INTO BOOK_METADATA_AGGREGATION_AUTHOR (SERIES_ID, NAME, ROLE) VALUES (?, ?, ?)",
        )
        .bind(fixture.series_id)
        .bind(name)
        .bind(role)
        .execute(&pool)
        .await
        .expect("legacy series fixture author should be inserted");
    }

    if let Some(collection_id) = fixture.collection_id {
        sqlx::query(
            "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?)",
        )
        .bind(collection_id)
        .bind(fixture.series_id)
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("legacy series fixture collection membership should be inserted");
    }

    pool.close().await;
}

async fn seed_series_read_progress_for(
    paths: &RuntimeDbPaths,
    series_id: &str,
    read_count: i64,
    in_progress_count: i64,
) {
    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("legacy series read-progress db should open");

    sqlx::query(
        "INSERT INTO READ_PROGRESS_SERIES (SERIES_ID, USER_ID, READ_COUNT, IN_PROGRESS_COUNT) \
         VALUES (?, ?, ?, ?) \
         ON CONFLICT (SERIES_ID, USER_ID) DO UPDATE \
         SET READ_COUNT = excluded.READ_COUNT, IN_PROGRESS_COUNT = excluded.IN_PROGRESS_COUNT",
    )
    .bind(series_id)
    .bind("admin-user")
    .bind(read_count)
    .bind(in_progress_count)
    .execute(&pool)
    .await
    .expect("legacy series read-progress should be upserted");

    pool.close().await;
}

async fn legacy_series_get_ids(app: &axum::Router, auth_token: &str, uri: &str) -> Vec<String> {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(uri)
                .header("x-auth-token", auth_token)
                .body(Body::empty())
                .expect("legacy series GET request should build"),
        )
        .await
        .expect("legacy series GET request should complete");

    let status = response.status();
    let payload = response_json(response).await;
    assert_eq!(status, StatusCode::OK, "uri: {uri}, payload: {payload}");
    series_page_ids(&payload)
}

#[tokio::test]
async fn router_discovery_series_get_routes_match_paperback_compatibility_shape() {
    let ctx = TestFixture::builder("router-discovery-series-papperback-get-compat")
        .with_search_index()
        .build()
        .await;
    let authorization =
        basic_authorization_header_value("admin@example.org", "router-contract-admin-123");

    let search_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series?page=0&size=20&search=Series%201&tag=Favorite&genre=SciFi")
                .header(header::AUTHORIZATION, authorization.as_str())
                .body(Body::empty())
                .expect("deprecated series GET request should build"),
        )
        .await
        .expect("deprecated series GET request should complete");

    let search_status = search_response.status();
    let search_payload = response_json(search_response).await;
    assert_eq!(search_status, StatusCode::OK, "payload: {search_payload}");
    assert_eq!(series_page_ids(&search_payload), vec!["series-1"]);

    let detail_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/series-1/")
                .header(header::AUTHORIZATION, authorization.as_str())
                .body(Body::empty())
                .expect("series detail trailing-slash request should build"),
        )
        .await
        .expect("series detail trailing-slash request should complete");

    assert_eq!(detail_response.status(), StatusCode::OK);
    let detail_payload = response_json(detail_response).await;
    assert_eq!(
        detail_payload.get("id"),
        Some(&Value::String("series-1".to_string()))
    );
}

#[tokio::test]
async fn router_discovery_series_get_supports_kotlin_legacy_filters_and_regex() {
    let ctx = TestFixture::builder("router-discovery-series-get-kotlin-legacy-filters")
        .with_seed(|paths| async move {
            seed_router_series_counts(&paths, 1, Some(1)).await;
            seed_router_series_read_progress(&paths, 1, 0).await;
        })
        .build()
        .await;
    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app().clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series?page=0&size=20&collection_id=collection-1&status=ONGOING&read_status=READ&publisher=PubHouse&language=EN&age_rating=16&release_year=2024&sharing_label=Family&complete=true&author=John+Doe,writer&search_regex=%5Eseries,title_sort")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("legacy series GET filter request should build"),
        )
        .await
        .expect("legacy series GET filter request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(series_page_ids(&payload), vec!["series-1"]);
}

#[tokio::test]
async fn router_discovery_series_get_keeps_collection_filter_when_combined_with_other_filters() {
    let ctx = TestFixture::builder("router-discovery-series-get-collection-filter-retained")
        .with_seed(|paths| async move {
            seed_router_series_counts(&paths, 1, Some(1)).await;
            seed_router_series_read_progress(&paths, 1, 0).await;
            seed_legacy_series_fixture(
                &paths,
                LegacySeriesFixture {
                    series_id: "series-2",
                    title: "Series Outside Collection",
                    age_rating: Some(16),
                    release_date: Some("2024-02-01"),
                    sharing_label: Some("Family"),
                    author: Some(("John Doe", "writer")),
                    collection_id: None,
                },
            )
            .await;
            seed_series_read_progress_for(&paths, "series-2", 1, 0).await;
        })
        .build()
        .await;
    let auth_token = ctx.login_admin().await;

    let ids = legacy_series_get_ids(
        ctx.app(),
        &auth_token,
        "/api/v1/series?page=0&size=20&collection_id=collection-1&read_status=READ&sharing_label=Family&author=John+Doe,writer",
    )
    .await;

    assert_eq!(ids, vec!["series-1"]);
}

#[tokio::test]
async fn router_discovery_series_get_treats_release_year_values_as_or() {
    let ctx = TestFixture::builder("router-discovery-series-get-release-year-or")
        .with_seed(|paths| async move {
            seed_legacy_series_fixture(
                &paths,
                LegacySeriesFixture {
                    series_id: "series-2",
                    title: "Alpha 2022",
                    age_rating: Some(16),
                    release_date: Some("2022-06-15"),
                    sharing_label: None,
                    author: None,
                    collection_id: None,
                },
            )
            .await;
            seed_legacy_series_fixture(
                &paths,
                LegacySeriesFixture {
                    series_id: "series-3",
                    title: "Beta 2023",
                    age_rating: Some(16),
                    release_date: Some("2023-07-20"),
                    sharing_label: None,
                    author: None,
                    collection_id: None,
                },
            )
            .await;
        })
        .build()
        .await;
    let auth_token = ctx.login_admin().await;

    let mut ids = legacy_series_get_ids(
        ctx.app(),
        &auth_token,
        "/api/v1/series?page=0&size=20&release_year=2022&release_year=2024&sort=metadata.titleSort,asc",
    )
    .await;
    ids.sort();

    assert_eq!(ids, vec!["series-1", "series-2"]);
}

#[tokio::test]
async fn router_discovery_series_get_matches_sharing_label_and_author_exactly() {
    let ctx = TestFixture::builder("router-discovery-series-get-sharing-author-exact")
        .with_seed(|paths| async move {
            seed_legacy_series_fixture(
                &paths,
                LegacySeriesFixture {
                    series_id: "series-2",
                    title: "Near Match Series",
                    age_rating: Some(16),
                    release_date: Some("2024-03-01"),
                    sharing_label: Some("Family Friendly"),
                    author: Some(("John Doe Jr", "writer")),
                    collection_id: None,
                },
            )
            .await;
        })
        .build()
        .await;
    let auth_token = ctx.login_admin().await;

    let ids = legacy_series_get_ids(
        ctx.app(),
        &auth_token,
        "/api/v1/series?page=0&size=20&sharing_label=Family&author=John+Doe",
    )
    .await;

    assert_eq!(ids, vec!["series-1"]);
}

#[tokio::test]
async fn router_discovery_series_get_only_applies_author_filter_when_query_contains_name_and_role()
{
    let ctx = TestFixture::builder("router-discovery-series-get-author-delimiter-semantics")
        .with_seed(|paths| async move {
            seed_legacy_series_fixture(
                &paths,
                LegacySeriesFixture {
                    series_id: "series-2",
                    title: "Second Author Series",
                    age_rating: Some(16),
                    release_date: Some("2024-03-02"),
                    sharing_label: None,
                    author: Some(("Jane Roe", "writer")),
                    collection_id: None,
                },
            )
            .await;
        })
        .build()
        .await;
    let auth_token = ctx.login_admin().await;

    let ignored_author_ids = legacy_series_get_ids(
        ctx.app(),
        &auth_token,
        "/api/v1/series?page=0&size=20&author=John+Doe",
    )
    .await;
    assert_eq!(ignored_author_ids, vec!["series-1", "series-2"]);

    let empty_name_ids = legacy_series_get_ids(
        ctx.app(),
        &auth_token,
        "/api/v1/series?page=0&size=20&author=%2Cwriter",
    )
    .await;
    assert!(empty_name_ids.is_empty());
}

#[tokio::test]
async fn router_discovery_series_get_supports_legacy_age_rating_numeric_or_null_values() {
    let ctx = TestFixture::builder("router-discovery-series-get-age-rating-null-or")
        .with_seed(|paths| async move {
            seed_legacy_series_fixture(
                &paths,
                LegacySeriesFixture {
                    series_id: "series-2",
                    title: "Null Age Series",
                    age_rating: None,
                    release_date: Some("2024-04-01"),
                    sharing_label: None,
                    author: None,
                    collection_id: None,
                },
            )
            .await;
            seed_legacy_series_fixture(
                &paths,
                LegacySeriesFixture {
                    series_id: "series-3",
                    title: "Adult Age Series",
                    age_rating: Some(18),
                    release_date: Some("2024-05-01"),
                    sharing_label: None,
                    author: None,
                    collection_id: None,
                },
            )
            .await;
        })
        .build()
        .await;
    let auth_token = ctx.login_admin().await;

    let mut ids = legacy_series_get_ids(
        ctx.app(),
        &auth_token,
        "/api/v1/series?page=0&size=20&age_rating=16&age_rating=bad-value&sort=metadata.titleSort,asc",
    )
    .await;
    ids.sort();

    assert_eq!(ids, vec!["series-1", "series-2"]);
}

#[tokio::test]
async fn router_discovery_series_get_keeps_kotlin_unsorted_default_when_no_sort_or_search() {
    let ctx = TestFixture::builder("router-discovery-series-get-unsorted-default")
        .with_seed(|paths| async move {
            seed_legacy_series_fixture(
                &paths,
                LegacySeriesFixture {
                    series_id: "series-2",
                    title: "A Sorted First Title",
                    age_rating: Some(16),
                    release_date: Some("2024-06-01"),
                    sharing_label: None,
                    author: None,
                    collection_id: None,
                },
            )
            .await;
        })
        .build()
        .await;
    let auth_token = ctx.login_admin().await;

    let ids = legacy_series_get_ids(ctx.app(), &auth_token, "/api/v1/series?page=0&size=20").await;

    assert_eq!(ids, vec!["series-1", "series-2"]);
}

#[tokio::test]
async fn router_discovery_series_get_sorts_by_series_name_and_returns_name_field() {
    let ctx = TestFixture::builder("router-discovery-series-get-sort-by-name")
        .with_seed(|paths| async move {
            seed_legacy_series_fixture(
                &paths,
                LegacySeriesFixture {
                    series_id: "series-2",
                    title: "Alpha Shelf Name",
                    age_rating: Some(16),
                    release_date: Some("2024-06-02"),
                    sharing_label: None,
                    author: None,
                    collection_id: None,
                },
            )
            .await;
            update_series_metadata_title(&paths, "series-2", "Zeta Display Title").await;
        })
        .build()
        .await;
    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series?page=0&size=20&sort=name,asc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("legacy series GET sort=name request should build"),
        )
        .await
        .expect("legacy series GET sort=name request should complete");

    let status = response.status();
    let payload = response_json(response).await;
    assert_eq!(status, StatusCode::OK, "payload: {payload}");
    assert_eq!(series_page_ids(&payload), vec!["series-2", "series-1"]);
    assert_eq!(
        series_page_names(&payload),
        vec!["Alpha Shelf Name", "Series 1"]
    );
}

#[tokio::test]
async fn router_discovery_series_get_sorts_by_collection_number_when_requested() {
    let ctx = TestFixture::builder("router-discovery-series-get-sort-by-collection-number")
        .with_seed(|paths| async move {
            seed_legacy_series_fixture(
                &paths,
                LegacySeriesFixture {
                    series_id: "series-2",
                    title: "Collection Number Two",
                    age_rating: Some(16),
                    release_date: Some("2024-06-03"),
                    sharing_label: None,
                    author: None,
                    collection_id: Some("collection-1"),
                },
            )
            .await;
            update_collection_series_number(&paths, "collection-1", "series-2", 5).await;
        })
        .build()
        .await;
    let auth_token = ctx.login_admin().await;

    let ids = legacy_series_get_ids(
        ctx.app(),
        &auth_token,
        "/api/v1/series?page=0&size=20&collection_id=collection-1&sort=collection.number,desc",
    )
    .await;

    assert_eq!(ids, vec!["series-2", "series-1"]);
}

#[tokio::test]
async fn router_discovery_series_get_treats_collection_number_sort_as_unsorted_without_collection_filter()
 {
    let ctx = TestFixture::builder("router-discovery-series-get-collection-number-without-filter")
        .with_seed(|paths| async move {
            seed_legacy_series_fixture(
                &paths,
                LegacySeriesFixture {
                    series_id: "series-2",
                    title: "Collection Number Detached",
                    age_rating: Some(16),
                    release_date: Some("2024-06-03"),
                    sharing_label: None,
                    author: None,
                    collection_id: Some("collection-1"),
                },
            )
            .await;
            update_collection_series_number(&paths, "collection-1", "series-2", 5).await;
        })
        .build()
        .await;
    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series?page=0&size=20&sort=collection.number,desc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("legacy series GET collection.number without filter request should build"),
        )
        .await
        .expect("legacy series GET collection.number without filter request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert!(!series_page_sorted(&payload));
}

#[tokio::test]
async fn router_discovery_series_get_sorts_by_read_date_when_requested() {
    let ctx = TestFixture::builder("router-discovery-series-get-sort-by-read-date")
        .with_seed(|paths| async move {
            seed_legacy_series_fixture(
                &paths,
                LegacySeriesFixture {
                    series_id: "series-2",
                    title: "Read Later Series",
                    age_rating: Some(16),
                    release_date: Some("2024-06-04"),
                    sharing_label: None,
                    author: None,
                    collection_id: None,
                },
            )
            .await;
            update_series_read_date(&paths, "series-1", "2024-06-10T00:00:00Z").await;
            update_series_read_date(&paths, "series-2", "2024-06-11T00:00:00Z").await;
        })
        .build()
        .await;
    let auth_token = ctx.login_admin().await;

    let ids = legacy_series_get_ids(
        ctx.app(),
        &auth_token,
        "/api/v1/series?page=0&size=20&sort=readDate,desc",
    )
    .await;

    assert_eq!(ids, vec!["series-2", "series-1"]);
}

#[tokio::test]
async fn router_discovery_series_get_does_not_inject_relevance_for_unsupported_explicit_sort() {
    let ctx = TestFixture::builder("router-discovery-series-get-unsupported-sort-with-search")
        .with_seed(|paths| async move {
            seed_legacy_series_fixture(
                &paths,
                LegacySeriesFixture {
                    series_id: "series-2",
                    title: "Series 1 Companion",
                    age_rating: Some(16),
                    release_date: Some("2024-06-05"),
                    sharing_label: None,
                    author: None,
                    collection_id: None,
                },
            )
            .await;
        })
        .build()
        .await;
    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series?page=0&size=20&search=Series%201&sort=unsupported.sort,asc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("legacy series GET unsupported sort with search request should build"),
        )
        .await
        .expect("legacy series GET unsupported sort with search request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert!(!series_page_sorted(&payload));
}

#[tokio::test]
async fn router_discovery_series_list_includes_soft_deleted_series_by_default() {
    let ctx = TestFixture::builder("router-discovery-series-list-default-deleted-visible")
        .with_seed(|paths| async move {
            seed_router_custom_series(&paths, "series-deleted", "Deleted Series", "library-1")
                .await;
            soft_delete_series(&paths, &["series-deleted"]).await;
        })
        .build()
        .await;
    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "LibraryId",
                            "operator": "is",
                            "value": "library-1"
                        }
                    })
                    .to_string(),
                ))
                .expect("default series/list request should build"),
        )
        .await
        .expect("default series/list request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let mut ids = series_page_ids(&payload);
    ids.sort();
    assert_eq!(ids, vec!["series-1", "series-deleted"]);
}

#[tokio::test]
async fn router_discovery_series_list_supports_deleted_filter_in_runtime_owned_mode() {
    let ctx = TestFixture::builder("router-discovery-series-list-strict-deleted")
        .with_seed(|paths| async move {
            seed_router_custom_series(&paths, "series-deleted", "Deleted Series", "library-1")
                .await;
            soft_delete_series(&paths, &["series-deleted"]).await;
        })
        .build()
        .await;
    let auth_token = ctx.login_admin().await;

    let not_deleted_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "Deleted",
                            "operator": "isFalse"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list deleted=false request should build"),
        )
        .await
        .expect("strict series/list deleted=false request should complete");
    assert_eq!(not_deleted_response.status(), StatusCode::OK);
    let not_deleted_payload = response_json(not_deleted_response).await;
    assert_eq!(series_page_ids(&not_deleted_payload), vec!["series-1"]);

    let deleted_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "Deleted",
                            "operator": "isTrue"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list deleted=true request should build"),
        )
        .await
        .expect("strict series/list deleted=true request should complete");
    assert_eq!(deleted_response.status(), StatusCode::OK);
    let deleted_payload = response_json(deleted_response).await;
    assert_eq!(series_page_ids(&deleted_payload), vec!["series-deleted"]);
}

#[tokio::test]
async fn router_discovery_series_list_deleted_filter_handles_deleted_only_library() {
    let ctx = TestFixture::builder("router-discovery-series-list-runtime-only-deleted-visible")
        .with_seed(|paths| async move {
            soft_delete_series(&paths, &["series-1"]).await;
        })
        .build()
        .await;
    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "Deleted",
                            "operator": "isTrue"
                        }
                    })
                    .to_string(),
                ))
                .expect("runtime-owned only deleted series/list request should build"),
        )
        .await
        .expect("runtime-owned only deleted series/list request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(series_page_ids(&payload), vec!["series-1"]);
}

#[tokio::test]
async fn router_discovery_deprecated_series_v1_alphabetical_groups_route_returns_groups() {
    let ctx = TestFixture::new("router-discovery-deprecated-v1-series-alphabetical-groups").await;
    let auth_token = ctx.login_admin().await;

    let route = "/api/v1/series/alphabetical-groups?page=0&size=20";
    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(route)
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("deprecated series v1 alphabetical-groups request should build"),
        )
        .await
        .expect("deprecated series v1 alphabetical-groups request should complete");

    assert_eq!(response.status(), StatusCode::OK, "route: {route}");
    let payload = response_json(response).await;
    assert!(
        payload.as_array().is_some_and(|groups| !groups.is_empty()),
        "deprecated alphabetical-groups route should return groups: {payload}"
    );
}

#[tokio::test]
async fn router_discovery_series_books_route_bridges_deprecated_numeric_series_id() {
    let ctx = TestFixture::builder("router-discovery-deprecated-series-books-id-bridge")
        .with_seed(|paths| async move {
            seed_router_custom_series(&paths, "custom-series-2", "Series 2", "library-1").await;

            let pool = connect_test_pool(paths.main_db.as_path(), 1)
                .await
                .expect("deprecated series books bridge db should open");
            sqlx::query("UPDATE BOOK SET SERIES_ID = ? WHERE ID = ?")
                .bind("custom-series-2")
                .bind("book-1")
                .execute(&pool)
                .await
                .expect("deprecated series books bridge book should move");
            sqlx::query("UPDATE SERIES SET BOOK_COUNT = ? WHERE ID = ?")
                .bind(0_i64)
                .bind("series-1")
                .execute(&pool)
                .await
                .expect("deprecated series books bridge source count should update");
            sqlx::query("UPDATE SERIES SET BOOK_COUNT = ? WHERE ID = ?")
                .bind(1_i64)
                .bind("custom-series-2")
                .execute(&pool)
                .await
                .expect("deprecated series books bridge target count should update");
            pool.close().await;
        })
        .build()
        .await;
    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/series-2/books?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("deprecated series books request should build"),
        )
        .await
        .expect("deprecated series books request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(book_page_ids(&payload), vec!["book-1"]);
}

#[tokio::test]
async fn router_discovery_series_alphabetical_groups_groups_by_title_sort_first_character() {
    let ctx = TestFixture::builder("router-discovery-series-alphabetical-groups-title-sort")
        .with_seed(|paths| async move {
            seed_router_custom_series(&paths, "series-2", "Series 2", "library-1").await;
            seed_router_custom_series(&paths, "series-3", "Series 3", "library-1").await;
            seed_router_series_title_sort(&paths, "series-1", "Alpha Shelf").await;
            seed_router_series_title_sort(&paths, "series-2", "Beta Shelf").await;
            seed_router_series_title_sort(&paths, "series-3", "Beta Archive").await;
        })
        .build()
        .await;
    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list/alphabetical-groups")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{}"))
                .expect("alphabetical-groups request should build"),
        )
        .await
        .expect("alphabetical-groups request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let groups = payload
        .as_array()
        .expect("alphabetical-groups payload should be an array")
        .iter()
        .map(|entry| {
            (
                entry
                    .get("group")
                    .and_then(Value::as_str)
                    .expect("group entry should expose group")
                    .to_string(),
                entry
                    .get("count")
                    .and_then(Value::as_i64)
                    .expect("group entry should expose count"),
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(groups, vec![("a".to_string(), 1), ("b".to_string(), 2)]);
}

#[tokio::test]
async fn router_discovery_series_alphabetical_groups_rejects_unknown_condition_type() {
    let ctx =
        TestFixture::new("router-discovery-series-alphabetical-groups-unknown-condition").await;
    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list/alphabetical-groups")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "UnknownSeriesCondition",
                            "operator": "isTrue"
                        }
                    })
                    .to_string(),
                ))
                .expect("invalid alphabetical-groups request should build"),
        )
        .await
        .expect("invalid alphabetical-groups request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(
        payload,
        json!({
            "error": "invalid series alphabetical-groups request: InvalidSemantics(\"unsupported series condition type: UnknownSeriesCondition\")"
        })
    );
}

#[tokio::test]
async fn router_discovery_series_alphabetical_groups_rejects_empty_untyped_condition() {
    let ctx = TestFixture::new("router-discovery-series-alphabetical-groups-empty-condition").await;
    let auth_token = ctx.login_admin().await;

    for (case, body) in [
        ("empty-condition", json!({ "condition": {} })),
        (
            "unknown-webui-leaf",
            json!({
                "condition": {
                    "unknownField": {
                        "operator": "isTrue"
                    }
                }
            }),
        ),
    ] {
        let response = ctx
            .app()
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/series/list/alphabetical-groups")
                    .header("x-auth-token", &auth_token)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body.to_string()))
                    .expect("alphabetical-groups invalid condition request should build"),
            )
            .await
            .expect("alphabetical-groups invalid condition request should complete");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "case: {case}");
        let payload = response_json(response).await;
        let error = payload
            .get("error")
            .and_then(Value::as_str)
            .expect("invalid condition response should expose error string");
        assert!(
            error.starts_with("invalid series alphabetical-groups request:"),
            "case: {case}"
        );
    }
}

#[tokio::test]
async fn router_discovery_series_alphabetical_groups_rejects_non_object_bodies() {
    let ctx = TestFixture::new("router-discovery-series-alphabetical-groups-non-object-body").await;
    let auth_token = ctx.login_admin().await;

    for (case, body) in [("array", Body::from("[]")), ("null", Body::from("null"))] {
        let response = ctx
            .app()
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/series/list/alphabetical-groups")
                    .header("x-auth-token", &auth_token)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(body)
                    .expect("non-object alphabetical-groups request should build"),
            )
            .await
            .expect("non-object alphabetical-groups request should complete");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "case: {case}");
    }
}

#[tokio::test]
async fn router_discovery_series_list_supports_oneshot_filter_in_runtime_owned_mode() {
    let ctx = TestFixture::new("router-discovery-series-list-strict-oneshot").await;
    let auth_token = ctx.login_admin().await;

    let not_oneshot_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "OneShot",
                            "operator": "isFalse"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list oneshot=false request should build"),
        )
        .await
        .expect("strict series/list oneshot=false request should complete");
    assert_eq!(not_oneshot_response.status(), StatusCode::OK);
    let not_oneshot_payload = response_json(not_oneshot_response).await;
    let not_oneshot_content = not_oneshot_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series oneshot=false payload should expose content array");
    assert_eq!(not_oneshot_content.len(), 1);

    let oneshot_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "OneShot",
                            "operator": "isTrue"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list oneshot=true request should build"),
        )
        .await
        .expect("strict series/list oneshot=true request should complete");
    assert_eq!(oneshot_response.status(), StatusCode::OK);
    let oneshot_payload = response_json(oneshot_response).await;
    let oneshot_content = oneshot_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series oneshot=true payload should expose content array");
    assert_eq!(oneshot_content.len(), 0);
}

#[tokio::test]
async fn router_discovery_series_list_rejects_unknown_condition_type_in_runtime_owned_mode() {
    let ctx = TestFixture::new("router-discovery-series-list-strict-unknown-condition").await;
    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "UnknownSeriesCondition",
                            "operator": "is",
                            "value": "whatever"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list unknown-condition request should build"),
        )
        .await
        .expect("strict series/list unknown-condition request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn router_discovery_series_list_rejects_unknown_operator_in_runtime_owned_mode() {
    let ctx = TestFixture::new("router-discovery-series-list-strict-unknown-operator").await;
    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "Deleted",
                            "operator": "maybe"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list unknown-operator request should build"),
        )
        .await
        .expect("strict series/list unknown-operator request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn router_discovery_series_list_applies_default_sort_for_unknown_sort_mode_in_runtime_owned_mode()
 {
    let ctx = TestFixture::new("router-discovery-series-list-strict-sort-modes").await;
    let auth_token = ctx.login_admin().await;

    for sort in [
        "metadata.titleSort,asc",
        "createdDate,desc",
        "created,desc",
        "lastModifiedDate,desc",
        "lastModified,desc",
        "booksMetadata.releaseDate,desc",
        "booksCount,desc",
    ] {
        let response = ctx
            .app()
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/series/list?page=0&size=20&sort={sort}"))
                    .header("x-auth-token", &auth_token)
                    .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "condition": {
                                "type": "LibraryId",
                                "operator": "is",
                                "value": "library-1"
                            }
                        })
                        .to_string(),
                    ))
                    .expect("strict series/list supported sort request should build"),
            )
            .await
            .expect("strict series/list supported sort request should complete");
        assert_eq!(response.status(), StatusCode::OK);
    }

    let unsupported_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20&sort=unsupported.sort,asc")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "LibraryId",
                            "operator": "is",
                            "value": "library-1"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list unsupported sort request should build"),
        )
        .await
        .expect("strict series/list unsupported sort request should complete");
    assert_eq!(unsupported_response.status(), StatusCode::OK);
    let unsupported_payload = response_json(unsupported_response).await;
    let unsupported_content = unsupported_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series unsupported sort payload should expose content array");
    assert_eq!(unsupported_content.len(), 1);
}

#[tokio::test]
async fn router_discovery_series_list_sorts_runtime_owned_results_by_release_date_books_count_and_alias_dates()
 {
    let ctx = TestFixture::builder("router-discovery-series-list-runtime-sort-order")
        .with_seed(|paths| async move {
            seed_router_custom_series(&paths, "series-2", "Series 2", "library-1").await;

            let pool = connect_test_pool(paths.main_db.as_path(), 1)
                .await
                .expect("series runtime sort db should open");
            sqlx::query("UPDATE SERIES_METADATA SET TITLE_SORT = ? WHERE SERIES_ID = ?")
                .bind("Zulu Series")
                .bind("series-1")
                .execute(&pool)
                .await
                .expect("series-1 title sort should update");
            sqlx::query("UPDATE SERIES_METADATA SET TITLE_SORT = ? WHERE SERIES_ID = ?")
                .bind("Alpha Series")
                .bind("series-2")
                .execute(&pool)
                .await
                .expect("series-2 title sort should update");
            for (series_id, created, last_modified, book_count) in [
                (
                    "series-1",
                    "2024-01-01 00:00:00",
                    "2024-01-10 00:00:00",
                    1_i64,
                ),
                (
                    "series-2",
                    "2024-02-01 00:00:00",
                    "2024-02-10 00:00:00",
                    3_i64,
                ),
            ] {
                sqlx::query(
                    "UPDATE SERIES \
                     SET CREATED_DATE = ?, LAST_MODIFIED_DATE = ?, BOOK_COUNT = ? \
                     WHERE ID = ?",
                )
                .bind(created)
                .bind(last_modified)
                .bind(book_count)
                .bind(series_id)
                .execute(&pool)
                .await
                .expect("series runtime sort fixture should update series timestamps and counts");
            }
            sqlx::query(
                "UPDATE BOOK_METADATA_AGGREGATION \
                 SET RELEASE_DATE = ? \
                 WHERE SERIES_ID = ?",
            )
            .bind("2024-01-15")
            .bind("series-1")
            .execute(&pool)
            .await
            .expect("series-1 aggregation release date should update");
            sqlx::query(
                "INSERT INTO BOOK_METADATA_AGGREGATION (RELEASE_DATE, SUMMARY, SUMMARY_NUMBER, SERIES_ID) \
                 VALUES (?, ?, ?, ?)",
            )
            .bind("2024-02-15")
            .bind("")
            .bind("")
            .bind("series-2")
            .execute(&pool)
            .await
            .expect("series-2 aggregation release date should insert");
            pool.close().await;
        })
        .build()
        .await;
    let auth_token = ctx.login_admin().await;

    for (sort, expected_ids) in [
        ("metadata.titleSort,asc", vec!["series-2", "series-1"]),
        ("titleSort,asc", vec!["series-2", "series-1"]),
        ("created,desc", vec!["series-2", "series-1"]),
        ("lastModified,desc", vec!["series-2", "series-1"]),
        (
            "booksMetadata.releaseDate,desc",
            vec!["series-2", "series-1"],
        ),
        ("booksCount,desc", vec!["series-2", "series-1"]),
    ] {
        let response = ctx
            .app()
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/series/list?page=0&size=20&sort={sort}"))
                    .header("x-auth-token", &auth_token)
                    .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "condition": {
                                "type": "LibraryId",
                                "operator": "is",
                                "value": "library-1"
                            }
                        })
                        .to_string(),
                    ))
                    .expect("series runtime sort request should build"),
            )
            .await
            .expect("series runtime sort request should complete");
        assert_eq!(response.status(), StatusCode::OK, "sort: {sort}");
        let payload = response_json(response).await;
        let ids = payload
            .get("content")
            .and_then(Value::as_array)
            .expect("series runtime sort payload should expose content array")
            .iter()
            .filter_map(|entry| entry.get("id").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert_eq!(ids, expected_ids, "sort: {sort}");
    }
}
