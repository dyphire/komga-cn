use super::*;

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
