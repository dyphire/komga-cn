use super::*;

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
async fn router_discovery_books_list_sorts_runtime_owned_results_by_number_series_id_and_alias_dates()
 {
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
