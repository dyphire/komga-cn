use super::*;

#[tokio::test]
async fn isolated_runtime_keeps_search_index_external_owned() {
    let paths = new_router_fixture("isolated-runtime-external-search-index").await;
    seed_router_contract_data(&paths).await;

    let mut config = runtime_config_for_paths(&paths);
    config.mode = RuntimeMode::Isolated;
    config.writer_ownership_policy = WriterOwnershipPolicy {
        isolation_root: Some(paths.config_dir.clone()),
        allow_isolated_writes: true,
    };

    let mut scheduler = TaskQueueScheduler::for_runtime(config.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new("REBUILD_INDEX", 1_000, None));
    let processed = scheduler
        .process_available(&config)
        .expect("isolated runtime should process queued tasks without task-execution failure");
    assert_eq!(
        processed, 1,
        "fixture sanity: rebuild task should be consumed once"
    );

    let search = SearchIndexLifecycle::bootstrap(config.lucene_data_directory.as_path())
        .expect("search index should bootstrap for ownership assertions");
    let hits = search
        .search_ids("Book 1", SearchEntityType::Book, 10)
        .expect("search lookup should succeed for ownership assertions");
    assert!(
        hits.is_empty(),
        "isolated runtime should leave external-owned search index untouched",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_rejects_legacy_upgrade_index_task_contract() {
    let paths = new_router_fixture("runtime-rejects-legacy-upgrade-index-task").await;
    seed_router_contract_data(&paths).await;

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new("UPGRADE_INDEX", 1_000, None));

    let error = scheduler
        .process_available(&runtime)
        .expect_err("legacy upgrade index task should no longer be executable");

    assert!(
        error.is_unsupported_task(),
        "legacy upgrade index task must fail as unsupported instead of aliasing rebuild",
    );
    assert_eq!(
        error.to_string(),
        "unsupported runtime task type: UPGRADE_INDEX",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_incremental_index_sync_contract_covers_entity_lifecycle_and_metadata_refresh() {
    let paths = new_router_fixture("runtime-incremental-index-sync-contract").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for incremental index sync fixture setup");
    sqlx::query(
        "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID, oneshot) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("series-oneshot")
    .bind(0_i64)
    .bind("OneShot Series")
    .bind("series/series-oneshot")
    .bind("library-1")
    .bind(true)
    .execute(&pool)
    .await
    .expect("oneshot series row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES_METADATA (\
             STATUS, TITLE, TITLE_SORT, PUBLISHER, LANGUAGE, AGE_RATING, SERIES_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("ONGOING")
    .bind("OneShot Series")
    .bind("OneShot Series")
    .bind("Oneshot Publisher")
    .bind("EN")
    .bind(16_i64)
    .bind("series-oneshot")
    .execute(&pool)
    .await
    .expect("oneshot series metadata row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK (\
             ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID, oneshot) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-oneshot")
    .bind(0_i64)
    .bind("book-oneshot.cbz")
    .bind("books/book-oneshot.cbz")
    .bind("series-oneshot")
    .bind(1024_i64)
    .bind(2_i64)
    .bind("library-1")
    .bind(true)
    .execute(&pool)
    .await
    .expect("oneshot book row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, ISBN, BOOK_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("2")
    .bind(2.0_f64)
    .bind("OneShot Book")
    .bind("978-oneshot")
    .bind("book-oneshot")
    .execute(&pool)
    .await
    .expect("oneshot book metadata row should be inserted");

    sqlx::query(
        "INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) \
         VALUES (?, ?, ?, ?)",
    )
    .bind("application/epub+zip")
    .bind("READY")
    .bind("book-oneshot")
    .bind(1_i64)
    .execute(&pool)
    .await
    .expect("oneshot media row should be inserted");
    pool.close().await;

    let config = runtime_config_for_paths(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(config.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new("REBUILD_INDEX", 1_000, None));
    scheduler
        .process_available(&config)
        .expect("rebuild index task should succeed for incremental sync contract");

    write_stale_analyzer_version_marker(config.lucene_data_directory.as_path());

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for series metadata update fixture");
    sqlx::query(
        "UPDATE SERIES_METADATA \
         SET PUBLISHER = ? \
         WHERE SERIES_ID = ?",
    )
    .bind("Café 東京 Updated")
    .bind("series-oneshot")
    .execute(&pool)
    .await
    .expect("oneshot series publisher should be updated");
    pool.close().await;

    scheduler.enqueue(TaskQueueRecord::new(
        "REFRESH_SERIES_METADATA:series-oneshot",
        1_000,
        Some("series-oneshot".to_string()),
    ));
    scheduler
        .process_available(&config)
        .expect("refresh-series-metadata task should process for incremental sync contract");

    let app = build_router_with_config(&config);
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let search_hits = |query: &str, entity_type: SearchEntityType| -> Vec<String> {
        SearchIndexLifecycle::bootstrap(config.lucene_data_directory.as_path())
            .expect("search index should bootstrap for incremental sync contract")
            .search_ids(query, entity_type, 10)
            .expect("search lookup should succeed for incremental sync contract")
    };

    assert_eq!(
        search_hits("publisher:Updated", SearchEntityType::Book),
        vec!["book-oneshot".to_string()],
        "series metadata refresh task should update oneshot-derived book fields",
    );
    assert_eq!(
        search_hits("publisher:cafe", SearchEntityType::Book),
        vec!["book-oneshot".to_string()],
        "runtime-owned incremental sync should rebuild analyzer-drifted indexes before refreshing accent-folded inherited metadata",
    );
    assert_eq!(
        search_hits("publisher:東京", SearchEntityType::Book),
        vec!["book-oneshot".to_string()],
        "runtime-owned incremental sync should preserve CJK recall after analyzer-rollout rebuilds",
    );
    assert_eq!(
        fs::read_to_string(
            config
                .lucene_data_directory
                .join(ANALYZER_VERSION_MARKER_FILE)
        )
        .expect("incremental sync contract should leave a readable analyzer version marker"),
        search_analyzer_version().to_string(),
        "runtime-owned incremental sync should restore the current analyzer version marker after rebuilding a drifted index",
    );

    let create_collection_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/collections")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "name": "Task6 Collection",
                        "ordered": true,
                        "seriesIds": ["series-1"]
                    })
                    .to_string(),
                ))
                .expect("collection create request should build"),
        )
        .await
        .expect("collection create request should complete");
    assert_eq!(create_collection_response.status(), StatusCode::OK);
    let collection_payload = response_json(create_collection_response).await;
    let collection_id = collection_payload
        .get("id")
        .and_then(Value::as_str)
        .expect("collection create payload should include id")
        .to_string();
    assert_eq!(
        search_hits("Task6 Collection", SearchEntityType::Collection),
        vec![collection_id.clone()],
        "collection create should upsert search document",
    );

    let update_collection_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/v1/collections/{collection_id}"))
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "name": "Task6 Collection Updated",
                        "ordered": true,
                        "seriesIds": ["series-1"]
                    })
                    .to_string(),
                ))
                .expect("collection update request should build"),
        )
        .await
        .expect("collection update request should complete");
    assert_eq!(update_collection_response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        search_hits("Task6 Collection Updated", SearchEntityType::Collection),
        vec![collection_id.clone()],
        "collection update should refresh search document",
    );

    let delete_collection_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/collections/{collection_id}"))
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("collection delete request should build"),
        )
        .await
        .expect("collection delete request should complete");
    assert_eq!(delete_collection_response.status(), StatusCode::NO_CONTENT);
    assert!(
        search_hits("Task6 Collection Updated", SearchEntityType::Collection).is_empty(),
        "collection delete should remove search document",
    );

    let create_readlist_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/readlists")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "name": "Task6 ReadList",
                        "summary": "task6",
                        "ordered": true,
                        "bookIds": ["book-1"]
                    })
                    .to_string(),
                ))
                .expect("readlist create request should build"),
        )
        .await
        .expect("readlist create request should complete");
    assert_eq!(create_readlist_response.status(), StatusCode::OK);
    let readlist_payload = response_json(create_readlist_response).await;
    let readlist_id = readlist_payload
        .get("id")
        .and_then(Value::as_str)
        .expect("readlist create payload should include id")
        .to_string();
    assert_eq!(
        search_hits("Task6 ReadList", SearchEntityType::ReadList),
        vec![readlist_id.clone()],
        "readlist create should upsert search document",
    );

    let update_readlist_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/v1/readlists/{readlist_id}"))
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "name": "Task6 ReadList Updated",
                        "summary": "task6-updated",
                        "ordered": true,
                        "bookIds": ["book-1"]
                    })
                    .to_string(),
                ))
                .expect("readlist update request should build"),
        )
        .await
        .expect("readlist update request should complete");
    assert_eq!(update_readlist_response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        search_hits("Task6 ReadList Updated", SearchEntityType::ReadList),
        vec![readlist_id.clone()],
        "readlist update should refresh search document",
    );

    let delete_readlist_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/readlists/{readlist_id}"))
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlist delete request should build"),
        )
        .await
        .expect("readlist delete request should complete");
    assert_eq!(delete_readlist_response.status(), StatusCode::NO_CONTENT);
    assert!(
        search_hits("Task6 ReadList Updated", SearchEntityType::ReadList).is_empty(),
        "readlist delete should remove search document",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_delete_sync_recovers_from_analyzer_drift_before_removing_search_document() {
    let paths = new_router_fixture("runtime-delete-sync-analyzer-drift").await;
    seed_router_contract_data(&paths).await;

    let config = runtime_config_for_paths(&paths);
    let app = build_router_with_config(&config);
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let create_collection_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/collections")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "name": "Delete Drift Collection",
                        "ordered": true,
                        "seriesIds": ["series-1"]
                    })
                    .to_string(),
                ))
                .expect("collection create request should build"),
        )
        .await
        .expect("collection create request should complete");
    assert_eq!(create_collection_response.status(), StatusCode::OK);
    let collection_payload = response_json(create_collection_response).await;
    let collection_id = collection_payload
        .get("id")
        .and_then(Value::as_str)
        .expect("collection create payload should include id")
        .to_string();

    let search_hits = |query: &str, entity_type: SearchEntityType| -> Vec<String> {
        SearchIndexLifecycle::bootstrap(config.lucene_data_directory.as_path())
            .expect("search index should bootstrap for delete sync contract")
            .search_ids(query, entity_type, 10)
            .expect("search lookup should succeed for delete sync contract")
    };

    assert_eq!(
        search_hits("Delete Drift Collection", SearchEntityType::Collection),
        vec![collection_id.clone()],
        "collection create should seed the search document before delete recovery is exercised",
    );

    write_stale_analyzer_version_marker(config.lucene_data_directory.as_path());

    let delete_collection_response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/collections/{collection_id}"))
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("collection delete request should build"),
        )
        .await
        .expect("collection delete request should complete");
    assert_eq!(delete_collection_response.status(), StatusCode::NO_CONTENT);
    assert!(
        search_hits("Delete Drift Collection", SearchEntityType::Collection).is_empty(),
        "runtime-owned delete sync should rebuild analyzer-drifted indexes before removing the stale search document",
    );
    assert_eq!(
        fs::read_to_string(
            config
                .lucene_data_directory
                .join(ANALYZER_VERSION_MARKER_FILE)
        )
        .expect("delete sync contract should leave a readable analyzer version marker"),
        search_analyzer_version().to_string(),
        "runtime-owned delete sync should restore the current analyzer version marker after rebuilding a drifted index",
    );

    cleanup_router_fixture(paths);
}
