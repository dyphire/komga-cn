use super::*;

#[tokio::test]
async fn router_removed_books_and_authors_v1_routes_return_not_found() {
    let paths = new_router_fixture("router-removed-books-and-authors-v1-routes").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for route in [
        "/api/v1/books?page=0&size=20",
        "/api/v1/authors?search=jane",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("removed v1 route request should build"),
            )
            .await
            .expect("removed v1 route request should complete");

        assert_eq!(response.status(), StatusCode::NOT_FOUND, "route: {route}");
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_author_endpoints_filter_to_authorized_libraries() {
    let paths = new_router_fixture("router-authors-authorized-libraries").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "library-1-user",
        "library1@example.org",
        "router-contract-library1-123",
        &["library-1"],
    )
    .await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("authors authorized-libraries db should open");
    sqlx::query("INSERT INTO BOOK_METADATA_AUTHOR (BOOK_ID, NAME, ROLE) VALUES (?, ?, ?)")
        .bind("book-3")
        .bind("Morgan Else")
        .bind("inker")
        .execute(&pool)
        .await
        .expect("cross-library inker role row should be inserted");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "library1@example.org",
        "router-contract-library1-123",
    )
    .await;

    for (route, expected) in [
        ("/api/v1/authors/names", json!(["Alex Side", "Jane Writer"])),
        ("/api/v1/authors/roles", json!(["writer"])),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("authors authorized-libraries request should build"),
            )
            .await
            .expect("authors authorized-libraries request should complete");

        assert_eq!(response.status(), StatusCode::OK, "route: {route}");
        let payload = response_json(response).await;
        assert_eq!(payload, expected, "route: {route}");
    }

    let v2_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v2/authors?unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("authors v2 authorized-libraries request should build"),
        )
        .await
        .expect("authors v2 authorized-libraries request should complete");

    assert_eq!(v2_response.status(), StatusCode::OK);
    let payload = response_json(v2_response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("authors v2 payload should expose content array");
    let author_names = content
        .iter()
        .filter_map(|entry| entry.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>();

    assert_eq!(author_names, vec!["Alex Side", "Jane Writer"]);
    assert_eq!(payload["totalElements"], json!(2));
    assert_eq!(payload["size"], json!(20));
    assert_eq!(payload["sort"]["empty"], json!(false));
    assert_eq!(payload["sort"]["sorted"], json!(true));
    assert_eq!(payload["sort"]["unsorted"], json!(false));
    assert_eq!(payload["pageable"]["pageSize"], json!(20));
    assert_eq!(payload["pageable"]["paged"], json!(true));
    assert_eq!(payload["pageable"]["unpaged"], json!(false));
    assert_eq!(payload["pageable"]["sort"]["empty"], json!(false));
    assert_eq!(payload["pageable"]["sort"]["sorted"], json!(true));
    assert_eq!(payload["pageable"]["sort"]["unsorted"], json!(false));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_authors_names_matches_search_without_accents() {
    let paths = new_router_fixture("router-authors-names-strip-accents").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("authors names strip-accents db should open");
    sqlx::query("INSERT INTO BOOK_METADATA_AUTHOR (BOOK_ID, NAME, ROLE) VALUES (?, ?, ?)")
        .bind("book-1")
        .bind("José Álvarez")
        .bind("writer")
        .execute(&pool)
        .await
        .expect("accented author row should be inserted");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/authors/names?search=jose")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("authors names strip-accents request should build"),
        )
        .await
        .expect("authors names strip-accents request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let names = payload
        .as_array()
        .expect("authors names strip-accents payload should be an array");
    assert_eq!(names, &vec![json!("José Álvarez")]);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_media_status_begins_with_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-operator").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "MediaStatus",
                            "operator": "beginsWith",
                            "value": "READY"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list request should build"),
        )
        .await
        .expect("strict books/list beginsWith request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict media-status beginsWith payload should expose content array");
    assert_eq!(content.len(), 1);
    assert_eq!(
        content[0].get("id"),
        Some(&Value::String("book-1".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_download_routes_do_not_get_shallow_etag_headers() {
    let paths = new_router_fixture("router-download-routes-no-shallow-etag").await;
    seed_router_contract_data(&paths).await;
    let books_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&books_dir)
        .expect("books directory should be created for download exclusion test");
    std::fs::write(books_dir.join("book-1.epub"), b"download-exclusion-body")
        .expect("book fixture file should be written for download exclusion test");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let libraries_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/libraries")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("libraries request should build for exclusion control"),
        )
        .await
        .expect("libraries request should complete for exclusion control");
    let cache_etag = libraries_response
        .headers()
        .get(header::ETAG)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .expect("non-download route should expose etag for exclusion control");

    for route in [
        "/api/v1/books/book-1/file/book-1.epub",
        "/opds/v2/books/book-1/file/book-1.epub",
        "/kobo/any-token/v1/books/book-1/file/epub",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .header(header::IF_NONE_MATCH, &cache_etag)
                    .body(Body::empty())
                    .expect("download exclusion request should build"),
            )
            .await
            .expect("download exclusion request should complete");

        assert_eq!(
            response.status(),
            StatusCode::OK,
            "download route should not turn into 304: {route}",
        );
        assert!(
            !response.headers().contains_key(header::ETAG),
            "download route should not receive shallow etag: {route}",
        );
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_media_status_is_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-media-status").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "MediaStatus",
                            "operator": "is",
                            "value": "READY"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list media-status request should build"),
        )
        .await
        .expect("strict books/list media-status request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("books/list media-status payload should expose content array");
    assert_eq!(content.len(), 1);
    assert_eq!(
        content[0].get("id"),
        Some(&Value::String("book-1".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_applies_default_sort_for_unknown_sort_mode_in_runtime_owned_mode()
 {
    let paths = new_router_fixture("router-discovery-books-list-strict-sort-modes").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for sort in [
        "metadata.title,asc",
        "series,metadata.numberSort,asc",
        "metadata.numberSort,asc",
        "number,asc",
        "createdDate,desc",
        "created,desc",
        "lastModifiedDate,desc",
        "lastModified,desc",
        "metadata.releaseDate,desc",
        "seriesId,asc",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/books/list?page=0&size=20&sort={sort}"))
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
                    .expect("strict books/list supported sort request should build"),
            )
            .await
            .expect("strict books/list supported sort request should complete");
        assert_eq!(response.status(), StatusCode::OK);
    }

    let unsupported_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20&sort=unsupported.sort,asc")
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
                .expect("strict books/list unsupported sort request should build"),
        )
        .await
        .expect("strict books/list unsupported sort request should complete");
    assert_eq!(unsupported_response.status(), StatusCode::OK);
    let unsupported_payload = response_json(unsupported_response).await;
    let unsupported_content = unsupported_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books unsupported sort payload should expose content array");
    assert_eq!(unsupported_content.len(), 1);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_sorts_runtime_owned_results_by_number_series_id_and_alias_dates() {
    let paths = new_router_fixture("router-discovery-books-list-runtime-sort-order").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("books runtime sort db should open");
    for (book_id, title, created, last_modified) in [
        (
            "book-1",
            "Zulu Book",
            "2024-01-01 00:00:00",
            "2024-01-10 00:00:00",
        ),
        (
            "book-2",
            "Alpha Book",
            "2024-02-01 00:00:00",
            "2024-02-10 00:00:00",
        ),
    ] {
        sqlx::query(
            "UPDATE BOOK \
             SET CREATED_DATE = ?, LAST_MODIFIED_DATE = ? \
             WHERE ID = ?",
        )
        .bind(created)
        .bind(last_modified)
        .bind(book_id)
        .execute(&pool)
        .await
        .expect("books runtime sort fixture should update timestamps");
        sqlx::query(
            "UPDATE BOOK_METADATA \
             SET TITLE = ? \
             WHERE BOOK_ID = ?",
        )
        .bind(title)
        .bind(book_id)
        .execute(&pool)
        .await
        .expect("books runtime sort fixture should update metadata title");
    }
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for (sort, expected_ids) in [
        ("metadata.title,asc", vec!["book-2", "book-1"]),
        ("series,metadata.numberSort,asc", vec!["book-1", "book-2"]),
        ("metadata.numberSort,asc", vec!["book-1", "book-2"]),
        ("number,asc", vec!["book-1", "book-2"]),
        ("seriesId,asc", vec!["book-1", "book-2"]),
        ("metadata.releaseDate,desc", vec!["book-2", "book-1"]),
        ("created,desc", vec!["book-2", "book-1"]),
        ("lastModified,desc", vec!["book-2", "book-1"]),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/books/list?page=0&size=20&sort={sort}"))
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
                    .expect("books runtime sort request should build"),
            )
            .await
            .expect("books runtime sort request should complete");
        assert_eq!(response.status(), StatusCode::OK, "sort: {sort}");
        let payload = response_json(response).await;
        let ids = payload
            .get("content")
            .and_then(Value::as_array)
            .expect("books runtime sort payload should expose content array")
            .iter()
            .filter_map(|entry| entry.get("id").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert_eq!(ids, expected_ids, "sort: {sort}");
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_media_status_is_not_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-media-status-is-not").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let excluded_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "MediaStatus",
                            "operator": "isNot",
                            "value": "READY"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list media-status isNot excluded request should build"),
        )
        .await
        .expect("strict books/list media-status isNot excluded request should complete");
    assert_eq!(excluded_response.status(), StatusCode::OK);
    let excluded_payload = response_json(excluded_response).await;
    let excluded_content = excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict media-status isNot excluded payload should expose content array");
    assert_eq!(excluded_content.len(), 0);

    let kept_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "MediaStatus",
                            "operator": "isNot",
                            "value": "UNKNOWN"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list media-status isNot kept request should build"),
        )
        .await
        .expect("strict books/list media-status isNot kept request should complete");
    assert_eq!(kept_response.status(), StatusCode::OK);
    let kept_payload = response_json(kept_response).await;
    let kept_content = kept_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict media-status isNot kept payload should expose content array");
    assert_eq!(kept_content.len(), 1);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_genre_condition_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-filter-combo").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "Genre",
                            "operator": "is",
                            "value": "SciFi"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list unsupported condition request should build"),
        )
        .await
        .expect("strict books/list genre request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict genre payload should expose content array");
    assert_eq!(content.len(), 1);
    assert_eq!(
        content[0].get("id"),
        Some(&Value::String("book-1".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_rejects_unknown_condition_type_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-unknown-condition").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "UnknownBookCondition",
                            "operator": "is",
                            "value": "whatever"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list unknown-condition request should build"),
        )
        .await
        .expect("strict books/list unknown-condition request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_rejects_unknown_operator_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-unknown-operator").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
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
                .expect("strict books/list unknown-operator request should build"),
        )
        .await
        .expect("strict books/list unknown-operator request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_series_metadata_conditions_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-series-metadata").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for condition in [
        json!({ "type": "Language", "operator": "is", "value": "EN" }),
        json!({ "type": "Publisher", "operator": "is", "value": "PubHouse" }),
        json!({ "type": "AgeRating", "operator": "is", "value": 16 }),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/books/list?page=0&size=20")
                    .header("x-auth-token", &auth_token)
                    .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(json!({ "condition": condition }).to_string()))
                    .expect("strict books/list series metadata request should build"),
            )
            .await
            .expect("strict books/list series metadata request should complete");

        assert_eq!(response.status(), StatusCode::OK);
        let payload = response_json(response).await;
        let content = payload
            .get("content")
            .and_then(Value::as_array)
            .expect("strict series metadata payload should expose content array");
        assert_eq!(content.len(), 1);
        assert_eq!(
            content[0].get("id"),
            Some(&Value::String("book-1".to_string()))
        );
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_series_id_with_query_is_not_silent_empty_in_runtime_owned_mode()
 {
    let paths = new_router_fixture("router-discovery-books-list-strict-seriesid-query").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "SeriesId",
                            "operator": "is",
                            "value": "series-1"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list seriesId request should build"),
        )
        .await
        .expect("strict books/list seriesId request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict seriesId request should expose content array");
    assert_eq!(content.len(), 1);
    assert_eq!(
        content[0].get("id"),
        Some(&Value::String("book-1".to_string()))
    );

    let excluded_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "SeriesId",
                            "operator": "isNot",
                            "value": "series-1"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list seriesId isNot request should build"),
        )
        .await
        .expect("strict books/list seriesId isNot request should complete");

    assert_eq!(excluded_response.status(), StatusCode::OK);
    let excluded_payload = response_json(excluded_response).await;
    let excluded_content = excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict seriesId isNot request should expose content array");
    assert_eq!(excluded_content.len(), 0);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_anyof_and_allof_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-anyof-allof").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let all_of_match_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "AllOfBook",
                            "conditions": [
                                {"type": "LibraryId", "operator": "is", "value": "library-1"},
                                {"type": "MediaStatus", "operator": "is", "value": "READY"}
                            ]
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list allOf match request should build"),
        )
        .await
        .expect("strict books/list allOf match request should complete");
    assert_eq!(all_of_match_response.status(), StatusCode::OK);
    let all_of_match_payload = response_json(all_of_match_response).await;
    let all_of_match_content = all_of_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books allOf match payload should expose content array");
    assert_eq!(all_of_match_content.len(), 1);

    let all_of_miss_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "AllOfBook",
                            "conditions": [
                                {"type": "LibraryId", "operator": "is", "value": "library-1"},
                                {"type": "MediaStatus", "operator": "is", "value": "UNKNOWN"}
                            ]
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list allOf miss request should build"),
        )
        .await
        .expect("strict books/list allOf miss request should complete");
    assert_eq!(all_of_miss_response.status(), StatusCode::OK);
    let all_of_miss_payload = response_json(all_of_miss_response).await;
    let all_of_miss_content = all_of_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books allOf miss payload should expose content array");
    assert_eq!(all_of_miss_content.len(), 0);

    let any_of_match_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "AnyOfBook",
                            "conditions": [
                                {"type": "MediaStatus", "operator": "is", "value": "UNKNOWN"},
                                {"type": "MediaStatus", "operator": "is", "value": "READY"}
                            ]
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list anyOf match request should build"),
        )
        .await
        .expect("strict books/list anyOf match request should complete");
    assert_eq!(any_of_match_response.status(), StatusCode::OK);
    let any_of_match_payload = response_json(any_of_match_response).await;
    let any_of_match_content = any_of_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books anyOf match payload should expose content array");
    assert_eq!(any_of_match_content.len(), 1);

    let any_of_miss_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "AnyOfBook",
                            "conditions": [
                                {"type": "MediaStatus", "operator": "is", "value": "UNKNOWN"},
                                {"type": "MediaStatus", "operator": "is", "value": "UNKNOWN"}
                            ]
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list anyOf miss request should build"),
        )
        .await
        .expect("strict books/list anyOf miss request should complete");
    assert_eq!(any_of_miss_response.status(), StatusCode::OK);
    let any_of_miss_payload = response_json(any_of_miss_response).await;
    let any_of_miss_content = any_of_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books anyOf miss payload should expose content array");
    assert_eq!(any_of_miss_content.len(), 0);

    cleanup_router_fixture(paths);
}
