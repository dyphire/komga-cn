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

pub async fn seed_router_series_read_progress(
    paths: &RuntimeDbPaths,
    read_count: i64,
    in_progress_count: i64,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract series read-progress db should open");

    sqlx::query(
        "INSERT INTO READ_PROGRESS_SERIES (SERIES_ID, USER_ID, READ_COUNT, IN_PROGRESS_COUNT) \
         VALUES (?, ?, ?, ?) \
         ON CONFLICT (SERIES_ID, USER_ID) DO UPDATE \
         SET READ_COUNT = excluded.READ_COUNT, IN_PROGRESS_COUNT = excluded.IN_PROGRESS_COUNT",
    )
    .bind("series-1")
    .bind("admin-user")
    .bind(read_count)
    .bind(in_progress_count)
    .execute(&pool)
    .await
    .expect("router contract series read-progress row should be upserted");

    pool.close().await;
}

pub async fn seed_router_series_counts(
    paths: &RuntimeDbPaths,
    book_count: i64,
    total_book_count: Option<i64>,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract series counts db should open");

    sqlx::query(
        "UPDATE SERIES \
                 SET BOOK_COUNT = ? \
                 WHERE ID = ?",
    )
    .bind(book_count)
    .bind("series-1")
    .execute(&pool)
    .await
    .expect("router contract series book_count should be updated");

    sqlx::query(
        "UPDATE SERIES_METADATA \
                 SET TOTAL_BOOK_COUNT = ? \
                 WHERE SERIES_ID = ?",
    )
    .bind(total_book_count)
    .bind("series-1")
    .execute(&pool)
    .await
    .expect("router contract series total_book_count should be updated");

    pool.close().await;
}

pub async fn seed_router_series_title_sort(
    paths: &RuntimeDbPaths,
    series_id: &str,
    title_sort: &str,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract series title-sort db should open");

    sqlx::query(
        "UPDATE SERIES_METADATA \
         SET TITLE_SORT = ? \
         WHERE SERIES_ID = ?",
    )
    .bind(title_sort)
    .bind(series_id)
    .execute(&pool)
    .await
    .expect("series metadata title_sort should be updated for contract test");

    pool.close().await;
}

pub async fn seed_router_series_alternate_title(
    paths: &RuntimeDbPaths,
    series_id: &str,
    label: &str,
    title: &str,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract series alternate-title db should open");

    sqlx::query(
        "INSERT INTO SERIES_METADATA_ALTERNATE_TITLE (SERIES_ID, LABEL, TITLE) \
         VALUES (?, ?, ?)",
    )
    .bind(series_id)
    .bind(label)
    .bind(title)
    .execute(&pool)
    .await
    .expect("series metadata alternate title should be inserted for contract test");

    pool.close().await;
}

pub async fn seed_router_series_aggregated_tag(paths: &RuntimeDbPaths, series_id: &str, tag: &str) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract series aggregated tag db should open");

    sqlx::query(
        "INSERT INTO BOOK_METADATA_AGGREGATION_TAG (SERIES_ID, TAG) \
         VALUES (?, ?)",
    )
    .bind(series_id)
    .bind(tag)
    .execute(&pool)
    .await
    .expect("series aggregated tag row should be inserted for contract test");

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
