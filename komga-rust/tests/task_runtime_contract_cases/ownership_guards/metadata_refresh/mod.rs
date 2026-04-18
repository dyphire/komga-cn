use super::*;

mod provider_formats;

pub(super) use provider_formats::{
    write_router_epub_with_package_document, write_router_epub_with_package_document_and_entries,
};

#[tokio::test]
async fn runtime_refresh_book_metadata_can_import_readlists_without_applying_book_fields() {
    for (fixture_name, xml, readlist_name, expected_number) in [
        (
            "runtime-refresh-book-metadata-readlists-without-book-fields",
            br#"<ComicInfo><Title>Should Stay Book 1</Title><AlternateSeries>Reading Order</AlternateSeries><AlternateNumber>7</AlternateNumber></ComicInfo>"#.as_slice(),
            "Reading Order",
            7_i64,
        ),
        (
            "runtime-refresh-book-metadata-readlists-without-explicit-number",
            br#"<ComicInfo><StoryArc>Unnumbered Reading Order</StoryArc></ComicInfo>"#.as_slice(),
            "Unnumbered Reading Order",
            0_i64,
        ),
    ] {
        let paths = new_router_fixture(fixture_name).await;
        seed_router_contract_data(&paths).await;

        let sidecar_dir = paths.config_dir.join("books");
        std::fs::create_dir_all(&sidecar_dir)
            .expect("book metadata sidecar directory should exist");
        std::fs::write(sidecar_dir.join("book-1.xml"), xml)
            .expect("book metadata sidecar fixture with read list should be written");

        let pool = connect_test_pool(paths.main_db.as_path(), 1)
            .await
            .expect("main db should open for readlist-only metadata fixture setup");
        sqlx::query(
            "UPDATE LIBRARY SET IMPORT_COMICINFO_BOOK = 0, IMPORT_COMICINFO_READLIST = 1 WHERE ID = ?",
        )
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("library ComicInfo import flags should be updated for readlist-only metadata test");
        sqlx::query("DELETE FROM SIDECAR WHERE PARENT_URL = ?")
            .bind("books/book-1.epub")
            .execute(&pool)
            .await
            .expect("existing book metadata sidecars should be cleared before readlist-only test");
        sqlx::query("DELETE FROM READLIST_BOOK")
            .execute(&pool)
            .await
            .expect("existing readlist memberships should be cleared before readlist-only test");
        sqlx::query("DELETE FROM READLIST")
            .execute(&pool)
            .await
            .expect("existing readlists should be cleared before readlist-only test");
        sqlx::query(
            "INSERT INTO SIDECAR (URL, PARENT_URL, LAST_MODIFIED_TIME, LIBRARY_ID) VALUES (?, ?, ?, ?)",
        )
        .bind("books/book-1.xml")
        .bind("books/book-1.epub")
        .bind(1_i64)
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("book metadata sidecar row should be inserted for readlist-only test");
        pool.close().await;

        let tasks_pool = connect_test_pool(paths.tasks_db.as_path(), 1)
            .await
            .expect("tasks db should open for readlist-only metadata task setup");
        sqlx::query(
            "INSERT INTO TASK (ID, PRIORITY, GROUP_ID, CLASS, SIMPLE_TYPE, PAYLOAD, OWNER) VALUES (?, ?, ?, ?, ?, ?, NULL)",
        )
        .bind("REFRESH_BOOK_METADATA_book-1")
        .bind(80_i64)
        .bind("series-1")
        .bind("org.gotson.komga.application.tasks.Task$RefreshBookMetadata")
        .bind("RefreshBookMetadata")
        .bind(
            json!({
                "bookId": "book-1",
                "capabilities": ["READ_LISTS"],
                "priority": 80,
                "groupId": "series-1",
                "uniqueId": "REFRESH_BOOK_METADATA_book-1"
            })
            .to_string(),
        )
        .execute(&tasks_pool)
        .await
        .expect("readlist-only metadata task row should be inserted");
        tasks_pool.close().await;

        let runtime = runtime_task_context(&paths);
        let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
        scheduler
            .process_available(&runtime)
            .expect("runtime should process readlist-only RefreshBookMetadata tasks successfully");

        let verify_pool = connect_test_pool(paths.main_db.as_path(), 1)
            .await
            .expect("main db should open for readlist-only metadata verification");
        let metadata = sqlx::query("SELECT TITLE FROM BOOK_METADATA WHERE BOOK_ID = ? LIMIT 1")
            .bind("book-1")
            .fetch_one(&verify_pool)
            .await
            .expect("book metadata row should be queryable after readlist-only task execution");
        let readlist = sqlx::query(
            "SELECT ID, NAME, BOOK_COUNT, ORDERED FROM READLIST WHERE NAME = ? LIMIT 1",
        )
        .bind(readlist_name)
        .fetch_one(&verify_pool)
        .await
        .expect("ComicInfo read list should be created when READ_LISTS capability is enabled");
        let readlist_book = sqlx::query(
            "SELECT NUMBER FROM READLIST_BOOK WHERE READLIST_ID = ? AND BOOK_ID = ? LIMIT 1",
        )
        .bind(readlist.get::<String, _>("ID"))
        .bind("book-1")
        .fetch_one(&verify_pool)
        .await
        .expect("ComicInfo read list should contain the refreshed book");
        verify_pool.close().await;

        assert_eq!(
            metadata.get::<String, _>("TITLE"),
            "Book 1",
            "READ_LISTS-only refresh must not apply ComicInfo book fields when importComicInfoBook is disabled",
        );
        assert_eq!(readlist.get::<String, _>("NAME"), readlist_name);
        assert_eq!(readlist.get::<i64, _>("BOOK_COUNT"), 1);
        assert_eq!(readlist.get::<i64, _>("ORDERED"), 1);
        assert_eq!(readlist_book.get::<i64, _>("NUMBER"), expected_number);

        cleanup_router_fixture(paths);
    }
}

#[tokio::test]
async fn runtime_refresh_book_metadata_applies_comicinfo_number_when_capability_requests_it() {
    let paths = new_router_fixture("runtime-refresh-book-metadata-applies-comicinfo-number").await;
    seed_router_contract_data(&paths).await;

    let sidecar_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&sidecar_dir).expect("book metadata sidecar directory should exist");
    std::fs::write(
        sidecar_dir.join("book-1.xml"),
        br#"<ComicInfo><Number>7</Number></ComicInfo>"#,
    )
    .expect("book metadata sidecar fixture with number should be written");

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for number metadata fixture setup");
    sqlx::query("DELETE FROM SIDECAR WHERE PARENT_URL = ?")
        .bind("books/book-1.epub")
        .execute(&pool)
        .await
        .expect("existing book metadata sidecars should be cleared before number capability test");
    sqlx::query(
        "INSERT INTO SIDECAR (URL, PARENT_URL, LAST_MODIFIED_TIME, LIBRARY_ID) VALUES (?, ?, ?, ?)",
    )
    .bind("books/book-1.xml")
    .bind("books/book-1.epub")
    .bind(1_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("book metadata sidecar row should be inserted for number capability test");
    pool.close().await;

    let tasks_pool = connect_test_pool(paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for number metadata task setup");
    sqlx::query(
        "INSERT INTO TASK (ID, PRIORITY, GROUP_ID, CLASS, SIMPLE_TYPE, PAYLOAD, OWNER) VALUES (?, ?, ?, ?, ?, ?, NULL)",
    )
    .bind("REFRESH_BOOK_METADATA_book-1")
    .bind(80_i64)
    .bind("series-1")
    .bind("org.gotson.komga.application.tasks.Task$RefreshBookMetadata")
    .bind("RefreshBookMetadata")
    .bind(
        json!({
            "bookId": "book-1",
            "capabilities": ["NUMBER"],
            "priority": 80,
            "groupId": "series-1",
            "uniqueId": "REFRESH_BOOK_METADATA_book-1"
        })
        .to_string(),
    )
    .execute(&tasks_pool)
    .await
    .expect("number-only metadata task row should be inserted");
    tasks_pool.close().await;

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler
        .process_available(&runtime)
        .expect("runtime should process number-only RefreshBookMetadata tasks successfully");

    let verify_pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for number metadata verification");
    let metadata =
        sqlx::query("SELECT NUMBER, NUMBER_SORT FROM BOOK_METADATA WHERE BOOK_ID = ? LIMIT 1")
            .bind("book-1")
            .fetch_one(&verify_pool)
            .await
            .expect("book metadata number row should be queryable after number capability task");
    verify_pool.close().await;

    assert_eq!(metadata.get::<String, _>("NUMBER"), "7");
    assert_eq!(metadata.get::<f64, _>("NUMBER_SORT"), 7.0_f64);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_refresh_book_metadata_applies_remaining_comicinfo_fields_with_lock_semantics() {
    let paths =
        new_router_fixture("runtime-refresh-book-metadata-applies-remaining-comicinfo-fields")
            .await;
    seed_router_contract_data(&paths).await;

    let sidecar_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&sidecar_dir).expect("book metadata sidecar directory should exist");
    std::fs::write(
        sidecar_dir.join("book-1.xml"),
        br#"<ComicInfo><Year>2025</Year><Month>3</Month><Day>4</Day><Writer>Alice Writer, Bob Writer</Writer><Penciller>Cara Pencil</Penciller><Web>https://example.com/series https://komga.org/docs invalid-url</Web><Tags>Sci-Fi, Adventure, sci-fi</Tags><GTIN>9780306406157</GTIN></ComicInfo>"#,
    )
    .expect("book metadata sidecar fixture with remaining ComicInfo fields should be written");

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for remaining ComicInfo metadata fixture setup");
    sqlx::query("DELETE FROM SIDECAR WHERE PARENT_URL = ?")
        .bind("books/book-1.epub")
        .execute(&pool)
        .await
        .expect(
            "existing book metadata sidecars should be cleared before remaining ComicInfo test",
        );
    sqlx::query(
        "INSERT INTO SIDECAR (URL, PARENT_URL, LAST_MODIFIED_TIME, LIBRARY_ID) VALUES (?, ?, ?, ?)",
    )
    .bind("books/book-1.xml")
    .bind("books/book-1.epub")
    .bind(1_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("book metadata sidecar row should be inserted for remaining ComicInfo test");
    sqlx::query(
        "UPDATE BOOK_METADATA SET RELEASE_DATE = ?, RELEASE_DATE_LOCK = 1, ISBN = ?, ISBN_LOCK = 1, AUTHORS_LOCK = 0, TAGS_LOCK = 0, LINKS_LOCK = 0 WHERE BOOK_ID = ?",
    )
    .bind("2024-01-15")
    .bind("9789999999991")
    .bind("book-1")
    .execute(&pool)
    .await
    .expect("book metadata lock state should be updated for remaining ComicInfo test");
    sqlx::query("DELETE FROM BOOK_METADATA_LINK WHERE BOOK_ID = ?")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("existing metadata links should be cleared before remaining ComicInfo test");
    sqlx::query("INSERT INTO BOOK_METADATA_LINK (BOOK_ID, LABEL, URL) VALUES (?, ?, ?)")
        .bind("book-1")
        .bind("old.example")
        .bind("https://old.example/link")
        .execute(&pool)
        .await
        .expect("seed metadata link should be inserted before remaining ComicInfo test");
    pool.close().await;

    let tasks_pool = connect_test_pool(paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for remaining ComicInfo metadata task setup");
    sqlx::query(
        "INSERT INTO TASK (ID, PRIORITY, GROUP_ID, CLASS, SIMPLE_TYPE, PAYLOAD, OWNER) VALUES (?, ?, ?, ?, ?, ?, NULL)",
    )
    .bind("REFRESH_BOOK_METADATA_book-1")
    .bind(80_i64)
    .bind("series-1")
    .bind("org.gotson.komga.application.tasks.Task$RefreshBookMetadata")
    .bind("RefreshBookMetadata")
    .bind(
        json!({
            "bookId": "book-1",
            "capabilities": ["RELEASE_DATE", "AUTHORS", "TAGS", "ISBN", "LINKS"],
            "priority": 80,
            "groupId": "series-1",
            "uniqueId": "REFRESH_BOOK_METADATA_book-1"
        })
        .to_string(),
    )
    .execute(&tasks_pool)
    .await
    .expect("remaining ComicInfo metadata task row should be inserted");
    tasks_pool.close().await;

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler
        .process_available(&runtime)
        .expect("runtime should process remaining ComicInfo metadata fields successfully");

    let verify_pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for remaining ComicInfo metadata verification");
    let metadata =
        sqlx::query("SELECT RELEASE_DATE, ISBN FROM BOOK_METADATA WHERE BOOK_ID = ? LIMIT 1")
            .bind("book-1")
            .fetch_one(&verify_pool)
            .await
            .expect(
                "book metadata row should be queryable after remaining ComicInfo task execution",
            );
    let authors = sqlx::query(
        "SELECT NAME, ROLE FROM BOOK_METADATA_AUTHOR WHERE BOOK_ID = ? ORDER BY ROLE ASC, NAME ASC",
    )
    .bind("book-1")
    .fetch_all(&verify_pool)
    .await
    .expect("book metadata authors should be queryable after remaining ComicInfo task execution");
    let tags = sqlx::query(
        "SELECT TAG FROM BOOK_METADATA_TAG WHERE BOOK_ID = ? ORDER BY TAG COLLATE NOCASE ASC",
    )
    .bind("book-1")
    .fetch_all(&verify_pool)
    .await
    .expect("book metadata tags should be queryable after remaining ComicInfo task execution");
    let links = sqlx::query(
        "SELECT LABEL, URL FROM BOOK_METADATA_LINK WHERE BOOK_ID = ? ORDER BY LABEL COLLATE NOCASE ASC, URL ASC",
    )
    .bind("book-1")
    .fetch_all(&verify_pool)
    .await
    .expect("book metadata links should be queryable after remaining ComicInfo task execution");
    verify_pool.close().await;

    assert_eq!(metadata.get::<String, _>("RELEASE_DATE"), "2024-01-15");
    assert_eq!(metadata.get::<String, _>("ISBN"), "9789999999991");
    assert_eq!(
        authors
            .iter()
            .map(|row| (row.get::<String, _>("NAME"), row.get::<String, _>("ROLE")))
            .collect::<Vec<_>>(),
        vec![
            ("Cara Pencil".to_string(), "penciller".to_string()),
            ("Alice Writer".to_string(), "writer".to_string()),
            ("Bob Writer".to_string(), "writer".to_string()),
        ],
        "unlocked authors should be replaced from ComicInfo using Kotlin author-role semantics",
    );
    assert_eq!(
        tags.iter()
            .map(|row| row.get::<String, _>("TAG"))
            .collect::<Vec<_>>(),
        vec!["adventure".to_string(), "sci-fi".to_string()],
        "unlocked tags should be lowercased and deduplicated from ComicInfo",
    );
    assert_eq!(
        links
            .iter()
            .map(|row| (row.get::<String, _>("LABEL"), row.get::<String, _>("URL")))
            .collect::<Vec<_>>(),
        vec![
            (
                "example.com".to_string(),
                "https://example.com/series".to_string(),
            ),
            (
                "komga.org".to_string(),
                "https://komga.org/docs".to_string(),
            ),
        ],
        "unlocked links should be replaced from valid ComicInfo Web URIs only",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_refresh_book_metadata_does_not_run_comicinfo_for_isbn_or_tags_only_capabilities() {
    let paths =
        new_router_fixture("runtime-refresh-book-metadata-skips-comicinfo-for-isbn-tags-only")
            .await;
    seed_router_contract_data(&paths).await;
    seed_router_cbz_book(
        &paths,
        "book-comicinfo-gate-1",
        "series-1",
        "comicinfo-gate.cbz",
        "ComicInfo Gate Book",
    )
    .await;

    let sidecar_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&sidecar_dir).expect("book metadata sidecar directory should exist");
    std::fs::write(
        sidecar_dir.join("comicinfo-gate.xml"),
        br#"<ComicInfo><Tags>Sci-Fi, Mystery</Tags><GTIN>9780306406157</GTIN></ComicInfo>"#,
    )
    .expect("ComicInfo ISBN/tags-only sidecar fixture should be written");

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for ComicInfo gate fixture setup");
    sqlx::query("DELETE FROM SIDECAR WHERE PARENT_URL = ?")
        .bind("books/comicinfo-gate.cbz")
        .execute(&pool)
        .await
        .expect("existing ComicInfo gate sidecars should be cleared before test");
    sqlx::query(
        "INSERT INTO SIDECAR (URL, PARENT_URL, LAST_MODIFIED_TIME, LIBRARY_ID) VALUES (?, ?, ?, ?)",
    )
    .bind("books/comicinfo-gate.xml")
    .bind("books/comicinfo-gate.cbz")
    .bind(1_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("ComicInfo gate sidecar row should be inserted");
    sqlx::query(
        "UPDATE LIBRARY SET IMPORT_COMICINFO_BOOK = 1, IMPORT_EPUB_BOOK = 0, IMPORT_BARCODE_ISBN = 0 WHERE ID = ?",
    )
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("library metadata import flags should isolate ComicInfo gate behavior for refresh test");
    sqlx::query("UPDATE BOOK_METADATA SET ISBN = ?, ISBN_LOCK = 0 WHERE BOOK_ID = ?")
        .bind("")
        .bind("book-comicinfo-gate-1")
        .execute(&pool)
        .await
        .expect("book metadata isbn should be reset before ComicInfo gate test");
    sqlx::query("DELETE FROM BOOK_METADATA_TAG WHERE BOOK_ID = ?")
        .bind("book-comicinfo-gate-1")
        .execute(&pool)
        .await
        .expect("book metadata tags should be cleared before ComicInfo gate test");
    pool.close().await;

    let tasks_pool = connect_test_pool(paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for ComicInfo gate metadata task setup");
    sqlx::query(
        "INSERT INTO TASK (ID, PRIORITY, GROUP_ID, CLASS, SIMPLE_TYPE, PAYLOAD, OWNER) VALUES (?, ?, ?, ?, ?, ?, NULL)",
    )
    .bind("REFRESH_BOOK_METADATA_book-comicinfo-gate-1")
    .bind(80_i64)
    .bind("series-1")
    .bind("org.gotson.komga.application.tasks.Task$RefreshBookMetadata")
    .bind("RefreshBookMetadata")
    .bind(
        json!({
            "bookId": "book-comicinfo-gate-1",
            "capabilities": ["ISBN", "TAGS"],
            "priority": 80,
            "groupId": "series-1",
            "uniqueId": "REFRESH_BOOK_METADATA_book-comicinfo-gate-1"
        })
        .to_string(),
    )
    .execute(&tasks_pool)
    .await
    .expect("ComicInfo gate metadata task row should be inserted");
    tasks_pool.close().await;

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler
        .process_available(&runtime)
        .expect("runtime should process ComicInfo gate metadata tasks successfully");

    let verify_pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for ComicInfo gate metadata verification");
    let metadata = sqlx::query("SELECT ISBN FROM BOOK_METADATA WHERE BOOK_ID = ? LIMIT 1")
        .bind("book-comicinfo-gate-1")
        .fetch_one(&verify_pool)
        .await
        .expect("book metadata row should be queryable after ComicInfo gate task execution");
    let tags = sqlx::query("SELECT TAG FROM BOOK_METADATA_TAG WHERE BOOK_ID = ?")
        .bind("book-comicinfo-gate-1")
        .fetch_all(&verify_pool)
        .await
        .expect("book metadata tags should be queryable after ComicInfo gate task execution");
    verify_pool.close().await;

    assert_eq!(
        metadata.get::<String, _>("ISBN"),
        "",
        "ISBN-only refresh must not trigger ComicInfo provider when Kotlin capability gate would skip it",
    );
    assert!(
        tags.is_empty(),
        "TAGS-only refresh must not trigger ComicInfo provider when Kotlin capability gate would skip it",
    );

    cleanup_router_fixture(paths);
}
