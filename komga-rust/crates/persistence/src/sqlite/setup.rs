use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{SqliteConnection, SqlitePool};

const MAIN_LATEST_SCHEMA_SQL: &str = include_str!("main_latest_schema.sql");
const TASKS_LATEST_SCHEMA_SQL: &str = include_str!("tasks_latest_schema.sql");

const READ_FIXTURE_SCHEMA_STATEMENTS: &[&str] = &[
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

const REQUIRED_MAIN_SCHEMA: &[(&str, &[&str])] = &[
    ("announcements_read", &["user_id", "announcement_id"]),
    (
        "authentication_activity",
        &[
            "user_id",
            "email",
            "ip",
            "user_agent",
            "success",
            "error",
            "date_time",
            "source",
            "api_key_id",
            "api_key_comment",
        ],
    ),
    (
        "book",
        &[
            "id",
            "created_date",
            "last_modified_date",
            "file_last_modified",
            "name",
            "url",
            "series_id",
            "file_size",
            "number",
            "library_id",
            "file_hash",
            "deleted_date",
            "oneshot",
            "file_hash_koreader",
        ],
    ),
    (
        "book_metadata",
        &[
            "created_date",
            "last_modified_date",
            "number",
            "number_lock",
            "number_sort",
            "number_sort_lock",
            "release_date",
            "release_date_lock",
            "summary",
            "summary_lock",
            "title",
            "title_lock",
            "authors_lock",
            "tags_lock",
            "book_id",
            "isbn",
            "isbn_lock",
            "links_lock",
        ],
    ),
    (
        "book_metadata_aggregation",
        &[
            "created_date",
            "last_modified_date",
            "release_date",
            "summary",
            "summary_number",
            "series_id",
        ],
    ),
    (
        "book_metadata_aggregation_author",
        &["name", "role", "series_id"],
    ),
    ("book_metadata_aggregation_tag", &["tag", "series_id"]),
    ("book_metadata_author", &["name", "role", "book_id"]),
    ("book_metadata_link", &["label", "url", "book_id"]),
    ("book_metadata_tag", &["tag", "book_id"]),
    (
        "client_settings_global",
        &["key", "value", "allow_unauthorized"],
    ),
    ("client_settings_user", &["user_id", "key", "value"]),
    (
        "collection",
        &[
            "id",
            "name",
            "ordered",
            "series_count",
            "created_date",
            "last_modified_date",
        ],
    ),
    (
        "collection_series",
        &["collection_id", "series_id", "number"],
    ),
    (
        "historical_event",
        &["id", "type", "book_id", "series_id", "timestamp"],
    ),
    ("historical_event_properties", &["id", "key", "value"]),
    (
        "library",
        &[
            "id",
            "created_date",
            "last_modified_date",
            "name",
            "root",
            "import_comicinfo_book",
            "import_comicinfo_series",
            "import_comicinfo_collection",
            "import_epub_book",
            "import_epub_series",
            "scan_force_modified_time",
            "scan_startup",
            "import_local_artwork",
            "import_comicinfo_readlist",
            "import_barcode_isbn",
            "convert_to_cbz",
            "repair_extensions",
            "empty_trash_after_scan",
            "import_mylar_series",
            "series_cover",
            "unavailable_date",
            "hash_files",
            "hash_pages",
            "analyze_dimensions",
            "import_comicinfo_series_append_volume",
            "oneshots_directory",
            "scan_cbx",
            "scan_pdf",
            "scan_epub",
            "scan_interval",
            "hash_koreader",
        ],
    ),
    ("library_exclusions", &["library_id", "exclusion"]),
    (
        "media",
        &[
            "media_type",
            "status",
            "created_date",
            "last_modified_date",
            "comment",
            "book_id",
            "page_count",
            "extension_class",
            "_unused",
            "extension_value_blob",
            "epub_divina_compatible",
            "epub_is_kepub",
        ],
    ),
    (
        "media_file",
        &[
            "file_name",
            "book_id",
            "media_type",
            "sub_type",
            "file_size",
        ],
    ),
    (
        "media_page",
        &[
            "file_name",
            "media_type",
            "number",
            "book_id",
            "width",
            "height",
            "file_hash",
            "file_size",
        ],
    ),
    (
        "page_hash",
        &[
            "hash",
            "size",
            "action",
            "delete_count",
            "created_date",
            "last_modified_date",
        ],
    ),
    ("page_hash_thumbnail", &["hash", "thumbnail"]),
    (
        "readlist",
        &[
            "id",
            "name",
            "book_count",
            "created_date",
            "last_modified_date",
            "summary",
            "ordered",
        ],
    ),
    ("readlist_book", &["readlist_id", "book_id", "number"]),
    (
        "read_progress",
        &[
            "book_id",
            "user_id",
            "created_date",
            "last_modified_date",
            "page",
            "completed",
            "read_date",
            "device_id",
            "device_name",
            "locator",
        ],
    ),
    (
        "read_progress_series",
        &[
            "series_id",
            "user_id",
            "read_count",
            "in_progress_count",
            "most_recent_read_date",
            "last_modified_date",
        ],
    ),
    (
        "series",
        &[
            "id",
            "created_date",
            "last_modified_date",
            "file_last_modified",
            "name",
            "url",
            "library_id",
            "book_count",
            "deleted_date",
            "oneshot",
        ],
    ),
    (
        "series_metadata",
        &[
            "created_date",
            "last_modified_date",
            "status",
            "status_lock",
            "title",
            "title_lock",
            "title_sort",
            "title_sort_lock",
            "series_id",
            "publisher",
            "publisher_lock",
            "reading_direction",
            "reading_direction_lock",
            "age_rating",
            "age_rating_lock",
            "summary",
            "summary_lock",
            "language",
            "language_lock",
            "genres_lock",
            "tags_lock",
            "total_book_count",
            "total_book_count_lock",
            "sharing_labels_lock",
            "links_lock",
            "alternate_titles_lock",
        ],
    ),
    (
        "series_metadata_alternate_title",
        &["label", "title", "series_id"],
    ),
    ("series_metadata_genre", &["genre", "series_id"]),
    ("series_metadata_link", &["label", "url", "series_id"]),
    ("series_metadata_sharing", &["label", "series_id"]),
    ("series_metadata_tag", &["tag", "series_id"]),
    ("server_settings", &["key", "value"]),
    (
        "sidecar",
        &["url", "parent_url", "last_modified_time", "library_id"],
    ),
    (
        "sync_point",
        &["id", "created_date", "user_id", "api_key_id"],
    ),
    (
        "sync_point_book",
        &[
            "sync_point_id",
            "book_id",
            "book_created_date",
            "book_last_modified_date",
            "book_file_last_modified",
            "book_file_size",
            "book_file_hash",
            "book_metadata_last_modified_date",
            "book_read_progress_last_modified_date",
            "synced",
            "book_thumbnail_id",
        ],
    ),
    (
        "sync_point_book_removed_synced",
        &["sync_point_id", "book_id"],
    ),
    (
        "sync_point_readlist",
        &[
            "sync_point_id",
            "readlist_id",
            "readlist_name",
            "readlist_created_date",
            "readlist_last_modified_date",
            "synced",
        ],
    ),
    (
        "sync_point_readlist_book",
        &["sync_point_id", "readlist_id", "book_id"],
    ),
    (
        "sync_point_readlist_removed_synced",
        &["sync_point_id", "readlist_id"],
    ),
    (
        "thumbnail_book",
        &[
            "id",
            "thumbnail",
            "url",
            "selected",
            "type",
            "created_date",
            "last_modified_date",
            "book_id",
            "width",
            "height",
            "media_type",
            "file_size",
        ],
    ),
    (
        "thumbnail_collection",
        &[
            "id",
            "selected",
            "thumbnail",
            "type",
            "collection_id",
            "created_date",
            "last_modified_date",
            "width",
            "height",
            "media_type",
            "file_size",
        ],
    ),
    (
        "thumbnail_readlist",
        &[
            "id",
            "selected",
            "thumbnail",
            "type",
            "readlist_id",
            "created_date",
            "last_modified_date",
            "width",
            "height",
            "media_type",
            "file_size",
        ],
    ),
    (
        "thumbnail_series",
        &[
            "id",
            "url",
            "selected",
            "thumbnail",
            "type",
            "created_date",
            "last_modified_date",
            "series_id",
            "width",
            "height",
            "media_type",
            "file_size",
        ],
    ),
    (
        "user",
        &[
            "id",
            "created_date",
            "last_modified_date",
            "email",
            "password",
            "shared_all_libraries",
            "age_restriction",
            "age_restriction_allow_only",
        ],
    ),
    (
        "user_api_key",
        &[
            "id",
            "user_id",
            "created_date",
            "last_modified_date",
            "api_key",
            "comment",
        ],
    ),
    ("user_library_sharing", &["user_id", "library_id"]),
    ("user_role", &["user_id", "role"]),
    ("user_sharing", &["label", "allow", "user_id"]),
];

const REQUIRED_TASKS_SCHEMA: &[(&str, &[&str])] = &[(
    "task",
    &[
        "id",
        "priority",
        "group_id",
        "class",
        "simple_type",
        "payload",
        "owner",
        "created_date",
        "last_modified_date",
    ],
)];

pub async fn open_in_memory_database() -> Result<SqlitePool, sqlx::Error> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await?;
    bootstrap_read_model_pool(&pool).await?;
    Ok(pool)
}

pub async fn open_in_memory_tasks_database() -> Result<SqlitePool, sqlx::Error> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await?;
    bootstrap_tasks_pool(&pool).await?;
    Ok(pool)
}

pub async fn bootstrap_pool(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let mut connection = pool.acquire().await?;
    bootstrap_or_validate_schema(
        connection.as_mut(),
        MAIN_LATEST_SCHEMA_SQL,
        REQUIRED_MAIN_SCHEMA,
    )
    .await
}

pub async fn bootstrap_read_model_pool(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let mut connection = pool.acquire().await?;
    bootstrap_read_model_connection(connection.as_mut()).await
}

pub async fn bootstrap_tasks_pool(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let mut connection = pool.acquire().await?;
    bootstrap_or_validate_schema(
        connection.as_mut(),
        TASKS_LATEST_SCHEMA_SQL,
        REQUIRED_TASKS_SCHEMA,
    )
    .await
}

pub async fn bootstrap_connection(connection: &mut SqliteConnection) -> Result<(), sqlx::Error> {
    bootstrap_or_validate_schema(connection, MAIN_LATEST_SCHEMA_SQL, REQUIRED_MAIN_SCHEMA).await
}

pub async fn bootstrap_read_model_connection(
    connection: &mut SqliteConnection,
) -> Result<(), sqlx::Error> {
    for statement in READ_FIXTURE_SCHEMA_STATEMENTS {
        sqlx::query(statement).execute(&mut *connection).await?;
    }
    Ok(())
}

pub async fn bootstrap_tasks_connection(
    connection: &mut SqliteConnection,
) -> Result<(), sqlx::Error> {
    bootstrap_or_validate_schema(connection, TASKS_LATEST_SCHEMA_SQL, REQUIRED_TASKS_SCHEMA).await
}

async fn bootstrap_or_validate_schema(
    connection: &mut SqliteConnection,
    schema_sql: &str,
    required_schema: &[(&str, &[&str])],
) -> Result<(), sqlx::Error> {
    if is_fresh_install_database(connection).await? {
        apply_schema_sql(connection, schema_sql).await?;
    }

    for (table, required_columns) in required_schema {
        ensure_required_table_columns(connection, table, required_columns).await?;
    }
    Ok(())
}

async fn is_fresh_install_database(connection: &mut SqliteConnection) -> Result<bool, sqlx::Error> {
    let tables = sqlx::query_as::<_, (String,)>(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
    )
    .fetch_all(&mut *connection)
    .await?;

    Ok(tables.is_empty())
}

async fn ensure_required_table_columns(
    connection: &mut SqliteConnection,
    table: &str,
    required_columns: &[&str],
) -> Result<(), sqlx::Error> {
    let pragma = format!("PRAGMA table_info({table})");
    let existing_columns =
        sqlx::query_as::<_, (i64, String, String, i64, Option<String>, i64)>(&pragma)
            .fetch_all(&mut *connection)
            .await?
            .into_iter()
            .map(|(_, name, _, _, _, _)| name.to_ascii_lowercase())
            .collect::<Vec<_>>();

    if existing_columns.is_empty() {
        return Err(outdated_schema_error(format!("in table `{table}`")));
    }

    for column in required_columns {
        if !existing_columns.iter().any(|existing| existing == column) {
            return Err(outdated_schema_error(format!(
                "in table `{table}`: missing required column `{column}`",
            )));
        }
    }

    Ok(())
}

fn outdated_schema_error(detail: String) -> sqlx::Error {
    sqlx::Error::Protocol(format!(
        "unsupported SQLite schema detected {detail}: run Kotlin Komga once to upgrade the database schema before starting Rust runtime",
    ))
}

async fn apply_schema_sql(
    connection: &mut SqliteConnection,
    schema_sql: &str,
) -> Result<(), sqlx::Error> {
    for statement in split_statements(schema_sql) {
        sqlx::query(&statement).execute(&mut *connection).await?;
    }
    Ok(())
}

fn split_statements(content: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let chars = content.chars().collect::<Vec<_>>();
    let mut i = 0;
    let mut in_single_quote = false;

    while i < chars.len() {
        let ch = chars[i];

        if ch == '\'' {
            if in_single_quote && i + 1 < chars.len() && chars[i + 1] == '\'' {
                current.push(ch);
                current.push(chars[i + 1]);
                i += 2;
                continue;
            }

            in_single_quote = !in_single_quote;
            current.push(ch);
            i += 1;
            continue;
        }

        if ch == ';' && !in_single_quote {
            let statement = current.trim();
            if !statement.is_empty() {
                statements.push(statement.to_string());
            }
            current.clear();
            i += 1;
            continue;
        }

        current.push(ch);
        i += 1;
    }

    let trailing = current.trim();
    if !trailing.is_empty() {
        statements.push(trailing.to_string());
    }

    statements
}
