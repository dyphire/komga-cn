use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::sqlite::{connect_private_pool, setup::bootstrap_pool};
use sqlx::SqlitePool;

use super::super::index_lifecycle::{SearchEntityType, SearchIndexLifecycle};
use super::{
    rebuild_index_from_database, sync_entity_delete_from_index, sync_entity_upsert_from_database,
    sync_series_and_oneshot_books_after_metadata_update,
};

async fn open_bootstrapped_pool(database_file: &std::path::Path) -> SqlitePool {
    let pool = connect_private_pool(database_file, 1)
        .await
        .expect("fixture sqlite database should open");
    bootstrap_pool(&pool)
        .await
        .expect("fixture sqlite database should bootstrap main schema");
    pool
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

    pool.close().await;

    rebuild_index_from_database(database_file.as_path(), index_dir.as_path())
        .expect("index rebuild should complete");

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

#[tokio::test]
async fn incremental_sync_updates_all_entity_documents_for_lifecycle_events() {
    let database_file = temp_db_path("search-incremental-sync-lifecycle");
    let index_dir = temp_index_dir("search-incremental-sync-lifecycle");

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
    .bind(false)
    .execute(&pool)
    .await
    .expect("series row should be inserted");

    sqlx::query(
        r#"INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID, oneshot)
VALUES (?, ?, ?, ?, ?, ?)"#,
    )
    .bind("series-oneshot")
    .bind(0_i64)
    .bind("OneShot Series")
    .bind("series/series-oneshot")
    .bind("library-1")
    .bind(true)
    .execute(&pool)
    .await
    .expect("oneshot series row should be inserted");

    sqlx::query(
        r#"INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, PUBLISHER, LANGUAGE, AGE_RATING, SERIES_ID)
VALUES (?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind("ONGOING")
    .bind("Series One")
    .bind("Series One")
    .bind("Publisher One")
    .bind("EN")
    .bind(13_i64)
    .bind("series-1")
    .execute(&pool)
    .await
    .expect("series metadata row should be inserted");

    sqlx::query(
        r#"INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, PUBLISHER, LANGUAGE, AGE_RATING, SERIES_ID)
VALUES (?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind("ONGOING")
    .bind("OneShot Series")
    .bind("OneShot Series")
    .bind("Oneshot Publisher")
    .bind("EN")
    .bind(16_i64)
    .bind("series-oneshot")
    .execute(&pool)
    .await
    .expect("oneshot series metadata row should be inserted");

    sqlx::query(
        r#"INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID, oneshot)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind("book-1")
    .bind(0_i64)
    .bind("book-1.cbz")
    .bind("books/book-1.cbz")
    .bind("series-1")
    .bind(1024_i64)
    .bind(1_i64)
    .bind("library-1")
    .bind(false)
    .execute(&pool)
    .await
    .expect("book row should be inserted");

    sqlx::query(
        r#"INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID, oneshot)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind("book-oneshot")
    .bind(0_i64)
    .bind("book-oneshot.cbz")
    .bind("books/book-oneshot.cbz")
    .bind("series-oneshot")
    .bind(1024_i64)
    .bind(1_i64)
    .bind("library-1")
    .bind(true)
    .execute(&pool)
    .await
    .expect("oneshot book row should be inserted");

    sqlx::query(
        r#"INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, ISBN, BOOK_ID)
VALUES (?, ?, ?, ?, ?)"#,
    )
    .bind("1")
    .bind(1.0_f64)
    .bind("Book One")
    .bind("978-1")
    .bind("book-1")
    .execute(&pool)
    .await
    .expect("book metadata row should be inserted");

    sqlx::query(
        r#"INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, ISBN, BOOK_ID)
VALUES (?, ?, ?, ?, ?)"#,
    )
    .bind("1")
    .bind(1.0_f64)
    .bind("OneShot Book")
    .bind("978-oneshot")
    .bind("book-oneshot")
    .execute(&pool)
    .await
    .expect("oneshot book metadata row should be inserted");

    sqlx::query(r#"INSERT INTO MEDIA (BOOK_ID, STATUS) VALUES (?, ?)"#)
        .bind("book-1")
        .bind("READY")
        .execute(&pool)
        .await
        .expect("media row should be inserted");

    sqlx::query(r#"INSERT INTO MEDIA (BOOK_ID, STATUS) VALUES (?, ?)"#)
        .bind("book-oneshot")
        .bind("READY")
        .execute(&pool)
        .await
        .expect("oneshot media row should be inserted");

    sqlx::query(r#"INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT) VALUES (?, ?, ?, ?)"#)
        .bind("collection-1")
        .bind("Collection One")
        .bind(false)
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("collection row should be inserted");

    sqlx::query(r#"INSERT INTO READLIST (ID, NAME, BOOK_COUNT) VALUES (?, ?, ?)"#)
        .bind("readlist-1")
        .bind("ReadList One")
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("readlist row should be inserted");

    pool.close().await;

    rebuild_index_from_database(database_file.as_path(), index_dir.as_path())
        .expect("index rebuild should complete");

    let pool = connect_private_pool(database_file.as_path(), 1)
        .await
        .expect("fixture sqlite database should reopen for collection update");

    sqlx::query(r#"UPDATE COLLECTION SET NAME = ? WHERE ID = ?"#)
        .bind("Collection Prime")
        .bind("collection-1")
        .execute(&pool)
        .await
        .expect("collection name should be updated");

    pool.close().await;

    sync_entity_upsert_from_database(
        database_file.as_path(),
        index_dir.as_path(),
        SearchEntityType::Collection,
        "collection-1",
    )
    .expect("collection upsert should succeed");

    let collection_hits = SearchIndexLifecycle::bootstrap(index_dir.as_path())
        .expect("index should bootstrap")
        .search_ids("Collection Prime", SearchEntityType::Collection, 10)
        .expect("collection query should succeed");
    assert_eq!(collection_hits, vec!["collection-1".to_string()]);

    sync_entity_delete_from_index(
        index_dir.as_path(),
        SearchEntityType::Collection,
        "collection-1",
    )
    .expect("collection delete should succeed");
    let deleted_collection_hits = SearchIndexLifecycle::bootstrap(index_dir.as_path())
        .expect("index should bootstrap")
        .search_ids("Collection Prime", SearchEntityType::Collection, 10)
        .expect("collection delete query should succeed");
    assert!(deleted_collection_hits.is_empty());

    let pool = connect_private_pool(database_file.as_path(), 1)
        .await
        .expect("fixture sqlite database should reopen");

    sqlx::query(r#"UPDATE READLIST SET NAME = ? WHERE ID = ?"#)
        .bind("ReadList Prime")
        .bind("readlist-1")
        .execute(&pool)
        .await
        .expect("readlist name should be updated");

    sqlx::query(
        r#"UPDATE BOOK_METADATA
SET TITLE = ?
WHERE BOOK_ID = ?"#,
    )
    .bind("Book Prime")
    .bind("book-1")
    .execute(&pool)
    .await
    .expect("book metadata title should be updated");

    sqlx::query(
        r#"UPDATE SERIES_METADATA
SET TITLE = ?
WHERE SERIES_ID = ?"#,
    )
    .bind("Series Prime")
    .bind("series-1")
    .execute(&pool)
    .await
    .expect("series metadata title should be updated");

    sqlx::query(r#"INSERT INTO BOOK_METADATA_AGGREGATION_TAG (SERIES_ID, TAG) VALUES (?, ?)"#)
        .bind("series-1")
        .bind("agg-tag")
        .execute(&pool)
        .await
        .expect("aggregated tag should be inserted");

    sqlx::query(
        r#"UPDATE SERIES_METADATA
SET PUBLISHER = ?
WHERE SERIES_ID = ?"#,
    )
    .bind("Oneshot Publisher Updated")
    .bind("series-oneshot")
    .execute(&pool)
    .await
    .expect("oneshot series publisher should be updated");

    pool.close().await;

    sync_entity_upsert_from_database(
        database_file.as_path(),
        index_dir.as_path(),
        SearchEntityType::ReadList,
        "readlist-1",
    )
    .expect("readlist upsert should succeed");
    sync_entity_upsert_from_database(
        database_file.as_path(),
        index_dir.as_path(),
        SearchEntityType::Book,
        "book-1",
    )
    .expect("book upsert should succeed");
    sync_entity_upsert_from_database(
        database_file.as_path(),
        index_dir.as_path(),
        SearchEntityType::Series,
        "series-1",
    )
    .expect("series upsert should succeed");
    sync_series_and_oneshot_books_after_metadata_update(
        database_file.as_path(),
        index_dir.as_path(),
        "series-oneshot",
    )
    .expect("series metadata driven oneshot refresh should succeed");

    let index = SearchIndexLifecycle::bootstrap(index_dir.as_path())
        .expect("index should bootstrap for lifecycle verification");
    assert_eq!(
        index
            .search_ids("ReadList Prime", SearchEntityType::ReadList, 10)
            .expect("readlist query should succeed"),
        vec!["readlist-1".to_string()]
    );
    assert_eq!(
        index
            .search_ids("Book Prime", SearchEntityType::Book, 10)
            .expect("book query should succeed"),
        vec!["book-1".to_string()]
    );
    assert_eq!(
        index
            .search_ids("Series Prime", SearchEntityType::Series, 10)
            .expect("series query should succeed"),
        vec!["series-1".to_string()]
    );
    assert_eq!(
        index
            .search_ids("tag:agg-tag", SearchEntityType::Series, 10)
            .expect("series aggregated tag query should succeed"),
        vec!["series-1".to_string()]
    );
    assert_eq!(
        index
            .search_ids("publisher:Updated", SearchEntityType::Book, 10)
            .expect("oneshot inherited metadata query should succeed"),
        vec!["book-oneshot".to_string()]
    );

    drop(index);

    sync_entity_delete_from_index(
        index_dir.as_path(),
        SearchEntityType::ReadList,
        "readlist-1",
    )
    .expect("readlist delete should succeed");
    sync_entity_delete_from_index(index_dir.as_path(), SearchEntityType::Book, "book-1")
        .expect("book delete should succeed");
    sync_entity_delete_from_index(index_dir.as_path(), SearchEntityType::Series, "series-1")
        .expect("series delete should succeed");

    let index = SearchIndexLifecycle::bootstrap(index_dir.as_path())
        .expect("index should bootstrap for delete verification");
    assert!(
        index
            .search_ids("ReadList Prime", SearchEntityType::ReadList, 10)
            .expect("readlist delete query should succeed")
            .is_empty()
    );
    assert!(
        index
            .search_ids("Book Prime", SearchEntityType::Book, 10)
            .expect("book delete query should succeed")
            .is_empty()
    );
    assert!(
        index
            .search_ids("Series Prime", SearchEntityType::Series, 10)
            .expect("series delete query should succeed")
            .is_empty()
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
