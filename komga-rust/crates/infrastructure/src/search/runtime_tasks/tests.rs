use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::sqlite::connect_main_write_context;
use sqlx::SqlitePool;

use super::super::index_lifecycle::{SearchEntityType, SearchIndexLifecycle};
use super::rebuild_index_from_database;

async fn open_bootstrapped_pool(database_file: &std::path::Path) -> SqlitePool {
    let context = connect_main_write_context(database_file)
        .await
        .expect("fixture sqlite database should bootstrap main schema");
    context.pool().clone()
}

#[tokio::test]
async fn rebuild_indexes_oneshot_inherited_series_metadata_and_book_isbn_fields() {
    let database_file = temp_db_path("search-rebuild-oneshot-inherited-metadata");
    let index_dir = temp_index_dir("search-rebuild-oneshot-inherited-metadata");

    let pool = open_bootstrapped_pool(database_file.as_path()).await;

    sqlx::query(r#"INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES (?, ?, ?)"#)
        .bind("library-1")
        .bind("Library 1")
        .bind("/tmp")
        .execute(&pool)
        .await
        .expect("library row should be inserted");

    sqlx::query(
        r#"INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID, oneshot)
VALUES (?, ?, ?, ?, ?, ?)"#,
    )
    .bind("series-1")
    .bind(0_i64)
    .bind("Series One")
    .bind("series/series-1")
    .bind("library-1")
    .bind(true)
    .execute(&pool)
    .await
    .expect("series row should be inserted");

    sqlx::query(
        r#"INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, PUBLISHER, LANGUAGE, AGE_RATING, SERIES_ID)
VALUES (?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind("ONGOING")
    .bind("Series One")
    .bind("Series One Sort")
    .bind("InheritedPub")
    .bind("EN")
    .bind(13_i64)
    .bind("series-1")
    .execute(&pool)
    .await
    .expect("series metadata should be inserted");

    sqlx::query(
        r#"INSERT INTO SERIES_METADATA_ALTERNATE_TITLE (SERIES_ID, LABEL, TITLE) VALUES (?, ?, ?)"#,
    )
    .bind("series-1")
    .bind("alt-1")
    .bind("Series Uno")
    .execute(&pool)
    .await
    .expect("series alternate title should be inserted");

    sqlx::query(
        r#"INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID, oneshot)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind("book-1")
    .bind(0_i64)
    .bind("book-1.epub")
    .bind("books/book-1.epub")
    .bind("series-1")
    .bind(1024_i64)
    .bind(1_i64)
    .bind("library-1")
    .bind(true)
    .execute(&pool)
    .await
    .expect("book row should be inserted");

    sqlx::query(r#"INSERT INTO BOOK_METADATA_AUTHOR (BOOK_ID, NAME, ROLE) VALUES (?, ?, ?)"#)
        .bind("book-1")
        .bind("Jane Writer")
        .bind("writer")
        .execute(&pool)
        .await
        .expect("book metadata author should be inserted");

    sqlx::query(
        r#"INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, ISBN, BOOK_ID)
VALUES (?, ?, ?, ?, ?)"#,
    )
    .bind("1")
    .bind(1.0_f64)
    .bind("One Shot")
    .bind("978-1-23")
    .bind("book-1")
    .execute(&pool)
    .await
    .expect("book metadata should be inserted");

    rebuild_index_from_database(&pool, index_dir.as_path())
        .await
        .expect("index rebuild should complete");
    pool.close().await;

    let index = SearchIndexLifecycle::bootstrap(index_dir.as_path())
        .expect("search index should bootstrap for verification");

    let publisher_hits = index
        .search_ids("publisher:InheritedPub", SearchEntityType::Book, 10)
        .expect("publisher field query should execute for book entity");
    assert_eq!(
        publisher_hits,
        vec!["book-1".to_string()],
        "oneshot book documents should inherit series metadata fields used by Kotlin-visible search",
    );

    let isbn_hits = index
        .search_ids("isbn:978-1-23", SearchEntityType::Book, 10)
        .expect("isbn field query should execute for book entity");
    assert_eq!(
        isbn_hits,
        vec!["book-1".to_string()],
        "book isbn should remain searchable through retained fielded queries",
    );

    let title_sort_hits = index
        .search_ids("title:Sort", SearchEntityType::Series, 10)
        .expect("title query should execute for series titleSort values");
    assert_eq!(
        title_sort_hits,
        vec!["series-1".to_string()],
        "series titleSort should remain searchable through retained title queries",
    );

    let alternate_title_hits = index
        .search_ids("title:Uno", SearchEntityType::Series, 10)
        .expect("title query should execute for series alternate title values");
    assert_eq!(
        alternate_title_hits,
        vec!["series-1".to_string()],
        "series alternateTitles should remain searchable through retained title queries",
    );

    let writer_hits = index
        .search_ids("writer:Jane", SearchEntityType::Book, 10)
        .expect("writer field query should execute for book entity");
    assert_eq!(
        writer_hits,
        vec!["book-1".to_string()],
        "author role fields such as writer should remain searchable through retained fielded queries",
    );

    if database_file.exists() {
        let _ = std::fs::remove_file(&database_file);
    }
    let _ = std::fs::remove_dir_all(index_dir);
}

fn temp_db_path(case: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "komga-rust-search-runtime-{case}-{}-{nanos}.db",
        std::process::id(),
    ))
}

fn temp_index_dir(case: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "komga-rust-search-runtime-index-{case}-{}-{nanos}",
        std::process::id(),
    ));
    std::fs::create_dir_all(&dir).expect("temporary index directory should be created");
    dir
}
