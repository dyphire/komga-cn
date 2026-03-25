use std::collections::BTreeMap;
use std::path::PathBuf;

use sqlx::Row;

use crate::context::SqlitePersistenceContext;
use crate::sqlite::connect_persistence_context;

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
                connect_persistence_context(database_file, 1).await
            }
            StoreBackend::Context(context) => Ok(context.clone()),
        }
    }

    pub async fn load_map(&self) -> Result<BTreeMap<String, Option<String>>, sqlx::Error> {
        let context = self.context().await?;
        let rows = sqlx::query("SELECT KEY, VALUE FROM SERVER_SETTINGS")
            .fetch_all(context.pool())
            .await?
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

    pub async fn apply_changes(
        &self,
        changes: &[(String, Option<String>)],
    ) -> Result<(), sqlx::Error> {
        if changes.is_empty() {
            return Ok(());
        }

        let context = self.context().await?;
        for (key, value) in changes {
            match value {
                Some(value) => {
                    sqlx::query(
                        "INSERT INTO SERVER_SETTINGS(KEY, VALUE) VALUES(?, ?)\
                         ON CONFLICT(KEY) DO UPDATE SET VALUE=excluded.VALUE",
                    )
                    .bind(key)
                    .bind(value)
                    .execute(context.pool())
                    .await?;
                }
                None => {
                    sqlx::query("DELETE FROM SERVER_SETTINGS WHERE KEY = ?")
                        .bind(key)
                        .execute(context.pool())
                        .await?;
                }
            }
        }
        Ok(())
    }
}
