use super::*;

#[tokio::test]
async fn runtime_empty_trash_resorts_remaining_books_in_affected_series() {
    let paths = new_router_fixture("runtime-empty-trash-resorts-affected-series").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for empty-trash sort fixture setup");
    for (book_id, name, url, file_size, number) in [
        (
            "book-2",
            "book-2.epub",
            "books/book-2.epub",
            2_048_i64,
            2_i64,
        ),
        (
            "book-3",
            "book-3.epub",
            "books/book-3.epub",
            3_072_i64,
            3_i64,
        ),
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
        .expect("book row should be inserted for empty-trash sort fixture");

        sqlx::query(
            "INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)",
        )
        .bind("application/epub+zip")
        .bind("READY")
        .bind(book_id)
        .bind(10_i64)
        .execute(&pool)
        .await
        .expect("media row should be inserted for empty-trash sort fixture");

        sqlx::query(
            "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, BOOK_ID) VALUES (?, ?, ?, ?)",
        )
        .bind(number.to_string())
        .bind(number as f64)
        .bind(format!("Book {}", number))
        .bind(book_id)
        .execute(&pool)
        .await
        .expect("book metadata row should be inserted for empty-trash sort fixture");
    }

    sqlx::query("UPDATE BOOK SET DELETED_DATE = CURRENT_TIMESTAMP WHERE ID = ?")
        .bind("book-2")
        .execute(&pool)
        .await
        .expect("trashed middle book should be marked deleted");
    pool.close().await;

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "EMPTY_TRASH:library-1",
        1_000,
        Some("library-1".to_string()),
    ));
    scheduler
        .process_available(&runtime)
        .expect("empty-trash cleanup should process successfully");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for empty-trash sort verification");
    let deleted_rows = sqlx::query("SELECT COUNT(*) AS COUNT FROM BOOK WHERE ID = ?")
        .bind("book-2")
        .fetch_one(&verify_pool)
        .await
        .expect("deleted middle book count should be queryable")
        .get::<i64, _>("COUNT");
    let remaining = sqlx::query(
        "SELECT b.ID AS ID, b.NUMBER AS BOOK_NUMBER, bm.NUMBER AS METADATA_NUMBER, \
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
    .expect("remaining books after empty-trash should be queryable");
    verify_pool.close().await;

    assert_eq!(
        deleted_rows, 0,
        "empty-trash must hard-delete trashed books"
    );
    assert_eq!(
        remaining.len(),
        2,
        "series should keep two non-deleted books"
    );
    assert_eq!(remaining[0].get::<String, _>("ID"), "book-1");
    assert_eq!(remaining[0].get::<i64, _>("BOOK_NUMBER"), 1);
    assert_eq!(remaining[0].get::<String, _>("METADATA_NUMBER"), "1");
    assert_eq!(remaining[0].get::<f64, _>("METADATA_NUMBER_SORT"), 1.0_f64);
    assert_eq!(remaining[1].get::<String, _>("ID"), "book-3");
    assert_eq!(remaining[1].get::<i64, _>("BOOK_NUMBER"), 2);
    assert_eq!(remaining[1].get::<String, _>("METADATA_NUMBER"), "2");
    assert_eq!(remaining[1].get::<f64, _>("METADATA_NUMBER_SORT"), 2.0_f64);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_empty_trash_uses_kotlin_like_natural_name_sort_for_remaining_series_books() {
    let paths = new_router_fixture("runtime-empty-trash-natural-name-sort").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
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

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "EMPTY_TRASH:library-1",
        1_000,
        Some("library-1".to_string()),
    ));
    scheduler
        .process_available(&runtime)
        .expect("empty-trash natural-sort cleanup should process successfully");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
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

    cleanup_router_fixture(paths);
}
