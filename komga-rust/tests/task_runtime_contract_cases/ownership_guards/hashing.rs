use super::*;
use komga_infrastructure::sqlite::{
    connect_task_pool, connect_task_write_pool, default_read_max_connections,
};

#[tokio::test]
async fn runtime_blocks_book_hash_when_main_database_is_external_owned() {
    let paths = new_router_fixture("runtime-blocked-main-database-hash-book").await;
    seed_router_contract_data(&paths).await;
    std::fs::create_dir_all(paths.config_dir.join("books"))
        .expect("book directory should exist for hash fixture");
    std::fs::write(
        paths.config_dir.join("books/book-1.epub"),
        b"hash-book-fixture",
    )
    .expect("book file should be written for hash fixture");

    let task_write_pool = connect_task_write_pool(&paths.main_db)
        .await
        .expect("test private write pool should open");
    let task_read_pool = connect_task_pool(&paths.main_db, default_read_max_connections())
        .await
        .expect("test private read pool should open");
    let runtime = TaskRuntimeContext {
        owns_main_database: false,
        task_write_pool,
        task_read_pool,
        ..runtime_task_context(&paths).await
    };
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    scheduler
        .enqueue(
            TaskQueueRecord::new("HashBook_book-1", 1_000, Some("book-1".to_string()))
                .with_simple_type("HashBook"),
        )
        .await;
    scheduler
        .process_available(&runtime)
        .await
        .expect("blocked main-database hash-book should still drain cleanly");

    let verify_pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for hash verification");
    let file_hash = sqlx::query("SELECT FILE_HASH FROM BOOK WHERE ID = ? LIMIT 1")
        .bind("book-1")
        .fetch_one(&verify_pool)
        .await
        .expect("book hash should be queryable")
        .get::<Option<String>, _>("FILE_HASH");
    verify_pool.close().await;

    assert_eq!(
        file_hash,
        Some(String::new()),
        "runtime must not persist book hashes when main database is external-owned",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_skips_book_hash_when_library_hash_files_was_disabled_after_enqueue() {
    let paths = new_router_fixture("runtime-skip-hash-book-when-library-hash-files-disabled").await;
    seed_router_contract_data(&paths).await;
    std::fs::create_dir_all(paths.config_dir.join("books"))
        .expect("book directory should exist for hash-files disabled fixture");
    std::fs::write(
        paths.config_dir.join("books/hash-book.cbz"),
        b"hash-files-disabled",
    )
    .expect("book file should be written for hash-files disabled fixture");

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for hash-files disabled fixture setup");
    sqlx::query("UPDATE LIBRARY SET HASH_FILES = 0 WHERE ID = ?")
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("library hash-files flag should be disabled for runtime hash skip test");
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID, FILE_HASH) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-hash-flag-1")
    .bind(0_i64)
    .bind("hash-book.cbz")
    .bind("books/hash-book.cbz")
    .bind("series-1")
    .bind(19_i64)
    .bind(2_i64)
    .bind("library-1")
    .bind("")
    .execute(&pool)
    .await
    .expect("hash-files disabled fixture book row should be inserted");
    pool.close().await;

    let runtime = runtime_task_context(&paths).await;
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    scheduler
        .enqueue(
            TaskQueueRecord::new("HashBook_book-hash-flag-1", 1_000, None)
                .with_simple_type("HashBook"),
        )
        .await;
    scheduler.process_available(&runtime).await.expect(
        "hash-book task should skip cleanly when library hash-files was disabled after enqueue",
    );

    let verify_pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for hash-files disabled verification");
    let file_hash = sqlx::query("SELECT FILE_HASH FROM BOOK WHERE ID = ? LIMIT 1")
        .bind("book-hash-flag-1")
        .fetch_one(&verify_pool)
        .await
        .expect("book hash should be queryable for disabled-flag verification")
        .get::<Option<String>, _>("FILE_HASH");
    verify_pool.close().await;

    assert_eq!(
        file_hash,
        Some(String::new()),
        "runtime must skip file hashing when library.hashFiles was disabled after the task was enqueued",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_skips_book_hash_when_book_already_has_hash() {
    let paths = new_router_fixture("runtime-skip-hash-book-when-already-present").await;
    seed_router_contract_data(&paths).await;
    std::fs::create_dir_all(paths.config_dir.join("books"))
        .expect("book directory should exist for existing hash fixture");
    std::fs::write(
        paths.config_dir.join("books/book-1.epub"),
        b"hash-should-not-overwrite",
    )
    .expect("book file should be written for existing hash fixture");

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for existing hash fixture setup");
    sqlx::query("UPDATE BOOK SET FILE_HASH = ? WHERE ID = ?")
        .bind("hash-book-existing")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("existing book hash should be seeded for skip test");
    pool.close().await;

    let runtime = runtime_task_context(&paths).await;
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    scheduler
        .enqueue(TaskQueueRecord::new("HashBook_book-1", 1_000, None).with_simple_type("HashBook"))
        .await;
    scheduler
        .process_available(&runtime)
        .await
        .expect("hash-book task should skip cleanly when the book already has a hash");

    let verify_pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for existing hash verification");
    let file_hash = sqlx::query("SELECT FILE_HASH FROM BOOK WHERE ID = ? LIMIT 1")
        .bind("book-1")
        .fetch_one(&verify_pool)
        .await
        .expect("existing file hash should be queryable")
        .get::<Option<String>, _>("FILE_HASH");
    verify_pool.close().await;

    assert_eq!(
        file_hash,
        Some("hash-book-existing".to_string()),
        "runtime must not overwrite an existing file hash",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_blocks_book_page_hash_when_main_database_is_external_owned() {
    let paths = new_router_fixture("runtime-blocked-main-database-page-hash").await;
    seed_router_contract_data(&paths).await;
    const GIF_1X1: &[u8] = &[
        0x47, 0x49, 0x46, 0x38, 0x37, 0x61, 0x01, 0x00, 0x01, 0x00, 0x80, 0x00, 0x00, 0xFF, 0xFF,
        0xFF, 0xFF, 0xFF, 0xFF, 0x2C, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x02,
        0x02, 0x44, 0x01, 0x00, 0x3B,
    ];
    std::fs::create_dir_all(paths.config_dir.join("books"))
        .expect("book directory should exist for page-hash fixture");
    std::fs::write(paths.config_dir.join("books/hash-image.gif"), GIF_1X1)
        .expect("image file should be written for page-hash fixture");

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for page-hash fixture setup");
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-hash-1")
    .bind(0_i64)
    .bind("hash-image.gif")
    .bind("books/hash-image.gif")
    .bind("series-1")
    .bind(i64::try_from(GIF_1X1.len()).expect("gif size should fit in i64"))
    .bind(2_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("page-hash fixture book row should be inserted");
    sqlx::query(
        "INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) \
         VALUES (?, ?, ?, ?)",
    )
    .bind("image/gif")
    .bind("READY")
    .bind("book-hash-1")
    .bind(1_i64)
    .execute(&pool)
    .await
    .expect("page-hash fixture media row should be inserted");
    sqlx::query(
        "INSERT INTO MEDIA_PAGE (FILE_NAME, MEDIA_TYPE, NUMBER, BOOK_ID, width, height, FILE_HASH, FILE_SIZE) VALUES (?, ?, ?, ?, NULL, NULL, '', ?)",
    )
    .bind("hash-image.gif")
    .bind("image/gif")
    .bind(1_i64)
    .bind("book-hash-1")
    .bind(i64::try_from(GIF_1X1.len()).expect("gif size should fit in i64"))
    .execute(&pool)
    .await
    .expect("page-hash fixture media page row should be inserted");
    pool.close().await;

    let task_write_pool = connect_task_write_pool(&paths.main_db)
        .await
        .expect("test private write pool should open");
    let task_read_pool = connect_task_pool(&paths.main_db, default_read_max_connections())
        .await
        .expect("test private read pool should open");
    let runtime = TaskRuntimeContext {
        owns_main_database: false,
        task_write_pool,
        task_read_pool,
        ..runtime_task_context(&paths).await
    };
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    scheduler
        .enqueue(
            TaskQueueRecord::new(
                "HashBookPages_book-hash-1",
                1_000,
                Some("book-hash-1".to_string()),
            )
            .with_simple_type("HashBookPages"),
        )
        .await;
    scheduler
        .process_available(&runtime)
        .await
        .expect("blocked main-database page-hash should still drain cleanly");

    let verify_pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for page-hash verification");
    let file_hash =
        sqlx::query("SELECT FILE_HASH FROM MEDIA_PAGE WHERE BOOK_ID = ? AND NUMBER = 1 LIMIT 1")
            .bind("book-hash-1")
            .fetch_one(&verify_pool)
            .await
            .expect("media page hash should be queryable")
            .get::<String, _>("FILE_HASH");
    verify_pool.close().await;

    assert_eq!(
        file_hash,
        String::new(),
        "runtime must not persist page hashes when main database is external-owned",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_skips_book_koreader_hash_when_library_hash_koreader_was_disabled_after_enqueue() {
    let paths =
        new_router_fixture("runtime-skip-koreader-hash-when-library-hash-koreader-disabled").await;
    seed_router_contract_data(&paths).await;
    std::fs::create_dir_all(paths.config_dir.join("books"))
        .expect("book directory should exist for koreader-hash disabled fixture");
    std::fs::write(
        paths.config_dir.join("books/koreader-book.cbz"),
        b"koreader-hash-disabled",
    )
    .expect("book file should be written for koreader-hash disabled fixture");

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for koreader-hash disabled fixture setup");
    sqlx::query("UPDATE LIBRARY SET HASH_KOREADER = 0 WHERE ID = ?")
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("library hash-koreader flag should be disabled for runtime skip test");
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID, FILE_HASH_KOREADER) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-koreader-flag-1")
    .bind(0_i64)
    .bind("koreader-book.cbz")
    .bind("books/koreader-book.cbz")
    .bind("series-1")
    .bind(22_i64)
    .bind(2_i64)
    .bind("library-1")
    .bind("")
    .execute(&pool)
    .await
    .expect("koreader-hash disabled fixture book row should be inserted");
    pool.close().await;

    let runtime = runtime_task_context(&paths).await;
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    scheduler
        .enqueue(
            TaskQueueRecord::new("HashBookKoreader_book-koreader-flag-1", 1_000, None)
                .with_simple_type("HashBookKoreader"),
        )
        .await;
    scheduler
        .process_available(&runtime).await
        .expect("koreader-hash task should skip cleanly when library hash-koreader was disabled after enqueue");

    let verify_pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for koreader-hash disabled verification");
    let file_hash = sqlx::query("SELECT FILE_HASH_KOREADER FROM BOOK WHERE ID = ? LIMIT 1")
        .bind("book-koreader-flag-1")
        .fetch_one(&verify_pool)
        .await
        .expect("koreader hash should be queryable for disabled-flag verification")
        .get::<Option<String>, _>("FILE_HASH_KOREADER");
    verify_pool.close().await;

    assert_eq!(
        file_hash,
        Some(String::new()),
        "runtime must skip koreader hashing when library.hashKoreader was disabled after the task was enqueued",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_skips_book_koreader_hash_when_book_already_has_hash() {
    let paths = new_router_fixture("runtime-skip-koreader-hash-when-already-present").await;
    seed_router_contract_data(&paths).await;
    std::fs::create_dir_all(paths.config_dir.join("books"))
        .expect("book directory should exist for existing koreader hash fixture");
    std::fs::write(
        paths.config_dir.join("books/book-1.epub"),
        b"koreader-hash-should-not-overwrite",
    )
    .expect("book file should be written for existing koreader hash fixture");

    let runtime = runtime_task_context(&paths).await;
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    scheduler
        .enqueue(
            TaskQueueRecord::new("HashBookKoreader_book-1", 1_000, None)
                .with_simple_type("HashBookKoreader"),
        )
        .await;
    scheduler
        .process_available(&runtime)
        .await
        .expect("koreader-hash task should skip cleanly when the book already has a koreader hash");

    let verify_pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for existing koreader hash verification");
    let file_hash = sqlx::query("SELECT FILE_HASH_KOREADER FROM BOOK WHERE ID = ? LIMIT 1")
        .bind("book-1")
        .fetch_one(&verify_pool)
        .await
        .expect("existing koreader hash should be queryable")
        .get::<Option<String>, _>("FILE_HASH_KOREADER");
    verify_pool.close().await;

    assert_eq!(
        file_hash,
        Some("hash-book-1".to_string()),
        "runtime must not overwrite an existing koreader hash",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_skips_book_page_hash_when_library_hash_pages_was_disabled_after_enqueue() {
    let paths = new_router_fixture("runtime-skip-page-hash-when-library-hash-pages-disabled").await;
    seed_router_contract_data(&paths).await;
    const GIF_1X1: &[u8] = &[
        0x47, 0x49, 0x46, 0x38, 0x37, 0x61, 0x01, 0x00, 0x01, 0x00, 0x80, 0x00, 0x00, 0xFF, 0xFF,
        0xFF, 0xFF, 0xFF, 0xFF, 0x2C, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x02,
        0x02, 0x44, 0x01, 0x00, 0x3B,
    ];
    std::fs::create_dir_all(paths.config_dir.join("books"))
        .expect("book directory should exist for page-hash disabled fixture");
    std::fs::write(paths.config_dir.join("books/hash-image.gif"), GIF_1X1)
        .expect("image file should be written for page-hash disabled fixture");

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for page-hash disabled fixture setup");
    sqlx::query("UPDATE LIBRARY SET HASH_PAGES = 0 WHERE ID = ?")
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("library hash-pages flag should be disabled for runtime page-hash skip test");
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-hash-flag-1")
    .bind(0_i64)
    .bind("hash-image.gif")
    .bind("books/hash-image.gif")
    .bind("series-1")
    .bind(i64::try_from(GIF_1X1.len()).expect("gif size should fit in i64"))
    .bind(2_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("page-hash disabled fixture book row should be inserted");
    sqlx::query("INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("image/gif")
        .bind("READY")
        .bind("book-hash-flag-1")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("page-hash disabled fixture media row should be inserted");
    sqlx::query(
        "INSERT INTO MEDIA_PAGE (FILE_NAME, MEDIA_TYPE, NUMBER, BOOK_ID, width, height, FILE_HASH, FILE_SIZE) VALUES (?, ?, ?, ?, NULL, NULL, '', ?)",
    )
    .bind("hash-image.gif")
    .bind("image/gif")
    .bind(1_i64)
    .bind("book-hash-flag-1")
    .bind(i64::try_from(GIF_1X1.len()).expect("gif size should fit in i64"))
    .execute(&pool)
    .await
    .expect("page-hash disabled fixture media page row should be inserted");
    pool.close().await;

    let runtime = runtime_task_context(&paths).await;
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    scheduler
        .enqueue(
            TaskQueueRecord::new("HashBookPages_book-hash-flag-1", 1_000, None)
                .with_simple_type("HashBookPages"),
        )
        .await;
    scheduler.process_available(&runtime).await.expect(
        "page-hash task should skip cleanly when library hash-pages was disabled after enqueue",
    );

    let verify_pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for page-hash disabled verification");
    let file_hash =
        sqlx::query("SELECT FILE_HASH FROM MEDIA_PAGE WHERE BOOK_ID = ? AND NUMBER = 1 LIMIT 1")
            .bind("book-hash-flag-1")
            .fetch_one(&verify_pool)
            .await
            .expect("page-hash disabled fixture media page hash should be queryable")
            .get::<String, _>("FILE_HASH");
    verify_pool.close().await;

    assert_eq!(
        file_hash,
        String::new(),
        "runtime must skip page hashing when library.hashPages was disabled after the task was enqueued",
    );

    cleanup_router_fixture(paths);
}
