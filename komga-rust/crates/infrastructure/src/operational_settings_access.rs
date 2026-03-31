use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::sqlite::connect_pool;
use crate::sqlite::read_models::{
    load_client_settings_global as load_client_settings_global_model,
    load_client_settings_user as load_client_settings_user_model,
};
use crate::sqlite::write_models::{
    delete_client_settings_global as delete_client_settings_global_model,
    delete_client_settings_user as delete_client_settings_user_model,
    upsert_client_settings_global as upsert_client_settings_global_model,
    upsert_client_settings_user as upsert_client_settings_user_model,
};
use serde_json::{Value, json};
use sqlx::Row;

use crate::ServerSettingsStore;

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

pub async fn delete_syncpoints_by_user_and_key_id(
    database_file: &Path,
    user_id: &str,
    key_id: &str,
) -> Result<(), sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    sqlx::query(
        "DELETE \
         FROM SYNC_POINT \
         WHERE USER_ID = ? \
         AND API_KEY_ID = ?",
    )
    .bind(user_id)
    .bind(key_id)
    .execute(&pool)
    .await?;
    Ok(())
}

pub async fn load_server_settings(
    settings_store: &ServerSettingsStore,
) -> Result<PersistedServerSettings, sqlx::Error> {
    let persisted = settings_store.load_map().await?;

    let remember_me_key = parse_non_blank_string(persisted.get("REMEMBER_ME_KEY"))
        .unwrap_or_else(generate_remember_me_key);

    if !persisted.contains_key("REMEMBER_ME_KEY")
        || persisted
            .get("REMEMBER_ME_KEY")
            .is_some_and(|value| value.as_deref().unwrap_or_default().trim().is_empty())
    {
        settings_store
            .apply_changes(&[("REMEMBER_ME_KEY".to_string(), Some(remember_me_key.clone()))])
            .await?;
    }

    Ok(PersistedServerSettings {
        delete_empty_collections: parse_bool(persisted.get("DELETE_EMPTY_COLLECTIONS"), false),
        delete_empty_read_lists: parse_bool(persisted.get("DELETE_EMPTY_READLISTS"), false),
        remember_me_key,
        remember_me_duration_days: parse_u64(persisted.get("REMEMBER_ME_DURATION")).unwrap_or(365),
        thumbnail_size: parse_thumbnail_size(persisted.get("THUMBNAIL_SIZE")).unwrap_or("DEFAULT"),
        task_pool_size: parse_u64(persisted.get("TASK_POOL_SIZE")).unwrap_or(1),
        server_port: parse_u16(persisted.get("SERVER_PORT")),
        server_context_path: parse_string(persisted.get("SERVER_CONTEXT_PATH")),
        kobo_proxy: parse_bool(persisted.get("KOBO_PROXY"), false),
        kobo_port: parse_u16(persisted.get("KOBO_PORT")),
    })
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

pub async fn load_history_page(
    database_file: &Path,
    page: u64,
    size: u64,
) -> Result<Value, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;

    let total_elements = sqlx::query(
        "SELECT COUNT(*) AS COUNT \
         FROM HISTORICAL_EVENT",
    )
    .fetch_one(&pool)
    .await?
    .get::<i64, _>("COUNT") as u64;

    let size = size.max(1);
    let offset = page.saturating_mul(size);

    let events = sqlx::query(
        "SELECT ID, TYPE, BOOK_ID, SERIES_ID, TIMESTAMP \
         FROM HISTORICAL_EVENT \
         ORDER BY TIMESTAMP DESC, ID DESC \
         LIMIT ? \
         OFFSET ?",
    )
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
            "SELECT ID, \"KEY\" AS EVENT_KEY, VALUE \
             FROM HISTORICAL_EVENT_PROPERTIES \
             WHERE ID IN ({placeholders})",
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
        (total_elements + size - 1) / size
    };
    let number_of_elements = content.len() as u64;
    let first = page == 0;
    let last = total_pages == 0 || page + 1 >= total_pages;

    Ok(json!({
        "content": content,
        "pageable": {
            "pageNumber": page,
            "pageSize": size,
            "sort": {
                "empty": false,
                "sorted": true,
                "unsorted": false,
            },
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
        "sort": {
            "empty": false,
            "sorted": true,
            "unsorted": false,
        },
        "numberOfElements": number_of_elements,
        "empty": number_of_elements == 0,
    }))
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

        sqlx::query("CREATE TABLE IF NOT EXISTS USER (ID varchar NOT NULL PRIMARY KEY)")
            .execute(&pool)
            .await
            .expect("user table should be created");
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS CLIENT_SETTINGS_GLOBAL (KEY varchar NOT NULL PRIMARY KEY, VALUE varchar NOT NULL, ALLOW_UNAUTHORIZED boolean NOT NULL DEFAULT 0)",
        )
        .execute(&pool)
        .await
        .expect("global client settings table should be created");
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS CLIENT_SETTINGS_USER (USER_ID varchar NOT NULL, KEY varchar NOT NULL, VALUE varchar NOT NULL, FOREIGN KEY (USER_ID) REFERENCES USER (ID), PRIMARY KEY (KEY, USER_ID))",
        )
        .execute(&pool)
        .await
        .expect("user client settings table should be created");

        sqlx::query("INSERT INTO USER (ID) VALUES (?)")
            .bind("user-1")
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

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS HISTORICAL_EVENT (ID varchar NOT NULL PRIMARY KEY, TYPE varchar NOT NULL, BOOK_ID varchar, SERIES_ID varchar, TIMESTAMP varchar NOT NULL)",
        )
        .execute(&pool)
        .await
        .expect("historical event table should be created");
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS HISTORICAL_EVENT_PROPERTIES (ID varchar NOT NULL, \"KEY\" varchar NOT NULL, VALUE varchar NOT NULL)",
        )
        .execute(&pool)
        .await
        .expect("historical event properties table should be created");

        (db_path, pool)
    }

    async fn create_syncpoint_test_db(case: &str) -> (PathBuf, sqlx::Pool<sqlx::Sqlite>) {
        let root = unique_temp_dir(case);
        fs::create_dir_all(&root).expect("temp root should be created");
        let db_path = root.join("operational-settings-syncpoints.sqlite");
        let pool = connect_pool(&db_path, 1)
            .await
            .expect("test db should open");

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS SYNC_POINT (ID varchar NOT NULL PRIMARY KEY, USER_ID varchar NOT NULL, API_KEY_ID varchar NOT NULL)",
        )
        .execute(&pool)
        .await
        .expect("sync point table should be created");

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
    async fn load_client_settings_global_filters_unauthorized_only_and_keeps_default() {
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
        assert_eq!(all["webui.oauth2.hide_login"]["value"], "false");

        let unauthorized_only = load_client_settings_global(db_path.as_path(), true)
            .await
            .expect("filtered global settings should load");
        let unauthorized_only = unauthorized_only
            .as_object()
            .expect("filtered global settings should be an object");
        assert_eq!(unauthorized_only["public.setting"]["value"], "public-value");
        assert!(unauthorized_only.get("private.setting").is_none());
        assert_eq!(
            unauthorized_only["webui.oauth2.hide_login"]["value"],
            "false"
        );
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

        let page = load_history_page(db_path.as_path(), 0, 20)
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
    async fn delete_syncpoints_by_user_and_key_id_removes_only_matching_rows() {
        let (db_path, pool) = create_syncpoint_test_db("syncpoints-delete").await;

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

        delete_syncpoints_by_user_and_key_id(db_path.as_path(), "user-1", "key-1")
            .await
            .expect("matching sync point should delete");

        let rows = sqlx::query("SELECT ID FROM SYNC_POINT ORDER BY ID")
            .fetch_all(&pool)
            .await
            .expect("remaining sync points should load");
        let ids = rows
            .iter()
            .map(|row| row.get::<String, _>("ID"))
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["sp-2".to_string(), "sp-3".to_string()]);
    }
}
