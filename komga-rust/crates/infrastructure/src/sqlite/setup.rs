use std::path::Path;
use std::sync::OnceLock;

use serde::Deserialize;
use sqlx::migrate::Migrator;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{SqliteConnection, SqlitePool};

const MAIN_PREFIX_SCHEMA_INVENTORIES_JSON: &str = include_str!(concat!(
    env!("OUT_DIR"),
    "/sqlx-migrations/main-prefix-schema-inventories.json"
));
const TASKS_PREFIX_SCHEMA_INVENTORIES_JSON: &str = include_str!(concat!(
    env!("OUT_DIR"),
    "/sqlx-migrations/tasks-prefix-schema-inventories.json"
));

#[derive(Deserialize)]
struct PrefixSchemaInventory {
    version: i64,
    objects: Vec<SchemaInventoryObject>,
}

#[derive(Deserialize)]
struct SchemaInventoryObject {
    object_type: String,
    name: String,
    table_name: String,
    sql: String,
}

const READ_FIXTURE_SCHEMA_STATEMENTS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS libraries (id TEXT PRIMARY KEY, name TEXT NOT NULL, root TEXT \
       NOT NULL DEFAULT '')",
    "CREATE TABLE IF NOT EXISTS series (id TEXT PRIMARY KEY, library_id TEXT NOT NULL, title \
       TEXT NOT NULL, age_rating INTEGER NULL, language TEXT NOT NULL DEFAULT '', publisher \
       TEXT NOT NULL DEFAULT '', release_date TEXT NULL, status TEXT NOT NULL DEFAULT '', \
       complete INTEGER NOT NULL DEFAULT 0, read_status TEXT NOT NULL DEFAULT '', deleted \
       INTEGER NOT NULL DEFAULT 0, oneshot INTEGER NOT NULL DEFAULT 0, created TEXT NOT NULL \
       DEFAULT '2026-01-01T00:00:00Z', last_modified TEXT NOT NULL DEFAULT \
       '2026-01-01T00:00:00Z', file_last_modified TEXT NOT NULL DEFAULT \
       '2024-01-02T03:04:05Z', url TEXT NOT NULL DEFAULT '')",
    "CREATE TABLE IF NOT EXISTS collections (id TEXT PRIMARY KEY, name TEXT NOT NULL, ordered \
       INTEGER NOT NULL DEFAULT 0, created_date TEXT NOT NULL DEFAULT '2026-01-01T00:00:00Z', \
       last_modified_date TEXT NOT NULL DEFAULT '2026-01-01T00:00:00Z')",
    "CREATE TABLE IF NOT EXISTS collection_series (collection_id TEXT NOT NULL, series_id \
       TEXT NOT NULL, position INTEGER NOT NULL DEFAULT 0)",
    "CREATE TABLE IF NOT EXISTS series_labels (series_id TEXT NOT NULL, label TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS series_genres (series_id TEXT NOT NULL, genre TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS series_tags (series_id TEXT NOT NULL, tag TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS series_authors (series_id TEXT NOT NULL, author TEXT NOT \
       NULL)",
    "CREATE TABLE IF NOT EXISTS books (id TEXT PRIMARY KEY, series_id TEXT NOT NULL, \
       library_id TEXT NOT NULL, title TEXT NOT NULL, url TEXT NOT NULL DEFAULT '', created \
       TEXT NOT NULL DEFAULT '2024-01-02T03:04:05Z', last_modified TEXT NOT NULL DEFAULT \
       '2024-01-02T03:04:05Z', file_last_modified TEXT NOT NULL DEFAULT \
       '2024-01-02T08:04:05Z', size_bytes INTEGER NOT NULL DEFAULT 0, media_status TEXT NOT \
       NULL DEFAULT 'UNKNOWN', media_profile TEXT NOT NULL DEFAULT '', media_type TEXT NOT \
       NULL DEFAULT '', media_pages_count INTEGER NOT NULL DEFAULT 0, metadata_release_date \
       TEXT NULL, number_sort INTEGER NOT NULL DEFAULT 1, read_status TEXT NOT NULL DEFAULT \
       '', deleted INTEGER NOT NULL DEFAULT 0, oneshot INTEGER NOT NULL DEFAULT 0)",
    "CREATE TABLE IF NOT EXISTS book_tags (book_id TEXT NOT NULL, tag TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS book_authors (book_id TEXT NOT NULL, author TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS read_progress (book_id TEXT NOT NULL, user_id TEXT NOT NULL, \
       page INTEGER NOT NULL, completed INTEGER NOT NULL DEFAULT 0, read_date TEXT NOT NULL, \
       created TEXT NOT NULL, last_modified TEXT NOT NULL, device_id TEXT NOT NULL DEFAULT \
       '', device_name TEXT NOT NULL DEFAULT '')",
    "CREATE TABLE IF NOT EXISTS readlists (id TEXT PRIMARY KEY, name TEXT NOT NULL, summary \
       TEXT NOT NULL DEFAULT '', ordered INTEGER NOT NULL DEFAULT 1, created_date TEXT NOT \
       NULL DEFAULT '2026-01-01T00:00:00Z', last_modified_date TEXT NOT NULL DEFAULT \
       '2026-01-01T00:00:00Z')",
    "CREATE TABLE IF NOT EXISTS readlist_books (readlist_id TEXT NOT NULL, book_id TEXT NOT \
       NULL, position INTEGER NOT NULL DEFAULT 0)",
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

const LEGACY_MAIN_SCHEMA_V20200706141854: &[(&str, &[&str])] = &[
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
            "role_admin",
            "role_file_download",
            "role_page_streaming",
        ],
    ),
    ("user_library_sharing", &["user_id", "library_id"]),
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
        ],
    ),
    (
        "media",
        &[
            "media_type",
            "status",
            "thumbnail",
            "created_date",
            "last_modified_date",
            "comment",
            "book_id",
            "page_count",
        ],
    ),
    (
        "media_page",
        &["file_name", "media_type", "number", "book_id"],
    ),
    ("media_file", &["file_name", "book_id"]),
    (
        "book_metadata",
        &[
            "created_date",
            "last_modified_date",
            "age_rating",
            "age_rating_lock",
            "number",
            "number_lock",
            "number_sort",
            "number_sort_lock",
            "publisher",
            "publisher_lock",
            "reading_direction",
            "reading_direction_lock",
            "release_date",
            "release_date_lock",
            "summary",
            "summary_lock",
            "title",
            "title_lock",
            "authors_lock",
            "book_id",
        ],
    ),
    ("book_metadata_author", &["name", "role", "book_id"]),
    (
        "read_progress",
        &[
            "book_id",
            "user_id",
            "created_date",
            "last_modified_date",
            "page",
            "completed",
        ],
    ),
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
];

const LEGACY_MAIN_SCHEMA_V20200706141854_VERSION: i64 = 20200706141854;

#[derive(Clone, Copy, Eq, PartialEq)]
enum SchemaTarget {
    Main,
    Tasks,
}

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
    let migrator = load_main_migrator().await?;
    let mut connection = pool.acquire().await?;
    bootstrap_or_migrate_schema(
        connection.as_mut(),
        &migrator,
        REQUIRED_MAIN_SCHEMA,
        SchemaTarget::Main,
    )
    .await
}

pub async fn bootstrap_read_model_pool(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let mut connection = pool.acquire().await?;
    bootstrap_read_model_connection(connection.as_mut()).await
}

pub async fn bootstrap_tasks_pool(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let migrator = load_tasks_migrator().await?;
    let mut connection = pool.acquire().await?;
    bootstrap_or_migrate_schema(
        connection.as_mut(),
        &migrator,
        REQUIRED_TASKS_SCHEMA,
        SchemaTarget::Tasks,
    )
    .await
}

pub async fn bootstrap_connection(connection: &mut SqliteConnection) -> Result<(), sqlx::Error> {
    let migrator = load_main_migrator().await?;
    bootstrap_or_migrate_schema(
        connection,
        &migrator,
        REQUIRED_MAIN_SCHEMA,
        SchemaTarget::Main,
    )
    .await
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
    let migrator = load_tasks_migrator().await?;
    bootstrap_or_migrate_schema(
        connection,
        &migrator,
        REQUIRED_TASKS_SCHEMA,
        SchemaTarget::Tasks,
    )
    .await
}

async fn load_main_migrator() -> Result<Migrator, sqlx::Error> {
    Migrator::new(Path::new(concat!(env!("OUT_DIR"), "/sqlx-migrations/main")))
        .await
        .map_err(map_migrate_error)
}

async fn load_tasks_migrator() -> Result<Migrator, sqlx::Error> {
    Migrator::new(Path::new(concat!(
        env!("OUT_DIR"),
        "/sqlx-migrations/tasks"
    )))
    .await
    .map_err(map_migrate_error)
}

async fn bootstrap_or_migrate_schema(
    connection: &mut SqliteConnection,
    migrator: &Migrator,
    required_schema: &[(&str, &[&str])],
    target: SchemaTarget,
) -> Result<(), sqlx::Error> {
    adopt_preexisting_schema(connection, migrator, required_schema, target).await?;
    migrator
        .run_direct(connection)
        .await
        .map_err(map_migrate_error)?;

    for (table, required_columns) in required_schema {
        ensure_required_table_columns(connection, table, required_columns).await?;
    }
    Ok(())
}

async fn is_fresh_install_database(connection: &mut SqliteConnection) -> Result<bool, sqlx::Error> {
    let tables = sqlx::query_as::<_, (String,)>(
        "SELECT name \
         FROM sqlite_master \
         WHERE type = 'table' \
         AND name NOT LIKE 'sqlite_%'",
    )
    .fetch_all(&mut *connection)
    .await?;

    Ok(tables.is_empty())
}

async fn adopt_preexisting_schema(
    connection: &mut SqliteConnection,
    migrator: &Migrator,
    required_schema: &[(&str, &[&str])],
    target: SchemaTarget,
) -> Result<(), sqlx::Error> {
    if has_applied_sqlx_migrations(connection).await?
        || is_fresh_install_database(connection).await?
    {
        return Ok(());
    }

    let flyway_versions = load_applied_flyway_versions(connection).await?;
    if !flyway_versions.is_empty() {
        stamp_sqlx_migrations(connection, migrator, |version| {
            flyway_versions.contains(&version)
        })
        .await?;
        return Ok(());
    }

    if let Some(version) = repair_historyless_schema_prefix(connection, target).await? {
        stamp_sqlx_migrations(connection, migrator, |migration_version| {
            migration_version <= version
        })
        .await?;
        return Ok(());
    }

    if schema_matches_required_shape(connection, required_schema).await? {
        stamp_sqlx_migrations(connection, migrator, |_| true).await?;
        return Ok(());
    }

    if let Some(version) = detect_legacy_schema_baseline(connection, target).await? {
        stamp_sqlx_migrations(connection, migrator, |migration_version| {
            migration_version <= version
        })
        .await?;
        return Ok(());
    }

    Err(outdated_schema_error(
        "without Flyway migration history or current Kotlin-compatible schema".to_string(),
    ))
}

async fn has_applied_sqlx_migrations(
    connection: &mut SqliteConnection,
) -> Result<bool, sqlx::Error> {
    if !table_exists(connection, "_sqlx_migrations").await? {
        return Ok(false);
    }

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(&mut *connection)
        .await?;
    Ok(count > 0)
}

async fn load_applied_flyway_versions(
    connection: &mut SqliteConnection,
) -> Result<Vec<i64>, sqlx::Error> {
    if !table_exists(connection, "flyway_schema_history").await? {
        return Ok(Vec::new());
    }

    let versions = sqlx::query_scalar::<_, String>(
        "SELECT version \
         FROM flyway_schema_history \
         WHERE success = 1 AND version IS NOT NULL \
         ORDER BY version",
    )
    .fetch_all(&mut *connection)
    .await?;

    versions
        .into_iter()
        .map(|version| {
            version.parse::<i64>().map_err(|_| {
                sqlx::Error::Protocol(format!(
                    "unexpected Flyway migration version `{version}` in flyway_schema_history"
                ))
            })
        })
        .collect()
}

async fn stamp_sqlx_migrations<F>(
    connection: &mut SqliteConnection,
    migrator: &Migrator,
    should_stamp: F,
) -> Result<(), sqlx::Error>
where
    F: Fn(i64) -> bool,
{
    create_sqlx_migrations_table(connection).await?;

    for migration in migrator.iter() {
        if !migration.migration_type.is_up_migration() || !should_stamp(migration.version) {
            continue;
        }

        sqlx::query(
            "INSERT OR IGNORE INTO _sqlx_migrations (version, description, success, checksum, execution_time) \
             VALUES (?1, ?2, 1, ?3, 0)",
        )
        .bind(migration.version)
        .bind(migration.description.as_ref())
        .bind(migration.checksum.as_ref().to_vec())
        .execute(&mut *connection)
        .await?;
    }

    Ok(())
}

async fn create_sqlx_migrations_table(
    connection: &mut SqliteConnection,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS _sqlx_migrations (\
             version BIGINT PRIMARY KEY,\
             description TEXT NOT NULL,\
             installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,\
             success BOOLEAN NOT NULL,\
             checksum BLOB NOT NULL,\
             execution_time BIGINT NOT NULL\
         )",
    )
    .execute(&mut *connection)
    .await?;
    Ok(())
}

async fn schema_matches_required_shape(
    connection: &mut SqliteConnection,
    required_schema: &[(&str, &[&str])],
) -> Result<bool, sqlx::Error> {
    for (table, required_columns) in required_schema {
        let existing_columns = table_columns(connection, table).await?;
        if existing_columns.is_empty() {
            return Ok(false);
        }

        if required_columns
            .iter()
            .any(|column| !existing_columns.iter().any(|existing| existing == column))
        {
            return Ok(false);
        }
    }

    Ok(true)
}

async fn detect_legacy_schema_baseline(
    connection: &mut SqliteConnection,
    target: SchemaTarget,
) -> Result<Option<i64>, sqlx::Error> {
    match target {
        SchemaTarget::Main => {
            if schema_matches_required_shape(connection, LEGACY_MAIN_SCHEMA_V20200706141854).await?
            {
                Ok(Some(LEGACY_MAIN_SCHEMA_V20200706141854_VERSION))
            } else if let Some(version) =
                detect_historyless_main_schema_prefix_version(connection).await?
            {
                Ok(Some(version))
            } else {
                Ok(None)
            }
        }
        SchemaTarget::Tasks => Ok(None),
    }
}

async fn table_exists(connection: &mut SqliteConnection, table: &str) -> Result<bool, sqlx::Error> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) \
         FROM sqlite_master \
         WHERE type = 'table' AND LOWER(name) = LOWER(?1)",
    )
    .bind(table)
    .fetch_one(&mut *connection)
    .await?;

    Ok(count > 0)
}

async fn detect_historyless_main_schema_prefix_version(
    connection: &mut SqliteConnection,
) -> Result<Option<i64>, sqlx::Error> {
    let live_inventory = comparable_schema_inventory(connection).await?;

    Ok(main_prefix_schema_inventories()
        .iter()
        .rev()
        .find(|entry| schema_inventory_matches(&live_inventory, &entry.objects))
        .map(|entry| entry.version))
}

async fn repair_historyless_schema_prefix(
    connection: &mut SqliteConnection,
    target: SchemaTarget,
) -> Result<Option<i64>, sqlx::Error> {
    let inventories = match target {
        SchemaTarget::Main => main_prefix_schema_inventories(),
        SchemaTarget::Tasks => tasks_prefix_schema_inventories(),
    };

    let Some(expected) = inventories.last() else {
        return Ok(None);
    };

    let live_inventory = comparable_schema_inventory(connection).await?;
    if schema_inventory_matches(&live_inventory, &expected.objects) {
        return Ok(Some(expected.version));
    }

    let Some(missing_objects) = missing_schema_objects(&live_inventory, &expected.objects) else {
        return Ok(None);
    };

    if missing_objects.is_empty() {
        return Ok(None);
    }

    if !can_repair_historyless_schema_objects(target, &missing_objects) {
        return Ok(None);
    }

    for object in missing_objects {
        if object.sql.is_empty() {
            return Ok(None);
        }
        sqlx::query(&object.sql).execute(&mut *connection).await?;
    }

    let repaired_inventory = comparable_schema_inventory(connection).await?;
    if schema_inventory_matches(&repaired_inventory, &expected.objects) {
        Ok(Some(expected.version))
    } else {
        Ok(None)
    }
}

fn main_prefix_schema_inventories() -> &'static [PrefixSchemaInventory] {
    static INVENTORIES: OnceLock<Vec<PrefixSchemaInventory>> = OnceLock::new();
    INVENTORIES
        .get_or_init(|| {
            serde_json::from_str(MAIN_PREFIX_SCHEMA_INVENTORIES_JSON)
                .expect("main prefix schema inventories JSON should parse")
        })
        .as_slice()
}

fn tasks_prefix_schema_inventories() -> &'static [PrefixSchemaInventory] {
    static INVENTORIES: OnceLock<Vec<PrefixSchemaInventory>> = OnceLock::new();
    INVENTORIES
        .get_or_init(|| {
            serde_json::from_str(TASKS_PREFIX_SCHEMA_INVENTORIES_JSON)
                .expect("tasks prefix schema inventories JSON should parse")
        })
        .as_slice()
}

fn can_repair_historyless_schema_objects(
    target: SchemaTarget,
    missing_objects: &[&SchemaInventoryObject],
) -> bool {
    match target {
        SchemaTarget::Main => missing_objects
            .iter()
            .all(|object| matches!(object.object_type.as_str(), "index" | "trigger" | "view")),
        SchemaTarget::Tasks => true,
    }
}

fn missing_schema_objects<'a>(
    live_inventory: &[(String, String, String, String)],
    expected_inventory: &'a [SchemaInventoryObject],
) -> Option<Vec<&'a SchemaInventoryObject>> {
    let mut missing = Vec::new();
    let mut live_index = 0usize;

    for expected in expected_inventory {
        let expected_row = (
            expected.object_type.as_str(),
            expected.name.as_str(),
            expected.table_name.as_str(),
            expected.sql.as_str(),
        );

        if let Some(live) = live_inventory.get(live_index) {
            let live_row = (
                live.0.as_str(),
                live.1.as_str(),
                live.2.as_str(),
                live.3.as_str(),
            );
            if live_row == expected_row {
                live_index += 1;
                continue;
            }
        }

        missing.push(expected);
    }

    if live_index == live_inventory.len() {
        Some(missing)
    } else {
        None
    }
}

fn schema_inventory_matches(
    live_inventory: &[(String, String, String, String)],
    expected_inventory: &[SchemaInventoryObject],
) -> bool {
    live_inventory.len() == expected_inventory.len()
        && live_inventory
            .iter()
            .zip(expected_inventory.iter())
            .all(|(live, expected)| {
                live.0 == expected.object_type
                    && live.1 == expected.name
                    && live.2 == expected.table_name
                    && live.3 == expected.sql
            })
}

async fn comparable_schema_inventory(
    connection: &mut SqliteConnection,
) -> Result<Vec<(String, String, String, String)>, sqlx::Error> {
    sqlx::query_as::<_, (String, String, String, String)>(
        "SELECT type, name, tbl_name, COALESCE(sql, '') AS sql \
         FROM sqlite_master \
         WHERE type IN ('table', 'index', 'trigger', 'view') \
         AND name NOT LIKE 'sqlite_%' \
         AND LOWER(name) NOT IN ('_sqlx_migrations', 'flyway_schema_history') \
         ORDER BY type, name",
    )
    .fetch_all(&mut *connection)
    .await
    .map(|rows| {
        rows.into_iter()
            .map(|(object_type, name, table_name, sql)| {
                (object_type, name, table_name, normalize_schema_sql(&sql))
            })
            .collect()
    })
}

fn normalize_schema_sql(sql: &str) -> String {
    sql.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace(" ,", ",")
        .replace(" )", ")")
        .replace("( ", "(")
}

fn map_migrate_error(error: sqlx::migrate::MigrateError) -> sqlx::Error {
    sqlx::Error::Protocol(error.to_string())
}

async fn ensure_required_table_columns(
    connection: &mut SqliteConnection,
    table: &str,
    required_columns: &[&str],
) -> Result<(), sqlx::Error> {
    let existing_columns = table_columns(connection, table).await?;

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

async fn table_columns(
    connection: &mut SqliteConnection,
    table: &str,
) -> Result<Vec<String>, sqlx::Error> {
    let pragma = format!("PRAGMA table_info({table})");
    let columns = sqlx::query_as::<_, (i64, String, String, i64, Option<String>, i64)>(&pragma)
        .fetch_all(&mut *connection)
        .await?
        .into_iter()
        .map(|(_, name, _, _, _, _)| name.to_ascii_lowercase())
        .collect::<Vec<_>>();
    Ok(columns)
}

fn outdated_schema_error(detail: String) -> sqlx::Error {
    sqlx::Error::Protocol(format!(
        "unsupported SQLite schema detected {detail}: run Kotlin Komga once to upgrade the database schema before starting Rust runtime",
    ))
}
