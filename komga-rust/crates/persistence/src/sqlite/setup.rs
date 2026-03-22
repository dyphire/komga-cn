use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{SqliteConnection, SqlitePool};

const BOOTSTRAP_STATEMENTS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS libraries (id TEXT PRIMARY KEY, name TEXT NOT NULL, root TEXT NOT NULL DEFAULT '')",
    "CREATE TABLE IF NOT EXISTS series (id TEXT PRIMARY KEY, library_id TEXT NOT NULL, title TEXT NOT NULL, age_rating INTEGER NULL, language TEXT NOT NULL DEFAULT '', publisher TEXT NOT NULL DEFAULT '', release_date TEXT NULL, status TEXT NOT NULL DEFAULT '', complete INTEGER NOT NULL DEFAULT 0, read_status TEXT NOT NULL DEFAULT '', deleted INTEGER NOT NULL DEFAULT 0, oneshot INTEGER NOT NULL DEFAULT 0, created TEXT NOT NULL DEFAULT '2026-01-01T00:00:00Z', last_modified TEXT NOT NULL DEFAULT '2026-01-01T00:00:00Z', file_last_modified TEXT NOT NULL DEFAULT '2024-01-02T03:04:05Z', url TEXT NOT NULL DEFAULT '')",
    "CREATE TABLE IF NOT EXISTS collections (id TEXT PRIMARY KEY, name TEXT NOT NULL, ordered INTEGER NOT NULL DEFAULT 0, created_date TEXT NOT NULL DEFAULT '2026-01-01T00:00:00Z', last_modified_date TEXT NOT NULL DEFAULT '2026-01-01T00:00:00Z')",
    "CREATE TABLE IF NOT EXISTS collection_series (collection_id TEXT NOT NULL, series_id TEXT NOT NULL, position INTEGER NOT NULL DEFAULT 0)",
    "CREATE TABLE IF NOT EXISTS series_labels (series_id TEXT NOT NULL, label TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS series_genres (series_id TEXT NOT NULL, genre TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS series_tags (series_id TEXT NOT NULL, tag TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS series_authors (series_id TEXT NOT NULL, author TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS books (id TEXT PRIMARY KEY, series_id TEXT NOT NULL, library_id TEXT NOT NULL, title TEXT NOT NULL, url TEXT NOT NULL DEFAULT '', created TEXT NOT NULL DEFAULT '2024-01-02T03:04:05Z', last_modified TEXT NOT NULL DEFAULT '2024-01-02T03:04:05Z', file_last_modified TEXT NOT NULL DEFAULT '2024-01-02T08:04:05Z', size_bytes INTEGER NOT NULL DEFAULT 0, media_status TEXT NOT NULL DEFAULT 'UNKNOWN', media_profile TEXT NOT NULL DEFAULT '', media_type TEXT NOT NULL DEFAULT '', media_pages_count INTEGER NOT NULL DEFAULT 0, metadata_release_date TEXT NULL, number_sort INTEGER NOT NULL DEFAULT 1, read_status TEXT NOT NULL DEFAULT '', deleted INTEGER NOT NULL DEFAULT 0, oneshot INTEGER NOT NULL DEFAULT 0)",
    "CREATE TABLE IF NOT EXISTS book_tags (book_id TEXT NOT NULL, tag TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS book_authors (book_id TEXT NOT NULL, author TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS read_progress (book_id TEXT NOT NULL, user_id TEXT NOT NULL, page INTEGER NOT NULL, completed INTEGER NOT NULL DEFAULT 0, read_date TEXT NOT NULL, created TEXT NOT NULL, last_modified TEXT NOT NULL, device_id TEXT NOT NULL DEFAULT '', device_name TEXT NOT NULL DEFAULT '')",
    "CREATE TABLE IF NOT EXISTS readlists (id TEXT PRIMARY KEY, name TEXT NOT NULL, summary TEXT NOT NULL DEFAULT '', ordered INTEGER NOT NULL DEFAULT 1, created_date TEXT NOT NULL DEFAULT '2026-01-01T00:00:00Z', last_modified_date TEXT NOT NULL DEFAULT '2026-01-01T00:00:00Z')",
    "CREATE TABLE IF NOT EXISTS readlist_books (readlist_id TEXT NOT NULL, book_id TEXT NOT NULL, position INTEGER NOT NULL DEFAULT 0)",
];

pub async fn open_in_memory_database() -> Result<SqlitePool, sqlx::Error> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await?;
    bootstrap_pool(&pool).await?;
    Ok(pool)
}

pub async fn bootstrap_pool(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let mut connection = pool.acquire().await?;
    bootstrap_connection(connection.as_mut()).await
}

pub async fn bootstrap_connection(connection: &mut SqliteConnection) -> Result<(), sqlx::Error> {
    for statement in BOOTSTRAP_STATEMENTS {
        sqlx::query(statement).execute(&mut *connection).await?;
    }
    Ok(())
}
