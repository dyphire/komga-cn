use super::*;

#[tokio::test]
async fn runtime_aggregate_series_metadata_refreshes_series_books_metadata_surfaces() {
    let paths = new_router_fixture("runtime-aggregate-series-books-metadata-surfaces").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for series booksMetadata aggregation fixture");
    sqlx::query("UPDATE BOOK_METADATA SET SUMMARY = ?, RELEASE_DATE = ? WHERE BOOK_ID = ?")
        .bind("Updated aggregate summary")
        .bind("2023-12-24")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book metadata should update for aggregation fixture");
    sqlx::query("DELETE FROM BOOK_METADATA_AUTHOR WHERE BOOK_ID = ?")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book metadata authors should clear for aggregation fixture");
    sqlx::query("DELETE FROM BOOK_METADATA_TAG WHERE BOOK_ID = ?")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book metadata tags should clear for aggregation fixture");
    sqlx::query("INSERT INTO BOOK_METADATA_AUTHOR (BOOK_ID, NAME, ROLE) VALUES (?, ?, ?)")
        .bind("book-1")
        .bind("Updated Writer")
        .bind("writer")
        .execute(&pool)
        .await
        .expect("first aggregated author should insert for aggregation fixture");
    sqlx::query("INSERT INTO BOOK_METADATA_AUTHOR (BOOK_ID, NAME, ROLE) VALUES (?, ?, ?)")
        .bind("book-1")
        .bind("Updated Penciller")
        .bind("penciller")
        .execute(&pool)
        .await
        .expect("second aggregated author should insert for aggregation fixture");
    sqlx::query("INSERT INTO BOOK_METADATA_TAG (BOOK_ID, TAG) VALUES (?, ?)")
        .bind("book-1")
        .bind("second-tag")
        .execute(&pool)
        .await
        .expect("first aggregated tag should insert for aggregation fixture");
    sqlx::query("INSERT INTO BOOK_METADATA_TAG (BOOK_ID, TAG) VALUES (?, ?)")
        .bind("book-1")
        .bind("updated-tag")
        .execute(&pool)
        .await
        .expect("second aggregated tag should insert for aggregation fixture");
    pool.close().await;

    let config = runtime_config_for_paths(&paths);
    let runtime = runtime_task_context_from_config(&config);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler
        .enqueue(
            TaskQueueRecord::new(
                "AggregateSeriesMetadata_series-1",
                1_000,
                Some("series-1".to_string()),
            )
            .with_simple_type("AggregateSeriesMetadata"),
        )
        .await;
    let processed = scheduler
        .process_available(&runtime)
        .await
        .expect("aggregate-series-metadata task should process for series booksMetadata fixture");
    assert_eq!(processed, 1);

    let app = build_router_with_config(&config).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let series_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/series-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series detail request should build"),
        )
        .await
        .expect("series detail request should complete");
    assert_eq!(series_response.status(), StatusCode::OK);

    let payload = response_json(series_response).await;
    let books_metadata = payload
        .get("booksMetadata")
        .and_then(Value::as_object)
        .expect("series detail payload should expose booksMetadata");
    assert_eq!(
        books_metadata.get("summary"),
        Some(&Value::String("Updated aggregate summary".to_string()))
    );
    assert_eq!(
        books_metadata.get("summaryNumber"),
        Some(&Value::String("1".to_string()))
    );
    assert_eq!(
        books_metadata.get("releaseDate"),
        Some(&Value::String("2023-12-24".to_string()))
    );

    let mut tags = books_metadata
        .get("tags")
        .and_then(Value::as_array)
        .expect("series booksMetadata tags should be an array")
        .iter()
        .filter_map(Value::as_str)
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    tags.sort();
    assert_eq!(
        tags,
        vec!["second-tag".to_string(), "updated-tag".to_string()]
    );

    let mut authors = books_metadata
        .get("authors")
        .and_then(Value::as_array)
        .expect("series booksMetadata authors should be an array")
        .iter()
        .map(|author| {
            (
                author
                    .get("name")
                    .and_then(Value::as_str)
                    .expect("aggregated author should expose name")
                    .to_string(),
                author
                    .get("role")
                    .and_then(Value::as_str)
                    .expect("aggregated author should expose role")
                    .to_string(),
            )
        })
        .collect::<Vec<_>>();
    authors.sort();
    assert_eq!(
        authors,
        vec![
            ("Updated Penciller".to_string(), "penciller".to_string()),
            ("Updated Writer".to_string(), "writer".to_string()),
        ]
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_aggregate_series_metadata_preserves_series_metadata_title_and_sort() {
    let paths = new_router_fixture("runtime-aggregate-series-preserves-metadata-title").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for metadata title aggregation fixture");
    sqlx::query("UPDATE SERIES SET NAME = ? WHERE ID = ?")
        .bind("Renamed Series Shelf")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series name should update for metadata title aggregation fixture");
    sqlx::query("UPDATE SERIES_METADATA SET TITLE = ?, TITLE_SORT = ? WHERE SERIES_ID = ?")
        .bind("Curated Series Title")
        .bind("Curated Sort Title")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series metadata title should update for metadata title aggregation fixture");
    pool.close().await;

    let config = runtime_config_for_paths(&paths);
    let runtime = runtime_task_context_from_config(&config);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler
        .enqueue(
            TaskQueueRecord::new(
                "AggregateSeriesMetadata_series-1",
                1_000,
                Some("series-1".to_string()),
            )
            .with_simple_type("AggregateSeriesMetadata"),
        )
        .await;
    let processed = scheduler
        .process_available(&runtime)
        .await
        .expect("aggregate-series-metadata task should preserve series metadata title fields");
    assert_eq!(processed, 1);

    let app = build_router_with_config(&config).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let series_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/series-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series detail request should build"),
        )
        .await
        .expect("series detail request should complete");
    assert_eq!(series_response.status(), StatusCode::OK);

    let payload = response_json(series_response).await;
    assert_eq!(
        payload.get("name"),
        Some(&Value::String("Renamed Series Shelf".to_string()))
    );
    assert_eq!(
        payload
            .get("metadata")
            .and_then(Value::as_object)
            .and_then(|metadata| metadata.get("title")),
        Some(&Value::String("Curated Series Title".to_string()))
    );
    assert_eq!(
        payload
            .get("metadata")
            .and_then(Value::as_object)
            .and_then(|metadata| metadata.get("titleSort")),
        Some(&Value::String("Curated Sort Title".to_string()))
    );

    cleanup_router_fixture(paths);
}
