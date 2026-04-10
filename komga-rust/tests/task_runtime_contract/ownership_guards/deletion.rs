use super::*;

#[tokio::test]
async fn runtime_blocks_book_delete_when_main_database_is_external_owned() {
    let paths = new_router_fixture("runtime-blocked-main-database-delete-book").await;
    seed_router_contract_data(&paths).await;

    let runtime = TaskRuntimeContext {
        owns_main_database: false,
        ..runtime_task_context(&paths)
    };
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "DELETE_BOOK:book-1",
        1_000,
        Some("book-1".to_string()),
    ));
    scheduler
        .process_available(&runtime)
        .expect("blocked main-database delete-book should still drain cleanly");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_delete_book_soft_deletes_rows_and_removes_book_sidecar_files() {
    let paths = new_router_fixture("runtime-delete-book-soft-delete-staging").await;
    seed_router_contract_data(&paths).await;

    let delete_dir = paths.config_dir.join("delete-book");
    std::fs::create_dir_all(&delete_dir).expect("delete-book fixture directory should exist");
    let book_file = delete_dir.join("book-1.epub");
    let sidecar_thumbnail = delete_dir.join("book-1.png");
    std::fs::write(&book_file, b"delete-book-fixture")
        .expect("delete-book fixture book file should be written");
    std::fs::write(&sidecar_thumbnail, fixture_png_bytes())
        .expect("delete-book fixture sidecar thumbnail should be written");

    let pool = connect_pool(paths.main_db.as_path(), 1)
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

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "DELETE_BOOK:book-1",
        1_000,
        Some("book-1".to_string()),
    ));
    scheduler
        .process_available(&runtime)
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

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
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
        0,
        "delete-book runtime should immediately recompute active series book count excluding soft-deleted books"
    );
    assert_ne!(
        series_row.get::<String, _>("LAST_MODIFIED"),
        series_old_last_modified,
        "delete-book runtime should refresh series last-modified so series changes remain externally visible",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_delete_book_oneshot_soft_deletes_series_and_removes_series_sidecar_files() {
    let paths = new_router_fixture("runtime-delete-book-oneshot-soft-delete-staging").await;
    seed_router_contract_data(&paths).await;

    let series_dir = paths.config_dir.join("delete-oneshot/series-1");
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

    let pool = connect_pool(paths.main_db.as_path(), 1)
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

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "DELETE_BOOK:book-1",
        1_000,
        Some("book-1".to_string()),
    ));
    scheduler
        .process_available(&runtime)
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

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
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
        0,
        "delete-book oneshot runtime should recompute active book count to zero"
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_delete_book_skips_soft_delete_when_book_file_is_missing() {
    let paths = new_router_fixture("runtime-delete-book-missing-file-no-staging").await;
    seed_router_contract_data(&paths).await;

    let delete_dir = paths.config_dir.join("delete-book-missing");
    std::fs::create_dir_all(&delete_dir)
        .expect("delete-book missing fixture directory should exist");
    let missing_book_file = delete_dir.join("book-1.epub");
    let sidecar_thumbnail = delete_dir.join("book-1.png");
    std::fs::write(&sidecar_thumbnail, fixture_png_bytes())
        .expect("delete-book missing fixture sidecar thumbnail should be written");

    let pool = connect_pool(paths.main_db.as_path(), 1)
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

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "DELETE_BOOK:book-1",
        1_000,
        Some("book-1".to_string()),
    ));
    scheduler
        .process_available(&runtime)
        .expect("delete-book missing-file runtime should still drain cleanly");

    assert!(
        !missing_book_file.exists(),
        "delete-book missing-file fixture intentionally keeps the main file absent"
    );
    assert!(
        sidecar_thumbnail.exists(),
        "delete-book missing-file should not delete sidecar thumbnails when the main file precondition fails"
    );

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
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
    verify_pool.close().await;

    assert!(
        book_deleted.is_none(),
        "delete-book missing-file should not soft-delete the book when filesystem preconditions fail",
    );
    assert_eq!(thumbnail_count, 2);
    assert_eq!(read_progress_count, 1);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_delete_book_oneshot_skips_soft_delete_when_series_directory_is_readonly() {
    let paths = new_router_fixture("runtime-delete-book-oneshot-readonly-series-no-staging").await;
    seed_router_contract_data(&paths).await;

    let series_dir = paths.config_dir.join("delete-oneshot-readonly/series-1");
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

    let pool = connect_pool(paths.main_db.as_path(), 1)
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

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "DELETE_BOOK:book-1",
        1_000,
        Some("book-1".to_string()),
    ));
    scheduler
        .process_available(&runtime)
        .expect("delete-book oneshot readonly runtime should still drain cleanly");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_delete_series_soft_deletes_rows_and_removes_series_sidecar_files() {
    let paths = new_router_fixture("runtime-delete-series-soft-delete-staging").await;
    seed_router_contract_data(&paths).await;

    let series_dir = paths.config_dir.join("delete-series/series-1");
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

    let pool = connect_pool(paths.main_db.as_path(), 1)
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

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "DELETE_SERIES:series-1",
        1_000,
        Some("series-1".to_string()),
    ));
    scheduler
        .process_available(&runtime)
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

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
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
        0,
        "delete-series runtime should immediately recompute active book count to zero"
    );
    assert_ne!(
        series_row.get::<String, _>("LAST_MODIFIED"),
        series_old_last_modified,
        "delete-series runtime should refresh series last-modified for downstream visibility",
    );
    assert_eq!(book_thumbnail_count, 2);
    assert_eq!(series_thumbnail_count, 1);
    assert_eq!(read_progress_count, 1);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_delete_series_skips_soft_delete_when_series_directory_is_missing() {
    let paths = new_router_fixture("runtime-delete-series-missing-directory-no-staging").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
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

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "DELETE_SERIES:series-1",
        1_000,
        Some("series-1".to_string()),
    ));
    scheduler
        .process_available(&runtime)
        .expect("delete-series missing-directory runtime should still drain cleanly");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
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

    cleanup_router_fixture(paths);
}
