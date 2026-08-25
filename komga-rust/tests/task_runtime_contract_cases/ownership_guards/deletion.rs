use super::*;

async fn enqueue_delete_book(scheduler: &mut TaskQueueScheduler, book_id: &str) {
    scheduler
        .enqueue(
            TaskQueueRecord::new(
                format!("DeleteBook_{book_id}"),
                1_000,
                Some(book_id.to_string()),
            )
            .with_simple_type("DeleteBook"),
        )
        .await
        .expect("task enqueue should succeed");
}

async fn enqueue_delete_series(scheduler: &mut TaskQueueScheduler, series_id: &str) {
    scheduler
        .enqueue(
            TaskQueueRecord::new(
                format!("DeleteSeries_{series_id}"),
                1_000,
                Some(series_id.to_string()),
            )
            .with_simple_type("DeleteSeries"),
        )
        .await
        .expect("task enqueue should succeed");
}

#[tokio::test]
async fn runtime_blocks_book_delete_when_main_database_is_external_owned() {
    let ctx = TestFixture::new("runtime-blocked-main-database-delete-book").await;

    let runtime = runtime_task_context_with_ownership(
        ctx.paths(),
        TaskRuntimeOwnership {
            owns_main_database: false,
            ..TaskRuntimeOwnership::all_owned()
        },
    )
    .await;
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    enqueue_delete_book(&mut scheduler, "book-1").await;
    komga_infrastructure_jobs::process_available(&scheduler, &runtime)
        .await
        .expect("blocked main-database delete-book should still drain cleanly");

    let verify_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for delete-book verification");
    let book_rows = sqlx::query("SELECT COUNT(*) AS COUNT FROM BOOK WHERE ID = ?")
        .bind("book-1")
        .fetch_one(&verify_pool)
        .await
        .expect("book row count should be queryable")
        .get::<i64, _>("COUNT");
    verify_pool.close().await;

    assert_eq!(
        book_rows, 1,
        "runtime must not delete books when main database is external-owned",
    );
}

#[tokio::test]
async fn runtime_blocks_series_delete_when_main_database_is_external_owned() {
    let ctx = TestFixture::new("runtime-blocked-main-database-delete-series").await;

    let series_dir = ctx
        .paths()
        .config_dir
        .join("blocked-delete-series/series-1");
    std::fs::create_dir_all(&series_dir)
        .expect("blocked delete-series fixture series directory should exist");
    let book_file = series_dir.join("book-1.epub");
    let book_sidecar_thumbnail = series_dir.join("book-1.png");
    let series_sidecar_thumbnail = series_dir.join("cover.png");
    std::fs::write(&book_file, b"blocked-delete-series-book")
        .expect("blocked delete-series book file should be written");
    std::fs::write(&book_sidecar_thumbnail, fixture_png_bytes())
        .expect("blocked delete-series book sidecar should be written");
    std::fs::write(&series_sidecar_thumbnail, fixture_png_bytes())
        .expect("blocked delete-series series sidecar should be written");

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for blocked delete-series fixture setup");
    sqlx::query("UPDATE SERIES SET URL = ? WHERE ID = ?")
        .bind("blocked-delete-series/series-1")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("blocked delete-series series url should be updated");
    sqlx::query("UPDATE BOOK SET URL = ? WHERE ID = ?")
        .bind("blocked-delete-series/series-1/book-1.epub")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("blocked delete-series book url should be updated");
    sqlx::query(
        "INSERT INTO THUMBNAIL_BOOK (ID, BOOK_ID, TYPE, URL, SELECTED) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("thumb-book-sidecar-blocked-delete-series")
    .bind("book-1")
    .bind("SIDECAR")
    .bind("blocked-delete-series/series-1/book-1.png")
    .bind(false)
    .execute(&pool)
    .await
    .expect("blocked delete-series book sidecar row should be inserted");
    sqlx::query(
        "INSERT INTO THUMBNAIL_SERIES (ID, SERIES_ID, TYPE, URL, SELECTED) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("thumb-series-sidecar-blocked-delete-series")
    .bind("series-1")
    .bind("SIDECAR")
    .bind("blocked-delete-series/series-1/cover.png")
    .bind(true)
    .execute(&pool)
    .await
    .expect("blocked delete-series series sidecar row should be inserted");
    pool.close().await;

    let runtime = runtime_task_context_with_ownership(
        ctx.paths(),
        TaskRuntimeOwnership {
            owns_main_database: false,
            ..TaskRuntimeOwnership::all_owned()
        },
    )
    .await;
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    enqueue_delete_series(&mut scheduler, "series-1").await;
    komga_infrastructure_jobs::process_available(&scheduler, &runtime)
        .await
        .expect("blocked main-database delete-series should still drain cleanly");

    assert!(
        book_file.exists() && book_sidecar_thumbnail.exists() && series_sidecar_thumbnail.exists(),
        "runtime must not delete series files when the main database is external-owned",
    );

    let verify_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for blocked delete-series verification");
    let book_rows = sqlx::query("SELECT COUNT(*) AS COUNT FROM BOOK WHERE ID = ?")
        .bind("book-1")
        .fetch_one(&verify_pool)
        .await
        .expect("blocked delete-series book row count should be queryable")
        .get::<i64, _>("COUNT");
    let series_rows = sqlx::query("SELECT COUNT(*) AS COUNT FROM SERIES WHERE ID = ?")
        .bind("series-1")
        .fetch_one(&verify_pool)
        .await
        .expect("blocked delete-series series row count should be queryable")
        .get::<i64, _>("COUNT");
    verify_pool.close().await;

    assert_eq!(book_rows, 1);
    assert_eq!(series_rows, 1);
}

#[tokio::test]
async fn runtime_delete_book_soft_deletes_rows_and_removes_book_sidecar_files() {
    let ctx = TestFixture::new("runtime-delete-book-soft-delete-staging").await;

    let delete_dir = ctx.paths().config_dir.join("delete-book");
    std::fs::create_dir_all(&delete_dir).expect("delete-book fixture directory should exist");
    let book_file = delete_dir.join("book-1.epub");
    let sidecar_thumbnail = delete_dir.join("book-1.png");
    std::fs::write(&book_file, b"delete-book-fixture")
        .expect("delete-book fixture book file should be written");
    std::fs::write(&sidecar_thumbnail, fixture_png_bytes())
        .expect("delete-book fixture sidecar thumbnail should be written");

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for delete-book fixture setup");
    sqlx::query("UPDATE BOOK SET URL = ? WHERE ID = ?")
        .bind("delete-book/book-1.epub")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("delete-book fixture book url should be updated");
    sqlx::query(
        "INSERT INTO THUMBNAIL_BOOK (ID, BOOK_ID, TYPE, URL, SELECTED) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("thumb-book-sidecar-delete")
    .bind("book-1")
    .bind("SIDECAR")
    .bind("delete-book/book-1.png")
    .bind(false)
    .execute(&pool)
    .await
    .expect("delete-book fixture sidecar thumbnail row should be inserted");
    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED) VALUES (?, ?, ?, ?)",
    )
    .bind("book-1")
    .bind("admin-user")
    .bind(3_i64)
    .bind(false)
    .execute(&pool)
    .await
    .expect("delete-book fixture read progress row should be inserted");
    let series_old_last_modified = sqlx::query(
        "SELECT COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED FROM SERIES WHERE ID = ? LIMIT 1",
    )
    .bind("series-1")
    .fetch_one(&pool)
    .await
    .expect("delete-book fixture series row should be queryable")
    .get::<String, _>("LAST_MODIFIED");
    pool.close().await;

    let runtime = runtime_task_context(ctx.paths()).await;
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    enqueue_delete_book(&mut scheduler, "book-1").await;
    komga_infrastructure_jobs::process_available(&scheduler, &runtime)
        .await
        .expect("delete-book runtime should stage soft deletion cleanly");

    assert!(
        !book_file.exists(),
        "delete-book runtime should remove the main book file from disk"
    );
    assert!(
        !sidecar_thumbnail.exists(),
        "delete-book runtime should remove book sidecar thumbnail files from disk"
    );
    assert!(
        !delete_dir.exists(),
        "delete-book runtime should remove the now-empty parent directory"
    );

    let verify_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for delete-book verification");
    let book_row = sqlx::query("SELECT DELETED_DATE FROM BOOK WHERE ID = ? LIMIT 1")
        .bind("book-1")
        .fetch_one(&verify_pool)
        .await
        .expect("soft-deleted book row should still be queryable");
    let thumbnail_rows =
        sqlx::query("SELECT ID, TYPE, URL FROM THUMBNAIL_BOOK WHERE BOOK_ID = ? ORDER BY ID ASC")
            .bind("book-1")
            .fetch_all(&verify_pool)
            .await
            .expect("soft-deleted book thumbnail rows should still be queryable");
    let read_progress_count =
        sqlx::query("SELECT COUNT(*) AS COUNT FROM READ_PROGRESS WHERE BOOK_ID = ?")
            .bind("book-1")
            .fetch_one(&verify_pool)
            .await
            .expect("soft-deleted book read-progress rows should be queryable")
            .get::<i64, _>("COUNT");
    let series_row = sqlx::query(
        "SELECT BOOK_COUNT, COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED FROM SERIES WHERE ID = ? LIMIT 1",
    )
    .bind("series-1")
    .fetch_one(&verify_pool)
    .await
    .expect("series row should be queryable after delete-book staging");
    verify_pool.close().await;

    assert!(
        book_row.get::<Option<String>, _>("DELETED_DATE").is_some(),
        "delete-book runtime should stage the book for trash instead of hard-deleting it"
    );
    assert_eq!(
        thumbnail_rows.len(),
        2,
        "delete-book runtime should preserve THUMBNAIL_BOOK rows until EmptyTrash performs hard cleanup"
    );
    assert_eq!(
        read_progress_count, 1,
        "delete-book runtime should preserve READ_PROGRESS rows until EmptyTrash performs hard cleanup"
    );
    assert_eq!(
        series_row.get::<i64, _>("BOOK_COUNT"),
        1,
        "delete-book runtime should keep trash-staged books in the series book count until EmptyTrash performs hard cleanup"
    );
    assert_ne!(
        series_row.get::<String, _>("LAST_MODIFIED"),
        series_old_last_modified,
        "delete-book runtime should refresh series last-modified so series changes remain externally visible",
    );
}

#[tokio::test]
async fn runtime_delete_book_emits_book_changed_event_after_soft_delete() {
    let ctx = TestFixture::new("runtime-delete-book-sse-book-changed").await;

    let delete_dir = ctx.paths().config_dir.join("delete-book-sse");
    std::fs::create_dir_all(&delete_dir).expect("delete-book sse fixture directory should exist");
    let book_file = delete_dir.join("book-1.epub");
    std::fs::write(&book_file, b"delete-book-sse-fixture")
        .expect("delete-book sse fixture book file should be written");

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for delete-book sse fixture setup");
    sqlx::query("UPDATE BOOK SET URL = ? WHERE ID = ?")
        .bind("delete-book-sse/book-1.epub")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("delete-book sse fixture book url should be updated");
    pool.close().await;

    let cursor = ctx.runtime_events().current_cursor();
    let runtime =
        runtime_task_context_with_runtime_events(ctx.paths(), ctx.runtime_events_arc()).await;
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    enqueue_delete_book(&mut scheduler, "book-1").await;
    komga_infrastructure_jobs::process_available(&scheduler, &runtime)
        .await
        .expect("delete-book runtime should process successfully for sse contract");

    let events = ctx
        .runtime_events()
        .pending_events(cursor, "runtime-contract-admin", true)
        .events;
    assert!(
        events.iter().any(|event| matches!(
            &event.event,
            RuntimeSseEvent::BookChanged {
                book_id,
                series_id,
                library_id,
            } if book_id == "book-1" && series_id == "series-1" && library_id == "library-1"
        )),
        "delete-book runtime should emit BookChanged SSE",
    );
}

#[tokio::test]
async fn runtime_delete_book_oneshot_soft_deletes_series_and_removes_series_sidecar_files() {
    let ctx = TestFixture::new("runtime-delete-book-oneshot-soft-delete-staging").await;

    let series_dir = ctx.paths().config_dir.join("delete-oneshot/series-1");
    std::fs::create_dir_all(&series_dir)
        .expect("delete-book oneshot series directory should exist");
    let book_file = series_dir.join("book-1.epub");
    let book_sidecar_thumbnail = series_dir.join("book-1.png");
    let series_sidecar_thumbnail = series_dir.join("cover.png");
    std::fs::write(&book_file, b"delete-book-oneshot-fixture")
        .expect("delete-book oneshot book file should be written");
    std::fs::write(&book_sidecar_thumbnail, fixture_png_bytes())
        .expect("delete-book oneshot book sidecar should be written");
    std::fs::write(&series_sidecar_thumbnail, fixture_png_bytes())
        .expect("delete-book oneshot series sidecar should be written");

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for delete-book oneshot fixture setup");
    sqlx::query("UPDATE SERIES SET URL = ? WHERE ID = ?")
        .bind("delete-oneshot/series-1")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("delete-book oneshot series url should be updated");
    sqlx::query("UPDATE BOOK SET URL = ?, ONESHOT = 1 WHERE ID = ?")
        .bind("delete-oneshot/series-1/book-1.epub")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("delete-book oneshot book row should be updated");
    sqlx::query(
        "INSERT INTO THUMBNAIL_BOOK (ID, BOOK_ID, TYPE, URL, SELECTED) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("thumb-book-sidecar-oneshot")
    .bind("book-1")
    .bind("SIDECAR")
    .bind("delete-oneshot/series-1/book-1.png")
    .bind(false)
    .execute(&pool)
    .await
    .expect("delete-book oneshot book sidecar row should be inserted");
    sqlx::query(
        "INSERT INTO THUMBNAIL_SERIES (ID, SERIES_ID, TYPE, URL, SELECTED) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("thumb-series-sidecar-oneshot")
    .bind("series-1")
    .bind("SIDECAR")
    .bind("delete-oneshot/series-1/cover.png")
    .bind(true)
    .execute(&pool)
    .await
    .expect("delete-book oneshot series sidecar row should be inserted");
    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED) VALUES (?, ?, ?, ?)",
    )
    .bind("book-1")
    .bind("admin-user")
    .bind(3_i64)
    .bind(false)
    .execute(&pool)
    .await
    .expect("delete-book oneshot read progress row should be inserted");
    let series_old_last_modified = sqlx::query(
        "SELECT COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED FROM SERIES WHERE ID = ? LIMIT 1",
    )
    .bind("series-1")
    .fetch_one(&pool)
    .await
    .expect("delete-book oneshot series row should be queryable")
    .get::<String, _>("LAST_MODIFIED");
    pool.close().await;

    let runtime = runtime_task_context(ctx.paths()).await;
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    enqueue_delete_book(&mut scheduler, "book-1").await;
    komga_infrastructure_jobs::process_available(&scheduler, &runtime)
        .await
        .expect("delete-book oneshot runtime should stage soft deletion cleanly");

    assert!(
        !book_file.exists(),
        "delete-book oneshot runtime should remove the oneshot book file from disk"
    );
    assert!(
        !book_sidecar_thumbnail.exists(),
        "delete-book oneshot runtime should remove book sidecar thumbnail files from disk"
    );
    assert!(
        !series_sidecar_thumbnail.exists(),
        "delete-book oneshot runtime should remove series sidecar thumbnail files from disk"
    );
    assert!(
        !series_dir.exists(),
        "delete-book oneshot runtime should remove the now-empty series directory"
    );

    let verify_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for delete-book oneshot verification");
    let book_row = sqlx::query("SELECT DELETED_DATE FROM BOOK WHERE ID = ? LIMIT 1")
        .bind("book-1")
        .fetch_one(&verify_pool)
        .await
        .expect("soft-deleted oneshot book row should still be queryable");
    let series_row = sqlx::query(
        "SELECT DELETED_DATE, BOOK_COUNT, COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED FROM SERIES WHERE ID = ? LIMIT 1",
    )
    .bind("series-1")
    .fetch_one(&verify_pool)
    .await
    .expect("soft-deleted oneshot series row should still be queryable");
    let book_thumbnail_rows =
        sqlx::query("SELECT COUNT(*) AS COUNT FROM THUMBNAIL_BOOK WHERE BOOK_ID = ?")
            .bind("book-1")
            .fetch_one(&verify_pool)
            .await
            .expect("soft-deleted oneshot book thumbnail rows should be queryable")
            .get::<i64, _>("COUNT");
    let series_thumbnail_rows =
        sqlx::query("SELECT COUNT(*) AS COUNT FROM THUMBNAIL_SERIES WHERE SERIES_ID = ?")
            .bind("series-1")
            .fetch_one(&verify_pool)
            .await
            .expect("soft-deleted oneshot series thumbnail rows should be queryable")
            .get::<i64, _>("COUNT");
    let read_progress_count =
        sqlx::query("SELECT COUNT(*) AS COUNT FROM READ_PROGRESS WHERE BOOK_ID = ?")
            .bind("book-1")
            .fetch_one(&verify_pool)
            .await
            .expect("soft-deleted oneshot read-progress rows should be queryable")
            .get::<i64, _>("COUNT");
    verify_pool.close().await;

    assert!(
        book_row.get::<Option<String>, _>("DELETED_DATE").is_some(),
        "delete-book oneshot runtime should still trash-stage the book row instead of hard-deleting it"
    );
    assert!(
        series_row
            .get::<Option<String>, _>("DELETED_DATE")
            .is_some(),
        "delete-book oneshot runtime should trash-stage the series row instead of hard-deleting it"
    );
    assert_eq!(
        series_row.get::<i64, _>("BOOK_COUNT"),
        1,
        "delete-book oneshot runtime should keep the trash-staged book in the series book count"
    );
    assert_ne!(
        series_row.get::<String, _>("LAST_MODIFIED"),
        series_old_last_modified,
        "delete-book oneshot runtime should refresh series last-modified for downstream visibility",
    );
    assert_eq!(
        book_thumbnail_rows, 2,
        "delete-book oneshot runtime should preserve THUMBNAIL_BOOK rows until EmptyTrash performs hard cleanup"
    );
    assert_eq!(
        series_thumbnail_rows, 1,
        "delete-book oneshot runtime should preserve THUMBNAIL_SERIES rows until EmptyTrash performs hard cleanup"
    );
    assert_eq!(
        read_progress_count, 1,
        "delete-book oneshot runtime should preserve READ_PROGRESS rows until EmptyTrash performs hard cleanup"
    );
}

#[tokio::test]
async fn runtime_delete_book_oneshot_deletes_every_book_in_the_series() {
    let ctx = TestFixture::new("runtime-delete-book-oneshot-deletes-full-series").await;

    let series_dir = ctx
        .paths()
        .config_dir
        .join("delete-oneshot-full-series/series-1");
    std::fs::create_dir_all(&series_dir)
        .expect("delete-book oneshot full-series directory should exist");
    let first_book_file = series_dir.join("book-1.epub");
    let first_book_sidecar = series_dir.join("book-1.png");
    let second_book_file = series_dir.join("book-2.epub");
    let second_book_sidecar = series_dir.join("book-2.png");
    let series_sidecar_thumbnail = series_dir.join("cover.png");
    std::fs::write(&first_book_file, b"delete-book-oneshot-full-series-book-1")
        .expect("delete-book oneshot full-series first book should be written");
    std::fs::write(&first_book_sidecar, fixture_png_bytes())
        .expect("delete-book oneshot full-series first sidecar should be written");
    std::fs::write(&second_book_file, b"delete-book-oneshot-full-series-book-2")
        .expect("delete-book oneshot full-series second book should be written");
    std::fs::write(&second_book_sidecar, fixture_png_bytes())
        .expect("delete-book oneshot full-series second sidecar should be written");
    std::fs::write(&series_sidecar_thumbnail, fixture_png_bytes())
        .expect("delete-book oneshot full-series series sidecar should be written");

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for delete-book oneshot full-series fixture setup");
    sqlx::query("UPDATE SERIES SET URL = ?, ONESHOT = 1 WHERE ID = ?")
        .bind("delete-oneshot-full-series/series-1")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("delete-book oneshot full-series series row should be updated");
    sqlx::query("UPDATE BOOK SET URL = ?, ONESHOT = 1 WHERE ID = ?")
        .bind("delete-oneshot-full-series/series-1/book-1.epub")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("delete-book oneshot full-series first book row should be updated");
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID, ONESHOT) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-2")
    .bind(0_i64)
    .bind("book-2.epub")
    .bind("delete-oneshot-full-series/series-1/book-2.epub")
    .bind("series-1")
    .bind(2_048_i64)
    .bind(2_i64)
    .bind("library-1")
    .bind(false)
    .execute(&pool)
    .await
    .expect("delete-book oneshot full-series second book row should be inserted");
    sqlx::query("INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("application/epub+zip")
        .bind("READY")
        .bind("book-2")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("delete-book oneshot full-series second media row should be inserted");
    sqlx::query(
        "INSERT INTO THUMBNAIL_BOOK (ID, BOOK_ID, TYPE, URL, SELECTED) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("thumb-book-sidecar-full-series-1")
    .bind("book-1")
    .bind("SIDECAR")
    .bind("delete-oneshot-full-series/series-1/book-1.png")
    .bind(false)
    .execute(&pool)
    .await
    .expect("delete-book oneshot full-series first sidecar row should be inserted");
    sqlx::query(
        "INSERT INTO THUMBNAIL_BOOK (ID, BOOK_ID, TYPE, URL, SELECTED) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("thumb-book-sidecar-full-series-2")
    .bind("book-2")
    .bind("SIDECAR")
    .bind("delete-oneshot-full-series/series-1/book-2.png")
    .bind(false)
    .execute(&pool)
    .await
    .expect("delete-book oneshot full-series second sidecar row should be inserted");
    sqlx::query(
        "INSERT INTO THUMBNAIL_SERIES (ID, SERIES_ID, TYPE, URL, SELECTED) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("thumb-series-sidecar-full-series")
    .bind("series-1")
    .bind("SIDECAR")
    .bind("delete-oneshot-full-series/series-1/cover.png")
    .bind(true)
    .execute(&pool)
    .await
    .expect("delete-book oneshot full-series series sidecar row should be inserted");
    pool.close().await;

    let runtime = runtime_task_context(ctx.paths()).await;
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    enqueue_delete_book(&mut scheduler, "book-1").await;
    komga_infrastructure_jobs::process_available(&scheduler, &runtime)
        .await
        .expect("delete-book oneshot full-series runtime should process successfully");

    assert!(
        !first_book_file.exists()
            && !second_book_file.exists()
            && !first_book_sidecar.exists()
            && !second_book_sidecar.exists()
            && !series_sidecar_thumbnail.exists()
            && !series_dir.exists(),
        "delete-book oneshot should delete every book file and sidecar in the series, then remove the empty series directory",
    );

    let verify_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for delete-book oneshot full-series verification");
    let first_book_deleted = sqlx::query("SELECT DELETED_DATE FROM BOOK WHERE ID = ? LIMIT 1")
        .bind("book-1")
        .fetch_one(&verify_pool)
        .await
        .expect("delete-book oneshot full-series first book row should be queryable")
        .get::<Option<String>, _>("DELETED_DATE");
    let second_book_deleted = sqlx::query("SELECT DELETED_DATE FROM BOOK WHERE ID = ? LIMIT 1")
        .bind("book-2")
        .fetch_one(&verify_pool)
        .await
        .expect("delete-book oneshot full-series second book row should be queryable")
        .get::<Option<String>, _>("DELETED_DATE");
    let series_deleted = sqlx::query("SELECT DELETED_DATE FROM SERIES WHERE ID = ? LIMIT 1")
        .bind("series-1")
        .fetch_one(&verify_pool)
        .await
        .expect("delete-book oneshot full-series series row should be queryable")
        .get::<Option<String>, _>("DELETED_DATE");
    verify_pool.close().await;

    assert!(
        first_book_deleted.is_some() && second_book_deleted.is_some() && series_deleted.is_some(),
        "delete-book oneshot should soft-delete every book row and the series row once the series path passes filesystem preconditions",
    );
}

#[tokio::test]
async fn runtime_delete_book_soft_deletes_rows_when_book_file_is_already_missing() {
    let ctx = TestFixture::new("runtime-delete-book-missing-file-soft-delete").await;

    let delete_dir = ctx.paths().config_dir.join("delete-book-missing");
    std::fs::create_dir_all(&delete_dir)
        .expect("delete-book missing fixture directory should exist");
    let missing_book_file = delete_dir.join("book-1.epub");
    let sidecar_thumbnail = delete_dir.join("book-1.png");
    std::fs::write(&sidecar_thumbnail, fixture_png_bytes())
        .expect("delete-book missing fixture sidecar thumbnail should be written");

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for delete-book missing fixture setup");
    sqlx::query("UPDATE BOOK SET URL = ? WHERE ID = ?")
        .bind("delete-book-missing/book-1.epub")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("delete-book missing fixture book url should be updated");
    sqlx::query(
        "INSERT INTO THUMBNAIL_BOOK (ID, BOOK_ID, TYPE, URL, SELECTED) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("thumb-book-sidecar-missing")
    .bind("book-1")
    .bind("SIDECAR")
    .bind("delete-book-missing/book-1.png")
    .bind(false)
    .execute(&pool)
    .await
    .expect("delete-book missing fixture sidecar thumbnail row should be inserted");
    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED) VALUES (?, ?, ?, ?)",
    )
    .bind("book-1")
    .bind("admin-user")
    .bind(3_i64)
    .bind(false)
    .execute(&pool)
    .await
    .expect("delete-book missing fixture read progress row should be inserted");
    pool.close().await;

    let runtime = runtime_task_context(ctx.paths()).await;
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    enqueue_delete_book(&mut scheduler, "book-1").await;
    komga_infrastructure_jobs::process_available(&scheduler, &runtime)
        .await
        .expect("delete-book missing-file runtime should still drain cleanly");

    assert!(
        !missing_book_file.exists(),
        "delete-book missing-file fixture intentionally keeps the main file absent"
    );
    assert!(
        !sidecar_thumbnail.exists(),
        "delete-book missing-file should still delete sidecar thumbnails while soft-deleting the book"
    );
    assert!(
        !delete_dir.exists(),
        "delete-book missing-file should still remove the now-empty parent directory"
    );

    let verify_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for delete-book missing-file verification");
    let book_deleted = sqlx::query("SELECT DELETED_DATE FROM BOOK WHERE ID = ? LIMIT 1")
        .bind("book-1")
        .fetch_one(&verify_pool)
        .await
        .expect("book row should be queryable after missing-file delete attempt")
        .get::<Option<String>, _>("DELETED_DATE");
    let thumbnail_count =
        sqlx::query("SELECT COUNT(*) AS COUNT FROM THUMBNAIL_BOOK WHERE BOOK_ID = ?")
            .bind("book-1")
            .fetch_one(&verify_pool)
            .await
            .expect("thumbnail rows should be queryable after missing-file delete attempt")
            .get::<i64, _>("COUNT");
    let read_progress_count =
        sqlx::query("SELECT COUNT(*) AS COUNT FROM READ_PROGRESS WHERE BOOK_ID = ?")
            .bind("book-1")
            .fetch_one(&verify_pool)
            .await
            .expect("read progress rows should be queryable after missing-file delete attempt")
            .get::<i64, _>("COUNT");
    let series_row = sqlx::query("SELECT BOOK_COUNT FROM SERIES WHERE ID = ? LIMIT 1")
        .bind("series-1")
        .fetch_one(&verify_pool)
        .await
        .expect("series row should be queryable after missing-file delete attempt");
    verify_pool.close().await;

    assert!(
        book_deleted.is_some(),
        "delete-book missing-file should still soft-delete the book when the file is already gone",
    );
    assert_eq!(thumbnail_count, 2);
    assert_eq!(read_progress_count, 1);
    assert_eq!(series_row.get::<i64, _>("BOOK_COUNT"), 1);
}

#[tokio::test]
async fn runtime_delete_book_oneshot_skips_soft_delete_when_series_directory_is_readonly() {
    let ctx = TestFixture::new("runtime-delete-book-oneshot-readonly-series-no-staging").await;

    let series_dir = ctx
        .paths()
        .config_dir
        .join("delete-oneshot-readonly/series-1");
    std::fs::create_dir_all(&series_dir)
        .expect("delete-book oneshot readonly series directory should exist");
    let book_file = series_dir.join("book-1.epub");
    let book_sidecar_thumbnail = series_dir.join("book-1.png");
    let series_sidecar_thumbnail = series_dir.join("cover.png");
    std::fs::write(&book_file, b"delete-book-oneshot-readonly-fixture")
        .expect("delete-book oneshot readonly book file should be written");
    std::fs::write(&book_sidecar_thumbnail, fixture_png_bytes())
        .expect("delete-book oneshot readonly book sidecar should be written");
    std::fs::write(&series_sidecar_thumbnail, fixture_png_bytes())
        .expect("delete-book oneshot readonly series sidecar should be written");

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for delete-book oneshot readonly fixture setup");
    sqlx::query("UPDATE SERIES SET URL = ? WHERE ID = ?")
        .bind("delete-oneshot-readonly/series-1")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("delete-book oneshot readonly series url should be updated");
    sqlx::query("UPDATE BOOK SET URL = ?, ONESHOT = 1 WHERE ID = ?")
        .bind("delete-oneshot-readonly/series-1/book-1.epub")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("delete-book oneshot readonly book row should be updated");
    sqlx::query(
        "INSERT INTO THUMBNAIL_BOOK (ID, BOOK_ID, TYPE, URL, SELECTED) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("thumb-book-sidecar-oneshot-readonly")
    .bind("book-1")
    .bind("SIDECAR")
    .bind("delete-oneshot-readonly/series-1/book-1.png")
    .bind(false)
    .execute(&pool)
    .await
    .expect("delete-book oneshot readonly book sidecar row should be inserted");
    sqlx::query(
        "INSERT INTO THUMBNAIL_SERIES (ID, SERIES_ID, TYPE, URL, SELECTED) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("thumb-series-sidecar-oneshot-readonly")
    .bind("series-1")
    .bind("SIDECAR")
    .bind("delete-oneshot-readonly/series-1/cover.png")
    .bind(true)
    .execute(&pool)
    .await
    .expect("delete-book oneshot readonly series sidecar row should be inserted");
    pool.close().await;

    let mut permissions = std::fs::metadata(&series_dir)
        .expect("delete-book oneshot readonly series metadata should be readable")
        .permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&series_dir, permissions)
        .expect("delete-book oneshot readonly series directory should become readonly");

    let runtime = runtime_task_context(ctx.paths()).await;
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    enqueue_delete_book(&mut scheduler, "book-1").await;
    komga_infrastructure_jobs::process_available(&scheduler, &runtime)
        .await
        .expect("delete-book oneshot readonly runtime should still drain cleanly");

    let verify_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for delete-book oneshot readonly verification");
    let book_deleted = sqlx::query("SELECT DELETED_DATE FROM BOOK WHERE ID = ? LIMIT 1")
        .bind("book-1")
        .fetch_one(&verify_pool)
        .await
        .expect("oneshot readonly book row should be queryable")
        .get::<Option<String>, _>("DELETED_DATE");
    let series_deleted = sqlx::query("SELECT DELETED_DATE FROM SERIES WHERE ID = ? LIMIT 1")
        .bind("series-1")
        .fetch_one(&verify_pool)
        .await
        .expect("oneshot readonly series row should be queryable")
        .get::<Option<String>, _>("DELETED_DATE");
    verify_pool.close().await;

    assert!(
        book_file.exists() && book_sidecar_thumbnail.exists() && series_sidecar_thumbnail.exists(),
        "delete-book oneshot readonly should not delete files when the series directory precondition fails",
    );
    assert!(
        book_deleted.is_none() && series_deleted.is_none(),
        "delete-book oneshot readonly should not soft-delete book or series when filesystem preconditions fail",
    );

    let mut cleanup_permissions = std::fs::metadata(&series_dir)
        .expect("delete-book oneshot readonly series metadata should still be readable")
        .permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        cleanup_permissions.set_mode(0o755);
    }
    #[cfg(not(unix))]
    {
        cleanup_permissions.set_readonly(false);
    }
    std::fs::set_permissions(&series_dir, cleanup_permissions).expect(
        "delete-book oneshot readonly series directory permissions should reset for cleanup",
    );
}

#[tokio::test]
async fn runtime_delete_series_soft_deletes_rows_and_removes_series_sidecar_files() {
    let ctx = TestFixture::new("runtime-delete-series-soft-delete-staging").await;

    let series_dir = ctx.paths().config_dir.join("delete-series/series-1");
    std::fs::create_dir_all(&series_dir)
        .expect("delete-series fixture series directory should exist");
    let book_file = series_dir.join("book-1.epub");
    let book_sidecar_thumbnail = series_dir.join("book-1.png");
    let series_sidecar_thumbnail = series_dir.join("cover.png");
    std::fs::write(&book_file, b"delete-series-fixture")
        .expect("delete-series fixture book file should be written");
    std::fs::write(&book_sidecar_thumbnail, fixture_png_bytes())
        .expect("delete-series fixture book sidecar should be written");
    std::fs::write(&series_sidecar_thumbnail, fixture_png_bytes())
        .expect("delete-series fixture series sidecar should be written");

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for delete-series fixture setup");
    sqlx::query("UPDATE SERIES SET URL = ? WHERE ID = ?")
        .bind("delete-series/series-1")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("delete-series fixture series url should be updated");
    sqlx::query("UPDATE BOOK SET URL = ? WHERE ID = ?")
        .bind("delete-series/series-1/book-1.epub")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("delete-series fixture book url should be updated");
    sqlx::query(
        "INSERT INTO THUMBNAIL_BOOK (ID, BOOK_ID, TYPE, URL, SELECTED) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("thumb-book-sidecar-delete-series")
    .bind("book-1")
    .bind("SIDECAR")
    .bind("delete-series/series-1/book-1.png")
    .bind(false)
    .execute(&pool)
    .await
    .expect("delete-series fixture book sidecar row should be inserted");
    sqlx::query(
        "INSERT INTO THUMBNAIL_SERIES (ID, SERIES_ID, TYPE, URL, SELECTED) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("thumb-series-sidecar-delete-series")
    .bind("series-1")
    .bind("SIDECAR")
    .bind("delete-series/series-1/cover.png")
    .bind(true)
    .execute(&pool)
    .await
    .expect("delete-series fixture series sidecar row should be inserted");
    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED) VALUES (?, ?, ?, ?)",
    )
    .bind("book-1")
    .bind("admin-user")
    .bind(5_i64)
    .bind(false)
    .execute(&pool)
    .await
    .expect("delete-series fixture read progress row should be inserted");
    let series_old_last_modified = sqlx::query(
        "SELECT COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED FROM SERIES WHERE ID = ? LIMIT 1",
    )
    .bind("series-1")
    .fetch_one(&pool)
    .await
    .expect("delete-series fixture series row should be queryable")
    .get::<String, _>("LAST_MODIFIED");
    pool.close().await;

    let runtime = runtime_task_context(ctx.paths()).await;
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    enqueue_delete_series(&mut scheduler, "series-1").await;
    komga_infrastructure_jobs::process_available(&scheduler, &runtime)
        .await
        .expect("delete-series runtime should stage soft deletion cleanly");

    assert!(
        !book_file.exists(),
        "delete-series runtime should remove the series book file from disk"
    );
    assert!(
        !book_sidecar_thumbnail.exists(),
        "delete-series runtime should remove book sidecar thumbnail files from disk"
    );
    assert!(
        !series_sidecar_thumbnail.exists(),
        "delete-series runtime should remove series sidecar thumbnail files from disk"
    );
    assert!(
        !series_dir.exists(),
        "delete-series runtime should remove the now-empty series directory"
    );

    let verify_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for delete-series verification");
    let book_row = sqlx::query("SELECT DELETED_DATE FROM BOOK WHERE ID = ? LIMIT 1")
        .bind("book-1")
        .fetch_one(&verify_pool)
        .await
        .expect("soft-deleted series book row should still be queryable");
    let series_row = sqlx::query(
        "SELECT DELETED_DATE, BOOK_COUNT, COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED FROM SERIES WHERE ID = ? LIMIT 1",
    )
    .bind("series-1")
    .fetch_one(&verify_pool)
    .await
    .expect("soft-deleted series row should still be queryable");
    let book_thumbnail_count =
        sqlx::query("SELECT COUNT(*) AS COUNT FROM THUMBNAIL_BOOK WHERE BOOK_ID = ?")
            .bind("book-1")
            .fetch_one(&verify_pool)
            .await
            .expect("soft-deleted series book thumbnails should be queryable")
            .get::<i64, _>("COUNT");
    let series_thumbnail_count =
        sqlx::query("SELECT COUNT(*) AS COUNT FROM THUMBNAIL_SERIES WHERE SERIES_ID = ?")
            .bind("series-1")
            .fetch_one(&verify_pool)
            .await
            .expect("soft-deleted series thumbnails should be queryable")
            .get::<i64, _>("COUNT");
    let read_progress_count =
        sqlx::query("SELECT COUNT(*) AS COUNT FROM READ_PROGRESS WHERE BOOK_ID = ?")
            .bind("book-1")
            .fetch_one(&verify_pool)
            .await
            .expect("soft-deleted series read progress rows should be queryable")
            .get::<i64, _>("COUNT");
    verify_pool.close().await;

    assert!(
        book_row.get::<Option<String>, _>("DELETED_DATE").is_some(),
        "delete-series runtime should soft-delete child book rows instead of hard-deleting them"
    );
    assert!(
        series_row
            .get::<Option<String>, _>("DELETED_DATE")
            .is_some(),
        "delete-series runtime should soft-delete the series row instead of hard-deleting it"
    );
    assert_eq!(
        series_row.get::<i64, _>("BOOK_COUNT"),
        1,
        "delete-series runtime should keep trash-staged books in the series book count until EmptyTrash performs hard cleanup"
    );
    assert_ne!(
        series_row.get::<String, _>("LAST_MODIFIED"),
        series_old_last_modified,
        "delete-series runtime should refresh series last-modified for downstream visibility",
    );
    assert_eq!(book_thumbnail_count, 2);
    assert_eq!(series_thumbnail_count, 1);
    assert_eq!(read_progress_count, 1);
}

#[tokio::test]
async fn runtime_delete_series_emits_series_changed_event_after_soft_delete() {
    let ctx = TestFixture::new("runtime-delete-series-sse-series-changed").await;

    let series_dir = ctx.paths().config_dir.join("delete-series-sse/series-1");
    std::fs::create_dir_all(&series_dir)
        .expect("delete-series sse fixture series directory should exist");
    let book_file = series_dir.join("book-1.epub");
    std::fs::write(&book_file, b"delete-series-sse-fixture")
        .expect("delete-series sse fixture book file should be written");

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for delete-series sse fixture setup");
    sqlx::query("UPDATE SERIES SET URL = ? WHERE ID = ?")
        .bind("delete-series-sse/series-1")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("delete-series sse fixture series url should be updated");
    sqlx::query("UPDATE BOOK SET URL = ? WHERE ID = ?")
        .bind("delete-series-sse/series-1/book-1.epub")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("delete-series sse fixture book url should be updated");
    pool.close().await;

    let cursor = ctx.runtime_events().current_cursor();
    let runtime =
        runtime_task_context_with_runtime_events(ctx.paths(), ctx.runtime_events_arc()).await;
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    enqueue_delete_series(&mut scheduler, "series-1").await;
    komga_infrastructure_jobs::process_available(&scheduler, &runtime)
        .await
        .expect("delete-series runtime should process successfully for sse contract");

    let events = ctx
        .runtime_events()
        .pending_events(cursor, "runtime-contract-admin", true)
        .events;
    assert!(
        events.iter().any(|event| matches!(
            &event.event,
            RuntimeSseEvent::SeriesChanged {
                series_id,
                library_id,
            } if series_id == "series-1" && library_id == "library-1"
        )),
        "delete-series runtime should emit SeriesChanged SSE",
    );
}

#[tokio::test]
async fn runtime_delete_series_skips_soft_delete_when_series_directory_is_missing() {
    let ctx = TestFixture::new("runtime-delete-series-missing-directory-no-staging").await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for delete-series missing-directory fixture setup");
    sqlx::query("UPDATE SERIES SET URL = ? WHERE ID = ?")
        .bind("missing-delete-series/series-1")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("delete-series missing-directory series url should be updated");
    sqlx::query("UPDATE BOOK SET URL = ? WHERE ID = ?")
        .bind("missing-delete-series/series-1/book-1.epub")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("delete-series missing-directory book url should be updated");
    pool.close().await;

    let runtime = runtime_task_context(ctx.paths()).await;
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    enqueue_delete_series(&mut scheduler, "series-1").await;
    komga_infrastructure_jobs::process_available(&scheduler, &runtime)
        .await
        .expect("delete-series missing-directory runtime should still drain cleanly");

    let verify_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for delete-series missing-directory verification");
    let book_deleted = sqlx::query("SELECT DELETED_DATE FROM BOOK WHERE ID = ? LIMIT 1")
        .bind("book-1")
        .fetch_one(&verify_pool)
        .await
        .expect("delete-series missing-directory book row should be queryable")
        .get::<Option<String>, _>("DELETED_DATE");
    let series_deleted = sqlx::query("SELECT DELETED_DATE FROM SERIES WHERE ID = ? LIMIT 1")
        .bind("series-1")
        .fetch_one(&verify_pool)
        .await
        .expect("delete-series missing-directory series row should be queryable")
        .get::<Option<String>, _>("DELETED_DATE");
    verify_pool.close().await;

    assert!(
        book_deleted.is_none() && series_deleted.is_none(),
        "delete-series missing-directory should not soft-delete rows when series filesystem preconditions fail",
    );
}
