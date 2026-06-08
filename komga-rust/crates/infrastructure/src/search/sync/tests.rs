use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use sqlx::SqlitePool;

use super::SearchIndexSync;
use crate::search::index_lifecycle::{SearchEntityType, SearchIndexLifecycle};
use crate::sqlite::connect_main_write_context;

struct SearchSyncFixture {
    database_file: PathBuf,
    index_dir: PathBuf,
    pool: SqlitePool,
}

impl SearchSyncFixture {
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

    fn sync(&self, owns_search_index: bool) -> SearchIndexSync {
        SearchIndexSync::new(self.pool.clone(), self.index_dir.clone(), owns_search_index)
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
async fn search_index_sync_upserts_and_deletes_entity_documents() {
    let fixture = SearchSyncFixture::new("sync-upsert-delete-entities").await;
    seed_series(
        &fixture.pool,
        "series-1",
        "Search Sync Series",
        false,
        "Series Publisher",
    )
    .await;
    seed_book(
        &fixture.pool,
        "book-1",
        "series-1",
        "Search Sync Book",
        false,
    )
    .await;
    seed_collection(&fixture.pool, "collection-1", "Search Sync Collection").await;
    seed_readlist(&fixture.pool, "readlist-1", "Search Sync Readlist").await;

    let sync = fixture.sync(true);

    assert!(
        sync.upsert_book("book-1")
            .await
            .expect("book upsert should succeed")
    );
    assert!(
        sync.upsert_series("series-1")
            .await
            .expect("series upsert should succeed")
    );
    assert!(
        sync.upsert_readlist("readlist-1")
            .await
            .expect("readlist upsert should succeed")
    );
    assert!(
        sync.upsert_collection("collection-1")
            .await
            .expect("collection upsert should succeed")
    );
    assert_search_hits(
        fixture.index_dir.as_path(),
        "Search Sync Book",
        SearchEntityType::Book,
        &["book-1"],
    );
    assert_search_hits(
        fixture.index_dir.as_path(),
        "Search Sync Series",
        SearchEntityType::Series,
        &["series-1"],
    );
    assert_search_hits(
        fixture.index_dir.as_path(),
        "Search Sync Collection",
        SearchEntityType::Collection,
        &["collection-1"],
    );
    assert_search_hits(
        fixture.index_dir.as_path(),
        "Search Sync Readlist",
        SearchEntityType::ReadList,
        &["readlist-1"],
    );

    sync.delete_book("book-1")
        .await
        .expect("book delete should succeed");
    sync.delete_series("series-1")
        .await
        .expect("series delete should succeed");
    sync.delete_collection("collection-1")
        .await
        .expect("collection delete should succeed");
    sync.delete_readlist("readlist-1")
        .await
        .expect("readlist delete should succeed");
    assert_search_hits(
        fixture.index_dir.as_path(),
        "Search Sync Book",
        SearchEntityType::Book,
        &[],
    );
    assert_search_hits(
        fixture.index_dir.as_path(),
        "Search Sync Series",
        SearchEntityType::Series,
        &[],
    );
    assert_search_hits(
        fixture.index_dir.as_path(),
        "Search Sync Collection",
        SearchEntityType::Collection,
        &[],
    );
    assert_search_hits(
        fixture.index_dir.as_path(),
        "Search Sync Readlist",
        SearchEntityType::ReadList,
        &[],
    );

    fixture.cleanup().await;
}

#[tokio::test]
async fn search_index_sync_refreshes_series_and_oneshot_book_documents_after_metadata_update() {
    let fixture = SearchSyncFixture::new("sync-refresh-series-oneshot").await;
    seed_series(
        &fixture.pool,
        "series-1",
        "Series One",
        true,
        "Initial Publisher",
    )
    .await;
    seed_book(&fixture.pool, "book-1", "series-1", "One Shot Book", true).await;

    let sync = fixture.sync(true);
    sync.rebuild_all()
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

    sync.refresh_series_after_metadata_update("series-1")
        .await
        .expect("series metadata refresh sync should succeed");

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
async fn search_index_sync_rebuilds_all_and_scoped_entities() {
    let fixture = SearchSyncFixture::new("sync-rebuild-scoped").await;
    seed_collection(&fixture.pool, "collection-1", "Collection Before").await;
    seed_readlist(&fixture.pool, "readlist-1", "Readlist Before").await;

    let sync = fixture.sync(true);
    sync.rebuild_all()
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

    sync.rebuild_entities(&[SearchEntityType::Collection])
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
async fn search_index_sync_skips_writes_when_search_index_is_external_owned() {
    let fixture = SearchSyncFixture::new("sync-ownership-noop").await;
    seed_collection(&fixture.pool, "collection-1", "External Owned Collection").await;

    let sync = fixture.sync(false);

    let upserted = sync
        .upsert_collection("collection-1")
        .await
        .expect("external-owned upsert should no-op");
    assert!(!upserted);
    assert!(
        !fixture.index_dir.join("meta.json").exists(),
        "external-owned sync must not create index files",
    );

    sync.rebuild_all()
        .await
        .expect("external-owned rebuild should no-op");
    assert!(
        !fixture.index_dir.join("meta.json").exists(),
        "external-owned rebuild must not create index files",
    );
    sync.delete_collection("collection-1")
        .await
        .expect("external-owned delete should no-op");
    assert!(
        !fixture.index_dir.join("meta.json").exists(),
        "external-owned delete must not create index files",
    );

    fixture.cleanup().await;
}

#[tokio::test]
async fn search_index_sync_recovers_corrupted_index_before_applying_delete() {
    let fixture = SearchSyncFixture::new("sync-delete-corruption-recovery").await;
    seed_collection(&fixture.pool, "collection-1", "Delete Drift Collection").await;

    let sync = fixture.sync(true);
    sync.rebuild_all()
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

    sync.delete_collection("collection-1")
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
async fn search_index_sync_recovers_corrupted_index_before_scoped_rebuild() {
    let fixture = SearchSyncFixture::new("sync-scoped-rebuild-corruption-recovery").await;
    seed_collection(
        &fixture.pool,
        "collection-1",
        "Scoped Drift Collection Before",
    )
    .await;

    let sync = fixture.sync(true);
    sync.rebuild_all()
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

    sync.rebuild_entities(&[SearchEntityType::Collection])
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
        "komga-rust-search-sync-{case}-{}-{nanos}.db",
        std::process::id(),
    ))
}

fn temp_index_dir(case: &str) -> PathBuf {
    let nanos = unique_nanos();
    let dir = std::env::temp_dir().join(format!(
        "komga-rust-search-sync-index-{case}-{}-{nanos}",
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
