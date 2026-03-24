use std::collections::BTreeMap;
use std::path::PathBuf;

use sqlx::Row;

use crate::sqlite::connect_persistence_context;

#[derive(Clone)]
pub struct ServerSettingsStore {
    database_file: PathBuf,
}

impl ServerSettingsStore {
    pub fn new(database_file: PathBuf) -> Self {
        Self { database_file }
    }

    pub async fn load_map(&self) -> Result<BTreeMap<String, Option<String>>, sqlx::Error> {
        let context = connect_persistence_context(&self.database_file, 1).await?;
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
        context.pool().close().await;
        Ok(rows)
    }

    pub async fn apply_changes(
        &self,
        changes: &[(String, Option<String>)],
    ) -> Result<(), sqlx::Error> {
        if changes.is_empty() {
            return Ok(());
        }

        let context = connect_persistence_context(&self.database_file, 1).await?;
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
        context.pool().close().await;
        Ok(())
    }
}
