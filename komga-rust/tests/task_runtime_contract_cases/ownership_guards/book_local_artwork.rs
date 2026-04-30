use super::*;

#[tokio::test]
async fn runtime_skips_book_local_artwork_refresh_when_library_import_local_artwork_is_disabled() {
    let paths = new_router_fixture("runtime-skip-book-local-artwork-when-import-disabled").await;
    seed_router_contract_data(&paths).await;

    let sidecar_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&sidecar_dir).expect("book artwork sidecar directory should exist");
    std::fs::write(sidecar_dir.join("book-1.png"), fixture_png_bytes())
        .expect("book artwork sidecar fixture should be written");

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for local artwork disabled fixture setup");
    sqlx::query("UPDATE LIBRARY SET IMPORT_LOCAL_ARTWORK = 0 WHERE ID = ?")
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("library import-local-artwork flag should be disabled");
    sqlx::query("DELETE FROM THUMBNAIL_BOOK WHERE BOOK_ID = ?")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("existing book thumbnails should be cleared before local artwork gating test");
    sqlx::query(
        "INSERT INTO SIDECAR (URL, PARENT_URL, LAST_MODIFIED_TIME, LIBRARY_ID) VALUES (?, ?, ?, ?)",
    )
    .bind("books/book-1.png")
    .bind("books/book-1.epub")
    .bind(1_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("book artwork sidecar row should be inserted");
    pool.close().await;

    let runtime = runtime_task_context(&paths).await;
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    scheduler
        .enqueue(TaskQueueRecord::new(
            "RefreshBookLocalArtwork:book-1",
            1_000,
            Some("book-1".to_string()),
        ))
        .await;
    scheduler.process_available(&runtime).await.expect(
        "book local artwork refresh should skip cleanly when library.importLocalArtwork is disabled",
    );

    let verify_pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for local artwork disabled verification");
    let sidecar_thumbnail_count = sqlx::query(
        "SELECT COUNT(*) AS COUNT FROM THUMBNAIL_BOOK WHERE BOOK_ID = ? AND TYPE = 'SIDECAR'",
    )
    .bind("book-1")
    .fetch_one(&verify_pool)
    .await
    .expect("sidecar thumbnail rows should be queryable")
    .get::<i64, _>("COUNT");
    verify_pool.close().await;

    assert_eq!(
        sidecar_thumbnail_count, 0,
        "runtime must not import book local artwork when library.importLocalArtwork is disabled",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_executes_kotlin_persisted_refresh_book_local_artwork_task() {
    let paths = new_router_fixture("runtime-executes-kotlin-refresh-book-local-artwork-task").await;
    seed_router_contract_data(&paths).await;

    let sidecar_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&sidecar_dir).expect("book artwork sidecar directory should exist");
    std::fs::write(sidecar_dir.join("book-1.png"), fixture_png_bytes())
        .expect("book artwork sidecar fixture should be written");

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for Kotlin persisted local artwork fixture setup");
    sqlx::query("UPDATE LIBRARY SET IMPORT_LOCAL_ARTWORK = 1 WHERE ID = ?")
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("library import-local-artwork flag should be enabled");
    sqlx::query("DELETE FROM THUMBNAIL_BOOK WHERE BOOK_ID = ?")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("existing book thumbnails should be cleared before Kotlin persisted task test");
    sqlx::query(
        "INSERT INTO SIDECAR (URL, PARENT_URL, LAST_MODIFIED_TIME, LIBRARY_ID) VALUES (?, ?, ?, ?)",
    )
    .bind("books/book-1.png")
    .bind("books/book-1.epub")
    .bind(1_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("book artwork sidecar row should be inserted for Kotlin persisted task test");
    pool.close().await;

    let tasks_pool = connect_test_pool(paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for Kotlin persisted local artwork task setup");
    sqlx::query(
        "INSERT INTO TASK (ID, PRIORITY, GROUP_ID, CLASS, SIMPLE_TYPE, PAYLOAD, OWNER) VALUES (?, ?, ?, ?, ?, ?, NULL)",
    )
    .bind("RefreshBookLocalArtwork_book-1")
    .bind(80_i64)
    .bind(Option::<String>::None)
    .bind("org.gotson.komga.application.tasks.Task$RefreshBookLocalArtwork")
    .bind("RefreshBookLocalArtwork")
    .bind(
        json!({
            "bookId": "book-1",
            "priority": 80,
            "groupId": Value::Null,
            "uniqueId": "RefreshBookLocalArtwork_book-1"
        })
        .to_string(),
    )
    .execute(&tasks_pool)
    .await
    .expect("Kotlin persisted local artwork task row should be inserted");
    tasks_pool.close().await;

    let runtime = runtime_task_context(&paths).await;
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    scheduler.process_available(&runtime).await.expect(
        "runtime should execute Kotlin persisted RefreshBookLocalArtwork tasks successfully",
    );

    let verify_pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for Kotlin persisted local artwork verification");
    let row = sqlx::query(
        "SELECT TYPE, URL, SELECTED FROM THUMBNAIL_BOOK WHERE BOOK_ID = ? ORDER BY ID ASC LIMIT 1",
    )
    .bind("book-1")
    .fetch_one(&verify_pool)
    .await
    .expect("sidecar thumbnail row should be queryable after Kotlin persisted task execution");
    verify_pool.close().await;

    assert_eq!(row.get::<String, _>("TYPE"), "SIDECAR");
    assert_eq!(row.get::<String, _>("URL"), "books/book-1.png");
    assert!(
        row.get::<bool, _>("SELECTED"),
        "executed Kotlin persisted local artwork task should import a selected SIDECAR thumbnail",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_imports_multiple_filesystem_book_local_artworks_and_selects_only_one_when_none_exists()
 {
    let paths =
        new_router_fixture("runtime-imports-multiple-filesystem-book-local-artworks-none-selected")
            .await;
    seed_router_contract_data(&paths).await;

    let sidecar_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&sidecar_dir).expect("book artwork directory should exist");
    std::fs::write(sidecar_dir.join("book-1.png"), fixture_png_bytes())
        .expect("primary local artwork should be written");
    std::fs::write(sidecar_dir.join("book-1-1.jpg"), fixture_png_bytes())
        .expect("secondary local artwork should be written");
    std::fs::write(sidecar_dir.join("book-12.png"), fixture_png_bytes())
        .expect("non-matching artwork should be written");

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for multi-artwork fixture setup");
    sqlx::query("UPDATE LIBRARY SET IMPORT_LOCAL_ARTWORK = 1 WHERE ID = ?")
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("library import-local-artwork flag should be enabled");
    sqlx::query("DELETE FROM THUMBNAIL_BOOK WHERE BOOK_ID = ?")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("existing thumbnails should be cleared for multi-artwork import test");
    pool.close().await;

    let runtime = runtime_task_context(&paths).await;
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    scheduler
        .enqueue(
            TaskQueueRecord::new("RefreshBookLocalArtwork_book-1", 1_000, None)
                .with_simple_type("RefreshBookLocalArtwork"),
        )
        .await;
    scheduler
        .process_available(&runtime)
        .await
        .expect("book local artwork refresh should import multiple filesystem candidates cleanly");

    let verify_pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for multi-artwork verification");
    let rows = sqlx::query(
        "SELECT TYPE, URL, SELECTED FROM THUMBNAIL_BOOK WHERE BOOK_ID = ? ORDER BY URL ASC",
    )
    .bind("book-1")
    .fetch_all(&verify_pool)
    .await
    .expect("book local artwork rows should be queryable after filesystem import");
    verify_pool.close().await;

    let urls = rows
        .iter()
        .map(|row| row.get::<Option<String>, _>("URL"))
        .collect::<Vec<_>>();
    let selected_count = rows
        .iter()
        .filter(|row| row.get::<bool, _>("SELECTED"))
        .count();

    assert_eq!(
        rows.len(),
        2,
        "runtime should import every matching local artwork file"
    );
    assert_eq!(
        urls,
        vec![
            Some("books/book-1-1.jpg".to_string()),
            Some("books/book-1.png".to_string())
        ],
        "runtime should only import basename and basename-<n> local artwork candidates",
    );
    assert!(
        rows.iter()
            .all(|row| row.get::<String, _>("TYPE") == "SIDECAR"),
        "runtime should import filesystem local artwork files as SIDECAR thumbnails",
    );
    assert_eq!(
        selected_count, 1,
        "runtime should select exactly one imported local artwork when no thumbnail was previously selected",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_preserves_existing_non_generated_selection_when_importing_book_local_artworks() {
    let paths =
        new_router_fixture("runtime-preserves-non-generated-selection-for-book-local-artworks")
            .await;
    seed_router_contract_data(&paths).await;

    let sidecar_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&sidecar_dir).expect("book artwork directory should exist");
    std::fs::write(sidecar_dir.join("book-1.png"), fixture_png_bytes())
        .expect("primary local artwork should be written");
    std::fs::write(sidecar_dir.join("book-1-1.jpg"), fixture_png_bytes())
        .expect("secondary local artwork should be written");

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for non-generated selection fixture setup");
    sqlx::query("UPDATE LIBRARY SET IMPORT_LOCAL_ARTWORK = 1 WHERE ID = ?")
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("library import-local-artwork flag should be enabled");
    sqlx::query("DELETE FROM THUMBNAIL_BOOK WHERE BOOK_ID = ? AND TYPE = 'SIDECAR'")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect(
            "existing sidecar thumbnails should be cleared before non-generated selection test",
        );
    pool.close().await;

    let runtime = runtime_task_context(&paths).await;
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    scheduler
        .enqueue(
            TaskQueueRecord::new("RefreshBookLocalArtwork_book-1", 1_000, None)
                .with_simple_type("RefreshBookLocalArtwork"),
        )
        .await;
    scheduler.process_available(&runtime).await.expect(
        "book local artwork refresh should preserve existing non-generated selections cleanly",
    );

    let verify_pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for non-generated selection verification");
    let rows = sqlx::query(
        "SELECT ID, TYPE, URL, SELECTED FROM THUMBNAIL_BOOK WHERE BOOK_ID = ? ORDER BY TYPE ASC, URL ASC, ID ASC",
    )
    .bind("book-1")
    .fetch_all(&verify_pool)
    .await
    .expect("book thumbnail rows should be queryable after preserving non-generated selection");
    verify_pool.close().await;

    let selected_rows = rows
        .iter()
        .filter(|row| row.get::<bool, _>("SELECTED"))
        .collect::<Vec<_>>();
    let imported_sidecars = rows
        .iter()
        .filter(|row| row.get::<String, _>("TYPE") == "SIDECAR")
        .collect::<Vec<_>>();

    assert_eq!(
        imported_sidecars.len(),
        2,
        "runtime should still import all matching local artworks"
    );
    assert_eq!(
        selected_rows.len(),
        1,
        "runtime should keep exactly one selected thumbnail"
    );
    assert_eq!(selected_rows[0].get::<String, _>("ID"), "thumb-book-1");
    assert_eq!(selected_rows[0].get::<String, _>("TYPE"), "USER_UPLOADED");
    assert!(
        imported_sidecars
            .iter()
            .all(|row| !row.get::<bool, _>("SELECTED")),
        "runtime should not override an existing non-generated selected thumbnail when importing local artworks",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_replaces_generated_selection_when_importing_book_local_artworks() {
    let paths =
        new_router_fixture("runtime-replaces-generated-selection-for-book-local-artworks").await;
    seed_router_contract_data(&paths).await;

    let sidecar_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&sidecar_dir).expect("book artwork directory should exist");
    std::fs::write(sidecar_dir.join("book-1.png"), fixture_png_bytes())
        .expect("primary local artwork should be written");
    std::fs::write(sidecar_dir.join("book-1-1.jpg"), fixture_png_bytes())
        .expect("secondary local artwork should be written");

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for generated selection fixture setup");
    sqlx::query("UPDATE LIBRARY SET IMPORT_LOCAL_ARTWORK = 1 WHERE ID = ?")
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("library import-local-artwork flag should be enabled");
    sqlx::query("DELETE FROM THUMBNAIL_BOOK WHERE BOOK_ID = ?")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("existing thumbnails should be cleared before generated selection test");
    sqlx::query(
        "INSERT INTO THUMBNAIL_BOOK (ID, BOOK_ID, TYPE, SELECTED, THUMBNAIL, MEDIA_TYPE) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("thumb-generated-book-1")
    .bind("book-1")
    .bind("GENERATED")
    .bind(true)
    .bind(fixture_png_bytes())
    .bind("image/png")
    .execute(&pool)
    .await
    .expect("generated selected thumbnail should be seeded for local artwork selection test");
    pool.close().await;

    let runtime = runtime_task_context(&paths).await;
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    scheduler
        .enqueue(
            TaskQueueRecord::new("RefreshBookLocalArtwork_book-1", 1_000, None)
                .with_simple_type("RefreshBookLocalArtwork"),
        )
        .await;
    scheduler
        .process_available(&runtime)
        .await
        .expect("book local artwork refresh should replace generated selection cleanly");

    let verify_pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for generated selection verification");
    let rows = sqlx::query(
        "SELECT ID, TYPE, URL, SELECTED FROM THUMBNAIL_BOOK WHERE BOOK_ID = ? ORDER BY TYPE ASC, URL ASC, ID ASC",
    )
    .bind("book-1")
    .fetch_all(&verify_pool)
    .await
    .expect("book thumbnail rows should be queryable after generated selection replacement");
    verify_pool.close().await;

    let selected_rows = rows
        .iter()
        .filter(|row| row.get::<bool, _>("SELECTED"))
        .collect::<Vec<_>>();
    let generated_rows = rows
        .iter()
        .filter(|row| row.get::<String, _>("TYPE") == "GENERATED")
        .collect::<Vec<_>>();
    let imported_sidecars = rows
        .iter()
        .filter(|row| row.get::<String, _>("TYPE") == "SIDECAR")
        .collect::<Vec<_>>();

    assert_eq!(
        imported_sidecars.len(),
        2,
        "runtime should import all matching local artworks"
    );
    assert_eq!(
        generated_rows.len(),
        1,
        "runtime should retain the pre-existing generated thumbnail row"
    );
    assert_eq!(
        selected_rows.len(),
        1,
        "runtime should keep exactly one selected thumbnail after import"
    );
    assert_eq!(selected_rows[0].get::<String, _>("TYPE"), "SIDECAR");
    assert!(
        !generated_rows[0].get::<bool, _>("SELECTED"),
        "runtime should unselect previously selected GENERATED thumbnails when the first local artwork is imported",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_book_local_artwork_refresh_emits_thumbnail_book_added_events() {
    let _guard = runtime_sse_contract_guard().await;
    let paths = new_router_fixture("runtime-book-local-artwork-sse-events").await;
    seed_router_contract_data(&paths).await;

    let sidecar_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&sidecar_dir).expect("book artwork directory should exist");
    std::fs::write(sidecar_dir.join("book-1.png"), fixture_png_bytes())
        .expect("primary local artwork should be written");
    std::fs::write(sidecar_dir.join("book-1-1.jpg"), fixture_png_bytes())
        .expect("secondary local artwork should be written");

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for local artwork sse fixture setup");
    sqlx::query("UPDATE LIBRARY SET IMPORT_LOCAL_ARTWORK = 1 WHERE ID = ?")
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("library import-local-artwork flag should be enabled");
    sqlx::query("DELETE FROM THUMBNAIL_BOOK WHERE BOOK_ID = ?")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("existing thumbnails should be cleared before local artwork sse test");
    pool.close().await;

    let cursor = komga_application::runtime_sse::current_runtime_sse_event_cursor();
    let runtime = runtime_task_context(&paths).await;
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    scheduler
        .enqueue(
            TaskQueueRecord::new("RefreshBookLocalArtwork_book-1", 1_000, None)
                .with_simple_type("RefreshBookLocalArtwork"),
        )
        .await;
    scheduler
        .process_available(&runtime)
        .await
        .expect("book local artwork refresh should complete for sse contract");

    let (_, events) = komga_application::runtime_sse::pending_runtime_sse_events(
        cursor,
        "runtime-contract-admin",
        true,
    );
    let thumbnail_events = events
        .iter()
        .filter(|event| event.name == "ThumbnailBookAdded")
        .filter(|event| {
            event.payload.get("bookId").and_then(|value| value.as_str()) == Some("book-1")
        })
        .filter(|event| {
            event
                .payload
                .get("seriesId")
                .and_then(|value| value.as_str())
                == Some("series-1")
        })
        .collect::<Vec<_>>();

    assert!(
        thumbnail_events.len() >= 2,
        "book local artwork refresh should emit thumbnail events for imported sidecar artwork rows",
    );
    let selected_states = thumbnail_events
        .iter()
        .filter_map(|event| event.payload.get("selected").and_then(Value::as_bool))
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        selected_states,
        std::collections::BTreeSet::from([false, true]),
        "book local artwork refresh should emit both selected and unselected ThumbnailBookAdded event states",
    );

    cleanup_router_fixture(paths);
}
