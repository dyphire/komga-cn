use std::collections::BTreeMap;
use std::path::PathBuf;

use komga_application::operational::{
    PersistedServerSettings, ServerSettingChange, ServerSettingsPort,
};
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

#[async_trait::async_trait]
impl ServerSettingsPort for ServerSettingsStore {
    async fn load_map(&self) -> anyhow::Result<BTreeMap<String, Option<String>>> {
        let context = self
            .context()
            .await
            .map_err(|e| anyhow::anyhow!(e).context("server settings context"))?;
        let rows = sqlx::query(
            r#"
            SELECT KEY, VALUE
            FROM SERVER_SETTINGS
        "#,
        )
        .fetch_all(context.pool())
        .await
        .map_err(|e| anyhow::anyhow!(e).context("load server settings map"))?
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

    async fn load_settings(&self) -> anyhow::Result<PersistedServerSettings> {
        crate::operational_access::load_server_settings(self)
            .await
            .map_err(|e| anyhow::anyhow!(e).context("load server settings"))
    }

    async fn apply_changes(&self, changes: &[ServerSettingChange]) -> anyhow::Result<()> {
        if changes.is_empty() {
            return Ok(());
        }

        let context = self
            .context()
            .await
            .map_err(|e| anyhow::anyhow!(e).context("server settings context"))?;
        for change in changes {
            match &change.value {
                Some(value) => {
                    sqlx::query(
                        r#"
                        INSERT INTO SERVER_SETTINGS(KEY, VALUE)
                        VALUES(?, ?)
                        ON CONFLICT(KEY) DO UPDATE
                        SET VALUE = excluded.VALUE
                    "#,
                    )
                    .bind(&change.key)
                    .bind(value)
                    .execute(context.pool())
                    .await
                    .map_err(|e| {
                        anyhow::anyhow!(e).context(format!("apply server setting {}: ", change.key))
                    })?;
                }
                None => {
                    sqlx::query(
                        r#"
                        DELETE FROM SERVER_SETTINGS
                        WHERE KEY = ?
                    "#,
                    )
                    .bind(&change.key)
                    .execute(context.pool())
                    .await
                    .map_err(|e| {
                        anyhow::anyhow!(e)
                            .context(format!("delete server setting {}: ", change.key))
                    })?;
                }
            }
        }
        Ok(())
    }
}
