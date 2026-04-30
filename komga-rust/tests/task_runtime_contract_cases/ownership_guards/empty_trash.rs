use super::*;

async fn run_empty_trash(paths: &RuntimeDbPaths) {
    let runtime = runtime_task_context(paths).await;
    let scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    scheduler
        .enqueue(
            TaskQueueRecord::new("EmptyTrash_library-1", 1_000, Some("library-1".to_string()))
                .with_simple_type("EmptyTrash"),
        )
        .await;
    scheduler
        .process_available(&runtime)
        .await
        .expect("empty-trash cleanup should process successfully");
}

#[tokio::test]
async fn runtime_empty_trash_uses_kotlin_like_natural_name_sort_for_remaining_series_books() {
    let ctx = TestFixture::new("runtime-empty-trash-natural-name-sort").await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for empty-trash natural-sort fixture setup");

    sqlx::query("UPDATE BOOK SET NAME = ?, URL = ? WHERE ID = ?")
        .bind("Vol 10.epub")
        .bind("books/Vol 10.epub")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book-1 name should be updated for natural-sort fixture");
    sqlx::query("UPDATE BOOK_METADATA SET TITLE = ? WHERE BOOK_ID = ?")
        .bind("Vol 10")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book-1 metadata title should be updated for natural-sort fixture");

    for (book_id, name, url, file_size, number) in [
        ("book-2", "Vol 1.epub", "books/Vol 1.epub", 2_048_i64, 2_i64),
        ("book-3", "Vol 2.epub", "books/Vol 2.epub", 3_072_i64, 3_i64),
    ] {
        sqlx::query(
            "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(book_id)
        .bind(0_i64)
        .bind(name)
        .bind(url)
        .bind("series-1")
        .bind(file_size)
        .bind(number)
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("book row should be inserted for natural-sort fixture");

        sqlx::query(
            "INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)",
        )
        .bind("application/epub+zip")
        .bind("READY")
        .bind(book_id)
        .bind(10_i64)
        .execute(&pool)
        .await
        .expect("media row should be inserted for natural-sort fixture");

        sqlx::query(
            "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, BOOK_ID) VALUES (?, ?, ?, ?)",
        )
        .bind(number.to_string())
        .bind(number as f64)
        .bind(name.replace(".epub", ""))
        .bind(book_id)
        .execute(&pool)
        .await
        .expect("book metadata row should be inserted for natural-sort fixture");
    }

    sqlx::query("UPDATE BOOK SET DELETED_DATE = CURRENT_TIMESTAMP WHERE ID = ?")
        .bind("book-2")
        .execute(&pool)
        .await
        .expect("trashed book should be marked deleted for natural-sort fixture");
    pool.close().await;

    run_empty_trash(ctx.paths()).await;

    let verify_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for empty-trash natural-sort verification");
    let remaining = sqlx::query(
        "SELECT b.ID AS ID, b.NAME AS NAME, b.NUMBER AS BOOK_NUMBER, bm.NUMBER AS METADATA_NUMBER, \
         bm.NUMBER_SORT AS METADATA_NUMBER_SORT \
         FROM BOOK b \
         JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID \
         WHERE b.SERIES_ID = ? \
           AND b.DELETED_DATE IS NULL \
         ORDER BY b.NUMBER ASC",
    )
    .bind("series-1")
    .fetch_all(&verify_pool)
    .await
    .expect("remaining books after natural-sort empty-trash should be queryable");
    verify_pool.close().await;

    assert_eq!(
        remaining.len(),
        2,
        "series should keep two non-deleted books"
    );
    assert_eq!(remaining[0].get::<String, _>("ID"), "book-3");
    assert_eq!(remaining[0].get::<String, _>("NAME"), "Vol 2.epub");
    assert_eq!(remaining[0].get::<i64, _>("BOOK_NUMBER"), 1);
    assert_eq!(remaining[0].get::<String, _>("METADATA_NUMBER"), "1");
    assert_eq!(remaining[0].get::<f64, _>("METADATA_NUMBER_SORT"), 1.0_f64);
    assert_eq!(remaining[1].get::<String, _>("ID"), "book-1");
    assert_eq!(remaining[1].get::<String, _>("NAME"), "Vol 10.epub");
    assert_eq!(remaining[1].get::<i64, _>("BOOK_NUMBER"), 2);
    assert_eq!(remaining[1].get::<String, _>("METADATA_NUMBER"), "2");
    assert_eq!(remaining[1].get::<f64, _>("METADATA_NUMBER_SORT"), 2.0_f64);
}

#[tokio::test]
async fn runtime_empty_trash_deletes_series_level_dependents_before_removing_empty_series() {
    let ctx = TestFixture::new("runtime-empty-trash-deletes-series-dependents").await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for empty-trash series dependency fixture setup");
    sqlx::query("INSERT INTO BOOK_METADATA_AGGREGATION_TAG (SERIES_ID, TAG) VALUES (?, ?)")
        .bind("series-1")
        .bind("aggregated-tag")
        .execute(&pool)
        .await
        .expect("aggregation tag row should be inserted for empty-trash series dependency fixture");
    sqlx::query(
        "INSERT INTO READ_PROGRESS_SERIES (SERIES_ID, USER_ID, READ_COUNT, IN_PROGRESS_COUNT, MOST_RECENT_READ_DATE, LAST_MODIFIED_DATE) \
         VALUES (?, ?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
    )
    .bind("series-1")
    .bind("admin-user")
    .bind(1_i64)
    .bind(0_i64)
    .execute(&pool)
    .await
    .expect("series read progress row should be inserted for empty-trash series dependency fixture");
    sqlx::query("UPDATE SERIES SET DELETED_DATE = CURRENT_TIMESTAMP WHERE ID = ?")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series should be soft-deleted before empty-trash series dependency cleanup");
    sqlx::query("UPDATE BOOK SET DELETED_DATE = CURRENT_TIMESTAMP WHERE ID = ?")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("seeded book should be soft-deleted before empty-trash");
    pool.close().await;

    run_empty_trash(ctx.paths()).await;

    let verify_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for empty-trash series dependency verification");
    let series_rows = sqlx::query("SELECT COUNT(*) AS COUNT FROM SERIES WHERE ID = ?")
        .bind("series-1")
        .fetch_one(&verify_pool)
        .await
        .expect("series row count should be queryable after empty-trash")
        .get::<i64, _>("COUNT");
    verify_pool.close().await;

    assert_eq!(
        series_rows, 0,
        "empty-trash must hard-delete deleted series even when read progress and aggregation rows exist"
    );
}

#[tokio::test]
async fn runtime_empty_trash_keeps_active_series_even_when_its_last_trashed_book_is_removed() {
    let ctx = TestFixture::new("runtime-empty-trash-keeps-active-empty-series").await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for active-series empty-trash fixture setup");
    sqlx::query("UPDATE SERIES SET BOOK_COUNT = ?, DELETED_DATE = NULL WHERE ID = ?")
        .bind(1_i64)
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series should stay active before removing its last trashed book");
    sqlx::query("UPDATE BOOK SET DELETED_DATE = CURRENT_TIMESTAMP WHERE ID = ?")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("seeded last book should be soft-deleted before empty-trash");
    pool.close().await;

    run_empty_trash(ctx.paths()).await;

    let verify_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for active-series empty-trash verification");
    let series = sqlx::query(
        "SELECT DELETED_DATE AS DELETED_DATE, BOOK_COUNT AS BOOK_COUNT FROM SERIES WHERE ID = ? LIMIT 1",
    )
    .bind("series-1")
    .fetch_optional(&verify_pool)
    .await
    .expect("series row should be queryable after active-series empty-trash");
    let book_rows = sqlx::query("SELECT COUNT(*) AS COUNT FROM BOOK WHERE SERIES_ID = ?")
        .bind("series-1")
        .fetch_one(&verify_pool)
        .await
        .expect("book row count should be queryable after active-series empty-trash")
        .get::<i64, _>("COUNT");
    let collection_rows =
        sqlx::query("SELECT COUNT(*) AS COUNT FROM COLLECTION_SERIES WHERE SERIES_ID = ?")
            .bind("series-1")
            .fetch_one(&verify_pool)
            .await
            .expect("collection membership should be queryable after active-series empty-trash")
            .get::<i64, _>("COUNT");
    verify_pool.close().await;

    let series = series.expect("active series must remain after removing its last trashed book");
    assert!(
        series.get::<Option<String>, _>("DELETED_DATE").is_none(),
        "empty-trash must not soft-delete an active series while removing its last trashed book"
    );
    assert_eq!(
        series.get::<i64, _>("BOOK_COUNT"),
        0,
        "empty-trash should refresh the surviving active series book count to zero"
    );
    assert_eq!(
        book_rows, 0,
        "empty-trash must hard-delete the trashed last book"
    );
    assert_eq!(
        collection_rows, 1,
        "empty-trash must keep collection membership for an active empty series like Kotlin"
    );
}

#[tokio::test]
async fn runtime_empty_trash_cleans_up_empty_sets_with_thumbnails_in_kotlin_order() {
    let ctx = TestFixture::new("runtime-empty-trash-cleans-empty-sets-with-thumbnails").await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for empty-set thumbnail cleanup fixture setup");
    sqlx::query("DELETE FROM COLLECTION_SERIES WHERE COLLECTION_ID = ?")
        .bind("collection-1")
        .execute(&pool)
        .await
        .expect("collection members should be removed before cleanup-empty-sets verification");
    sqlx::query("DELETE FROM READLIST_BOOK WHERE READLIST_ID = ?")
        .bind("readlist-1")
        .execute(&pool)
        .await
        .expect("readlist members should be removed before cleanup-empty-sets verification");
    sqlx::query("INSERT OR REPLACE INTO SERVER_SETTINGS (KEY, VALUE) VALUES (?, ?)")
        .bind("DELETE_EMPTY_COLLECTIONS")
        .bind("true")
        .execute(&pool)
        .await
        .expect("delete empty collections setting should be enabled for thumbnail cleanup");
    sqlx::query("INSERT OR REPLACE INTO SERVER_SETTINGS (KEY, VALUE) VALUES (?, ?)")
        .bind("DELETE_EMPTY_READLISTS")
        .bind("true")
        .execute(&pool)
        .await
        .expect("delete empty readlists setting should be enabled for thumbnail cleanup");
    sqlx::query(
        "INSERT INTO THUMBNAIL_COLLECTION (ID, COLLECTION_ID, THUMBNAIL, TYPE, SELECTED) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("thumb-collection-1")
    .bind("collection-1")
    .bind(vec![0_u8])
    .bind("USER_UPLOADED")
    .bind(true)
    .execute(&pool)
    .await
    .expect("collection thumbnail should be inserted for empty-set cleanup verification");
    sqlx::query(
        "INSERT INTO THUMBNAIL_READLIST (ID, READLIST_ID, THUMBNAIL, TYPE, SELECTED) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("thumb-readlist-1")
    .bind("readlist-1")
    .bind(vec![0_u8])
    .bind("USER_UPLOADED")
    .bind(true)
    .execute(&pool)
    .await
    .expect("readlist thumbnail should be inserted for empty-set cleanup verification");
    pool.close().await;

    run_empty_trash(ctx.paths()).await;

    let verify_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for empty-set thumbnail cleanup verification");
    let collection_rows = sqlx::query("SELECT COUNT(*) AS COUNT FROM COLLECTION WHERE ID = ?")
        .bind("collection-1")
        .fetch_one(&verify_pool)
        .await
        .expect("collection row count should be queryable after thumbnail cleanup")
        .get::<i64, _>("COUNT");
    let readlist_rows = sqlx::query("SELECT COUNT(*) AS COUNT FROM READLIST WHERE ID = ?")
        .bind("readlist-1")
        .fetch_one(&verify_pool)
        .await
        .expect("readlist row count should be queryable after thumbnail cleanup")
        .get::<i64, _>("COUNT");
    verify_pool.close().await;

    assert_eq!(
        collection_rows, 0,
        "empty-trash must delete empty collections when enabled"
    );
    assert_eq!(
        readlist_rows, 0,
        "empty-trash must delete empty readlists when enabled"
    );
}
