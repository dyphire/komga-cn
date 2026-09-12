use super::*;

async fn books_list_response(
    app: &axum::Router,
    auth_token: &str,
    runtime_owned: bool,
    body: Body,
) -> axum::response::Response {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/api/v1/books/list?page=0&size=20")
        .header("x-auth-token", auth_token)
        .header(header::CONTENT_TYPE, "application/json");
    if runtime_owned {
        builder = builder.header("x-komga-runtime-search-ownership", "runtime-rust-owned");
    }

    app.clone()
        .oneshot(builder.body(body).expect("books/list request should build"))
        .await
        .expect("books/list request should complete")
}

fn page_ids(payload: &Value) -> Vec<String> {
    payload
        .get("content")
        .and_then(Value::as_array)
        .expect("book page payload should expose content array")
        .iter()
        .filter_map(|entry| entry.get("id").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

#[tokio::test]
async fn router_discovery_books_get_route_matches_paperback_compatibility_shape() {
    let ctx = TestFixture::builder("router-discovery-books-get-paperback-compat")
        .with_search_index()
        .build()
        .await;

    let authorization =
        basic_authorization_header_value("admin@example.org", "router-contract-admin-123");
    let route = "/api/v1/books?page=0&size=20&search=Book%201&tag=Favorite&media_status=READY&read_status=UNREAD&released_after=2023-01-01&library_id=library-1";
    let body = json!({
        "condition": {
            "type": "AllOfBook",
            "conditions": [
                { "type": "LibraryId", "operator": "is", "value": "library-1" },
                { "type": "Tag", "operator": "is", "value": "Favorite" },
                { "type": "MediaStatus", "operator": "is", "value": "READY" },
                { "type": "ReadStatus", "operator": "is", "value": "UNREAD" },
                { "type": "ReleaseDate", "operator": "after", "dateTime": "2023-01-01" }
            ]
        },
        "fullTextSearch": "Book 1"
    });

    let get_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(route)
                .header(header::AUTHORIZATION, authorization.as_str())
                .header("x-auth-token", "")
                .body(Body::empty())
                .expect("deprecated books GET request should build"),
        )
        .await
        .expect("deprecated books GET request should complete");

    let post_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header(header::AUTHORIZATION, authorization.as_str())
                .header("x-auth-token", "")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("books/list parity request should build"),
        )
        .await
        .expect("books/list parity request should complete");

    assert_eq!(get_response.status(), StatusCode::OK);
    assert_eq!(post_response.status(), StatusCode::OK);

    let get_payload = response_json(get_response).await;
    let post_payload = response_json(post_response).await;
    assert_eq!(page_ids(&get_payload), page_ids(&post_payload));
    assert_eq!(
        get_payload.get("totalElements"),
        post_payload.get("totalElements")
    );
    assert_eq!(get_payload.get("number"), post_payload.get("number"));
    assert_eq!(get_payload.get("size"), post_payload.get("size"));
}

#[tokio::test]
async fn router_discovery_books_list_locks_main_search_parity_for_retained_inputs() {
    let ctx = TestFixture::builder("router-discovery-books-list-main-search-parity")
        .with_search_index()
        .with_seed(|paths| async move {
            seed_router_authors_scope_variants(&paths).await;
            update_book_search_fixture_title(&paths, "book-2", "Book Book 2").await;
        })
        .build()
        .await;

    let admin_token = ctx.login_admin().await;

    let blank_ids = books_list_ids(
        &ctx.app().clone(),
        &admin_token,
        Some("relevance,desc"),
        Some("   "),
    )
    .await;
    assert_eq!(blank_ids, vec!["book-1", "book-2", "book-3"]);

    let relevance_desc_ids = books_list_ids(
        &ctx.app().clone(),
        &admin_token,
        Some("relevance,desc"),
        Some("book"),
    )
    .await;
    assert_eq!(relevance_desc_ids, vec!["book-3", "book-1", "book-2"]);

    let default_relevance_ids =
        books_list_ids(&ctx.app().clone(), &admin_token, None, Some("book")).await;
    assert_eq!(default_relevance_ids, vec!["book-2", "book-1", "book-3"]);

    let relevance_asc_ids = books_list_ids(
        &ctx.app().clone(),
        &admin_token,
        Some("relevance,asc"),
        Some("book"),
    )
    .await;
    assert_eq!(relevance_asc_ids, vec!["book-2", "book-1", "book-3"]);

    let fielded_ids = books_list_ids(
        &ctx.app().clone(),
        &admin_token,
        Some("relevance,desc"),
        Some("title:book"),
    )
    .await;
    assert_eq!(fielded_ids, vec!["book-3", "book-1", "book-2"]);

    let invalid_query_ids = books_list_ids(
        &ctx.app().clone(),
        &admin_token,
        Some("relevance,desc"),
        Some("title:("),
    )
    .await;
    assert!(invalid_query_ids.is_empty());

    seed_router_age_exclude_user_with_roles(
        ctx.paths(),
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        16,
        &["USER", "PAGE_STREAMING"],
    )
    .await;
    let restricted_token = ctx
        .login_with_credentials("restricted@example.org", "router-contract-restricted-123")
        .await;
    let visible_ids = books_list_ids(
        &ctx.app().clone(),
        &restricted_token,
        Some("relevance,desc"),
        Some("book"),
    )
    .await;
    assert_eq!(visible_ids, vec!["book-3"]);
}

#[tokio::test]
async fn router_discovery_books_list_retains_accent_folded_and_cjk_recall() {
    let ctx = TestFixture::builder("router-discovery-books-list-accent-cjk-recall")
        .with_search_index()
        .with_seed(|paths| async move {
            update_book_search_fixture_title(&paths, "book-1", "Café 東京 Book 1").await;
        })
        .build()
        .await;

    let admin_token = ctx.login_admin().await;

    let accent_cjk_ids = books_list_ids(
        &ctx.app().clone(),
        &admin_token,
        Some("relevance,desc"),
        Some("cafe 東京"),
    )
    .await;
    assert_eq!(
        accent_cjk_ids,
        vec!["book-1"],
        "books/list should retain accent-folded mixed CJK recall at the route boundary",
    );
}

#[tokio::test]
async fn router_discovery_books_list_ignores_legacy_regex_search_body_input() {
    let ctx = TestFixture::new("router-discovery-books-list-legacy-regex-search").await;

    let auth_token = ctx.login_admin().await;
    let baseline_ids = books_list_ids(&ctx.app().clone(), &auth_token, None, None).await;

    for legacy_field in ["regexSearch", "searchRegex", "search_regex"] {
        let mut payload = json!({
            "condition": {
                "type": "Title",
                "operator": "contains",
                "value": "book"
            }
        });
        payload[legacy_field] = Value::String("(".to_string());

        let response = books_list_response(
            &ctx.app().clone(),
            &auth_token,
            true,
            Body::from(payload.to_string()),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK, "field={legacy_field}");

        let payload = response_json(response).await;
        let ids = payload
            .get("content")
            .and_then(Value::as_array)
            .expect("books/list payload should expose content array")
            .iter()
            .filter_map(|entry| entry.get("id").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert_eq!(ids, baseline_ids, "field={legacy_field}");
    }
}

#[tokio::test]
async fn router_discovery_books_list_rejects_invalid_request_bodies() {
    let ctx = TestFixture::new("router-discovery-books-list-invalid-bodies").await;
    let auth_token = ctx.login_admin().await;

    for (case, body) in [
        ("empty", Body::empty()),
        ("invalid-json", Body::from("{")),
        ("array-body", Body::from("[]")),
    ] {
        let response = books_list_response(ctx.app(), &auth_token, false, body).await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "case={case}");
    }
}

#[tokio::test]
async fn router_discovery_books_list_blank_full_text_search_does_not_report_relevance_sort() {
    let ctx = TestFixture::new("router-discovery-books-list-blank-search-unsorted-meta").await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20&sort=relevance,desc")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "Title",
                            "operator": "contains",
                            "value": "book"
                        },
                        "fullTextSearch": "   "
                    })
                    .to_string(),
                ))
                .expect("blank-search books/list request should build"),
        )
        .await
        .expect("blank-search books/list request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload.pointer("/sort/sorted"), Some(&json!(false)));
    assert_eq!(payload.pointer("/sort/unsorted"), Some(&json!(true)));
    assert_eq!(
        payload.pointer("/pageable/sort/sorted"),
        Some(&json!(false))
    );
    assert_eq!(
        payload.pointer("/pageable/sort/unsorted"),
        Some(&json!(true))
    );
}

async fn get_books_search_page(
    app: &axum::Router,
    auth_token: &str,
    search: &str,
    library_id: Option<&str>,
    page: usize,
    size: usize,
) -> Value {
    let mut uri = format!("/api/v1/books?page={page}&size={size}&search={search}");
    if let Some(library_id) = library_id {
        uri.push_str("&library_id=");
        uri.push_str(library_id);
    }

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(uri)
                .header("x-auth-token", auth_token)
                .body(Body::empty())
                .expect("library-filtered search request should build"),
        )
        .await
        .expect("library-filtered search request should complete");
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await
}

#[tokio::test]
async fn router_discovery_books_list_library_filtered_pure_search_pages_correctly() {
    let ctx = TestFixture::builder("router-discovery-books-list-library-filtered-fast-path")
        .with_search_index()
        .with_seed(|paths| async move {
            seed_router_authors_scope_variants(&paths).await;
        })
        .build()
        .await;

    let admin_token = ctx.login_admin().await;

    let all_payload =
        get_books_search_page(&ctx.app().clone(), &admin_token, "book", None, 0, 20).await;
    let mut all_ids = page_ids(&all_payload);
    assert_eq!(all_payload.get("totalElements"), Some(&json!(3)));
    all_ids.sort();
    assert_eq!(all_ids, vec!["book-1", "book-2", "book-3"]);

    let lib1_payload = get_books_search_page(
        &ctx.app().clone(),
        &admin_token,
        "book",
        Some("library-1"),
        0,
        20,
    )
    .await;
    let mut lib1_ids = page_ids(&lib1_payload);
    assert_eq!(lib1_payload.get("totalElements"), Some(&json!(2)));
    lib1_ids.sort();
    assert_eq!(lib1_ids, vec!["book-1", "book-2"]);

    let lib2_payload = get_books_search_page(
        &ctx.app().clone(),
        &admin_token,
        "book",
        Some("library-2"),
        0,
        20,
    )
    .await;
    assert_eq!(page_ids(&lib2_payload), vec!["book-3"]);
    assert_eq!(lib2_payload.get("totalElements"), Some(&json!(1)));

    let missing_payload = get_books_search_page(
        &ctx.app().clone(),
        &admin_token,
        "book",
        Some("library-missing"),
        0,
        20,
    )
    .await;
    assert!(page_ids(&missing_payload).is_empty());
    assert_eq!(missing_payload.get("totalElements"), Some(&json!(0)));

    let p0 = get_books_search_page(
        &ctx.app().clone(),
        &admin_token,
        "book",
        Some("library-1"),
        0,
        1,
    )
    .await;
    let p1 = get_books_search_page(
        &ctx.app().clone(),
        &admin_token,
        "book",
        Some("library-1"),
        1,
        1,
    )
    .await;
    let p0_ids = page_ids(&p0);
    let p1_ids = page_ids(&p1);
    assert_eq!(p0_ids.len(), 1);
    assert_eq!(p1_ids.len(), 1);
    assert_ne!(p0_ids[0], p1_ids[0]);
    let mut union = p0_ids.clone();
    union.extend(p1_ids.iter().cloned());
    union.sort();
    assert_eq!(union, vec!["book-1", "book-2"]);
    assert_eq!(p0.get("totalElements"), Some(&json!(2)));
    assert_eq!(p1.get("totalElements"), Some(&json!(2)));
}

#[tokio::test]
async fn router_discovery_books_list_marks_unsorted_page_shape_without_full_text_search() {
    let ctx = TestFixture::new("router-discovery-books-list-unsorted-page-shape").await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "Title",
                            "operator": "contains",
                            "value": "book"
                        }
                    })
                    .to_string(),
                ))
                .expect("unsorted books/list request should build"),
        )
        .await
        .expect("unsorted books/list request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload.pointer("/sort/sorted"), Some(&json!(false)));
    assert_eq!(payload.pointer("/sort/unsorted"), Some(&json!(true)));
    assert_eq!(
        payload.pointer("/pageable/sort/sorted"),
        Some(&json!(false))
    );
    assert_eq!(
        payload.pointer("/pageable/sort/unsorted"),
        Some(&json!(true))
    );
}

async fn search_with_condition_ids(
    app: &axum::Router,
    auth_token: &str,
    sort: Option<&str>,
    full_text_search: Option<&str>,
    condition: Value,
) -> Vec<String> {
    let mut uri = String::from("/api/v1/books/list?page=0&size=20");
    if let Some(sort) = sort {
        uri.push_str("&sort=");
        uri.push_str(sort);
    }
    let mut payload = json!({
        "condition": condition,
    });
    if let Some(search) = full_text_search {
        payload["fullTextSearch"] = Value::String(search.to_string());
    }
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("x-auth-token", auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(payload.to_string()))
                .expect("books search condition request should build"),
        )
        .await
        .expect("books search condition request should complete");
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("books search condition body should read");
    let payload: Value = serde_json::from_slice(&body)
        .expect("books search condition payload should parse");
    page_ids(&payload)
}

#[tokio::test]
async fn router_search_with_lightweight_media_status_condition_filters_on_fast_path() {
    let ctx = TestFixture::builder("search-lightweight-media-status-fast-path")
        .with_search_index()
        .with_seed(|paths| async move {
            let pool = connect_test_pool(&paths.main_db, 1)
                .await
                .expect("seed db should open");
            sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES ('lib-lw', 'LW', '/lw')")
                .execute(&pool)
                .await
                .expect("library row should be inserted");
            sqlx::query(
                "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) \
                 VALUES ('series-lw', 0, 'LW Series', 'lw/series', 'lib-lw')",
            )
            .execute(&pool)
            .await
            .expect("series row should be inserted");
            for (id, title, media_status) in [
                ("book-a", "Zeta Alpha", "READY"),
                ("book-b", "Zeta Beta", "UNKNOWN"),
                ("book-c", "Zeta Gamma", "READY"),
            ] {
                sqlx::query(
                    "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) \
                     VALUES (?, 0, ?, ?, 'series-lw', 0, 0, 'lib-lw')",
                )
                .bind(id)
                .bind(format!("{title}.epub"))
                .bind(format!("lw/{id}"))
                .execute(&pool)
                .await
                .expect("book row should be inserted");
                sqlx::query(
                    "INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) \
                     VALUES ('application/epub+zip', ?, ?, 10)",
                )
                .bind(media_status)
                .bind(id)
                .execute(&pool)
                .await
                .expect("media row should be inserted");
            }
        })
        .build()
        .await;

    let admin_token = ctx.login_admin().await;

    let ready_ids = search_with_condition_ids(
        &ctx.app().clone(),
        &admin_token,
        Some("title,asc"),
        Some("Zeta"),
        json!({ "type": "MediaStatus", "operator": "is", "value": "READY" }),
    )
    .await;
    assert_eq!(ready_ids, vec!["book-a", "book-c"]);

    let unknown_ids = search_with_condition_ids(
        &ctx.app().clone(),
        &admin_token,
        Some("title,asc"),
        Some("Zeta"),
        json!({ "type": "MediaStatus", "operator": "is", "value": "UNKNOWN" }),
    )
    .await;
    assert_eq!(unknown_ids, vec!["book-b"]);

    let excluded_ids = search_with_condition_ids(
        &ctx.app().clone(),
        &admin_token,
        Some("title,asc"),
        Some("Zeta"),
        json!({ "type": "MediaStatus", "operator": "isNot", "value": "READY" }),
    )
    .await;
    assert_eq!(excluded_ids, vec!["book-b"]);

    let all_ids = search_with_condition_ids(
        &ctx.app().clone(),
        &admin_token,
        Some("title,asc"),
        Some("Zeta"),
        json!({ "type": "Title", "operator": "contains", "value": "Zeta" }),
    )
    .await;
    assert_eq!(all_ids, vec!["book-a", "book-b", "book-c"]);
}

async fn series_search_with_condition_ids(
    app: &axum::Router,
    auth_token: &str,
    sort: Option<&str>,
    full_text_search: Option<&str>,
    condition: Value,
) -> Vec<String> {
    let mut uri = String::from("/api/v1/series/list?page=0&size=20");
    if let Some(sort) = sort {
        uri.push_str("&sort=");
        uri.push_str(sort);
    }
    let mut payload = json!({
        "condition": condition,
    });
    if let Some(search) = full_text_search {
        payload["fullTextSearch"] = Value::String(search.to_string());
    }
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("x-auth-token", auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(payload.to_string()))
                .expect("series search condition request should build"),
        )
        .await
        .expect("series search condition request should complete");
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("series search condition body should read");
    let payload: Value = serde_json::from_slice(&body)
        .expect("series search condition payload should parse");
    page_ids(&payload)
}

#[tokio::test]
async fn router_series_search_with_lightweight_oneshot_condition_filters_on_fast_path() {
    let ctx = TestFixture::builder("series-search-lightweight-oneshot-fast-path")
        .with_search_index()
        .with_seed(|paths| async move {
            let pool = connect_test_pool(&paths.main_db, 1)
                .await
                .expect("seed db should open");
            sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES ('lib-lw', 'LW', '/lw')")
                .execute(&pool)
                .await
                .expect("library row should be inserted");
            for (id, name, oneshot) in [
                ("series-a", "Zeta Alpha", 0),
                ("series-b", "Zeta Beta", 1),
                ("series-c", "Zeta Gamma", 0),
            ] {
                sqlx::query(
                    "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID, ONESHOT) \
                     VALUES (?, 0, ?, ?, 'lib-lw', ?)",
                )
                .bind(id)
                .bind(name)
                .bind(format!("lw/{id}"))
                .bind(oneshot)
                .execute(&pool)
                .await
                .expect("series row should be inserted");
            }
        })
        .build()
        .await;

    let admin_token = ctx.login_admin().await;

    let non_oneshot_ids = series_search_with_condition_ids(
        &ctx.app().clone(),
        &admin_token,
        Some("titleSort,asc"),
        Some("Zeta"),
        json!({ "type": "OneShot", "operator": "istrue" }),
    )
    .await;
    assert_eq!(non_oneshot_ids, vec!["series-b"]);

    let regular_ids = series_search_with_condition_ids(
        &ctx.app().clone(),
        &admin_token,
        Some("titleSort,asc"),
        Some("Zeta"),
        json!({ "type": "OneShot", "operator": "isfalse" }),
    )
    .await;
    assert_eq!(regular_ids, vec!["series-a", "series-c"]);

    let all_ids = series_search_with_condition_ids(
        &ctx.app().clone(),
        &admin_token,
        Some("titleSort,asc"),
        Some("Zeta"),
        json!({ "type": "Title", "operator": "contains", "value": "Zeta" }),
    )
    .await;
    assert_eq!(all_ids, vec!["series-a", "series-b", "series-c"]);
}

#[tokio::test]
async fn router_book_search_with_multi_key_sort_filters_on_fast_path() {
    let ctx = TestFixture::builder("book-search-multi-key-sort-fast-path")
        .with_search_index()
        .with_seed(|paths| async move {
            let pool = connect_test_pool(&paths.main_db, 1)
                .await
                .expect("seed db should open");
            sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES ('lib-mk', 'MK', '/mk')")
                .execute(&pool)
                .await
                .expect("library row should be inserted");
            for (series_id, series_name) in
                [("series-x", "X Series"), ("series-y", "Y Series")]
            {
                sqlx::query(
                    "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) \
                     VALUES (?, 0, ?, ?, 'lib-mk')",
                )
                .bind(series_id)
                .bind(series_name)
                .bind(format!("mk/{series_id}"))
                .execute(&pool)
                .await
                .expect("series row should be inserted");
            }
            for (id, title, series_id) in [
                ("book-a", "Zeta Alpha", "series-x"),
                ("book-b", "Zeta Beta", "series-x"),
                ("book-c", "Zeta Gamma", "series-y"),
            ] {
                sqlx::query(
                    "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) \
                     VALUES (?, 0, ?, ?, ?, 0, 0, 'lib-mk')",
                )
                .bind(id)
                .bind(format!("{title}.epub"))
                .bind(format!("mk/{id}"))
                .bind(series_id)
                .execute(&pool)
                .await
                .expect("book row should be inserted");
                sqlx::query(
                    "INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) \
                     VALUES ('application/epub+zip', 'READY', ?, 10)",
                )
                .bind(id)
                .execute(&pool)
                .await
                .expect("media row should be inserted");
            }
        })
        .build()
        .await;

    let admin_token = ctx.login_admin().await;

    let ids = search_with_condition_ids(
        &ctx.app().clone(),
        &admin_token,
        Some("seriesId,asc&sort=title,desc"),
        Some("Zeta"),
        json!({ "type": "Title", "operator": "contains", "value": "Zeta" }),
    )
    .await;
    assert_eq!(ids, vec!["book-b", "book-a", "book-c"]);
}

#[tokio::test]
async fn router_series_search_with_multi_key_sort_filters_on_fast_path() {
    let ctx = TestFixture::builder("series-search-multi-key-sort-fast-path")
        .with_search_index()
        .with_seed(|paths| async move {
            let pool = connect_test_pool(&paths.main_db, 1)
                .await
                .expect("seed db should open");
            sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES ('lib-mks', 'MKS', '/mks')")
                .execute(&pool)
                .await
                .expect("library row should be inserted");
            for (id, name, books_count) in [
                ("series-a", "Zeta Alpha", 2),
                ("series-b", "Zeta Beta", 2),
                ("series-c", "Zeta Gamma", 1),
            ] {
                sqlx::query(
                    "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID, BOOK_COUNT) \
                     VALUES (?, 0, ?, ?, 'lib-mks', ?)",
                )
                .bind(id)
                .bind(name)
                .bind(format!("mks/{id}"))
                .bind(books_count)
                .execute(&pool)
                .await
                .expect("series row should be inserted");
            }
        })
        .build()
        .await;

    let admin_token = ctx.login_admin().await;

    let ids = series_search_with_condition_ids(
        &ctx.app().clone(),
        &admin_token,
        Some("booksCount,desc&sort=titleSort,asc"),
        Some("Zeta"),
        json!({ "type": "Title", "operator": "contains", "value": "Zeta" }),
    )
    .await;
    assert_eq!(ids, vec!["series-a", "series-b", "series-c"]);
}

#[tokio::test]
async fn router_book_search_without_sort_filters_on_fast_path() {
    let ctx = TestFixture::builder("book-search-without-sort-fast-path")
        .with_search_index()
        .with_seed(|paths| async move {
            let pool = connect_test_pool(&paths.main_db, 1)
                .await
                .expect("seed db should open");
            sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES ('lib-ns', 'NS', '/ns')")
                .execute(&pool)
                .await
                .expect("library row should be inserted");
            sqlx::query(
                "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) \
                 VALUES ('series-ns', 0, 'NS Series', 'ns/series', 'lib-ns')",
            )
            .execute(&pool)
            .await
            .expect("series row should be inserted");
            for (id, title) in [
                ("book-a", "Zeta Alpha"),
                ("book-b", "Zeta Beta"),
                ("book-c", "Zeta Gamma"),
            ] {
                sqlx::query(
                    "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) \
                     VALUES (?, 0, ?, ?, 'series-ns', 0, 0, 'lib-ns')",
                )
                .bind(id)
                .bind(format!("{title}.epub"))
                .bind(format!("ns/{id}"))
                .execute(&pool)
                .await
                .expect("book row should be inserted");
                sqlx::query(
                    "INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) \
                     VALUES ('application/epub+zip', 'READY', ?, 10)",
                )
                .bind(id)
                .execute(&pool)
                .await
                .expect("media row should be inserted");
            }
        })
        .build()
        .await;

    let admin_token = ctx.login_admin().await;
    let title_condition =
        json!({ "type": "Title", "operator": "contains", "value": "Zeta" });

    // No sort parameter: the slow path ranks hits by score desc + title asc and
    // the engine does not re-sort, i.e. exactly the RelevanceAsc ordering.
    let no_sort_ids = search_with_condition_ids(
        &ctx.app().clone(),
        &admin_token,
        None,
        Some("Zeta"),
        title_condition.clone(),
    )
    .await;
    let relevance_ids = search_with_condition_ids(
        &ctx.app().clone(),
        &admin_token,
        Some("relevance,asc"),
        Some("Zeta"),
        title_condition.clone(),
    )
    .await;
    assert_eq!(no_sort_ids, relevance_ids);
    assert_eq!(no_sort_ids.len(), 3);
}

#[tokio::test]
async fn router_series_search_without_sort_filters_on_fast_path() {
    let ctx = TestFixture::builder("series-search-without-sort-fast-path")
        .with_search_index()
        .with_seed(|paths| async move {
            let pool = connect_test_pool(&paths.main_db, 1)
                .await
                .expect("seed db should open");
            sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES ('lib-nss', 'NSS', '/nss')")
                .execute(&pool)
                .await
                .expect("library row should be inserted");
            for (id, name) in [
                ("series-a", "Zeta Alpha"),
                ("series-b", "Zeta Beta"),
                ("series-c", "Zeta Gamma"),
            ] {
                sqlx::query(
                    "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) \
                     VALUES (?, 0, ?, ?, 'lib-nss')",
                )
                .bind(id)
                .bind(name)
                .bind(format!("nss/{id}"))
                .execute(&pool)
                .await
                .expect("series row should be inserted");
            }
        })
        .build()
        .await;

    let admin_token = ctx.login_admin().await;
    let title_condition =
        json!({ "type": "Title", "operator": "contains", "value": "Zeta" });

    let no_sort_ids = series_search_with_condition_ids(
        &ctx.app().clone(),
        &admin_token,
        None,
        Some("Zeta"),
        title_condition.clone(),
    )
    .await;
    let relevance_ids = series_search_with_condition_ids(
        &ctx.app().clone(),
        &admin_token,
        Some("relevance,asc"),
        Some("Zeta"),
        title_condition.clone(),
    )
    .await;
    assert_eq!(no_sort_ids, relevance_ids);
    assert_eq!(no_sort_ids.len(), 3);
}

async fn series_search_unpaged_ids(
    app: &axum::Router,
    auth_token: &str,
    full_text_search: Option<&str>,
    condition: Value,
) -> Value {
    let mut uri = String::from("/api/v1/series/list?page=0&size=20&unpaged=true");
    let mut payload = json!({
        "condition": condition,
    });
    if let Some(search) = full_text_search {
        payload["fullTextSearch"] = Value::String(search.to_string());
    }
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("x-auth-token", auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(payload.to_string()))
                .expect("series unpaged search request should build"),
        )
        .await
        .expect("series unpaged search request should complete");
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("series unpaged search body should read");
    serde_json::from_slice(&body).expect("series unpaged search payload should parse")
}

#[tokio::test]
async fn router_series_unpaged_search_filters_on_fast_path() {
    // Mirrors webui SeriesPickerDialog: getSeriesList({fullTextSearch, condition
    // OneShot isfalse}, {unpaged: true}) with no sort.
    let ctx = TestFixture::builder("series-unpaged-search-fast-path")
        .with_search_index()
        .with_seed(|paths| async move {
            let pool = connect_test_pool(&paths.main_db, 1)
                .await
                .expect("seed db should open");
            sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES ('lib-up', 'UP', '/up')")
                .execute(&pool)
                .await
                .expect("library row should be inserted");
            for (id, name, oneshot) in [
                ("series-a", "Zeta Alpha", 0),
                ("series-b", "Zeta Beta", 1),
                ("series-c", "Zeta Gamma", 0),
            ] {
                sqlx::query(
                    "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID, ONESHOT) \
                     VALUES (?, 0, ?, ?, 'lib-up', ?)",
                )
                .bind(id)
                .bind(name)
                .bind(format!("up/{id}"))
                .bind(oneshot)
                .execute(&pool)
                .await
                .expect("series row should be inserted");
            }
        })
        .build()
        .await;

    let admin_token = ctx.login_admin().await;

    let oneshot_condition = json!({ "type": "OneShot", "operator": "isfalse" });
    let payload = series_search_unpaged_ids(
        &ctx.app().clone(),
        &admin_token,
        Some("Zeta"),
        oneshot_condition.clone(),
    )
    .await;
    let ids = page_ids(&payload);
    assert_eq!(ids, vec!["series-a", "series-c"]);
    // The fast path and the slow path both order by relevance (score desc +
    // title asc) when no sort is given, so compare against the paged
    // relevance,asc request to pin the ordering semantics.
    let paged_relevance = series_search_with_condition_ids(
        &ctx.app().clone(),
        &admin_token,
        Some("relevance,asc"),
        Some("Zeta"),
        oneshot_condition,
    )
    .await;
    assert_eq!(ids, paged_relevance);
    assert_eq!(payload.pointer("/pageable/pageNumber"), Some(&json!(0)));
    assert_eq!(payload.pointer("/pageable/pageSize"), Some(&json!(2)));
    assert_eq!(payload.pointer("/pageable/unpaged"), Some(&json!(true)));
    assert_eq!(payload.pointer("/totalElements"), Some(&json!(2)));
}

#[tokio::test]
async fn router_book_unpaged_search_filters_on_fast_path() {
    let ctx = TestFixture::builder("book-unpaged-search-fast-path")
        .with_search_index()
        .with_seed(|paths| async move {
            let pool = connect_test_pool(&paths.main_db, 1)
                .await
                .expect("seed db should open");
            sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES ('lib-bup', 'BUP', '/bup')")
                .execute(&pool)
                .await
                .expect("library row should be inserted");
            sqlx::query(
                "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) \
                 VALUES ('series-bup', 0, 'BUP Series', 'bup/series', 'lib-bup')",
            )
            .execute(&pool)
            .await
            .expect("series row should be inserted");
            for (id, title) in [
                ("book-a", "Zeta Alpha"),
                ("book-b", "Zeta Beta"),
                ("book-c", "Zeta Gamma"),
            ] {
                sqlx::query(
                    "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) \
                     VALUES (?, 0, ?, ?, 'series-bup', 0, 0, 'lib-bup')",
                )
                .bind(id)
                .bind(format!("{title}.epub"))
                .bind(format!("bup/{id}"))
                .execute(&pool)
                .await
                .expect("book row should be inserted");
                sqlx::query(
                    "INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) \
                     VALUES ('application/epub+zip', 'READY', ?, 10)",
                )
                .bind(id)
                .execute(&pool)
                .await
                .expect("media row should be inserted");
            }
        })
        .build()
        .await;

    let admin_token = ctx.login_admin().await;

    let title_condition =
        json!({ "type": "Title", "operator": "contains", "value": "Zeta" });
    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20&unpaged=true")
                .header("x-auth-token", &admin_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": title_condition.clone(),
                        "fullTextSearch": "Zeta",
                    })
                    .to_string(),
                ))
                .expect("book unpaged search request should build"),
        )
        .await
        .expect("book unpaged search request should complete");
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("book unpaged search body should read");
    let payload: Value = serde_json::from_slice(&body)
        .expect("book unpaged search payload should parse");
    let unpaged_ids = page_ids(&payload);
    let paged_relevance = search_with_condition_ids(
        &ctx.app().clone(),
        &admin_token,
        Some("relevance,asc"),
        Some("Zeta"),
        title_condition,
    )
    .await;
    assert_eq!(unpaged_ids, paged_relevance);
    assert_eq!(unpaged_ids.len(), 3);
    assert_eq!(payload.pointer("/pageable/pageNumber"), Some(&json!(0)));
    assert_eq!(payload.pointer("/pageable/pageSize"), Some(&json!(3)));
    assert_eq!(payload.pointer("/pageable/unpaged"), Some(&json!(true)));
    assert_eq!(payload.pointer("/totalElements"), Some(&json!(3)));
}

