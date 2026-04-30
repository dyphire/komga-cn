use super::*;
use komga_infrastructure::sqlite::{
    connect_task_pool, connect_task_write_pool, default_read_max_connections,
};

#[tokio::test]
async fn runtime_refresh_series_metadata_applies_epub_from_book_provider_patch() {
    let ctx = TestFixture::new("runtime-refresh-series-metadata-applies-epub-provider").await;

    write_router_epub_with_package_document(
        ctx.paths(),
        "books/book-1.epub",
        r##"<?xml version="1.0" encoding="UTF-8"?>
        <package version="3.0" xmlns="http://www.idpf.org/2007/opf" xmlns:dc="http://purl.org/dc/elements/1.1/" unique-identifier="bookid">
          <metadata>
            <dc:identifier id="bookid">book-1</dc:identifier>
            <dc:title>EPUB Series Metadata Fixture</dc:title>
            <dc:publisher>EPUB Provider House</dc:publisher>
            <dc:language>EN-us</dc:language>
            <dc:subject>Adventure</dc:subject>
            <dc:subject>Mystery</dc:subject>
            <meta property="belongs-to-collection">EPUB Provider Series</meta>
          </metadata>
          <manifest>
            <item id="main" href="chapter.xhtml" media-type="application/xhtml+xml"/>
          </manifest>
          <spine page-progression-direction="rtl">
            <itemref idref="main"/>
          </spine>
        </package>"##,
    );

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for EPUB series metadata fixture setup");
    sqlx::query(
        "UPDATE LIBRARY SET IMPORT_COMICINFO_SERIES = 0, IMPORT_COMICINFO_COLLECTION = 0, IMPORT_EPUB_SERIES = 1 WHERE ID = ?",
    )
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("library flags should isolate EPUB series metadata provider");
    sqlx::query(
        "UPDATE SERIES_METADATA SET TITLE = ?, TITLE_LOCK = 0, TITLE_SORT = ?, TITLE_SORT_LOCK = 0, READING_DIRECTION = NULL, READING_DIRECTION_LOCK = 0, PUBLISHER = ?, PUBLISHER_LOCK = 0, LANGUAGE = ?, LANGUAGE_LOCK = 0, GENRES_LOCK = 0 WHERE SERIES_ID = ?",
    )
    .bind("Stale EPUB Series")
    .bind("Stale EPUB Series")
    .bind("")
    .bind("")
    .bind("series-1")
    .execute(&pool)
    .await
    .expect("series metadata should be reset before EPUB provider refresh test");
    sqlx::query("DELETE FROM SERIES_METADATA_GENRE WHERE SERIES_ID = ?")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("existing series genres should be cleared before EPUB provider refresh test");
    pool.close().await;

    let task_write_pool = connect_task_write_pool(&ctx.paths().main_db)
        .await
        .expect("test private write pool should open");
    let task_read_pool = connect_task_pool(&ctx.paths().main_db, default_read_max_connections())
        .await
        .expect("test private read pool should open");
    let runtime = TaskRuntimeContext {
        owns_search_index: false,
        task_write_pool,
        task_read_pool,
        ..runtime_task_context(ctx.paths()).await
    };
    let scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    scheduler
        .enqueue(
            TaskQueueRecord::new(
                "RefreshSeriesMetadata_series-1",
                1_000,
                Some("series-1".to_string()),
            )
            .with_simple_type("RefreshSeriesMetadata"),
        )
        .await;
    scheduler
        .process_available(&runtime)
        .await
        .expect("EPUB series metadata refresh task should process successfully");

    let verify_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for EPUB series metadata verification");
    let metadata = sqlx::query(
        "SELECT TITLE, TITLE_SORT, READING_DIRECTION, PUBLISHER, LANGUAGE FROM SERIES_METADATA WHERE SERIES_ID = ? LIMIT 1",
    )
    .bind("series-1")
    .fetch_one(&verify_pool)
    .await
    .expect("series metadata row should be queryable after EPUB provider refresh");
    let genres = sqlx::query(
        "SELECT GENRE FROM SERIES_METADATA_GENRE WHERE SERIES_ID = ? ORDER BY GENRE COLLATE NOCASE ASC",
    )
    .bind("series-1")
    .fetch_all(&verify_pool)
    .await
    .expect("series genres should be queryable after EPUB provider refresh");
    verify_pool.close().await;

    assert_eq!(metadata.get::<String, _>("TITLE"), "EPUB Provider Series");
    assert_eq!(
        metadata.get::<String, _>("TITLE_SORT"),
        "EPUB Provider Series"
    );
    assert_eq!(
        metadata.get::<Option<String>, _>("READING_DIRECTION"),
        Some("RIGHT_TO_LEFT".to_string())
    );
    assert_eq!(
        metadata.get::<String, _>("PUBLISHER"),
        "EPUB Provider House"
    );
    assert_eq!(metadata.get::<String, _>("LANGUAGE"), "en-US");
    assert_eq!(
        genres
            .into_iter()
            .map(|row| row.get::<String, _>("GENRE"))
            .collect::<Vec<_>>(),
        vec!["Adventure".to_string(), "Mystery".to_string()],
    );
}

#[tokio::test]
async fn runtime_refresh_series_metadata_ignores_non_iso_language_tags_from_book_providers() {
    let ctx = TestFixture::new("runtime-refresh-series-metadata-ignores-non-iso-language").await;

    write_router_epub_with_package_document(
        ctx.paths(),
        "books/book-1.epub",
        r##"<?xml version="1.0" encoding="UTF-8"?>
        <package version="3.0" xmlns="http://www.idpf.org/2007/opf" xmlns:dc="http://purl.org/dc/elements/1.1/" unique-identifier="bookid">
          <metadata>
            <dc:identifier id="bookid">book-1</dc:identifier>
            <dc:title>EPUB Invalid Language Fixture</dc:title>
            <dc:publisher>EPUB Invalid Language House</dc:publisher>
            <dc:language>zz-YY</dc:language>
            <meta property="belongs-to-collection">EPUB Invalid Language Series</meta>
          </metadata>
          <manifest>
            <item id="main" href="chapter.xhtml" media-type="application/xhtml+xml"/>
          </manifest>
          <spine>
            <itemref idref="main"/>
          </spine>
        </package>"##,
    );

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for invalid EPUB language series metadata fixture setup");
    sqlx::query(
        "UPDATE LIBRARY SET IMPORT_COMICINFO_SERIES = 0, IMPORT_COMICINFO_COLLECTION = 0, IMPORT_EPUB_SERIES = 1 WHERE ID = ?",
    )
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("library flags should isolate EPUB provider for invalid language test");
    sqlx::query(
        "UPDATE SERIES_METADATA SET TITLE = ?, TITLE_LOCK = 0, TITLE_SORT = ?, TITLE_SORT_LOCK = 0, PUBLISHER = ?, PUBLISHER_LOCK = 0, LANGUAGE = ?, LANGUAGE_LOCK = 0 WHERE SERIES_ID = ?",
    )
    .bind("Baseline Invalid Language Title")
    .bind("Baseline Invalid Language Title")
    .bind("")
    .bind("en-US")
    .bind("series-1")
    .execute(&pool)
    .await
    .expect("series metadata should be reset before invalid language refresh test");
    pool.close().await;

    let task_write_pool = connect_task_write_pool(&ctx.paths().main_db)
        .await
        .expect("test private write pool should open");
    let task_read_pool = connect_task_pool(&ctx.paths().main_db, default_read_max_connections())
        .await
        .expect("test private read pool should open");
    let runtime = TaskRuntimeContext {
        owns_search_index: false,
        task_write_pool,
        task_read_pool,
        ..runtime_task_context(ctx.paths()).await
    };
    let scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    scheduler
        .enqueue(
            TaskQueueRecord::new(
                "RefreshSeriesMetadata_series-1",
                1_000,
                Some("series-1".to_string()),
            )
            .with_simple_type("RefreshSeriesMetadata"),
        )
        .await;
    scheduler
        .process_available(&runtime)
        .await
        .expect("invalid language series metadata refresh task should process successfully");

    let verify_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for invalid language series metadata verification");
    let metadata = sqlx::query(
        "SELECT TITLE, TITLE_SORT, PUBLISHER, LANGUAGE FROM SERIES_METADATA WHERE SERIES_ID = ? LIMIT 1",
    )
    .bind("series-1")
    .fetch_one(&verify_pool)
    .await
    .expect("series metadata row should be queryable after invalid language refresh");
    verify_pool.close().await;

    assert_eq!(
        metadata.get::<String, _>("TITLE"),
        "EPUB Invalid Language Series"
    );
    assert_eq!(
        metadata.get::<String, _>("TITLE_SORT"),
        "EPUB Invalid Language Series"
    );
    assert_eq!(
        metadata.get::<String, _>("PUBLISHER"),
        "EPUB Invalid Language House"
    );
    assert_eq!(
        metadata.get::<String, _>("LANGUAGE"),
        "en-US",
        "non-ISO language tags should be ignored to match Kotlin BCP47TagValidator semantics",
    );
}

#[tokio::test]
async fn runtime_refresh_series_metadata_ignores_generic_series_xml_sidecar_without_matching_provider()
 {
    let ctx = TestFixture::new("runtime-refresh-series-metadata-ignores-generic-series-xml").await;

    let series_sidecar_path = ctx.paths().config_dir.join("series/series-1.xml");
    if let Some(parent) = series_sidecar_path.parent() {
        std::fs::create_dir_all(parent).expect("series sidecar parent directory should be created");
    }
    std::fs::write(
        &series_sidecar_path,
        br#"<ComicInfo><Title>Unexpected Series Sidecar Title</Title><Summary>Unexpected Series Sidecar Summary</Summary></ComicInfo>"#,
    )
    .expect("series sidecar fixture should be written");

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for generic series sidecar fixture setup");
    sqlx::query(
        "INSERT INTO SIDECAR (URL, PARENT_URL, LAST_MODIFIED_TIME, LIBRARY_ID) VALUES (?, ?, ?, ?)",
    )
    .bind("series/series-1.xml")
    .bind("series/series-1")
    .bind(1_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("generic series sidecar row should be inserted");
    sqlx::query(
        "UPDATE LIBRARY SET IMPORT_COMICINFO_SERIES = 0, IMPORT_COMICINFO_COLLECTION = 0, IMPORT_EPUB_SERIES = 0, IMPORT_MYLAR_SERIES = 0 WHERE ID = ?",
    )
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("library flags should disable matching series metadata providers");
    sqlx::query(
        "UPDATE SERIES_METADATA SET TITLE = ?, TITLE_LOCK = 0, TITLE_SORT = ?, TITLE_SORT_LOCK = 0, SUMMARY = ?, SUMMARY_LOCK = 0 WHERE SERIES_ID = ?",
    )
    .bind("Series Baseline Title")
    .bind("Series Baseline Title")
    .bind("Series Baseline Summary")
    .bind("series-1")
    .execute(&pool)
    .await
    .expect("series metadata should be reset before generic sidecar refresh test");
    pool.close().await;

    let task_write_pool = connect_task_write_pool(&ctx.paths().main_db)
        .await
        .expect("test private write pool should open");
    let task_read_pool = connect_task_pool(&ctx.paths().main_db, default_read_max_connections())
        .await
        .expect("test private read pool should open");
    let runtime = TaskRuntimeContext {
        owns_search_index: false,
        task_write_pool,
        task_read_pool,
        ..runtime_task_context(ctx.paths()).await
    };
    let scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    scheduler
        .enqueue(
            TaskQueueRecord::new(
                "RefreshSeriesMetadata_series-1",
                1_000,
                Some("series-1".to_string()),
            )
            .with_simple_type("RefreshSeriesMetadata"),
        )
        .await;
    scheduler
        .process_available(&runtime)
        .await
        .expect("generic series sidecar refresh task should process successfully");

    let verify_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for generic series sidecar verification");
    let metadata = sqlx::query(
        "SELECT TITLE, TITLE_SORT, SUMMARY FROM SERIES_METADATA WHERE SERIES_ID = ? LIMIT 1",
    )
    .bind("series-1")
    .fetch_one(&verify_pool)
    .await
    .expect("series metadata row should be queryable after generic sidecar refresh");
    verify_pool.close().await;

    assert_eq!(metadata.get::<String, _>("TITLE"), "Series Baseline Title");
    assert_eq!(
        metadata.get::<String, _>("TITLE_SORT"),
        "Series Baseline Title"
    );
    assert_eq!(
        metadata.get::<String, _>("SUMMARY"),
        "Series Baseline Summary"
    );
}

#[tokio::test]
async fn runtime_refresh_series_metadata_applies_comicinfo_from_book_provider_and_collection_side_effects()
 {
    let ctx = TestFixture::new("runtime-refresh-series-metadata-applies-comicinfo-provider").await;

    write_router_epub_with_package_document_and_entries(
        ctx.paths(),
        "books/book-1.epub",
        r##"<?xml version="1.0" encoding="UTF-8"?>
        <package version="3.0" xmlns="http://www.idpf.org/2007/opf" xmlns:dc="http://purl.org/dc/elements/1.1/" unique-identifier="bookid">
          <metadata>
            <dc:identifier id="bookid">book-1</dc:identifier>
            <dc:title>ComicInfo Series Metadata Fixture</dc:title>
            <dc:language>en</dc:language>
          </metadata>
          <manifest>
            <item id="main" href="chapter.xhtml" media-type="application/xhtml+xml"/>
          </manifest>
          <spine>
            <itemref idref="main"/>
          </spine>
        </package>"##,
        &[(
            "ComicInfo.xml",
            br#"<ComicInfo><Series>ComicInfo Series</Series><Volume>2</Volume><Count>9</Count><Publisher>ComicInfo House</Publisher><LanguageISO>EN-us</LanguageISO><Genre>Drama, Action, Drama</Genre><Manga>YesAndRightToLeft</Manga><AgeRating>MA 15+</AgeRating><SeriesGroup>Collection 1, New Refresh Collection</SeriesGroup></ComicInfo>"#,
        )],
    );

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for ComicInfo series metadata fixture setup");
    sqlx::query(
        "UPDATE LIBRARY SET IMPORT_COMICINFO_SERIES = 1, IMPORT_COMICINFO_COLLECTION = 1, IMPORT_COMICINFO_SERIES_APPEND_VOLUME = 1, IMPORT_EPUB_SERIES = 0 WHERE ID = ?",
    )
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("library flags should isolate ComicInfo series metadata provider");
    sqlx::query(
        "UPDATE SERIES_METADATA SET TITLE = ?, TITLE_LOCK = 0, TITLE_SORT = ?, TITLE_SORT_LOCK = 0, READING_DIRECTION = NULL, READING_DIRECTION_LOCK = 0, PUBLISHER = ?, PUBLISHER_LOCK = 0, AGE_RATING = NULL, AGE_RATING_LOCK = 0, LANGUAGE = ?, LANGUAGE_LOCK = 0, TOTAL_BOOK_COUNT = NULL, TOTAL_BOOK_COUNT_LOCK = 0, GENRES_LOCK = 0 WHERE SERIES_ID = ?",
    )
    .bind("Stale ComicInfo Series")
    .bind("Stale ComicInfo Series")
    .bind("")
    .bind("")
    .bind("series-1")
    .execute(&pool)
    .await
    .expect("series metadata should be reset before ComicInfo provider refresh test");
    sqlx::query("DELETE FROM SERIES_METADATA_GENRE WHERE SERIES_ID = ?")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("existing series genres should be cleared before ComicInfo provider refresh test");
    sqlx::query("INSERT INTO SERIES_METADATA_GENRE (SERIES_ID, GENRE) VALUES (?, ?)")
        .bind("series-1")
        .bind("Action")
        .execute(&pool)
        .await
        .expect("baseline Action genre should be inserted before ComicInfo provider refresh test");
    sqlx::query("INSERT INTO SERIES_METADATA_GENRE (SERIES_ID, GENRE) VALUES (?, ?)")
        .bind("series-1")
        .bind("Drama")
        .execute(&pool)
        .await
        .expect("baseline Drama genre should be inserted before ComicInfo provider refresh test");
    sqlx::query("DELETE FROM COLLECTION_SERIES WHERE COLLECTION_ID = ? AND SERIES_ID <> ?")
        .bind("collection-1")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("existing collection memberships should be normalized before ComicInfo provider refresh test");
    pool.close().await;

    let task_write_pool = connect_task_write_pool(&ctx.paths().main_db)
        .await
        .expect("test private write pool should open");
    let task_read_pool = connect_task_pool(&ctx.paths().main_db, default_read_max_connections())
        .await
        .expect("test private read pool should open");
    let runtime = TaskRuntimeContext {
        owns_search_index: false,
        task_write_pool,
        task_read_pool,
        ..runtime_task_context(ctx.paths()).await
    };
    let scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    scheduler
        .enqueue(
            TaskQueueRecord::new(
                "RefreshSeriesMetadata_series-1",
                1_000,
                Some("series-1".to_string()),
            )
            .with_simple_type("RefreshSeriesMetadata"),
        )
        .await;
    scheduler
        .process_available(&runtime)
        .await
        .expect("ComicInfo series metadata refresh task should process successfully");

    let verify_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for ComicInfo series metadata verification");
    let metadata = sqlx::query(
        "SELECT TITLE, TITLE_SORT, READING_DIRECTION, PUBLISHER, AGE_RATING, LANGUAGE, TOTAL_BOOK_COUNT FROM SERIES_METADATA WHERE SERIES_ID = ? LIMIT 1",
    )
    .bind("series-1")
    .fetch_one(&verify_pool)
    .await
    .expect("series metadata row should be queryable after ComicInfo provider refresh");
    let genres = sqlx::query(
        "SELECT GENRE FROM SERIES_METADATA_GENRE WHERE SERIES_ID = ? ORDER BY ROWID ASC",
    )
    .bind("series-1")
    .fetch_all(&verify_pool)
    .await
    .expect("series genres should be queryable after ComicInfo provider refresh");
    let new_collection = sqlx::query(
        "SELECT ID, SERIES_COUNT FROM COLLECTION WHERE NAME = ? COLLATE NOCASE LIMIT 1",
    )
    .bind("New Refresh Collection")
    .fetch_one(&verify_pool)
    .await
    .expect("new ComicInfo collection should be created");
    let new_collection_members = sqlx::query(
        "SELECT SERIES_ID FROM COLLECTION_SERIES WHERE COLLECTION_ID = ? ORDER BY NUMBER ASC",
    )
    .bind(new_collection.get::<String, _>("ID"))
    .fetch_all(&verify_pool)
    .await
    .expect("new ComicInfo collection membership should be queryable");
    let existing_membership_count = sqlx::query(
        "SELECT COUNT(*) AS COUNT FROM COLLECTION_SERIES WHERE COLLECTION_ID = ? AND SERIES_ID = ?",
    )
    .bind("collection-1")
    .bind("series-1")
    .fetch_one(&verify_pool)
    .await
    .expect("existing collection membership should be queryable")
    .get::<i64, _>("COUNT");
    verify_pool.close().await;

    assert_eq!(metadata.get::<String, _>("TITLE"), "ComicInfo Series (2)");
    assert_eq!(
        metadata.get::<String, _>("TITLE_SORT"),
        "ComicInfo Series (2)"
    );
    assert_eq!(
        metadata.get::<Option<String>, _>("READING_DIRECTION"),
        Some("RIGHT_TO_LEFT".to_string())
    );
    assert_eq!(metadata.get::<String, _>("PUBLISHER"), "ComicInfo House");
    assert_eq!(metadata.get::<Option<i64>, _>("AGE_RATING"), Some(15_i64));
    assert_eq!(metadata.get::<String, _>("LANGUAGE"), "en-US");
    assert_eq!(
        metadata.get::<Option<i64>, _>("TOTAL_BOOK_COUNT"),
        Some(9_i64)
    );
    assert_eq!(
        genres
            .into_iter()
            .map(|row| row.get::<String, _>("GENRE"))
            .collect::<Vec<_>>(),
        vec!["Action".to_string(), "Drama".to_string()],
    );
    assert_eq!(new_collection.get::<i64, _>("SERIES_COUNT"), 1_i64);
    assert_eq!(
        new_collection_members
            .into_iter()
            .map(|row| row.get::<String, _>("SERIES_ID"))
            .collect::<Vec<_>>(),
        vec!["series-1".to_string()],
    );
    assert_eq!(existing_membership_count, 1_i64);
}

#[tokio::test]
async fn runtime_refresh_series_metadata_ignores_deleted_books_from_book_providers() {
    let ctx = TestFixture::new("runtime-refresh-series-metadata-ignores-deleted-books").await;

    write_router_epub_with_package_document(
        ctx.paths(),
        "books/book-1.epub",
        r##"<?xml version="1.0" encoding="UTF-8"?>
        <package version="3.0" xmlns="http://www.idpf.org/2007/opf" xmlns:dc="http://purl.org/dc/elements/1.1/" unique-identifier="bookid">
          <metadata>
            <dc:identifier id="bookid">book-1</dc:identifier>
            <dc:title>Active Book Without Series Patch</dc:title>
          </metadata>
          <manifest>
            <item id="main" href="chapter.xhtml" media-type="application/xhtml+xml"/>
          </manifest>
          <spine>
            <itemref idref="main"/>
          </spine>
        </package>"##,
    );
    write_router_epub_with_package_document_and_entries(
        ctx.paths(),
        "books/book-deleted-series-provider.epub",
        r##"<?xml version="1.0" encoding="UTF-8"?>
        <package version="3.0" xmlns="http://www.idpf.org/2007/opf" xmlns:dc="http://purl.org/dc/elements/1.1/" unique-identifier="bookid">
          <metadata>
            <dc:identifier id="bookid">book-deleted-series-provider</dc:identifier>
            <dc:title>Deleted Provider Fixture</dc:title>
          </metadata>
          <manifest>
            <item id="main" href="chapter.xhtml" media-type="application/xhtml+xml"/>
          </manifest>
          <spine>
            <itemref idref="main"/>
          </spine>
        </package>"##,
        &[(
            "ComicInfo.xml",
            br#"<ComicInfo><Series>Deleted Provider Series</Series><Publisher>Deleted Provider House</Publisher><LanguageISO>en-US</LanguageISO><Count>99</Count><SeriesGroup>Deleted Provider Collection</SeriesGroup></ComicInfo>"#,
        )],
    );

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for deleted series-provider fixture setup");
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-deleted-series-provider")
    .bind(0_i64)
    .bind("book-deleted-series-provider.epub")
    .bind("books/book-deleted-series-provider.epub")
    .bind("series-1")
    .bind(4_096_i64)
    .bind(2_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("deleted provider book row should be inserted");
    sqlx::query("INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("application/epub+zip")
        .bind("READY")
        .bind("book-deleted-series-provider")
        .bind(12_i64)
        .execute(&pool)
        .await
        .expect("deleted provider media row should be inserted");
    sqlx::query("UPDATE BOOK SET DELETED_DATE = CURRENT_TIMESTAMP WHERE ID = ?")
        .bind("book-deleted-series-provider")
        .execute(&pool)
        .await
        .expect("deleted provider book should be marked deleted");
    sqlx::query(
        "UPDATE LIBRARY SET IMPORT_COMICINFO_SERIES = 1, IMPORT_COMICINFO_COLLECTION = 1, IMPORT_COMICINFO_SERIES_APPEND_VOLUME = 0, IMPORT_EPUB_SERIES = 0, IMPORT_MYLAR_SERIES = 0 WHERE ID = ?",
    )
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("library flags should isolate ComicInfo series provider for deleted-book test");
    sqlx::query(
        "UPDATE SERIES_METADATA SET TITLE = ?, TITLE_LOCK = 0, TITLE_SORT = ?, TITLE_SORT_LOCK = 0, PUBLISHER = ?, PUBLISHER_LOCK = 0, LANGUAGE = ?, LANGUAGE_LOCK = 0, TOTAL_BOOK_COUNT = ?, TOTAL_BOOK_COUNT_LOCK = 0 WHERE SERIES_ID = ?",
    )
    .bind("Baseline Series Title")
    .bind("Baseline Series Title")
    .bind("Baseline Series Publisher")
    .bind("en-US")
    .bind(5_i64)
    .bind("series-1")
    .execute(&pool)
    .await
    .expect("series metadata should be reset before deleted-book provider refresh test");
    pool.close().await;

    let task_write_pool = connect_task_write_pool(&ctx.paths().main_db)
        .await
        .expect("test private write pool should open");
    let task_read_pool = connect_task_pool(&ctx.paths().main_db, default_read_max_connections())
        .await
        .expect("test private read pool should open");
    let runtime = TaskRuntimeContext {
        owns_search_index: false,
        task_write_pool,
        task_read_pool,
        ..runtime_task_context(ctx.paths()).await
    };
    let scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    scheduler
        .enqueue(
            TaskQueueRecord::new(
                "RefreshSeriesMetadata_series-1",
                1_000,
                Some("series-1".to_string()),
            )
            .with_simple_type("RefreshSeriesMetadata"),
        )
        .await;
    scheduler
        .process_available(&runtime)
        .await
        .expect("deleted-book series metadata refresh task should process successfully");

    let verify_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for deleted-book provider verification");
    let metadata = sqlx::query(
        "SELECT TITLE, TITLE_SORT, PUBLISHER, LANGUAGE, TOTAL_BOOK_COUNT FROM SERIES_METADATA WHERE SERIES_ID = ? LIMIT 1",
    )
    .bind("series-1")
    .fetch_one(&verify_pool)
    .await
    .expect("series metadata row should be queryable after deleted-book provider refresh");
    let deleted_collection =
        sqlx::query("SELECT COUNT(*) AS COUNT FROM COLLECTION WHERE NAME = ? COLLATE NOCASE")
            .bind("Deleted Provider Collection")
            .fetch_one(&verify_pool)
            .await
            .expect("deleted provider collection count should be queryable")
            .get::<i64, _>("COUNT");
    verify_pool.close().await;

    assert_eq!(metadata.get::<String, _>("TITLE"), "Baseline Series Title");
    assert_eq!(
        metadata.get::<String, _>("TITLE_SORT"),
        "Baseline Series Title"
    );
    assert_eq!(
        metadata.get::<String, _>("PUBLISHER"),
        "Baseline Series Publisher"
    );
    assert_eq!(metadata.get::<String, _>("LANGUAGE"), "en-US");
    assert_eq!(
        metadata.get::<Option<i64>, _>("TOTAL_BOOK_COUNT"),
        Some(5_i64)
    );
    assert_eq!(
        deleted_collection, 0,
        "deleted books must not create collection side effects during series metadata refresh",
    );
}

#[tokio::test]
async fn runtime_refresh_series_metadata_applies_mylar_series_provider() {
    let ctx = TestFixture::new("runtime-refresh-series-metadata-applies-mylar-provider").await;

    let series_json_path = ctx.paths().config_dir.join("series/series-1/series.json");
    if let Some(parent) = series_json_path.parent() {
        std::fs::create_dir_all(parent)
            .expect("mylar series sidecar parent directory should exist");
    }
    std::fs::write(
        &series_json_path,
        r#"{
  "metadata": {
    "type": "comicSeries",
    "publisher": "Mylar House",
    "imprint": null,
    "name": "Mylar Saga",
    "cid": "123",
    "year": 2005,
    "description_text": "Plain summary",
    "description_formatted": "Formatted summary",
    "volume": 2,
    "booktype": "Print",
    "age_rating": "17+",
    "comic_image": "cover.jpg",
    "total_issues": 13,
    "publication_run": "2005-present",
    "status": "Continuing"
  }
}"#,
    )
    .expect("mylar series sidecar fixture should be written");

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for Mylar series metadata fixture setup");
    sqlx::query(
        "UPDATE LIBRARY SET IMPORT_COMICINFO_SERIES = 0, IMPORT_COMICINFO_COLLECTION = 0, IMPORT_EPUB_SERIES = 0, IMPORT_MYLAR_SERIES = 1 WHERE ID = ?",
    )
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("library flags should isolate Mylar series metadata provider");
    sqlx::query(
        "UPDATE SERIES_METADATA SET TITLE = ?, TITLE_LOCK = 0, TITLE_SORT = ?, TITLE_SORT_LOCK = 0, STATUS = ?, STATUS_LOCK = 0, SUMMARY = ?, SUMMARY_LOCK = 0, PUBLISHER = ?, PUBLISHER_LOCK = 0, AGE_RATING = NULL, AGE_RATING_LOCK = 0, TOTAL_BOOK_COUNT = NULL, TOTAL_BOOK_COUNT_LOCK = 0 WHERE SERIES_ID = ?",
    )
    .bind("Stale Mylar Title")
    .bind("Stale Mylar Title")
    .bind("ENDED")
    .bind("Stale Mylar Summary")
    .bind("")
    .bind("series-1")
    .execute(&pool)
    .await
    .expect("series metadata should be reset before Mylar refresh test");
    pool.close().await;

    let task_write_pool = connect_task_write_pool(&ctx.paths().main_db)
        .await
        .expect("test private write pool should open");
    let task_read_pool = connect_task_pool(&ctx.paths().main_db, default_read_max_connections())
        .await
        .expect("test private read pool should open");
    let runtime = TaskRuntimeContext {
        owns_search_index: false,
        task_write_pool,
        task_read_pool,
        ..runtime_task_context(ctx.paths()).await
    };
    let scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    scheduler
        .enqueue(
            TaskQueueRecord::new(
                "RefreshSeriesMetadata_series-1",
                1_000,
                Some("series-1".to_string()),
            )
            .with_simple_type("RefreshSeriesMetadata"),
        )
        .await;
    scheduler
        .process_available(&runtime)
        .await
        .expect("Mylar series metadata refresh task should process successfully");

    let verify_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for Mylar series metadata verification");
    let metadata = sqlx::query(
        "SELECT TITLE, TITLE_SORT, STATUS, SUMMARY, PUBLISHER, AGE_RATING, TOTAL_BOOK_COUNT FROM SERIES_METADATA WHERE SERIES_ID = ? LIMIT 1",
    )
    .bind("series-1")
    .fetch_one(&verify_pool)
    .await
    .expect("series metadata row should be queryable after Mylar refresh");
    verify_pool.close().await;

    assert_eq!(metadata.get::<String, _>("TITLE"), "Mylar Saga (2005)");
    assert_eq!(metadata.get::<String, _>("TITLE_SORT"), "Mylar Saga (2005)");
    assert_eq!(metadata.get::<String, _>("STATUS"), "ONGOING");
    assert_eq!(metadata.get::<String, _>("SUMMARY"), "Formatted summary");
    assert_eq!(metadata.get::<String, _>("PUBLISHER"), "Mylar House");
    assert_eq!(metadata.get::<Option<i64>, _>("AGE_RATING"), Some(17_i64));
    assert_eq!(
        metadata.get::<Option<i64>, _>("TOTAL_BOOK_COUNT"),
        Some(13_i64)
    );
}

#[tokio::test]
async fn runtime_refresh_series_metadata_ignores_mylar_series_json_when_library_gate_is_disabled() {
    let ctx = TestFixture::new("runtime-refresh-series-metadata-ignores-mylar-when-disabled").await;

    let series_json_path = ctx.paths().config_dir.join("series/series-1/series.json");
    if let Some(parent) = series_json_path.parent() {
        std::fs::create_dir_all(parent)
            .expect("mylar series sidecar parent directory should exist");
    }
    std::fs::write(
        &series_json_path,
        r#"{
  "metadata": {
    "type": "comicSeries",
    "publisher": "Blocked Mylar House",
    "imprint": null,
    "name": "Blocked Mylar Saga",
    "cid": "456",
    "year": 2010,
    "description_text": "Blocked summary",
    "description_formatted": "Blocked formatted summary",
    "volume": 2,
    "booktype": "Print",
    "age_rating": "Adult",
    "comic_image": "cover.jpg",
    "total_issues": 22,
    "publication_run": "2010-present",
    "status": "Ended"
  }
}"#,
    )
    .expect("disabled Mylar series sidecar fixture should be written");

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for disabled Mylar fixture setup");
    sqlx::query(
        "UPDATE LIBRARY SET IMPORT_COMICINFO_SERIES = 0, IMPORT_COMICINFO_COLLECTION = 0, IMPORT_EPUB_SERIES = 0, IMPORT_MYLAR_SERIES = 0 WHERE ID = ?",
    )
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("library flags should disable Mylar series metadata provider");
    sqlx::query(
        "UPDATE SERIES_METADATA SET TITLE = ?, TITLE_LOCK = 0, TITLE_SORT = ?, TITLE_SORT_LOCK = 0, STATUS = ?, STATUS_LOCK = 0, SUMMARY = ?, SUMMARY_LOCK = 0, PUBLISHER = ?, PUBLISHER_LOCK = 0, AGE_RATING = ?, AGE_RATING_LOCK = 0, TOTAL_BOOK_COUNT = ?, TOTAL_BOOK_COUNT_LOCK = 0 WHERE SERIES_ID = ?",
    )
    .bind("Baseline Mylar Title")
    .bind("Baseline Mylar Title")
    .bind("ONGOING")
    .bind("Baseline Mylar Summary")
    .bind("Baseline Mylar Publisher")
    .bind(9_i64)
    .bind(5_i64)
    .bind("series-1")
    .execute(&pool)
    .await
    .expect("series metadata should be reset before disabled Mylar refresh test");
    pool.close().await;

    let task_write_pool = connect_task_write_pool(&ctx.paths().main_db)
        .await
        .expect("test private write pool should open");
    let task_read_pool = connect_task_pool(&ctx.paths().main_db, default_read_max_connections())
        .await
        .expect("test private read pool should open");
    let runtime = TaskRuntimeContext {
        owns_search_index: false,
        task_write_pool,
        task_read_pool,
        ..runtime_task_context(ctx.paths()).await
    };
    let scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    scheduler
        .enqueue(
            TaskQueueRecord::new(
                "RefreshSeriesMetadata_series-1",
                1_000,
                Some("series-1".to_string()),
            )
            .with_simple_type("RefreshSeriesMetadata"),
        )
        .await;
    scheduler
        .process_available(&runtime)
        .await
        .expect("disabled Mylar series metadata refresh task should process successfully");

    let verify_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for disabled Mylar verification");
    let metadata = sqlx::query(
        "SELECT TITLE, TITLE_SORT, STATUS, SUMMARY, PUBLISHER, AGE_RATING, TOTAL_BOOK_COUNT FROM SERIES_METADATA WHERE SERIES_ID = ? LIMIT 1",
    )
    .bind("series-1")
    .fetch_one(&verify_pool)
    .await
    .expect("series metadata row should be queryable after disabled Mylar refresh");
    verify_pool.close().await;

    assert_eq!(metadata.get::<String, _>("TITLE"), "Baseline Mylar Title");
    assert_eq!(
        metadata.get::<String, _>("TITLE_SORT"),
        "Baseline Mylar Title"
    );
    assert_eq!(metadata.get::<String, _>("STATUS"), "ONGOING");
    assert_eq!(
        metadata.get::<String, _>("SUMMARY"),
        "Baseline Mylar Summary"
    );
    assert_eq!(
        metadata.get::<String, _>("PUBLISHER"),
        "Baseline Mylar Publisher"
    );
    assert_eq!(metadata.get::<Option<i64>, _>("AGE_RATING"), Some(9_i64));
    assert_eq!(
        metadata.get::<Option<i64>, _>("TOTAL_BOOK_COUNT"),
        Some(5_i64)
    );
}
