use super::*;

const GIF_1X1: &[u8] = &[
    0x47, 0x49, 0x46, 0x38, 0x37, 0x61, 0x01, 0x00, 0x01, 0x00, 0x80, 0x00, 0x00, 0xFF, 0xFF, 0xFF,
    0xFF, 0xFF, 0xFF, 0x2C, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x02, 0x02, 0x44,
    0x01, 0x00, 0x3B,
];

#[tokio::test]
async fn runtime_blocks_book_thumbnail_generation_when_main_database_is_external_owned() {
    let paths = new_router_fixture("runtime-blocked-main-database-thumbnail").await;
    seed_router_contract_data(&paths).await;
    write_router_epub_resource(&paths, "books/book-1.epub", "OEBPS/cover.gif", GIF_1X1);

    let runtime = TaskRuntimeContext {
        owns_main_database: false,
        ..runtime_task_context(&paths)
    };
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler
        .enqueue(TaskQueueRecord::new(
            "GenerateBookThumbnail:book-1",
            1_000,
            Some("book-1".to_string()),
        ))
        .await;
    scheduler
        .process_available(&runtime)
        .await
        .expect("blocked main-database thumbnail generation should still drain cleanly");

    let verify_pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for thumbnail verification");
    let generated_count = sqlx::query(
        "SELECT COUNT(*) AS COUNT FROM THUMBNAIL_BOOK WHERE BOOK_ID = ? AND TYPE = 'GENERATED'",
    )
    .bind("book-1")
    .fetch_one(&verify_pool)
    .await
    .expect("generated thumbnail rows should be queryable")
    .get::<i64, _>("COUNT");
    verify_pool.close().await;

    assert_eq!(
        generated_count, 0,
        "runtime must not generate book thumbnails when main database is external-owned",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_generate_book_thumbnail_replaces_invalid_selected_thumbnail_with_generated_selection()
 {
    let paths =
        new_router_fixture("runtime-generate-book-thumbnail-replaces-invalid-selected-thumbnail")
            .await;
    seed_router_contract_data(&paths).await;
    write_router_epub_resource(&paths, "books/book-1.epub", "OEBPS/cover.gif", GIF_1X1);

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler
        .enqueue(
            TaskQueueRecord::new("GenerateBookThumbnail_book-1", 1_000, None)
                .with_simple_type("GenerateBookThumbnail"),
        )
        .await;
    scheduler
        .process_available(&runtime)
        .await
        .expect("generate-book-thumbnail task should replace invalid selected thumbnail cleanly");

    let verify_pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for generated thumbnail verification");
    let thumbnails = sqlx::query(
        "SELECT ID, TYPE, SELECTED FROM THUMBNAIL_BOOK WHERE BOOK_ID = ? ORDER BY ID ASC",
    )
    .bind("book-1")
    .fetch_all(&verify_pool)
    .await
    .expect("book thumbnail rows should be queryable after generated thumbnail task");
    verify_pool.close().await;

    assert_eq!(
        thumbnails.len(),
        1,
        "kotlin parity requires invalid selected thumbnails to be cleaned up during generated thumbnail insert",
    );
    assert_eq!(thumbnails[0].get::<String, _>("TYPE"), "GENERATED");
    assert!(
        thumbnails[0].get::<bool, _>("SELECTED"),
        "generated thumbnail should become selected after housekeeping removes the invalid previous selection",
    );

    cleanup_router_fixture(paths);
}
