use std::collections::BTreeMap;
use std::path::PathBuf;

use async_trait::async_trait;
use komga_application::operational::{PersistedServerSettings, ServerSettingsPort};
use sqlx::Row;

use crate::context::SqlitePersistenceContext;
use crate::sqlite::connect_main_write_context;

#[derive(Clone)]
pub struct ServerSettingsStore {
    backend: StoreBackend,
}

#[derive(Clone)]
enum StoreBackend {
    DatabaseFile(PathBuf),
    Context(SqlitePersistenceContext),
}

impl ServerSettingsStore {
    pub fn new(database_file: PathBuf) -> Self {
        Self {
            backend: StoreBackend::DatabaseFile(database_file),
        }
    }

    pub fn from_context(context: SqlitePersistenceContext) -> Self {
        Self {
            backend: StoreBackend::Context(context),
        }
    }

    async fn context(&self) -> Result<SqlitePersistenceContext, sqlx::Error> {
        match &self.backend {
            StoreBackend::DatabaseFile(database_file) => {
                connect_main_write_context(database_file).await
            }
            StoreBackend::Context(context) => Ok(context.clone()),
        }
    }
}

#[async_trait]
impl ServerSettingsPort for ServerSettingsStore {
    async fn load_map(&self) -> Result<BTreeMap<String, Option<String>>, String> {
        let context = self
            .context()
            .await
            .map_err(|e| format!("server settings context: {e}"))?;
        let rows = sqlx::query(
            r#"
            SELECT KEY, VALUE
            FROM SERVER_SETTINGS
        "#,
        )
        .fetch_all(context.pool())
        .await
        .map_err(|e| format!("load server settings map: {e}"))?
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("KEY"),
                row.get::<Option<String>, _>("VALUE"),
            )
        })
        .collect::<BTreeMap<_, _>>();
        Ok(rows)
    }

    async fn load_settings(&self) -> Result<PersistedServerSettings, String> {
        crate::operational_access::load_server_settings(self)
            .await
            .map_err(|e| format!("load server settings: {e}"))
    }

    async fn apply_changes(&self, changes: &[(String, Option<String>)]) -> Result<(), String> {
        if changes.is_empty() {
            return Ok(());
        }

        let context = self
            .context()
            .await
            .map_err(|e| format!("server settings context: {e}"))?;
        for (key, value) in changes {
            match value {
                Some(value) => {
                    sqlx::query(
                        r#"
                        INSERT INTO SERVER_SETTINGS(KEY, VALUE)
                        VALUES(?, ?)
                        ON CONFLICT(KEY) DO UPDATE
                        SET VALUE = excluded.VALUE
                    "#,
                    )
                    .bind(key)
                    .bind(value)
                    .execute(context.pool())
                    .await
                    .map_err(|e| format!("apply server setting {key}: {e}"))?;
                }
                None => {
                    sqlx::query(
                        r#"
                        DELETE FROM SERVER_SETTINGS
                        WHERE KEY = ?
                    "#,
                    )
                    .bind(key)
                    .execute(context.pool())
                    .await
                    .map_err(|e| format!("delete server setting {key}: {e}"))?;
                }
            }
        }
        Ok(())
    }
}
