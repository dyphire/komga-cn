use super::*;

#[tokio::test]
async fn router_discovery_books_list_supports_read_status_is_and_is_not_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-read-status").await;
    seed_router_contract_data(&paths).await;
    seed_router_read_progress(&paths, true).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let unread_response = app
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
                            "type": "ReadStatus",
                            "operator": "is",
                            "value": "UNREAD"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list unread request should build"),
        )
        .await
        .expect("strict books/list unread request should complete");
    assert_eq!(unread_response.status(), StatusCode::OK);
    let unread_payload = response_json(unread_response).await;
    let unread_content = unread_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict unread payload should expose content array");
    assert_eq!(unread_content.len(), 0);

    let read_response = app
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
                            "type": "ReadStatus",
                            "operator": "is",
                            "value": "READ"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list read request should build"),
        )
        .await
        .expect("strict books/list read request should complete");
    assert_eq!(read_response.status(), StatusCode::OK);
    let read_payload = response_json(read_response).await;
    let read_content = read_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict read payload should expose content array");
    assert_eq!(read_content.len(), 1);
    assert_eq!(
        read_content[0].get("id"),
        Some(&Value::String("book-1".to_string()))
    );

    let excluded_read_response = app
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
                            "type": "ReadStatus",
                            "operator": "isNot",
                            "value": "READ"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list read isNot excluded request should build"),
        )
        .await
        .expect("strict books/list read isNot excluded request should complete");
    assert_eq!(excluded_read_response.status(), StatusCode::OK);
    let excluded_read_payload = response_json(excluded_read_response).await;
    let excluded_read_content = excluded_read_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict read isNot excluded payload should expose content array");
    assert_eq!(excluded_read_content.len(), 0);

    let kept_not_unread_response = app
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
                            "type": "ReadStatus",
                            "operator": "isNot",
                            "value": "UNREAD"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list read isNot kept request should build"),
        )
        .await
        .expect("strict books/list read isNot kept request should complete");
    assert_eq!(kept_not_unread_response.status(), StatusCode::OK);
    let kept_not_unread_payload = response_json(kept_not_unread_response).await;
    let kept_not_unread_content = kept_not_unread_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict read isNot kept payload should expose content array");
    assert_eq!(kept_not_unread_content.len(), 1);
    assert_eq!(
        kept_not_unread_content[0].get("id"),
        Some(&Value::String("book-1".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_library_id_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-library-id").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let matched_response = app
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
                            "type": "LibraryId",
                            "operator": "is",
                            "value": "library-1"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list library-id match request should build"),
        )
        .await
        .expect("strict books/list library-id match request should complete");
    assert_eq!(matched_response.status(), StatusCode::OK);
    let matched_payload = response_json(matched_response).await;
    let matched_content = matched_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books library-id match payload should expose content array");
    assert_eq!(matched_content.len(), 1);

    let missing_response = app
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
                            "type": "LibraryId",
                            "operator": "is",
                            "value": "library-missing"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list library-id miss request should build"),
        )
        .await
        .expect("strict books/list library-id miss request should complete");
    assert_eq!(missing_response.status(), StatusCode::OK);
    let missing_payload = response_json(missing_response).await;
    let missing_content = missing_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books library-id miss payload should expose content array");
    assert_eq!(missing_content.len(), 0);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_deleted_filter_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-deleted").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let not_deleted_response = app
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
                            "type": "Deleted",
                            "operator": "isFalse"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list deleted isFalse request should build"),
        )
        .await
        .expect("strict books/list deleted isFalse request should complete");
    assert_eq!(not_deleted_response.status(), StatusCode::OK);
    let not_deleted_payload = response_json(not_deleted_response).await;
    let not_deleted_content = not_deleted_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books deleted isFalse payload should expose content array");
    assert_eq!(not_deleted_content.len(), 1);

    let deleted_response = app
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
                            "operator": "isTrue"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list deleted isTrue request should build"),
        )
        .await
        .expect("strict books/list deleted isTrue request should complete");
    assert_eq!(deleted_response.status(), StatusCode::OK);
    let deleted_payload = response_json(deleted_response).await;
    let deleted_content = deleted_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books deleted isTrue payload should expose content array");
    assert_eq!(deleted_content.len(), 0);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_oneshot_filter_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-oneshot").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let oneshot_true_response = app
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
                            "type": "OneShot",
                            "operator": "isTrue"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list oneshot=true request should build"),
        )
        .await
        .expect("strict books/list oneshot=true request should complete");
    assert_eq!(oneshot_true_response.status(), StatusCode::OK);
    let oneshot_true_payload = response_json(oneshot_true_response).await;
    let oneshot_true_content = oneshot_true_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict oneshot=true payload should expose content array");
    assert_eq!(oneshot_true_content.len(), 0);

    let oneshot_false_response = app
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
                            "type": "OneShot",
                            "operator": "isFalse"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list oneshot=false request should build"),
        )
        .await
        .expect("strict books/list oneshot=false request should complete");
    assert_eq!(oneshot_false_response.status(), StatusCode::OK);
    let oneshot_false_payload = response_json(oneshot_false_response).await;
    let oneshot_false_content = oneshot_false_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict oneshot=false payload should expose content array");
    assert_eq!(oneshot_false_content.len(), 1);
    assert_eq!(
        oneshot_false_content[0].get("id"),
        Some(&Value::String("book-1".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_series_latest_supports_deleted_filter() {
    let paths = new_router_fixture("router-series-latest-deleted-filter").await;
    seed_router_contract_data(&paths).await;
    seed_router_custom_series(&paths, "series-deleted", "Deleted Series", "library-1").await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("series latest deleted db should open");
    sqlx::query("UPDATE SERIES SET DELETED_DATE = ? WHERE ID = ?")
        .bind("2025-01-01 00:00:00")
        .bind("series-deleted")
        .execute(&pool)
        .await
        .expect("series deleted date should update");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let deleted_true_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/latest?deleted=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series latest deleted=true request should build"),
        )
        .await
        .expect("series latest deleted=true request should complete");
    assert_eq!(deleted_true_response.status(), StatusCode::OK);
    let deleted_true_payload = response_json(deleted_true_response).await;
    let deleted_true_content = deleted_true_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("series latest deleted=true payload should expose content array");
    assert_eq!(deleted_true_content.len(), 1);
    assert_eq!(
        deleted_true_content[0].get("id"),
        Some(&json!("series-deleted"))
    );

    let deleted_false_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/latest?deleted=false")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series latest deleted=false request should build"),
        )
        .await
        .expect("series latest deleted=false request should complete");
    assert_eq!(deleted_false_response.status(), StatusCode::OK);
    let deleted_false_payload = response_json(deleted_false_response).await;
    let deleted_false_content = deleted_false_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("series latest deleted=false payload should expose content array");
    assert_eq!(deleted_false_content.len(), 1);
    assert_eq!(deleted_false_content[0].get("id"), Some(&json!("series-1")));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_books_duplicates_requires_admin() {
    let paths = new_router_fixture("router-books-duplicates-requires-admin").await;
    seed_router_contract_data(&paths).await;
    seed_router_age_exclude_user(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        18,
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let restricted_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted@example.org",
        "router-contract-restricted-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/duplicates")
                .header("x-auth-token", &restricted_token)
                .body(Body::empty())
                .expect("books duplicates restricted request should build"),
        )
        .await
        .expect("books duplicates restricted request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_books_duplicates_returns_full_book_dto_page() {
    let paths = new_router_fixture("router-books-duplicates-full-book-dto").await;
    seed_router_contract_data(&paths).await;
    seed_router_contract_nullable_samples(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("books duplicates dto db should open");
    for book_id in ["book-1", "book-2"] {
        sqlx::query("UPDATE BOOK SET FILE_HASH = ? WHERE ID = ?")
            .bind("duplicate-file-hash")
            .bind(book_id)
            .execute(&pool)
            .await
            .expect("duplicate file hash should update");
    }
    sqlx::query("UPDATE BOOK SET FILE_SIZE = ? WHERE ID = ?")
        .bind(1_024_i64)
        .bind("book-2")
        .execute(&pool)
        .await
        .expect("duplicate file size should align for book-2");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/duplicates?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("books duplicates request should build"),
        )
        .await
        .expect("books duplicates request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("books duplicates payload should expose content array");
    assert_eq!(content.len(), 2);

    let first = content
        .iter()
        .find(|entry| entry.get("id") == Some(&json!("book-1")))
        .expect("books duplicates payload should include book-1");
    assert_eq!(first.get("seriesId"), Some(&json!("series-1")));
    assert_eq!(first.get("libraryId"), Some(&json!("library-1")));
    assert_eq!(first.get("fileHash"), Some(&json!("duplicate-file-hash")));
    assert_eq!(
        first.pointer("/media/mediaType"),
        Some(&json!("application/epub+zip"))
    );
    assert!(first.get("fileLastModified").is_some_and(Value::is_string));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_books_duplicates_ignores_same_hash_with_different_sizes() {
    let paths = new_router_fixture("router-books-duplicates-hash-size-pair").await;
    seed_router_contract_data(&paths).await;
    seed_router_contract_nullable_samples(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("books duplicates hash-size db should open");
    for book_id in ["book-1", "book-2"] {
        sqlx::query("UPDATE BOOK SET FILE_HASH = ? WHERE ID = ?")
            .bind("same-hash-different-size")
            .bind(book_id)
            .execute(&pool)
            .await
            .expect("hash-size duplicate file hash should update");
    }
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/duplicates?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("books duplicates hash-size request should build"),
        )
        .await
        .expect("books duplicates hash-size request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("books duplicates hash-size payload should expose content array");
    assert!(content.is_empty());
    assert_eq!(payload.get("totalElements"), Some(&json!(0)));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_books_duplicates_honors_sort_query() {
    let paths = new_router_fixture("router-books-duplicates-sort-query").await;
    seed_router_contract_data(&paths).await;
    seed_router_contract_nullable_samples(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("books duplicates sort db should open");
    for book_id in ["book-1", "book-2"] {
        sqlx::query("UPDATE BOOK SET FILE_HASH = ? WHERE ID = ?")
            .bind("duplicate-sort-hash")
            .bind(book_id)
            .execute(&pool)
            .await
            .expect("sort duplicate file hash should update");
    }
    sqlx::query("UPDATE BOOK SET FILE_SIZE = ? WHERE ID = ?")
        .bind(1_024_i64)
        .bind("book-2")
        .execute(&pool)
        .await
        .expect("sort duplicate file size should align for book-2");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/duplicates?page=0&size=20&sort=metadata.title,desc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("books duplicates sorted request should build"),
        )
        .await
        .expect("books duplicates sorted request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("books duplicates sorted payload should expose content array");
    assert_eq!(content.len(), 2);
    assert_eq!(content[0].get("id"), Some(&json!("book-2")));
    assert_eq!(content[1].get("id"), Some(&json!("book-1")));
    assert_eq!(payload.pointer("/sort/sorted"), Some(&json!(true)));
    assert_eq!(payload.pointer("/pageable/sort/sorted"), Some(&json!(true)));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_books_duplicates_sorts_series_by_title_sort() {
    let paths = new_router_fixture("router-books-duplicates-series-title-sort").await;
    seed_router_contract_data(&paths).await;
    seed_router_contract_nullable_samples(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("books duplicates series sort db should open");
    for book_id in ["book-1", "book-2"] {
        sqlx::query("UPDATE BOOK SET FILE_HASH = ?, FILE_SIZE = ? WHERE ID = ?")
            .bind("duplicate-series-sort-hash")
            .bind(1_024_i64)
            .bind(book_id)
            .execute(&pool)
            .await
            .expect("series-sort duplicate pair should update");
    }
    sqlx::query("UPDATE SERIES_METADATA SET TITLE = ?, TITLE_SORT = ? WHERE SERIES_ID = ?")
        .bind("Zulu Series")
        .bind("Alpha Series")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series-1 title sort should update");
    sqlx::query("UPDATE SERIES_METADATA SET TITLE = ?, TITLE_SORT = ? WHERE SERIES_ID = ?")
        .bind("Alpha Series")
        .bind("Zulu Series")
        .bind("series-2")
        .execute(&pool)
        .await
        .expect("series-2 title sort should update");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/duplicates?page=0&size=20&sort=series,asc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("books duplicates series sort request should build"),
        )
        .await
        .expect("books duplicates series sort request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("books duplicates series sort payload should expose content array");
    assert_eq!(content.len(), 2);
    assert_eq!(content[0].get("id"), Some(&json!("book-1")));
    assert_eq!(content[1].get("id"), Some(&json!("book-2")));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_books_duplicates_defaults_to_file_hash_asc_sort() {
    let paths = new_router_fixture("router-books-duplicates-default-filehash-sort").await;
    seed_router_contract_data(&paths).await;
    seed_router_contract_nullable_samples(&paths).await;
    seed_router_pdf_book(&paths, "book-3", "series-1", "book-3.pdf", "Book 3").await;
    seed_router_pdf_book(&paths, "book-4", "series-1", "book-4.pdf", "Book 4").await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("books duplicates default sort db should open");
    for book_id in ["book-1", "book-2"] {
        sqlx::query("UPDATE BOOK SET FILE_HASH = ?, FILE_SIZE = ? WHERE ID = ?")
            .bind("z-hash")
            .bind(1_024_i64)
            .bind(book_id)
            .execute(&pool)
            .await
            .expect("z-hash duplicate pair should update");
    }
    for book_id in ["book-3", "book-4"] {
        sqlx::query("UPDATE BOOK SET FILE_HASH = ?, FILE_SIZE = ? WHERE ID = ?")
            .bind("a-hash")
            .bind(4_096_i64)
            .bind(book_id)
            .execute(&pool)
            .await
            .expect("a-hash duplicate pair should update");
    }
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/duplicates?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("books duplicates default sort request should build"),
        )
        .await
        .expect("books duplicates default sort request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("books duplicates default sort payload should expose content array");
    assert_eq!(content.len(), 4);
    assert_eq!(content[0].get("fileHash"), Some(&json!("a-hash")));
    assert_eq!(content[1].get("fileHash"), Some(&json!("a-hash")));
    assert_eq!(content[2].get("fileHash"), Some(&json!("z-hash")));
    assert_eq!(content[3].get("fileHash"), Some(&json!("z-hash")));
    assert_eq!(payload.pointer("/sort/sorted"), Some(&json!(true)));
    assert_eq!(payload.pointer("/pageable/sort/sorted"), Some(&json!(true)));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_books_duplicates_unpaged_ignores_explicit_sort_query() {
    let paths = new_router_fixture("router-books-duplicates-unpaged-ignore-sort").await;
    seed_router_contract_data(&paths).await;
    seed_router_contract_nullable_samples(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("books duplicates unpaged ignore-sort db should open");
    for book_id in ["book-1", "book-2"] {
        sqlx::query("UPDATE BOOK SET FILE_HASH = ?, FILE_SIZE = ? WHERE ID = ?")
            .bind("duplicate-unpaged-ignore-sort")
            .bind(1_024_i64)
            .bind(book_id)
            .execute(&pool)
            .await
            .expect("unpaged ignore-sort duplicate pair should update");
    }
    sqlx::query("UPDATE BOOK_METADATA SET TITLE = ? WHERE BOOK_ID = ?")
        .bind("A title")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book-1 title should update for unpaged ignore-sort test");
    sqlx::query("UPDATE BOOK_METADATA SET TITLE = ? WHERE BOOK_ID = ?")
        .bind("Z title")
        .bind("book-2")
        .execute(&pool)
        .await
        .expect("book-2 title should update for unpaged ignore-sort test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/duplicates?unpaged=true&sort=metadata.title,desc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("books duplicates unpaged ignore-sort request should build"),
        )
        .await
        .expect("books duplicates unpaged ignore-sort request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("books duplicates unpaged ignore-sort payload should expose content array");
    assert_eq!(content.len(), 2);
    assert_eq!(content[0].get("id"), Some(&json!("book-1")));
    assert_eq!(content[1].get("id"), Some(&json!("book-2")));
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_books_duplicates_includes_same_hash_books_outside_duplicate_size_pair() {
    let paths = new_router_fixture("router-books-duplicates-hash-key-expands-selection").await;
    seed_router_contract_data(&paths).await;
    seed_router_contract_nullable_samples(&paths).await;
    seed_router_pdf_book(&paths, "book-3", "series-1", "book-3.pdf", "Book 3").await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("books duplicates expanded selection db should open");
    for book_id in ["book-1", "book-2"] {
        sqlx::query("UPDATE BOOK SET FILE_HASH = ?, FILE_SIZE = ? WHERE ID = ?")
            .bind("shared-hash")
            .bind(1_024_i64)
            .bind(book_id)
            .execute(&pool)
            .await
            .expect("duplicate size pair should update");
    }
    sqlx::query("UPDATE BOOK SET FILE_HASH = ?, FILE_SIZE = ? WHERE ID = ?")
        .bind("shared-hash")
        .bind(4_096_i64)
        .bind("book-3")
        .execute(&pool)
        .await
        .expect("same hash different size book should update");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/duplicates?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("books duplicates expanded selection request should build"),
        )
        .await
        .expect("books duplicates expanded selection request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("books duplicates expanded selection payload should expose content array");
    assert_eq!(content.len(), 3);
    let ids = content
        .iter()
        .filter_map(|entry| entry.get("id").and_then(Value::as_str))
        .collect::<Vec<_>>();
    assert!(ids.contains(&"book-1"));
    assert!(ids.contains(&"book-2"));
    assert!(ids.contains(&"book-3"));
    assert_eq!(payload.get("totalElements"), Some(&json!(3)));
    assert_eq!(payload.get("totalPages"), Some(&json!(1)));
    assert_eq!(payload.get("last"), Some(&json!(true)));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_series_latest_supports_oneshot_filter() {
    let paths = new_router_fixture("router-series-latest-oneshot-filter").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("series latest oneshot db should open");
    sqlx::query(
        "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID, ONESHOT) VALUES (?, ?, ?, ?, ?, ?)",
    )
        .bind("series-oneshot")
        .bind(0_i64)
        .bind("OneShot Series")
        .bind("series/series-oneshot")
        .bind("library-1")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("oneshot series row should insert");
    sqlx::query(
        "INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, PUBLISHER, LANGUAGE, AGE_RATING, SERIES_ID) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
        .bind("ONGOING")
        .bind("OneShot Series")
        .bind("OneShot Series")
        .bind("PubHouse")
        .bind("EN")
        .bind(16_i64)
        .bind("series-oneshot")
        .execute(&pool)
        .await
        .expect("oneshot series metadata should insert");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let oneshot_true_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/latest?oneshot=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series latest oneshot=true request should build"),
        )
        .await
        .expect("series latest oneshot=true request should complete");
    assert_eq!(oneshot_true_response.status(), StatusCode::OK);
    let oneshot_true_payload = response_json(oneshot_true_response).await;
    let oneshot_true_content = oneshot_true_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("series latest oneshot=true payload should expose content array");
    assert_eq!(oneshot_true_content.len(), 1);
    assert_eq!(
        oneshot_true_content[0].get("id"),
        Some(&json!("series-oneshot"))
    );

    let oneshot_false_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/latest?oneshot=false")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series latest oneshot=false request should build"),
        )
        .await
        .expect("series latest oneshot=false request should complete");
    assert_eq!(oneshot_false_response.status(), StatusCode::OK);
    let oneshot_false_payload = response_json(oneshot_false_response).await;
    let oneshot_false_content = oneshot_false_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("series latest oneshot=false payload should expose content array");
    assert_eq!(oneshot_false_content.len(), 1);
    assert_eq!(oneshot_false_content[0].get("id"), Some(&json!("series-1")));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_series_new_sorts_by_created_desc() {
    let paths = new_router_fixture("router-series-new-created-desc").await;
    seed_router_contract_data(&paths).await;
    seed_router_custom_series(&paths, "series-new", "New Series", "library-1").await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("series new created-desc db should open");
    sqlx::query("UPDATE SERIES SET LAST_MODIFIED_DATE = ?, CREATED_DATE = ? WHERE ID = ?")
        .bind("2025-03-01 00:00:00")
        .bind("2025-01-01 00:00:00")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("baseline series timestamps should update");
    sqlx::query("UPDATE SERIES SET LAST_MODIFIED_DATE = ?, CREATED_DATE = ? WHERE ID = ?")
        .bind("2025-02-01 00:00:00")
        .bind("2025-04-01 00:00:00")
        .bind("series-new")
        .execute(&pool)
        .await
        .expect("new series timestamps should update");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/new")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series new request should build"),
        )
        .await
        .expect("series new request should complete");
    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("series new payload should expose content array");
    assert_eq!(content.len(), 2);
    assert_eq!(content[0].get("id"), Some(&json!("series-new")));
    assert_eq!(content[1].get("id"), Some(&json!("series-1")));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_series_updated_excludes_newly_added_series() {
    let paths = new_router_fixture("router-series-updated-excludes-newly-added").await;
    seed_router_contract_data(&paths).await;
    seed_router_custom_series(&paths, "series-new", "New Series", "library-1").await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("series updated db should open");
    sqlx::query("UPDATE SERIES SET LAST_MODIFIED_DATE = ?, CREATED_DATE = ? WHERE ID = ?")
        .bind("2025-04-01 00:00:00")
        .bind("2025-01-01 00:00:00")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("updated series timestamps should update");
    sqlx::query("UPDATE SERIES SET LAST_MODIFIED_DATE = ?, CREATED_DATE = ? WHERE ID = ?")
        .bind("2025-04-02 00:00:00")
        .bind("2025-04-02 00:00:00")
        .bind("series-new")
        .execute(&pool)
        .await
        .expect("newly added series timestamps should update");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/updated")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series updated request should build"),
        )
        .await
        .expect("series updated request should complete");
    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("series updated payload should expose content array");
    assert_eq!(content.len(), 1);
    assert_eq!(content[0].get("id"), Some(&json!("series-1")));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_series_updated_unpaged_keeps_kotlin_page_shape() {
    let paths = new_router_fixture("router-series-updated-unpaged-shape").await;
    seed_router_contract_data(&paths).await;
    seed_router_custom_series(&paths, "series-new", "New Series", "library-1").await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("series updated unpaged db should open");
    sqlx::query("UPDATE SERIES SET LAST_MODIFIED_DATE = ?, CREATED_DATE = ? WHERE ID = ?")
        .bind("2025-04-01 00:00:00")
        .bind("2025-01-01 00:00:00")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("updated series timestamps should update for unpaged shape test");
    sqlx::query("UPDATE SERIES SET LAST_MODIFIED_DATE = ?, CREATED_DATE = ? WHERE ID = ?")
        .bind("2025-04-02 00:00:00")
        .bind("2025-04-02 00:00:00")
        .bind("series-new")
        .execute(&pool)
        .await
        .expect("newly added series timestamps should update for unpaged shape test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/updated?unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series updated unpaged request should build"),
        )
        .await
        .expect("series updated unpaged request should complete");
    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("series updated unpaged payload should expose content array");
    assert_eq!(content.len(), 1);
    assert_eq!(content[0].get("id"), Some(&json!("series-1")));
    assert_eq!(payload.get("size"), Some(&json!(20)));
    assert_eq!(payload.get("number"), Some(&json!(0)));
    assert_eq!(payload.get("totalElements"), Some(&json!(1)));
    assert_eq!(payload.get("totalPages"), Some(&json!(1)));
    let pageable = payload
        .get("pageable")
        .expect("series updated unpaged payload should expose pageable object");
    assert_eq!(pageable.get("pageSize"), Some(&json!(20)));
    assert_eq!(pageable.get("pageNumber"), Some(&json!(0)));
    assert_eq!(pageable.get("paged"), Some(&json!(true)));
    assert_eq!(pageable.get("unpaged"), Some(&json!(false)));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_series_latest_rejects_malformed_boolean_filters() {
    let paths = new_router_fixture("router-series-latest-invalid-boolean-filter").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let invalid_deleted_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/latest?deleted=foo")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series latest invalid deleted filter request should build"),
        )
        .await
        .expect("series latest invalid deleted filter request should complete");
    assert_eq!(invalid_deleted_response.status(), StatusCode::BAD_REQUEST);

    let invalid_oneshot_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/latest?oneshot=foo")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series latest invalid oneshot filter request should build"),
        )
        .await
        .expect("series latest invalid oneshot filter request should complete");
    assert_eq!(invalid_oneshot_response.status(), StatusCode::BAD_REQUEST);

    cleanup_router_fixture(paths);
}
