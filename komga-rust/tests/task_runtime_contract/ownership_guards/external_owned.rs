use super::*;

#[tokio::test]
async fn runtime_blocks_authentication_activity_cleanup_when_main_database_is_external_owned() {
    let paths = new_router_fixture("runtime-blocked-main-database-auth-cleanup").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for auth-cleanup fixture setup");
    sqlx::query(
        r#"
        INSERT INTO AUTHENTICATION_ACTIVITY (
            USER_ID,
            EMAIL,
            IP,
            USER_AGENT,
            SUCCESS,
            ERROR,
            DATE_TIME,
            SOURCE,
            API_KEY_ID,
            API_KEY_COMMENT
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind("admin-user")
    .bind("admin@example.org")
    .bind("127.0.0.1")
    .bind("test-agent")
    .bind(true)
    .bind(Option::<String>::None)
    .bind("2000-01-01 00:00:00")
    .bind("basic")
    .bind(Option::<String>::None)
    .bind(Option::<String>::None)
    .execute(&pool)
    .await
    .expect("authentication activity row should be inserted");
    pool.close().await;

    let runtime = TaskRuntimeContext {
        owns_main_database: false,
        ..runtime_task_context(&paths)
    };
    komga_rust::infrastructure::task_queue::cleanup_authentication_activity_once(&runtime).await;

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for auth-cleanup verification");
    let activity_rows = sqlx::query("SELECT COUNT(*) AS COUNT FROM AUTHENTICATION_ACTIVITY")
        .fetch_one(&verify_pool)
        .await
        .expect("authentication activity count should be queryable")
        .get::<i64, _>("COUNT");
    verify_pool.close().await;

    assert_eq!(
        activity_rows, 1,
        "runtime must not delete authentication activity rows when main database is external-owned",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_blocks_book_media_analysis_when_main_database_is_external_owned() {
    let paths = new_router_fixture("runtime-blocked-main-database-analyze-book").await;
    seed_router_contract_data(&paths).await;
    write_router_epub_resource(
        &paths,
        "books/book-1.epub",
        "OEBPS/chapter.xhtml",
        br#"<html xmlns='http://www.w3.org/1999/xhtml'><body><p>Analyze Fixture</p></body></html>"#,
    );

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for analyze-book fixture setup");
    sqlx::query(
        r#"
        UPDATE MEDIA
        SET STATUS = ?, PAGE_COUNT = ?
        WHERE BOOK_ID = ?
        "#,
    )
    .bind("ERROR")
    .bind(0_i64)
    .bind("book-1")
    .execute(&pool)
    .await
    .expect("media row should be downgraded for analyze-book fixture");
    sqlx::query(
        r#"
        INSERT INTO MEDIA_PAGE (
            FILE_NAME,
            MEDIA_TYPE,
            NUMBER,
            BOOK_ID,
            width,
            height,
            FILE_HASH,
            FILE_SIZE
        ) VALUES (?, ?, ?, ?, NULL, NULL, ?, ?)
        "#,
    )
    .bind("stale-page.xhtml")
    .bind("application/xhtml+xml")
    .bind(1_i64)
    .bind("book-1")
    .bind("stale-page-hash")
    .bind(123_i64)
    .execute(&pool)
    .await
    .expect("stale media page row should be inserted");
    pool.close().await;

    let runtime = TaskRuntimeContext {
        owns_main_database: false,
        owns_search_index: false,
        ..runtime_task_context(&paths)
    };
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "ANALYZE_BOOK:book-1",
        1_000,
        Some("book-1".to_string()),
    ));
    scheduler
        .process_available(&runtime)
        .expect("blocked main-database analyze-book should still drain cleanly");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for analyze-book verification");
    let media_row = sqlx::query(
        r#"
        SELECT STATUS, PAGE_COUNT
        FROM MEDIA
        WHERE BOOK_ID = ?
        LIMIT 1
        "#,
    )
    .bind("book-1")
    .fetch_one(&verify_pool)
    .await
    .expect("media row should be queryable");
    let stale_page_rows = sqlx::query(
        r#"
        SELECT COUNT(*) AS COUNT
        FROM MEDIA_PAGE
        WHERE BOOK_ID = ?
        AND FILE_NAME = ?
        AND FILE_HASH = ?
        "#,
    )
    .bind("book-1")
    .bind("stale-page.xhtml")
    .bind("stale-page-hash")
    .fetch_one(&verify_pool)
    .await
    .expect("stale media page rows should be queryable")
    .get::<i64, _>("COUNT");
    verify_pool.close().await;

    assert_eq!(
        media_row.get::<String, _>("STATUS"),
        "ERROR",
        "runtime must not rewrite MEDIA status during analyze-book when main database is external-owned",
    );
    assert_eq!(
        media_row.get::<i64, _>("PAGE_COUNT"),
        0,
        "runtime must not rewrite MEDIA page count during analyze-book when main database is external-owned",
    );
    assert_eq!(
        stale_page_rows, 1,
        "runtime must not replace MEDIA_PAGE rows during analyze-book when main database is external-owned",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_blocks_sidecar_metadata_refresh_when_sidecar_output_is_external_owned() {
    let paths = new_router_fixture("runtime-blocked-sidecar-output").await;
    seed_router_contract_data(&paths).await;

    let sidecar_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&sidecar_dir).expect("book sidecar directory should be created");
    std::fs::write(
        sidecar_dir.join("book-1.xml"),
        br#"<ComicInfo><Title>Blocked Sidecar Title</Title></ComicInfo>"#,
    )
    .expect("book sidecar fixture should be written");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for sidecar fixture setup");
    sqlx::query(
        "INSERT INTO SIDECAR (URL, PARENT_URL, LAST_MODIFIED_TIME, LIBRARY_ID) VALUES (?, ?, ?, ?)",
    )
    .bind("books/book-1.xml")
    .bind("books/book-1.epub")
    .bind(1_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("book sidecar row should be inserted");
    pool.close().await;

    let runtime = TaskRuntimeContext {
        owns_sidecar_output: false,
        ..runtime_task_context(&paths)
    };
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "REFRESH_BOOK_METADATA:book-1",
        1_000,
        Some("book-1".to_string()),
    ));
    scheduler
        .process_available(&runtime)
        .expect("blocked sidecar metadata refresh should still drain cleanly");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for verification");
    let title = sqlx::query("SELECT TITLE FROM BOOK_METADATA WHERE BOOK_ID = ? LIMIT 1")
        .bind("book-1")
        .fetch_one(&verify_pool)
        .await
        .expect("book metadata title should be queryable")
        .get::<String, _>("TITLE");
    verify_pool.close().await;

    assert_eq!(
        title, "Book 1",
        "runtime must not apply sidecar metadata when sidecar output is external-owned",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_blocks_series_metadata_aggregation_when_main_database_is_external_owned() {
    let paths = new_router_fixture("runtime-blocked-main-database-aggregation").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for aggregation fixture setup");
    sqlx::query("UPDATE SERIES SET NAME = ? WHERE ID = ?")
        .bind("Renamed Series From Main DB")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series name should be updated for aggregation fixture");
    sqlx::query(
        "UPDATE SERIES_METADATA \
         SET TITLE = ?, TITLE_SORT = ? \
         WHERE SERIES_ID = ?",
    )
    .bind("Original Aggregation Title")
    .bind("Original Aggregation Title")
    .bind("series-1")
    .execute(&pool)
    .await
    .expect("series metadata title should be updated for aggregation fixture");
    pool.close().await;

    let runtime = TaskRuntimeContext {
        owns_main_database: false,
        ..runtime_task_context(&paths)
    };
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "AGGREGATE_SERIES_METADATA:series-1",
        1_000,
        Some("series-1".to_string()),
    ));
    scheduler
        .process_available(&runtime)
        .expect("blocked main-database aggregation should still drain cleanly");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for aggregation verification");
    let row =
        sqlx::query("SELECT TITLE, TITLE_SORT FROM SERIES_METADATA WHERE SERIES_ID = ? LIMIT 1")
            .bind("series-1")
            .fetch_one(&verify_pool)
            .await
            .expect("series metadata aggregation row should be queryable");
    verify_pool.close().await;

    assert_eq!(
        row.get::<String, _>("TITLE"),
        "Original Aggregation Title",
        "runtime must not aggregate series metadata when main database is external-owned",
    );
    assert_eq!(
        row.get::<String, _>("TITLE_SORT"),
        "Original Aggregation Title",
        "runtime must not rewrite title sort when main database is external-owned",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_blocks_empty_trash_cleanup_when_main_database_is_external_owned() {
    let paths = new_router_fixture("runtime-blocked-main-database-empty-trash").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for cleanup fixture setup");
    sqlx::query("DELETE FROM COLLECTION_SERIES WHERE COLLECTION_ID = ?")
        .bind("collection-1")
        .execute(&pool)
        .await
        .expect("collection members should be removed for cleanup fixture");
    sqlx::query("DELETE FROM READLIST_BOOK WHERE READLIST_ID = ?")
        .bind("readlist-1")
        .execute(&pool)
        .await
        .expect("readlist members should be removed for cleanup fixture");
    sqlx::query("INSERT OR REPLACE INTO SERVER_SETTINGS (KEY, VALUE) VALUES (?, ?)")
        .bind("DELETE_EMPTY_COLLECTIONS")
        .bind("true")
        .execute(&pool)
        .await
        .expect("delete empty collections setting should be enabled");
    sqlx::query("INSERT OR REPLACE INTO SERVER_SETTINGS (KEY, VALUE) VALUES (?, ?)")
        .bind("DELETE_EMPTY_READLISTS")
        .bind("true")
        .execute(&pool)
        .await
        .expect("delete empty readlists setting should be enabled");
    pool.close().await;

    let runtime = TaskRuntimeContext {
        owns_main_database: false,
        ..runtime_task_context(&paths)
    };
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "EMPTY_TRASH:library-1",
        1_000,
        Some("library-1".to_string()),
    ));
    scheduler
        .process_available(&runtime)
        .expect("blocked main-database cleanup should still drain cleanly");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for cleanup verification");
    let collection_rows = sqlx::query("SELECT COUNT(*) AS COUNT FROM COLLECTION WHERE ID = ?")
        .bind("collection-1")
        .fetch_one(&verify_pool)
        .await
        .expect("collection row count should be queryable")
        .get::<i64, _>("COUNT");
    let readlist_rows = sqlx::query("SELECT COUNT(*) AS COUNT FROM READLIST WHERE ID = ?")
        .bind("readlist-1")
        .fetch_one(&verify_pool)
        .await
        .expect("readlist row count should be queryable")
        .get::<i64, _>("COUNT");
    verify_pool.close().await;

    assert_eq!(
        collection_rows, 1,
        "runtime must not delete empty collections when main database is external-owned",
    );
    assert_eq!(
        readlist_rows, 1,
        "runtime must not delete empty readlists when main database is external-owned",
    );

    cleanup_router_fixture(paths);
}
