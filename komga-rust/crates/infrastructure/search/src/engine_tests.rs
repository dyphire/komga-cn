use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use sqlx::SqlitePool;

use super::SearchIndexEngine;
use crate::lifecycle::{
    SearchDocument, SearchEntityType, SearchField, SearchFieldEntry, SearchIndexLifecycle,
};
use komga_infrastructure_base::sqlite::connect_main_write_context;

struct SearchIndexEngineFixture {
    database_file: PathBuf,
    index_dir: PathBuf,
    pool: SqlitePool,
}

impl SearchIndexEngineFixture {
    async fn new(case: &str) -> Self {
        let database_file = temp_db_path(case);
        let index_dir = temp_index_dir(case);
        let context = connect_main_write_context(database_file.as_path())
            .await
            .expect("fixture sqlite database should bootstrap main schema");
        let pool = context.pool().clone();

        seed_library(&pool).await;

        Self {
            database_file,
            index_dir,
            pool,
        }
    }

    fn engine(&self, owns_search_index: bool) -> SearchIndexEngine {
        SearchIndexEngine::new(self.pool.clone(), self.index_dir.clone(), owns_search_index)
    }

    async fn cleanup(self) {
        self.pool.close().await;
        if self.database_file.exists() {
            let _ = std::fs::remove_file(&self.database_file);
        }
        let _ = std::fs::remove_dir_all(self.index_dir);
    }
}

#[tokio::test]
async fn missing_index_searches_fail_without_creating_index_state() {
    let fixture = SearchIndexEngineFixture::new("missing-index-query").await;
    let _ = std::fs::remove_dir_all(&fixture.index_dir);
    let engine = fixture.engine(false);

    let ids_error = engine
        .search_ids("anything", SearchEntityType::Book, 10)
        .expect_err("missing index should fail unscored search");
    assert!(
        ids_error
            .to_string()
            .contains("failed to open search index for query"),
        "{ids_error}"
    );

    let scored_error = engine
        .search_scored_ids("anything", SearchEntityType::Book, 10)
        .expect_err("missing index should fail scored search");
    assert!(
        scored_error
            .to_string()
            .contains("failed to open search index for query"),
        "{scored_error}"
    );
    assert!(
        !fixture.index_dir.exists(),
        "read-only search boundary must not create missing index directories"
    );

    fixture.cleanup().await;
}

#[tokio::test]
async fn explicit_index_dir_is_the_only_query_source() {
    let fixture = SearchIndexEngineFixture::new("explicit-index-query").await;
    let other_index_dir = temp_index_dir("explicit-index-query-other");

    let explicit_index = SearchIndexLifecycle::bootstrap(fixture.index_dir.as_path())
        .expect("explicit index fixture should bootstrap");
    explicit_index
        .rebuild(&[collection_document("collection-1", "Explicit Shelf")])
        .expect("explicit index fixture should rebuild");
    let other_index = SearchIndexLifecycle::bootstrap(other_index_dir.as_path())
        .expect("other index fixture should bootstrap");
    other_index
        .rebuild(&[collection_document("collection-2", "Other Shelf")])
        .expect("other index fixture should rebuild");

    let hits = fixture
        .engine(false)
        .search_ids("Explicit", SearchEntityType::Collection, 10)
        .expect("explicit search should execute");

    assert_eq!(hits, vec!["collection-1".to_string()]);

    let _ = explicit_index.shutdown();
    let _ = other_index.shutdown();
    let _ = std::fs::remove_dir_all(other_index_dir);
    fixture.cleanup().await;
}

#[tokio::test]
async fn search_index_engine_upserts_and_deletes_entity_documents() {
    let fixture = SearchIndexEngineFixture::new("engine-upsert-delete-entities").await;
    seed_series(
        &fixture.pool,
        "series-1",
        "Search Engine Series",
        false,
        "Series Publisher",
    )
    .await;
    seed_book(
        &fixture.pool,
        "book-1",
        "series-1",
        "Search Engine Book",
        false,
    )
    .await;
    seed_collection(&fixture.pool, "collection-1", "Search Engine Collection").await;
    seed_readlist(&fixture.pool, "readlist-1", "Search Engine Readlist").await;

    let engine = fixture.engine(true);

    assert!(
        engine
            .upsert_book("book-1")
            .await
            .expect("book upsert should succeed")
    );
    assert!(
        engine
            .upsert_series("series-1")
            .await
            .expect("series upsert should succeed")
    );
    assert!(
        engine
            .upsert_readlist("readlist-1")
            .await
            .expect("readlist upsert should succeed")
    );
    assert!(
        engine
            .upsert_collection("collection-1")
            .await
            .expect("collection upsert should succeed")
    );
    assert_search_hits(
        fixture.index_dir.as_path(),
        "Search Engine Book",
        SearchEntityType::Book,
        &["book-1"],
    );
    assert_search_hits(
        fixture.index_dir.as_path(),
        "Search Engine Series",
        SearchEntityType::Series,
        &["series-1"],
    );
    assert_search_hits(
        fixture.index_dir.as_path(),
        "Search Engine Collection",
        SearchEntityType::Collection,
        &["collection-1"],
    );
    assert_search_hits(
        fixture.index_dir.as_path(),
        "Search Engine Readlist",
        SearchEntityType::ReadList,
        &["readlist-1"],
    );

    engine
        .delete_book("book-1")
        .await
        .expect("book delete should succeed");
    engine
        .delete_series("series-1")
        .await
        .expect("series delete should succeed");
    engine
        .delete_collection("collection-1")
        .await
        .expect("collection delete should succeed");
    engine
        .delete_readlist("readlist-1")
        .await
        .expect("readlist delete should succeed");
    assert_search_hits(
        fixture.index_dir.as_path(),
        "Search Engine Book",
        SearchEntityType::Book,
        &[],
    );
    assert_search_hits(
        fixture.index_dir.as_path(),
        "Search Engine Series",
        SearchEntityType::Series,
        &[],
    );
    assert_search_hits(
        fixture.index_dir.as_path(),
        "Search Engine Collection",
        SearchEntityType::Collection,
        &[],
    );
    assert_search_hits(
        fixture.index_dir.as_path(),
        "Search Engine Readlist",
        SearchEntityType::ReadList,
        &[],
    );

    fixture.cleanup().await;
}

#[tokio::test]
async fn search_index_engine_refreshes_series_and_oneshot_book_documents_after_metadata_update() {
    let fixture = SearchIndexEngineFixture::new("engine-refresh-series-oneshot").await;
    seed_series(
        &fixture.pool,
        "series-1",
        "Series One",
        true,
        "Initial Publisher",
    )
    .await;
    seed_book(&fixture.pool, "book-1", "series-1", "One Shot Book", true).await;

    let engine = fixture.engine(true);
    engine
        .rebuild_all()
        .await
        .expect("initial full rebuild should succeed");
    assert_search_hits(
        fixture.index_dir.as_path(),
        "publisher:Initial",
        SearchEntityType::Book,
        &["book-1"],
    );

    sqlx::query("UPDATE SERIES_METADATA SET PUBLISHER = ? WHERE SERIES_ID = ?")
        .bind("Updated Publisher")
        .bind("series-1")
        .execute(&fixture.pool)
        .await
        .expect("series metadata should update");

    engine
        .refresh_series_after_metadata_update("series-1")
        .await
        .expect("series metadata refresh should succeed");

    assert_search_hits(
        fixture.index_dir.as_path(),
        "publisher:Updated",
        SearchEntityType::Series,
        &["series-1"],
    );
    assert_search_hits(
        fixture.index_dir.as_path(),
        "publisher:Updated",
        SearchEntityType::Book,
        &["book-1"],
    );

    fixture.cleanup().await;
}

#[tokio::test]
async fn search_index_engine_rebuilds_all_and_scoped_entities() {
    let fixture = SearchIndexEngineFixture::new("engine-rebuild-scoped").await;
    seed_collection(&fixture.pool, "collection-1", "Collection Before").await;
    seed_readlist(&fixture.pool, "readlist-1", "Readlist Before").await;

    let engine = fixture.engine(true);
    engine
        .rebuild_all()
        .await
        .expect("initial full rebuild should succeed");
    assert_search_hits(
        fixture.index_dir.as_path(),
        "Before",
        SearchEntityType::Collection,
        &["collection-1"],
    );
    assert_search_hits(
        fixture.index_dir.as_path(),
        "Before",
        SearchEntityType::ReadList,
        &["readlist-1"],
    );

    rename_collection(&fixture.pool, "collection-1", "Collection After").await;
    rename_readlist(&fixture.pool, "readlist-1", "Readlist After").await;

    engine
        .rebuild_entities(&[SearchEntityType::Collection])
        .await
        .expect("scoped collection rebuild should succeed");

    assert_search_hits(
        fixture.index_dir.as_path(),
        "Collection After",
        SearchEntityType::Collection,
        &["collection-1"],
    );
    assert_search_hits(
        fixture.index_dir.as_path(),
        "Readlist After",
        SearchEntityType::ReadList,
        &[],
    );
    assert_search_hits(
        fixture.index_dir.as_path(),
        "Readlist Before",
        SearchEntityType::ReadList,
        &["readlist-1"],
    );

    fixture.cleanup().await;
}

#[tokio::test]
async fn search_index_engine_rebuild_indexes_oneshot_inherited_metadata_and_book_isbn_fields() {
    let fixture = SearchIndexEngineFixture::new("engine-rebuild-oneshot-inherited-metadata").await;
    seed_series(
        &fixture.pool,
        "series-1",
        "Series One",
        true,
        "InheritedPub",
    )
    .await;
    sqlx::query(
        r#"UPDATE SERIES_METADATA SET TITLE_SORT = ?, LANGUAGE = ?, AGE_RATING = ? WHERE SERIES_ID = ?"#,
    )
    .bind("Series One Sort")
    .bind("EN")
    .bind(13_i64)
    .bind("series-1")
    .execute(&fixture.pool)
    .await
    .expect("series metadata should be updated");
    sqlx::query(
        r#"INSERT INTO SERIES_METADATA_ALTERNATE_TITLE (SERIES_ID, LABEL, TITLE) VALUES (?, ?, ?)"#,
    )
    .bind("series-1")
    .bind("alt-1")
    .bind("Series Uno")
    .execute(&fixture.pool)
    .await
    .expect("series alternate title should be inserted");
    seed_book(&fixture.pool, "book-1", "series-1", "book-1.epub", true).await;
    sqlx::query(r#"INSERT INTO BOOK_METADATA_AUTHOR (BOOK_ID, NAME, ROLE) VALUES (?, ?, ?)"#)
        .bind("book-1")
        .bind("Jane Writer")
        .bind("writer")
        .execute(&fixture.pool)
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
    .execute(&fixture.pool)
    .await
    .expect("book metadata should be inserted");

    fixture
        .engine(true)
        .rebuild_all()
        .await
        .expect("index rebuild should complete");

    assert_search_hits(
        fixture.index_dir.as_path(),
        "publisher:InheritedPub",
        SearchEntityType::Book,
        &["book-1"],
    );
    assert_search_hits(
        fixture.index_dir.as_path(),
        "isbn:978-1-23",
        SearchEntityType::Book,
        &["book-1"],
    );
    assert_search_hits(
        fixture.index_dir.as_path(),
        "title:Sort",
        SearchEntityType::Series,
        &["series-1"],
    );
    assert_search_hits(
        fixture.index_dir.as_path(),
        "title:Uno",
        SearchEntityType::Series,
        &["series-1"],
    );
    assert_search_hits(
        fixture.index_dir.as_path(),
        "writer:Jane",
        SearchEntityType::Book,
        &["book-1"],
    );

    fixture.cleanup().await;
}

#[tokio::test]
async fn search_index_engine_skips_writes_when_search_index_is_external_owned() {
    let fixture = SearchIndexEngineFixture::new("engine-ownership-noop").await;
    seed_collection(&fixture.pool, "collection-1", "External Owned Collection").await;

    let engine = fixture.engine(false);

    let upserted = engine
        .upsert_collection("collection-1")
        .await
        .expect("external-owned upsert should no-op");
    assert!(!upserted);
    assert!(
        !fixture.index_dir.join("meta.json").exists(),
        "external-owned engine must not create index files",
    );

    engine
        .rebuild_all()
        .await
        .expect("external-owned rebuild should no-op");
    assert!(
        !fixture.index_dir.join("meta.json").exists(),
        "external-owned rebuild must not create index files",
    );
    engine
        .delete_collection("collection-1")
        .await
        .expect("external-owned delete should no-op");
    assert!(
        !fixture.index_dir.join("meta.json").exists(),
        "external-owned delete must not create index files",
    );

    fixture.cleanup().await;
}

#[tokio::test]
async fn search_index_engine_recovers_corrupted_index_before_applying_delete() {
    let fixture = SearchIndexEngineFixture::new("engine-delete-corruption-recovery").await;
    seed_collection(&fixture.pool, "collection-1", "Delete Drift Collection").await;

    let engine = fixture.engine(true);
    engine
        .rebuild_all()
        .await
        .expect("initial full rebuild should succeed");
    assert_search_hits(
        fixture.index_dir.as_path(),
        "Delete Drift Collection",
        SearchEntityType::Collection,
        &["collection-1"],
    );
    std::fs::remove_file(fixture.index_dir.join(".komga-search-analyzer-version"))
        .expect("analyzer marker should be removable");

    engine
        .delete_collection("collection-1")
        .await
        .expect("delete should recover the index before applying");

    assert_search_hits(
        fixture.index_dir.as_path(),
        "Delete Drift Collection",
        SearchEntityType::Collection,
        &[],
    );

    fixture.cleanup().await;
}

#[tokio::test]
async fn search_index_engine_recovers_corrupted_index_before_scoped_rebuild() {
    let fixture = SearchIndexEngineFixture::new("engine-scoped-rebuild-corruption-recovery").await;
    seed_collection(
        &fixture.pool,
        "collection-1",
        "Scoped Drift Collection Before",
    )
    .await;

    let engine = fixture.engine(true);
    engine
        .rebuild_all()
        .await
        .expect("initial full rebuild should succeed");
    assert_search_hits(
        fixture.index_dir.as_path(),
        "Before",
        SearchEntityType::Collection,
        &["collection-1"],
    );

    std::fs::remove_file(fixture.index_dir.join(".komga-search-analyzer-version"))
        .expect("analyzer marker should be removable");
    rename_collection(
        &fixture.pool,
        "collection-1",
        "Scoped Drift Collection After",
    )
    .await;

    engine
        .rebuild_entities(&[SearchEntityType::Collection])
        .await
        .expect("scoped rebuild should recover the index before applying");

    assert_search_hits(
        fixture.index_dir.as_path(),
        "After",
        SearchEntityType::Collection,
        &["collection-1"],
    );

    fixture.cleanup().await;
}

async fn seed_library(pool: &SqlitePool) {
    sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES (?, ?, ?)")
        .bind("library-1")
        .bind("Library 1")
        .bind("/tmp")
        .execute(pool)
        .await
        .expect("library row should be inserted");
}

async fn seed_series(
    pool: &SqlitePool,
    series_id: &str,
    title: &str,
    oneshot: bool,
    publisher: &str,
) {
    sqlx::query(
        r#"INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID, oneshot)
VALUES (?, ?, ?, ?, ?, ?)"#,
    )
    .bind(series_id)
    .bind(0_i64)
    .bind(title)
    .bind(format!("series/{series_id}"))
    .bind("library-1")
    .bind(oneshot)
    .execute(pool)
    .await
    .expect("series row should be inserted");

    sqlx::query(
        r#"INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, PUBLISHER, SERIES_ID)
VALUES (?, ?, ?, ?, ?)"#,
    )
    .bind("ONGOING")
    .bind(title)
    .bind(title)
    .bind(publisher)
    .bind(series_id)
    .execute(pool)
    .await
    .expect("series metadata row should be inserted");
}

async fn seed_book(pool: &SqlitePool, book_id: &str, series_id: &str, title: &str, oneshot: bool) {
    sqlx::query(
        r#"INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID, oneshot)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(book_id)
    .bind(0_i64)
    .bind(title)
    .bind(format!("books/{book_id}.cbz"))
    .bind(series_id)
    .bind(1024_i64)
    .bind(1_i64)
    .bind("library-1")
    .bind(oneshot)
    .execute(pool)
    .await
    .expect("book row should be inserted");
}

async fn seed_collection(pool: &SqlitePool, collection_id: &str, name: &str) {
    sqlx::query("INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT) VALUES (?, ?, ?, ?)")
        .bind(collection_id)
        .bind(name)
        .bind(false)
        .bind(0_i64)
        .execute(pool)
        .await
        .expect("collection row should be inserted");
}

async fn seed_readlist(pool: &SqlitePool, readlist_id: &str, name: &str) {
    sqlx::query(
        "INSERT INTO READLIST (ID, NAME, BOOK_COUNT, SUMMARY, ORDERED) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(readlist_id)
    .bind(name)
    .bind(0_i64)
    .bind("")
    .bind(true)
    .execute(pool)
    .await
    .expect("readlist row should be inserted");
}

async fn rename_collection(pool: &SqlitePool, collection_id: &str, name: &str) {
    sqlx::query("UPDATE COLLECTION SET NAME = ? WHERE ID = ?")
        .bind(name)
        .bind(collection_id)
        .execute(pool)
        .await
        .expect("collection row should be renamed");
}

async fn rename_readlist(pool: &SqlitePool, readlist_id: &str, name: &str) {
    sqlx::query("UPDATE READLIST SET NAME = ? WHERE ID = ?")
        .bind(name)
        .bind(readlist_id)
        .execute(pool)
        .await
        .expect("readlist row should be renamed");
}

fn collection_document(id: &str, name: &str) -> SearchDocument {
    SearchDocument {
        entity_type: SearchEntityType::Collection,
        id: id.to_string(),
        title: name.to_string(),
        fields: vec![SearchFieldEntry::new(SearchField::Name, name)],
    }
}

fn assert_search_hits(
    index_dir: &Path,
    query: &str,
    entity_type: SearchEntityType,
    expected: &[&str],
) {
    let index = SearchIndexLifecycle::bootstrap(index_dir).expect("search index should bootstrap");
    let hits = index
        .search_ids(query, entity_type, 10)
        .expect("search query should execute");
    assert_eq!(
        hits,
        expected
            .iter()
            .map(|id| (*id).to_string())
            .collect::<Vec<_>>(),
    );
}

fn temp_db_path(case: &str) -> PathBuf {
    let nanos = unique_nanos();
    std::env::temp_dir().join(format!(
        "komga-rust-search-engine-{case}-{}-{nanos}.db",
        std::process::id(),
    ))
}

fn temp_index_dir(case: &str) -> PathBuf {
    let nanos = unique_nanos();
    let dir = std::env::temp_dir().join(format!(
        "komga-rust-search-engine-index-{case}-{}-{nanos}",
        std::process::id(),
    ));
    std::fs::create_dir_all(&dir).expect("temporary index directory should be created");
    dir
}

fn unique_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos()
}
