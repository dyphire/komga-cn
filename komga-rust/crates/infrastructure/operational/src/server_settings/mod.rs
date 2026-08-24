use anyhow::Context;
use std::collections::BTreeMap;
use std::path::Path;

use komga_application::operational::{
    PersistedServerSettings, ServerSettingChange, ServerSettingsPort, ThumbnailSize,
    is_valid_server_context_path,
};
use rusqlite::{Connection, params};

use komga_infrastructure_base::random_hex_token;

mod store;

pub use store::ServerSettingsStore;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RememberMeRuntimeSettings {
    pub key: String,
    pub duration_days: u64,
}

pub(crate) async fn load_server_settings(
    settings_store: &ServerSettingsStore,
) -> anyhow::Result<PersistedServerSettings> {
    let persisted = settings_store.load_map().await?;
    let normalized = normalize_server_settings(&persisted)?;

    if let Some(remember_me_key) = normalized.generated_remember_me_key.clone() {
        settings_store
            .apply_changes(&[ServerSettingChange::set("REMEMBER_ME_KEY", remember_me_key)])
            .await?;
    }

    Ok(normalized.settings)
}

pub fn load_remember_me_runtime_settings(
    database_file: &Path,
) -> anyhow::Result<RememberMeRuntimeSettings> {
    let connection = Connection::open(database_file).context("open server settings sqlite db")?;
    let rows = load_server_settings_map_sync(&connection)?;
    let normalized = normalize_server_settings(&rows)?;

    if let Some(generated_key) = normalized.generated_remember_me_key.as_deref() {
        connection
            .execute(
                "INSERT INTO SERVER_SETTINGS(KEY, VALUE) VALUES(?, ?) ON CONFLICT(KEY) DO UPDATE SET VALUE = excluded.VALUE",
                params!["REMEMBER_ME_KEY", generated_key],
            )
            .context("persist generated remember-me key")?;
    }

    let settings = normalized.settings;
    Ok(RememberMeRuntimeSettings {
        key: settings.remember_me_key,
        duration_days: settings.remember_me_duration_days,
    })
}

fn generate_remember_me_key() -> String {
    random_hex_token(32)
}

struct NormalizedServerSettings {
    settings: PersistedServerSettings,
    generated_remember_me_key: Option<String>,
}

fn normalize_server_settings(
    persisted: &BTreeMap<String, Option<String>>,
) -> anyhow::Result<NormalizedServerSettings> {
    let generated_remember_me_key = (!persisted.contains_key("REMEMBER_ME_KEY")
        || persisted
            .get("REMEMBER_ME_KEY")
            .is_some_and(|value| value.as_deref().unwrap_or_default().trim().is_empty()))
    .then(generate_remember_me_key);
    let remember_me_key = parse_non_blank_string(persisted.get("REMEMBER_ME_KEY"))
        .or_else(|| generated_remember_me_key.clone())
        .expect("generated remember-me key should exist when persisted key is blank or missing");

    Ok(NormalizedServerSettings {
        settings: PersistedServerSettings {
            delete_empty_collections: parse_bool(persisted, "DELETE_EMPTY_COLLECTIONS", false)?,
            delete_empty_read_lists: parse_bool(persisted, "DELETE_EMPTY_READLISTS", false)?,
            remember_me_key,
            remember_me_duration_days: parse_positive_u64(persisted, "REMEMBER_ME_DURATION", 365)?,
            thumbnail_size: parse_thumbnail_size(persisted, "THUMBNAIL_SIZE")?.unwrap_or_default(),
            task_pool_size: parse_positive_u64(persisted, "TASK_POOL_SIZE", 1)?,
            server_port: parse_port(persisted, "SERVER_PORT")?,
            server_context_path: parse_server_context_path(persisted, "SERVER_CONTEXT_PATH")?,
            kobo_proxy: parse_bool(persisted, "KOBO_PROXY", false)?,
            kobo_port: parse_port(persisted, "KOBO_PORT")?,
            kepubify_path: parse_non_blank_string(persisted.get("KEPUBIFY_PATH")),
        },
        generated_remember_me_key,
    })
}

fn load_server_settings_map_sync(
    connection: &Connection,
) -> anyhow::Result<BTreeMap<String, Option<String>>> {
    let mut statement = connection
        .prepare("SELECT KEY, VALUE FROM SERVER_SETTINGS")
        .context("prepare server settings read query")?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })
        .context("query server settings rows")?;
    rows.collect::<Result<BTreeMap<_, _>, _>>()
        .context("collect server settings rows")
}

fn optional_setting<'a>(
    persisted: &'a BTreeMap<String, Option<String>>,
    key: &str,
) -> Option<&'a str> {
    persisted
        .get(key)
        .and_then(|value| value.as_deref())
        .map(str::trim)
}

fn raw_optional_setting<'a>(
    persisted: &'a BTreeMap<String, Option<String>>,
    key: &str,
) -> Option<&'a str> {
    persisted.get(key).and_then(|value| value.as_deref())
}

fn parse_positive_u64(
    persisted: &BTreeMap<String, Option<String>>,
    key: &str,
    default: u64,
) -> anyhow::Result<u64> {
    let Some(value) = optional_setting(persisted, key) else {
        return Ok(default);
    };
    let parsed = value
        .parse::<u64>()
        .map_err(|_| anyhow::Error::msg(invalid_server_setting(key, value)))?;
    if parsed == 0 {
        return Err(anyhow::Error::msg(invalid_server_setting(key, value)));
    }
    Ok(parsed)
}

fn parse_port(
    persisted: &BTreeMap<String, Option<String>>,
    key: &str,
) -> anyhow::Result<Option<u16>> {
    let Some(value) = optional_setting(persisted, key) else {
        return Ok(None);
    };
    let parsed = value
        .parse::<u16>()
        .map_err(|_| anyhow::Error::msg(invalid_server_setting(key, value)))?;
    if parsed == 0 {
        return Err(anyhow::Error::msg(invalid_server_setting(key, value)));
    }
    Ok(Some(parsed))
}

fn parse_server_context_path(
    persisted: &BTreeMap<String, Option<String>>,
    key: &str,
) -> anyhow::Result<Option<String>> {
    let Some(value) = raw_optional_setting(persisted, key) else {
        return Ok(None);
    };
    if !is_valid_server_context_path(value) {
        return Err(anyhow::Error::msg(invalid_server_setting(key, value)));
    }
    Ok(Some(value.to_string()))
}

fn parse_non_blank_string(value: Option<&Option<String>>) -> Option<String> {
    value
        .and_then(|value| value.as_ref())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn parse_thumbnail_size(
    persisted: &BTreeMap<String, Option<String>>,
    key: &str,
) -> anyhow::Result<Option<ThumbnailSize>> {
    optional_setting(persisted, key)
        .map(|value| {
            ThumbnailSize::parse(value)
                .ok_or_else(|| anyhow::Error::msg(invalid_server_setting(key, value)))
        })
        .transpose()
}

fn parse_bool(
    persisted: &BTreeMap<String, Option<String>>,
    key: &str,
    default: bool,
) -> anyhow::Result<bool> {
    let Some(value) = optional_setting(persisted, key) else {
        return Ok(default);
    };
    if value.eq_ignore_ascii_case("true") {
        return Ok(true);
    }
    if value == "1" {
        return Ok(true);
    }
    if value.eq_ignore_ascii_case("false") {
        return Ok(false);
    }
    if value == "0" {
        return Ok(false);
    }
    Err(anyhow::Error::msg(invalid_server_setting(key, value)))
}

fn invalid_server_setting(key: &str, value: &str) -> String {
    format!("invalid persisted server setting {key}: {value}")
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use komga_application::operational::ServerSettingsPort;

    use super::*;

    #[tokio::test]
    async fn load_settings_rejects_invalid_persisted_server_settings() {
        let cases = [
            ("invalid-thumbnail-size", "THUMBNAIL_SIZE", "SMALL"),
            ("invalid-task-pool-size", "TASK_POOL_SIZE", "0"),
            (
                "invalid-remember-me-duration",
                "REMEMBER_ME_DURATION",
                "never",
            ),
            ("invalid-server-port", "SERVER_PORT", "0"),
            ("invalid-kobo-port", "KOBO_PORT", "70000"),
            (
                "invalid-delete-empty-collections",
                "DELETE_EMPTY_COLLECTIONS",
                "maybe",
            ),
            ("invalid-kobo-proxy", "KOBO_PROXY", "yes"),
            (
                "invalid-server-context-path",
                "SERVER_CONTEXT_PATH",
                "/komga/",
            ),
        ];

        for (case_name, key, value) in cases {
            let error = load_settings_with_seeded_change(case_name, key, value)
                .await
                .expect_err("invalid persisted setting should fail settings load");

            assert!(format!("{error:#}").contains(key), "{error:#}");
        }
    }

    #[tokio::test]
    async fn load_settings_accepts_valid_persisted_server_settings() {
        let root = unique_fixture_root("valid-settings");
        std::fs::create_dir_all(&root).expect("fixture root should be created");
        let database_file = root.join("main.db");
        let store = ServerSettingsStore::new(database_file.clone());
        store
            .load_map()
            .await
            .expect("schema bootstrap should succeed");
        store
            .apply_changes(&[
                ServerSettingChange::set("DELETE_EMPTY_COLLECTIONS", "1"),
                ServerSettingChange::set("DELETE_EMPTY_READLISTS", "false"),
                ServerSettingChange::set("REMEMBER_ME_DURATION", "30"),
                ServerSettingChange::set("THUMBNAIL_SIZE", "LARGE"),
                ServerSettingChange::set("TASK_POOL_SIZE", "2"),
                ServerSettingChange::set("SERVER_PORT", "25601"),
                ServerSettingChange::set("SERVER_CONTEXT_PATH", "/komga"),
                ServerSettingChange::set("KOBO_PROXY", "true"),
                ServerSettingChange::set("KOBO_PORT", "8085"),
                ServerSettingChange::set("KEPUBIFY_PATH", "/usr/bin/kepubify"),
            ])
            .await
            .expect("valid persisted settings should be seeded");

        let settings = store
            .load_settings()
            .await
            .expect("valid persisted settings should load");

        assert!(settings.delete_empty_collections);
        assert!(!settings.delete_empty_read_lists);
        assert_eq!(settings.remember_me_duration_days, 30);
        assert_eq!(settings.thumbnail_size, ThumbnailSize::Large);
        assert_eq!(settings.task_pool_size, 2);
        assert_eq!(settings.server_port, Some(25601));
        assert_eq!(settings.server_context_path.as_deref(), Some("/komga"));
        assert!(settings.kobo_proxy);
        assert_eq!(settings.kobo_port, Some(8085));
        assert_eq!(settings.kepubify_path.as_deref(), Some("/usr/bin/kepubify"));

        cleanup_fixture(root, database_file).await;
    }

    #[tokio::test]
    async fn load_settings_generates_and_persists_32_byte_hex_remember_me_key() {
        let root = unique_fixture_root("generated-remember-me-key");
        std::fs::create_dir_all(&root).expect("fixture root should be created");
        let database_file = root.join("main.db");
        let store = ServerSettingsStore::new(database_file.clone());
        store
            .load_map()
            .await
            .expect("schema bootstrap should succeed");
        store
            .apply_changes(&[ServerSettingChange::delete("REMEMBER_ME_KEY")])
            .await
            .expect("remember-me key should be removed before generation check");

        let settings = store
            .load_settings()
            .await
            .expect("missing remember-me key should be generated");

        assert_eq!(settings.remember_me_key.len(), 64);
        assert!(
            settings
                .remember_me_key
                .chars()
                .all(|ch| ch.is_ascii_hexdigit())
        );

        let persisted = store
            .load_map()
            .await
            .expect("persisted generated remember-me key should be readable");
        assert_eq!(
            persisted
                .get("REMEMBER_ME_KEY")
                .and_then(|value| value.as_deref()),
            Some(settings.remember_me_key.as_str())
        );

        cleanup_fixture(root, database_file).await;
    }

    async fn load_settings_with_seeded_change(
        case_name: &str,
        key: &str,
        value: &str,
    ) -> anyhow::Result<PersistedServerSettings> {
        let root = unique_fixture_root(case_name);
        std::fs::create_dir_all(&root).expect("fixture root should be created");
        let database_file = root.join("main.db");
        let store = ServerSettingsStore::new(database_file.clone());
        store
            .load_map()
            .await
            .expect("schema bootstrap should succeed");
        store
            .apply_changes(&[ServerSettingChange::set(key, value)])
            .await
            .expect("invalid persisted setting should be seeded");

        let result = store.load_settings().await;

        cleanup_fixture(root, database_file).await;
        result
    }

    async fn cleanup_fixture(root: std::path::PathBuf, database_file: std::path::PathBuf) {
        for pool in komga_infrastructure_base::evict_shared_pools_for_paths(&[database_file]) {
            pool.close().await;
        }
        std::fs::remove_dir_all(root).expect("fixture root should be removed");
    }

    fn unique_fixture_root(case_name: &str) -> std::path::PathBuf {
        let unique_suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "komga-rust-server-settings-{case_name}-{}-{unique_suffix}",
            std::process::id()
        ))
    }
}
