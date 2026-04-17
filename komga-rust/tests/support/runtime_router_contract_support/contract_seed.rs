use bcrypt::{DEFAULT_COST, hash as hash_bcrypt_password};
use komga_infrastructure::sqlite::connect_pool;

use super::RuntimeDbPaths;

async fn add_read_progress_column_if_missing(pool: &sqlx::SqlitePool, column: &str, sql: &str) {
    match sqlx::query(sql).execute(pool).await {
        Ok(_) => {}
        Err(error)
            if error.to_string().contains("duplicate column name")
                && error.to_string().contains(column) => {}
        Err(error) => panic!(
            "read progress fixture schema should only ignore existing column {column}: {error}"
        ),
    }
}

pub async fn seed_router_contract_data(paths: &RuntimeDbPaths) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract db should open");

    for (column, sql) in [
        (
            "DEVICE_ID",
            "ALTER TABLE READ_PROGRESS ADD COLUMN DEVICE_ID varchar NOT NULL DEFAULT ''",
        ),
        (
            "DEVICE_NAME",
            "ALTER TABLE READ_PROGRESS ADD COLUMN DEVICE_NAME varchar NOT NULL DEFAULT ''",
        ),
        (
            "LOCATOR",
            "ALTER TABLE READ_PROGRESS ADD COLUMN LOCATOR blob",
        ),
    ] {
        add_read_progress_column_if_missing(&pool, column, sql).await;
    }

    sqlx::query(
        "INSERT INTO LIBRARY (ID, NAME, ROOT) \
                 VALUES (?, ?, ?)",
    )
    .bind("library-1")
    .bind("Library 1")
    .bind(paths.config_dir.to_string_lossy().to_string())
    .execute(&pool)
    .await
    .expect("library row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("series-1")
    .bind(0_i64)
    .bind("Series 1")
    .bind("series/series-1")
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("series row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, PUBLISHER, LANGUAGE, AGE_RATING, \
           SERIES_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("ONGOING")
    .bind("Series 1")
    .bind("Series 1")
    .bind("PubHouse")
    .bind("EN")
    .bind(16_i64)
    .bind("series-1")
    .execute(&pool)
    .await
    .expect("series metadata row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES_METADATA_GENRE (SERIES_ID, GENRE) \
                 VALUES (?, ?)",
    )
    .bind("series-1")
    .bind("SciFi")
    .execute(&pool)
    .await
    .expect("series metadata genre row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES_METADATA_TAG (SERIES_ID, TAG) \
                 VALUES (?, ?)",
    )
    .bind("series-1")
    .bind("Favorite")
    .execute(&pool)
    .await
    .expect("series metadata tag row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES_METADATA_SHARING (SERIES_ID, LABEL) \
                 VALUES (?, ?)",
    )
    .bind("series-1")
    .bind("Family")
    .execute(&pool)
    .await
    .expect("series metadata sharing row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA_AGGREGATION_AUTHOR (SERIES_ID, NAME, ROLE) \
         VALUES (?, ?, ?)",
    )
    .bind("series-1")
    .bind("John Doe")
    .bind("writer")
    .execute(&pool)
    .await
    .expect("book metadata aggregation author row should be inserted");

    sqlx::query(
        "INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT) \
                 VALUES (?, ?, ?, ?)",
    )
    .bind("collection-1")
    .bind("Collection 1")
    .bind(false)
    .bind(1_i64)
    .execute(&pool)
    .await
    .expect("collection row should be inserted");

    sqlx::query(
        "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) \
         VALUES (?, ?, ?)",
    )
    .bind("collection-1")
    .bind("series-1")
    .bind(0_i64)
    .execute(&pool)
    .await
    .expect("collection series row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, \
           LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-1")
    .bind(0_i64)
    .bind("book-1.epub")
    .bind("books/book-1.epub")
    .bind("series-1")
    .bind(1_024_i64)
    .bind(1_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("book row should be inserted");

    sqlx::query(
        "UPDATE BOOK \
                 SET FILE_HASH_KOREADER = ? \
                 WHERE ID = ?",
    )
    .bind("hash-book-1")
    .bind("book-1")
    .execute(&pool)
    .await
    .expect("book koreader hash should be set");

    sqlx::query(
        "INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) \
                 VALUES (?, ?, ?, ?)",
    )
    .bind("application/epub+zip")
    .bind("READY")
    .bind("book-1")
    .bind(10_i64)
    .execute(&pool)
    .await
    .expect("media row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("1")
    .bind(1.0_f64)
    .bind("Book 1")
    .bind("2024-01-15")
    .bind("book-1")
    .execute(&pool)
    .await
    .expect("book metadata row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA_TAG (BOOK_ID, TAG) \
                 VALUES (?, ?)",
    )
    .bind("book-1")
    .bind("favorite-tag")
    .execute(&pool)
    .await
    .expect("book metadata tag row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA_AUTHOR (BOOK_ID, NAME, ROLE) \
                 VALUES (?, ?, ?)",
    )
    .bind("book-1")
    .bind("Jane Writer")
    .bind("writer")
    .execute(&pool)
    .await
    .expect("book metadata author row should be inserted");

    sqlx::query(
        "INSERT INTO THUMBNAIL_BOOK (ID, BOOK_ID, TYPE, SELECTED) \
                 VALUES (?, ?, ?, ?)",
    )
    .bind("thumb-book-1")
    .bind("book-1")
    .bind("USER_UPLOADED")
    .bind(true)
    .execute(&pool)
    .await
    .expect("book thumbnail row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA_AGGREGATION (RELEASE_DATE, SUMMARY, SUMMARY_NUMBER, SERIES_ID) \
         VALUES (?, ?, ?, ?)",
    )
    .bind("2024-01-15")
    .bind("")
    .bind("")
    .bind("series-1")
    .execute(&pool)
    .await
    .expect("book metadata aggregation row should be inserted");

    sqlx::query(
        "INSERT INTO READLIST (ID, NAME, BOOK_COUNT) \
                 VALUES (?, ?, ?)",
    )
    .bind("readlist-1")
    .bind("ReadList 1")
    .bind(1_i64)
    .execute(&pool)
    .await
    .expect("readlist row should be inserted");

    sqlx::query(
        "INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) \
                 VALUES (?, ?, ?)",
    )
    .bind("readlist-1")
    .bind("book-1")
    .bind(0_i64)
    .execute(&pool)
    .await
    .expect("readlist book row should be inserted");

    let hashed_password = hash_bcrypt_password("router-contract-admin-123", DEFAULT_COST)
        .expect("bcrypt hash should be computed");
    sqlx::query(
        "INSERT INTO USER (ID, EMAIL, PASSWORD, SHARED_ALL_LIBRARIES) \
                 VALUES (?, ?, ?, ?)",
    )
    .bind("admin-user")
    .bind("admin@example.org")
    .bind(hashed_password)
    .bind(true)
    .execute(&pool)
    .await
    .expect("admin user should be inserted");

    for role in [
        "USER",
        "ADMIN",
        "FILE_DOWNLOAD",
        "PAGE_STREAMING",
        "KOBO_SYNC",
        "KOREADER_SYNC",
    ] {
        sqlx::query(
            "INSERT INTO USER_ROLE (USER_ID, ROLE) \
                     VALUES (?, ?)",
        )
        .bind("admin-user")
        .bind(role)
        .execute(&pool)
        .await
        .expect("admin role should be inserted");
    }

    pool.close().await;
}
