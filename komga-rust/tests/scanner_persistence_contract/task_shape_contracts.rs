use super::support::*;
use super::*;

#[tokio::test]
async fn scanner_persists_hash_book_tasks_with_kotlin_task_shape() {
    let fixture = ScannerPersistenceFixture::new("scanner-persistence-hash-book-shape")
        .await
        .expect("scanner hash-book task fixture should be created");

    let mut scheduler = TaskQueueScheduler::for_runtime(fixture.config.clone(), "rust-main");
    scheduler
        .enqueue(TaskQueueRecord::new("HASH_BOOK_book-1", 0, None).with_simple_type("HASH_BOOK"));

    assert_persisted_task_shape(
        fixture.paths.tasks_db.as_path(),
        "HASH_BOOK_book-1",
        "org.gotson.komga.application.tasks.Task$HashBook",
        "HashBook",
        None,
        json!({
            "bookId": "book-1",
            "priority": 0,
            "groupId": Value::Null,
            "uniqueId": "HASH_BOOK_book-1"
        }),
    )
    .await;

    fixture.cleanup();
}

#[tokio::test]
async fn scanner_persists_refresh_book_metadata_tasks_with_kotlin_task_shape() {
    let fixture = ScannerPersistenceFixture::new("scanner-persistence-refresh-book-metadata-shape")
        .await
        .expect("scanner refresh-book-metadata task fixture should be created");

    let mut scheduler = TaskQueueScheduler::for_runtime(fixture.config.clone(), "rust-main");
    scheduler.enqueue(
        TaskQueueRecord::new(
            "REFRESH_BOOK_METADATA_book-1",
            80,
            Some("series-1".to_string()),
        )
        .with_simple_type("REFRESH_BOOK_METADATA"),
    );

    assert_persisted_task_shape(
        fixture.paths.tasks_db.as_path(),
        "REFRESH_BOOK_METADATA_book-1",
        "org.gotson.komga.application.tasks.Task$RefreshBookMetadata",
        "RefreshBookMetadata",
        Some("series-1"),
        json!({
            "bookId": "book-1",
            "capabilities": [
                "TITLE",
                "SUMMARY",
                "NUMBER",
                "NUMBER_SORT",
                "RELEASE_DATE",
                "AUTHORS",
                "TAGS",
                "ISBN",
                "READ_LISTS",
                "THUMBNAILS",
                "LINKS"
            ],
            "priority": 80,
            "groupId": "series-1",
            "uniqueId": "REFRESH_BOOK_METADATA_book-1"
        }),
    )
    .await;

    fixture.cleanup();
}

#[tokio::test]
async fn scanner_persists_find_duplicate_pages_to_delete_tasks_with_kotlin_task_shape() {
    let fixture = ScannerPersistenceFixture::new("scanner-persistence-find-duplicate-pages-shape")
        .await
        .expect("scanner duplicate-pages task fixture should be created");

    let mut scheduler = TaskQueueScheduler::for_runtime(fixture.config.clone(), "rust-main");
    scheduler.enqueue(
        TaskQueueRecord::new("FIND_DUPLICATE_PAGES_TO_DELETE_library-1", 85, None)
            .with_simple_type("FIND_DUPLICATE_PAGES_TO_DELETE"),
    );

    assert_persisted_task_shape(
        fixture.paths.tasks_db.as_path(),
        "FIND_DUPLICATE_PAGES_TO_DELETE_library-1",
        "org.gotson.komga.application.tasks.Task$FindDuplicatePagesToDelete",
        "FindDuplicatePagesToDelete",
        None,
        json!({
            "libraryId": "library-1",
            "priority": 85,
            "groupId": Value::Null,
            "uniqueId": "FIND_DUPLICATE_PAGES_TO_DELETE_library-1"
        }),
    )
    .await;

    fixture.cleanup();
}

#[tokio::test]
async fn scanner_persists_find_books_with_missing_page_hash_tasks_with_kotlin_task_shape() {
    let fixture =
        ScannerPersistenceFixture::new("scanner-persistence-find-missing-page-hash-shape")
            .await
            .expect("scanner missing-page-hash task fixture should be created");

    let mut scheduler = TaskQueueScheduler::for_runtime(fixture.config.clone(), "rust-main");
    scheduler.enqueue(
        TaskQueueRecord::new("FIND_BOOKS_WITH_MISSING_PAGE_HASH_library-1", 0, None)
            .with_simple_type("FIND_BOOKS_WITH_MISSING_PAGE_HASH"),
    );

    assert_persisted_task_shape(
        fixture.paths.tasks_db.as_path(),
        "FIND_BOOKS_WITH_MISSING_PAGE_HASH_library-1",
        "org.gotson.komga.application.tasks.Task$FindBooksWithMissingPageHash",
        "FindBooksWithMissingPageHash",
        None,
        json!({
            "libraryId": "library-1",
            "priority": 0,
            "groupId": Value::Null,
            "uniqueId": "FIND_BOOKS_WITH_MISSING_PAGE_HASH_library-1"
        }),
    )
    .await;

    fixture.cleanup();
}
