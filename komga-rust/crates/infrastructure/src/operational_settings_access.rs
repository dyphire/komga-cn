use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::sqlite::connect_pool;
use crate::sqlite::read_models::client_settings::{
    load_client_settings_global as load_client_settings_global_model,
    load_client_settings_user as load_client_settings_user_model,
};
use crate::sqlite::write_models::client_settings::{
    delete_client_settings_global as delete_client_settings_global_model,
    delete_client_settings_user as delete_client_settings_user_model,
    upsert_client_settings_global as upsert_client_settings_global_model,
    upsert_client_settings_user as upsert_client_settings_user_model,
};
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use sqlx::Row;

use crate::sqlite::write_models::server_settings::ServerSettingsStore;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedServerSettings {
    pub delete_empty_collections: bool,
    pub delete_empty_read_lists: bool,
    pub remember_me_key: String,
    pub remember_me_duration_days: u64,
    pub thumbnail_size: &'static str,
    pub task_pool_size: u64,
    pub server_port: Option<u16>,
    pub server_context_path: Option<String>,
    pub kobo_proxy: bool,
    pub kobo_port: Option<u16>,
}

struct PersistedHistoricalEvent {
    id: String,
    event_type: String,
    book_id: Option<String>,
    series_id: Option<String>,
    timestamp: String,
}

pub async fn delete_syncpoints_by_user(
    database_file: &Path,
    user_id: &str,
) -> Result<(), sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let mut tx = pool.begin().await?;
    let sync_point_ids = load_syncpoint_ids_for_user(&mut tx, user_id, None).await?;
    delete_syncpoint_children(&mut tx, &sync_point_ids).await?;
    sqlx::query(
        r#"DELETE
        FROM SYNC_POINT
        WHERE USER_ID = ?"#,
    )
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn delete_syncpoints_by_user_and_key_ids(
    database_file: &Path,
    user_id: &str,
    key_ids: &[String],
) -> Result<(), sqlx::Error> {
    if key_ids.is_empty() {
        return delete_syncpoints_by_user(database_file, user_id).await;
    }

    let pool = connect_pool(database_file, 1).await?;
    let mut tx = pool.begin().await?;
    let sync_point_ids = load_syncpoint_ids_for_user(&mut tx, user_id, Some(key_ids)).await?;
    delete_syncpoint_children(&mut tx, &sync_point_ids).await?;

    let mut query =
        sqlx::QueryBuilder::<sqlx::Sqlite>::new("DELETE FROM SYNC_POINT WHERE USER_ID = ");
    query.push_bind(user_id);
    query.push(" AND API_KEY_ID IN (");
    let mut separated = query.separated(", ");
    for key_id in key_ids {
        separated.push_bind(key_id);
    }
    separated.push_unseparated(")");
    query.build().execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

async fn load_syncpoint_ids_for_user(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    user_id: &str,
    key_ids: Option<&[String]>,
) -> Result<Vec<String>, sqlx::Error> {
    let mut query =
        sqlx::QueryBuilder::<sqlx::Sqlite>::new("SELECT ID FROM SYNC_POINT WHERE USER_ID = ");
    query.push_bind(user_id);
    if let Some(key_ids) = key_ids {
        query.push(" AND API_KEY_ID IN (");
        let mut separated = query.separated(", ");
        for key_id in key_ids {
            separated.push_bind(key_id);
        }
        separated.push_unseparated(")");
    }

    Ok(query
        .build()
        .fetch_all(&mut **tx)
        .await?
        .into_iter()
        .map(|row| row.get::<String, _>("ID"))
        .collect())
}

async fn delete_syncpoint_children(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    sync_point_ids: &[String],
) -> Result<(), sqlx::Error> {
    for sync_point_id in sync_point_ids {
        for sql in [
            "DELETE FROM SYNC_POINT_READLIST_REMOVED_SYNCED WHERE SYNC_POINT_ID = ?",
            "DELETE FROM SYNC_POINT_READLIST_BOOK WHERE SYNC_POINT_ID = ?",
            "DELETE FROM SYNC_POINT_READLIST WHERE SYNC_POINT_ID = ?",
            "DELETE FROM SYNC_POINT_BOOK_REMOVED_SYNCED WHERE SYNC_POINT_ID = ?",
            "DELETE FROM SYNC_POINT_BOOK WHERE SYNC_POINT_ID = ?",
        ] {
            sqlx::query(sql)
                .bind(sync_point_id)
                .execute(&mut **tx)
                .await?;
        }
    }

    Ok(())
}

pub async fn load_server_settings(
    settings_store: &ServerSettingsStore,
) -> Result<PersistedServerSettings, sqlx::Error> {
    let persisted = settings_store.load_map().await?;
    let normalized = normalize_server_settings(&persisted);

    if let Some(remember_me_key) = normalized.generated_remember_me_key.clone() {
        settings_store
            .apply_changes(&[("REMEMBER_ME_KEY".to_string(), Some(remember_me_key))])
            .await?;
    }

    Ok(normalized.settings)
}

pub fn load_remember_me_runtime_settings(database_file: &Path) -> Result<(String, u64), String> {
    let connection = Connection::open(database_file)
        .map_err(|error| format!("open server settings sqlite db: {error}"))?;
    let rows = load_server_settings_map_sync(&connection)?;
    let normalized = normalize_server_settings(&rows);

    if let Some(generated_key) = normalized.generated_remember_me_key.as_deref() {
        connection
            .execute(
                "INSERT INTO SERVER_SETTINGS(KEY, VALUE) VALUES(?, ?) ON CONFLICT(KEY) DO UPDATE SET VALUE = excluded.VALUE",
                params!["REMEMBER_ME_KEY", generated_key],
            )
            .map_err(|error| format!("persist generated remember-me key: {error}"))?;
    }

    let settings = normalized.settings;
    Ok((settings.remember_me_key, settings.remember_me_duration_days))
}

pub async fn apply_server_settings_changes(
    settings_store: &ServerSettingsStore,
    changes: &[(String, Option<String>)],
) -> Result<(), sqlx::Error> {
    settings_store.apply_changes(changes).await
}

pub fn generate_remember_me_key() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
    let raw = format!("{nanos:032x}{sequence:016x}");
    raw.chars().take(32).collect()
}

struct NormalizedServerSettings {
    settings: PersistedServerSettings,
    generated_remember_me_key: Option<String>,
}

fn normalize_server_settings(
    persisted: &BTreeMap<String, Option<String>>,
) -> NormalizedServerSettings {
    let generated_remember_me_key = (!persisted.contains_key("REMEMBER_ME_KEY")
        || persisted
            .get("REMEMBER_ME_KEY")
            .is_some_and(|value| value.as_deref().unwrap_or_default().trim().is_empty()))
    .then(generate_remember_me_key);
    let remember_me_key = parse_non_blank_string(persisted.get("REMEMBER_ME_KEY"))
        .or_else(|| generated_remember_me_key.clone())
        .expect("generated remember-me key should exist when persisted key is blank or missing");

    NormalizedServerSettings {
        settings: PersistedServerSettings {
            delete_empty_collections: parse_bool(persisted.get("DELETE_EMPTY_COLLECTIONS"), false),
            delete_empty_read_lists: parse_bool(persisted.get("DELETE_EMPTY_READLISTS"), false),
            remember_me_key,
            remember_me_duration_days: parse_u64(persisted.get("REMEMBER_ME_DURATION"))
                .unwrap_or(365),
            thumbnail_size: parse_thumbnail_size(persisted.get("THUMBNAIL_SIZE"))
                .unwrap_or("DEFAULT"),
            task_pool_size: parse_u64(persisted.get("TASK_POOL_SIZE")).unwrap_or(1),
            server_port: parse_u16(persisted.get("SERVER_PORT")),
            server_context_path: parse_string(persisted.get("SERVER_CONTEXT_PATH")),
            kobo_proxy: parse_bool(persisted.get("KOBO_PROXY"), false),
            kobo_port: parse_u16(persisted.get("KOBO_PORT")),
        },
        generated_remember_me_key,
    }
}

fn load_server_settings_map_sync(
    connection: &Connection,
) -> Result<BTreeMap<String, Option<String>>, String> {
    let mut statement = connection
        .prepare("SELECT KEY, VALUE FROM SERVER_SETTINGS")
        .map_err(|error| format!("prepare server settings read query: {error}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })
        .map_err(|error| format!("query server settings rows: {error}"))?;
    rows.collect::<Result<BTreeMap<_, _>, _>>()
        .map_err(|error| format!("collect server settings rows: {error}"))
}

pub async fn load_history_page(
    database_file: &Path,
    page: u64,
    size: u64,
    sorts: &[String],
) -> Result<Value, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;

    let total_elements = sqlx::query(
        r#"SELECT COUNT(*) AS COUNT
        FROM HISTORICAL_EVENT"#,
    )
    .fetch_one(&pool)
    .await?
    .get::<i64, _>("COUNT") as u64;

    let size = size.max(1);
    let offset = page.saturating_mul(size);

    let (order_by, sort_payload) = history_sort_details(sorts);
    let mut sql = String::from(
        r#"SELECT ID, TYPE, BOOK_ID, SERIES_ID, TIMESTAMP
        FROM HISTORICAL_EVENT"#,
    );
    if !order_by.is_empty() {
        sql.push_str(" ORDER BY ");
        sql.push_str(&order_by.join(", "));
    }
    sql.push_str(" LIMIT ? OFFSET ?");

    let events = sqlx::query(&sql)
        .bind((size.min(i64::MAX as u64)) as i64)
        .bind((offset.min(i64::MAX as u64)) as i64)
        .fetch_all(&pool)
        .await?
        .into_iter()
        .map(|row| PersistedHistoricalEvent {
            id: row.get::<String, _>("ID"),
            event_type: row.get::<String, _>("TYPE"),
            book_id: row.get::<Option<String>, _>("BOOK_ID"),
            series_id: row.get::<Option<String>, _>("SERIES_ID"),
            timestamp: row.get::<String, _>("TIMESTAMP"),
        })
        .collect::<Vec<_>>();

    let mut properties_by_id: HashMap<String, serde_json::Map<String, Value>> = HashMap::new();
    if !events.is_empty() {
        let placeholders = std::iter::repeat_n("?", events.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            r#"SELECT ID, "KEY" AS EVENT_KEY, VALUE
            FROM HISTORICAL_EVENT_PROPERTIES
            WHERE ID IN ({placeholders})"#,
        );

        let mut query = sqlx::query(&sql);
        for event in &events {
            query = query.bind(&event.id);
        }

        let property_rows = query.fetch_all(&pool).await?;
        for row in property_rows {
            let event_id = row.get::<String, _>("ID");
            let key = row.get::<String, _>("EVENT_KEY");
            let value = row.get::<String, _>("VALUE");
            properties_by_id
                .entry(event_id)
                .or_default()
                .insert(key, Value::String(value));
        }
    }

    let content = events
        .into_iter()
        .map(|event| {
            let properties = properties_by_id.remove(&event.id).unwrap_or_default();
            json!({
                "id": event.id,
                "type": event.event_type,
                "bookId": event.book_id,
                "seriesId": event.series_id,
                "timestamp": event.timestamp,
                "properties": properties,
            })
        })
        .collect::<Vec<_>>();

    let total_pages = if total_elements == 0 {
        0
    } else {
        total_elements.div_ceil(size)
    };
    let number_of_elements = content.len() as u64;
    let first = page == 0;
    let last = total_pages == 0 || page + 1 >= total_pages;

    Ok(json!({
        "content": content,
        "pageable": {
            "pageNumber": page,
            "pageSize": size,
            "sort": sort_payload.clone(),
            "offset": offset,
            "paged": true,
            "unpaged": false,
        },
        "last": last,
        "totalElements": total_elements,
        "totalPages": total_pages,
        "first": first,
        "size": size,
        "number": page,
        "sort": sort_payload,
        "numberOfElements": number_of_elements,
        "empty": number_of_elements == 0,
    }))
}

fn history_sort_details(sorts: &[String]) -> (Vec<String>, Value) {
    let order_by = history_order_by(sorts);
    let is_sorted = sorts.is_empty() || !order_by.is_empty();
    let payload = json!({
        "empty": !is_sorted,
        "sorted": is_sorted,
        "unsorted": !is_sorted,
    });

    (order_by, payload)
}

fn history_order_by(sorts: &[String]) -> Vec<String> {
    if sorts.is_empty() {
        return vec!["TIMESTAMP DESC".to_string()];
    }

    sorts
        .iter()
        .filter_map(|sort| history_sort_clause(sort))
        .collect()
}

fn history_sort_clause(sort: &str) -> Option<String> {
    let (property, direction) = match sort.split_once(',') {
        Some((property, direction)) => (property.trim(), direction.trim()),
        None => (sort.trim(), "asc"),
    };

    let field = match property {
        "type" => "TYPE",
        "bookId" => "BOOK_ID",
        "seriesId" => "SERIES_ID",
        "timestamp" => "TIMESTAMP",
        _ => return None,
    };
    let direction = if direction.eq_ignore_ascii_case("desc") {
        "DESC"
    } else {
        "ASC"
    };

    Some(format!("{field} {direction}"))
}

pub async fn load_client_settings_global(
    database_file: &Path,
    allow_unauthorized_only: bool,
) -> Result<Value, sqlx::Error> {
    load_client_settings_global_model(database_file, allow_unauthorized_only).await
}

pub async fn load_client_settings_user(
    database_file: &Path,
    user_id: &str,
) -> Result<Value, sqlx::Error> {
    load_client_settings_user_model(database_file, user_id).await
}

pub async fn upsert_client_settings_global(
    database_file: &Path,
    settings: &[(String, String, bool)],
) -> Result<(), sqlx::Error> {
    upsert_client_settings_global_model(database_file, settings).await
}

pub async fn upsert_client_settings_user(
    database_file: &Path,
    user_id: &str,
    settings: &[(String, String)],
) -> Result<(), sqlx::Error> {
    upsert_client_settings_user_model(database_file, user_id, settings).await
}

pub async fn delete_client_settings_global(
    database_file: &Path,
    keys: &[String],
) -> Result<(), sqlx::Error> {
    delete_client_settings_global_model(database_file, keys).await
}

pub async fn delete_client_settings_user(
    database_file: &Path,
    user_id: &str,
    keys: &[String],
) -> Result<(), sqlx::Error> {
    delete_client_settings_user_model(database_file, user_id, keys).await
}

fn parse_u64(value: Option<&Option<String>>) -> Option<u64> {
    value
        .and_then(|value| value.as_deref())
        .and_then(|value| value.trim().parse::<u64>().ok())
}

fn parse_u16(value: Option<&Option<String>>) -> Option<u16> {
    value
        .and_then(|value| value.as_deref())
        .and_then(|value| value.trim().parse::<u16>().ok())
}

fn parse_string(value: Option<&Option<String>>) -> Option<String> {
    value
        .and_then(|value| value.as_ref())
        .map(|value| value.to_string())
}

fn parse_non_blank_string(value: Option<&Option<String>>) -> Option<String> {
    value
        .and_then(|value| value.as_ref())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn parse_thumbnail_size(value: Option<&Option<String>>) -> Option<&'static str> {
    match value.and_then(|value| value.as_deref()) {
        Some("DEFAULT") => Some("DEFAULT"),
        Some("MEDIUM") => Some("MEDIUM"),
        Some("LARGE") => Some("LARGE"),
        Some("XLARGE") => Some("XLARGE"),
        _ => None,
    }
}

fn parse_bool(value: Option<&Option<String>>, default: bool) -> bool {
    value
        .and_then(|value| value.as_deref())
        .map(|value| value.trim().eq_ignore_ascii_case("true"))
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::connect_pool;
    use sqlx::Row;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    async fn create_test_db(case: &str) -> (PathBuf, sqlx::Pool<sqlx::Sqlite>) {
        let root = unique_temp_dir(case);
        fs::create_dir_all(&root).expect("temp root should be created");
        let db_path = root.join("operational-settings.sqlite");
        let pool = connect_pool(&db_path, 1)
            .await
            .expect("test db should open");
        crate::sqlite::setup::bootstrap_pool(&pool)
            .await
            .expect("test db should bootstrap main schema");

        sqlx::query("INSERT INTO USER (ID, EMAIL, PASSWORD) VALUES (?, ?, ?)")
            .bind("user-1")
            .bind("user-1@example.org")
            .bind("test-password")
            .execute(&pool)
            .await
            .expect("user row should be inserted");

        (db_path, pool)
    }

    async fn create_history_test_db(case: &str) -> (PathBuf, sqlx::Pool<sqlx::Sqlite>) {
        let root = unique_temp_dir(case);
        fs::create_dir_all(&root).expect("temp root should be created");
        let db_path = root.join("operational-settings-history.sqlite");
        let pool = connect_pool(&db_path, 1)
            .await
            .expect("test db should open");
        crate::sqlite::setup::bootstrap_pool(&pool)
            .await
            .expect("history test db should bootstrap main schema");

        (db_path, pool)
    }

    async fn create_syncpoint_test_db(case: &str) -> (PathBuf, sqlx::Pool<sqlx::Sqlite>) {
        let root = unique_temp_dir(case);
        fs::create_dir_all(&root).expect("temp root should be created");
        let db_path = root.join("operational-settings-syncpoints.sqlite");
        let pool = connect_pool(&db_path, 1)
            .await
            .expect("test db should open");
        crate::sqlite::setup::bootstrap_pool(&pool)
            .await
            .expect("sync point test db should bootstrap main schema");
        for user_id in ["user-1", "user-2"] {
            sqlx::query("INSERT INTO USER (ID, EMAIL, PASSWORD) VALUES (?, ?, ?)")
                .bind(user_id)
                .bind(format!("{user_id}@example.org"))
                .bind("test-password")
                .execute(&pool)
                .await
                .expect("sync point fixture user should be inserted");
        }

        (db_path, pool)
    }

    fn unique_temp_dir(case: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "komga-operational-settings-{case}-{nanos}-{}",
            std::process::id()
        ))
    }

    #[tokio::test]
    async fn load_client_settings_global_filters_unauthorized_only_without_injecting_defaults() {
        let (db_path, pool) = create_test_db("load-global").await;

        sqlx::query(
            "INSERT INTO CLIENT_SETTINGS_GLOBAL (KEY, VALUE, ALLOW_UNAUTHORIZED) VALUES (?, ?, ?)",
        )
        .bind("public.setting")
        .bind("public-value")
        .bind(true)
        .execute(&pool)
        .await
        .expect("public setting should be inserted");
        sqlx::query(
            "INSERT INTO CLIENT_SETTINGS_GLOBAL (KEY, VALUE, ALLOW_UNAUTHORIZED) VALUES (?, ?, ?)",
        )
        .bind("private.setting")
        .bind("private-value")
        .bind(false)
        .execute(&pool)
        .await
        .expect("private setting should be inserted");

        let all = load_client_settings_global(db_path.as_path(), false)
            .await
            .expect("global settings should load");
        let all = all
            .as_object()
            .expect("global settings should be an object");
        assert_eq!(all["public.setting"]["value"], "public-value");
        assert_eq!(all["private.setting"]["value"], "private-value");
        assert!(all.get("webui.oauth2.hide_login").is_none());

        let unauthorized_only = load_client_settings_global(db_path.as_path(), true)
            .await
            .expect("filtered global settings should load");
        let unauthorized_only = unauthorized_only
            .as_object()
            .expect("filtered global settings should be an object");
        assert_eq!(unauthorized_only["public.setting"]["value"], "public-value");
        assert!(unauthorized_only.get("private.setting").is_none());
        assert!(unauthorized_only.get("webui.oauth2.hide_login").is_none());
    }

    #[tokio::test]
    async fn client_settings_access_round_trips_global_and_user_changes() {
        let (db_path, _pool) = create_test_db("round-trip").await;

        upsert_client_settings_global(
            db_path.as_path(),
            &[
                (
                    "public.setting".to_string(),
                    "public-value".to_string(),
                    true,
                ),
                (
                    "private.setting".to_string(),
                    "private-value".to_string(),
                    false,
                ),
            ],
        )
        .await
        .expect("global settings should persist");
        upsert_client_settings_user(
            db_path.as_path(),
            "user-1",
            &[("reader.page_size".to_string(), "42".to_string())],
        )
        .await
        .expect("user settings should persist");

        let global = load_client_settings_global(db_path.as_path(), false)
            .await
            .expect("global settings should reload");
        let global = global
            .as_object()
            .expect("global settings should be an object");
        assert_eq!(global["public.setting"]["value"], "public-value");
        assert_eq!(global["private.setting"]["value"], "private-value");

        let user = load_client_settings_user(db_path.as_path(), "user-1")
            .await
            .expect("user settings should reload");
        let user = user.as_object().expect("user settings should be an object");
        assert_eq!(user["reader.page_size"]["value"], "42");

        delete_client_settings_global(db_path.as_path(), &["private.setting".to_string()])
            .await
            .expect("global setting should delete");
        delete_client_settings_user(
            db_path.as_path(),
            "user-1",
            &["reader.page_size".to_string()],
        )
        .await
        .expect("user setting should delete");

        let global = load_client_settings_global(db_path.as_path(), false)
            .await
            .expect("global settings should reload after delete");
        let global = global
            .as_object()
            .expect("global settings should be an object");
        assert!(global.get("private.setting").is_none());
        assert_eq!(global["public.setting"]["value"], "public-value");

        let user = load_client_settings_user(db_path.as_path(), "user-1")
            .await
            .expect("user settings should reload after delete");
        assert!(
            user.as_object()
                .expect("user settings should be an object")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn load_history_page_returns_expected_shape_and_order() {
        let (db_path, pool) = create_history_test_db("history-page").await;

        sqlx::query(
            "INSERT INTO HISTORICAL_EVENT (ID, TYPE, BOOK_ID, SERIES_ID, TIMESTAMP) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("event-1")
        .bind("BOOK_ADDED")
        .bind(Some("book-1"))
        .bind(None::<&str>)
        .bind("2024-01-01T00:00:00Z")
        .execute(&pool)
        .await
        .expect("older event should be inserted");
        sqlx::query(
            "INSERT INTO HISTORICAL_EVENT (ID, TYPE, BOOK_ID, SERIES_ID, TIMESTAMP) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("event-2")
        .bind("SERIES_ADDED")
        .bind(None::<&str>)
        .bind(Some("series-1"))
        .bind("2024-02-01T00:00:00Z")
        .execute(&pool)
        .await
        .expect("newer event should be inserted");
        sqlx::query(
            "INSERT INTO HISTORICAL_EVENT_PROPERTIES (ID, \"KEY\", VALUE) VALUES (?, ?, ?)",
        )
        .bind("event-2")
        .bind("source")
        .bind("scanner")
        .execute(&pool)
        .await
        .expect("event property should be inserted");

        let page = load_history_page(db_path.as_path(), 0, 20, &[])
            .await
            .expect("history page should load");
        let page = page.as_object().expect("history page should be an object");

        assert_eq!(page["totalElements"], 2);
        assert_eq!(page["totalPages"], 1);
        assert_eq!(page["number"], 0);
        assert_eq!(page["size"], 20);
        assert_eq!(page["numberOfElements"], 2);
        assert_eq!(page["first"], true);
        assert_eq!(page["last"], true);
        assert_eq!(page["empty"], false);

        let pageable = page["pageable"]
            .as_object()
            .expect("pageable should be an object");
        assert_eq!(pageable["pageNumber"], 0);
        assert_eq!(pageable["pageSize"], 20);
        assert_eq!(pageable["offset"], 0);
        assert_eq!(pageable["paged"], true);
        assert_eq!(pageable["unpaged"], false);

        let content = page["content"]
            .as_array()
            .expect("content should be an array");
        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["id"], "event-2");
        assert_eq!(content[0]["type"], "SERIES_ADDED");
        assert_eq!(content[0]["seriesId"], "series-1");
        assert_eq!(content[0]["properties"]["source"], "scanner");
        assert_eq!(content[1]["id"], "event-1");
        assert_eq!(content[1]["bookId"], "book-1");
        assert_eq!(content[1]["properties"], serde_json::json!({}));
    }

    #[tokio::test]
    async fn load_history_page_honors_supported_sort_override() {
        let (db_path, pool) = create_history_test_db("history-page-type-sort").await;

        sqlx::query(
            "INSERT INTO HISTORICAL_EVENT (ID, TYPE, BOOK_ID, SERIES_ID, TIMESTAMP) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("event-series")
        .bind("SERIES_ADDED")
        .bind(None::<&str>)
        .bind(Some("series-1"))
        .bind("2024-02-01T00:00:00Z")
        .execute(&pool)
        .await
        .expect("series event should be inserted");
        sqlx::query(
            "INSERT INTO HISTORICAL_EVENT (ID, TYPE, BOOK_ID, SERIES_ID, TIMESTAMP) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("event-book")
        .bind("BOOK_ADDED")
        .bind(Some("book-1"))
        .bind(None::<&str>)
        .bind("2024-01-01T00:00:00Z")
        .execute(&pool)
        .await
        .expect("book event should be inserted");

        let page = load_history_page(db_path.as_path(), 0, 20, &["type,asc".to_string()])
            .await
            .expect("history page with type sort should load");
        let content = page["content"]
            .as_array()
            .expect("history content should be an array");

        assert_eq!(content[0]["id"], "event-book");
        assert_eq!(content[1]["id"], "event-series");
    }

    #[tokio::test]
    async fn load_history_page_marks_unknown_sort_as_unsorted() {
        let (db_path, pool) = create_history_test_db("history-page-unknown-sort").await;

        sqlx::query(
            "INSERT INTO HISTORICAL_EVENT (ID, TYPE, BOOK_ID, SERIES_ID, TIMESTAMP) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("event-1")
        .bind("BOOK_ADDED")
        .bind(Some("book-1"))
        .bind(None::<&str>)
        .bind("2024-01-01T00:00:00Z")
        .execute(&pool)
        .await
        .expect("event should be inserted");

        let page = load_history_page(db_path.as_path(), 0, 20, &["unknown,asc".to_string()])
            .await
            .expect("history page with unknown sort should load");

        assert_eq!(
            page["sort"],
            serde_json::json!({
                "empty": true,
                "sorted": false,
                "unsorted": true,
            })
        );
        assert_eq!(
            page["pageable"]["sort"],
            serde_json::json!({
                "empty": true,
                "sorted": false,
                "unsorted": true,
            })
        );
    }

    #[tokio::test]
    async fn delete_syncpoints_by_user_removes_all_rows_for_user() {
        let (db_path, pool) = create_syncpoint_test_db("syncpoints-delete-all").await;

        for (id, user_id, key_id) in [
            ("sp-1", "user-1", "key-1"),
            ("sp-2", "user-1", "key-2"),
            ("sp-3", "user-2", "key-1"),
        ] {
            sqlx::query("INSERT INTO SYNC_POINT (ID, USER_ID, API_KEY_ID) VALUES (?, ?, ?)")
                .bind(id)
                .bind(user_id)
                .bind(key_id)
                .execute(&pool)
                .await
                .expect("sync point should be inserted");
        }

        delete_syncpoints_by_user(db_path.as_path(), "user-1")
            .await
            .expect("all sync points for user should delete");

        let rows = sqlx::query("SELECT ID FROM SYNC_POINT ORDER BY ID")
            .fetch_all(&pool)
            .await
            .expect("remaining sync points should load");
        let ids = rows
            .iter()
            .map(|row| row.get::<String, _>("ID"))
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["sp-3".to_string()]);
    }

    #[tokio::test]
    async fn delete_syncpoints_by_user_and_key_ids_removes_matching_key_set() {
        let (db_path, pool) = create_syncpoint_test_db("syncpoints-delete-many").await;

        for (id, user_id, key_id) in [
            ("sp-1", "user-1", "key-1"),
            ("sp-2", "user-1", "key-2"),
            ("sp-3", "user-1", "key-3"),
            ("sp-4", "user-2", "key-1"),
        ] {
            sqlx::query("INSERT INTO SYNC_POINT (ID, USER_ID, API_KEY_ID) VALUES (?, ?, ?)")
                .bind(id)
                .bind(user_id)
                .bind(key_id)
                .execute(&pool)
                .await
                .expect("sync point should be inserted");
        }

        delete_syncpoints_by_user_and_key_ids(
            db_path.as_path(),
            "user-1",
            &["key-1".to_string(), "key-3".to_string()],
        )
        .await
        .expect("matching sync points for key set should delete");

        let rows = sqlx::query("SELECT ID FROM SYNC_POINT ORDER BY ID")
            .fetch_all(&pool)
            .await
            .expect("remaining sync points should load");
        let ids = rows
            .iter()
            .map(|row| row.get::<String, _>("ID"))
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["sp-2".to_string(), "sp-4".to_string()]);
    }
}
