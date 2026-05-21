use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use komga_application::operational::{PersistedServerSettings, ServerSettingsPort};
use rusqlite::{Connection, params};

use crate::sqlite::write_models::server_settings::ServerSettingsStore;

pub async fn load_server_settings(
    settings_store: &ServerSettingsStore,
) -> Result<PersistedServerSettings, String> {
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

fn generate_remember_me_key() -> String {
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
