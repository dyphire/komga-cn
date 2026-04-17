use komga_infrastructure::sqlite::connect_pool;

use super::RuntimeDbPaths;

pub async fn seed_router_contract_nullable_samples(paths: &RuntimeDbPaths) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract nullable db should open");

    sqlx::query(
        "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("series-2")
    .bind(0_i64)
    .bind("Series 2")
    .bind("series/series-2")
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("nullable series row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, PUBLISHER, LANGUAGE, AGE_RATING, \
           SERIES_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("ONGOING")
    .bind("Series 2")
    .bind("Series 2")
    .bind("NullPub")
    .bind("EN")
    .bind(18_i64)
    .bind("series-2")
    .execute(&pool)
    .await
    .expect("nullable series metadata row should be inserted");

    sqlx::query(
        "UPDATE SERIES \
                 SET BOOK_COUNT = ? \
                 WHERE ID = ?",
    )
    .bind(1_i64)
    .bind("series-2")
    .execute(&pool)
    .await
    .expect("nullable series book count should be updated");

    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, \
           LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-2")
    .bind(0_i64)
    .bind("book-2.epub")
    .bind("books/book-2.epub")
    .bind("series-2")
    .bind(2_048_i64)
    .bind(2_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("nullable book row should be inserted");

    sqlx::query(
        "INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) \
                 VALUES (?, ?, ?, ?)",
    )
    .bind("application/epub+zip")
    .bind("READY")
    .bind("book-2")
    .bind(12_i64)
    .execute(&pool)
    .await
    .expect("nullable media row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("2")
    .bind(2.0_f64)
    .bind("Book 2")
    .bind("2024-01-16")
    .bind("book-2")
    .execute(&pool)
    .await
    .expect("nullable book metadata row should be inserted");

    pool.close().await;
}

pub async fn seed_router_read_progress(paths: &RuntimeDbPaths, completed: bool) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract read-progress db should open");

    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED) \
         VALUES (?, ?, ?, ?)",
    )
    .bind("book-1")
    .bind("admin-user")
    .bind(if completed { 10_i64 } else { 1_i64 })
    .bind(completed)
    .execute(&pool)
    .await
    .expect("router contract read-progress row should be inserted");

    pool.close().await;
}

pub async fn seed_router_custom_series(
    paths: &RuntimeDbPaths,
    series_id: &str,
    name: &str,
    library_id: &str,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract custom series db should open");

    sqlx::query(
        "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(series_id)
    .bind(0_i64)
    .bind(name)
    .bind(format!("series/{series_id}"))
    .bind(library_id)
    .execute(&pool)
    .await
    .expect("custom series row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, PUBLISHER, LANGUAGE, AGE_RATING, \
           SERIES_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("ONGOING")
    .bind(name)
    .bind(name)
    .bind("PubHouse")
    .bind("EN")
    .bind(16_i64)
    .bind(series_id)
    .execute(&pool)
    .await
    .expect("custom series metadata row should be inserted");

    pool.close().await;
}

pub async fn seed_router_authors_scope_variants(paths: &RuntimeDbPaths) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract authors scope db should open");

    sqlx::query(
        "INSERT INTO LIBRARY (ID, NAME, ROOT) \
         VALUES (?, ?, ?)",
    )
    .bind("library-2")
    .bind("Library 2")
    .bind(paths.config_dir.to_string_lossy().to_string())
    .execute(&pool)
    .await
    .expect("secondary library row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("series-2")
    .bind(0_i64)
    .bind("Series 2")
    .bind("series/series-2")
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("secondary same-library series row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, PUBLISHER, LANGUAGE, AGE_RATING, SERIES_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("ONGOING")
    .bind("Series 2")
    .bind("Series 2")
    .bind("PubHouse")
    .bind("EN")
    .bind(16_i64)
    .bind("series-2")
    .execute(&pool)
    .await
    .expect("secondary same-library series metadata row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-2")
    .bind(0_i64)
    .bind("book-2.epub")
    .bind("books/book-2.epub")
    .bind("series-2")
    .bind(2_048_i64)
    .bind(2_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("secondary same-library book row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("2")
    .bind(2.0_f64)
    .bind("Book 2")
    .bind("2024-01-16")
    .bind("book-2")
    .execute(&pool)
    .await
    .expect("secondary same-library book metadata row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA_AUTHOR (BOOK_ID, NAME, ROLE) \
         VALUES (?, ?, ?)",
    )
    .bind("book-2")
    .bind("Alex Side")
    .bind("writer")
    .execute(&pool)
    .await
    .expect("secondary same-library book author row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("series-3")
    .bind(0_i64)
    .bind("Series 3")
    .bind("series/series-3")
    .bind("library-2")
    .execute(&pool)
    .await
    .expect("cross-library series row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, PUBLISHER, LANGUAGE, AGE_RATING, SERIES_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("ONGOING")
    .bind("Series 3")
    .bind("Series 3")
    .bind("AltPub")
    .bind("EN")
    .bind(12_i64)
    .bind("series-3")
    .execute(&pool)
    .await
    .expect("cross-library series metadata row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-3")
    .bind(0_i64)
    .bind("book-3.epub")
    .bind("books/book-3.epub")
    .bind("series-3")
    .bind(4_096_i64)
    .bind(3_i64)
    .bind("library-2")
    .execute(&pool)
    .await
    .expect("cross-library book row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("3")
    .bind(3.0_f64)
    .bind("Book 3")
    .bind("2024-01-17")
    .bind("book-3")
    .execute(&pool)
    .await
    .expect("cross-library book metadata row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA_AUTHOR (BOOK_ID, NAME, ROLE) \
         VALUES (?, ?, ?)",
    )
    .bind("book-3")
    .bind("Morgan Else")
    .bind("writer")
    .execute(&pool)
    .await
    .expect("cross-library book author row should be inserted");

    pool.close().await;
}
