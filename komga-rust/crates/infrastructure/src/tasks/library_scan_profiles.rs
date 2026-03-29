use std::path::Path;

use sqlx::Row;

use crate::sqlite::connect_pool;

#[derive(Clone, Debug)]
pub struct PersistedLibraryScanProfile {
    pub library_id: String,
    pub scan_startup: bool,
    pub scan_interval: String,
}

pub fn load_persisted_library_scan_profiles(
    database_file: &Path,
) -> Vec<PersistedLibraryScanProfile> {
    if !database_file.exists() {
        return Vec::new();
    }

    let database_file = database_file.to_path_buf();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build();
        let Ok(runtime) = runtime else {
            return Vec::new();
        };

        runtime.block_on(async move {
            let Ok(pool) = connect_pool(database_file.as_path(), 1).await else {
                return Vec::new();
            };

            let rows = sqlx::query(
                "SELECT ID, SCAN_STARTUP, SCAN_INTERVAL\n                 FROM LIBRARY\n                 ORDER BY ID ASC",
            )
            .fetch_all(&pool)
            .await;

            let Ok(rows) = rows else {
                return Vec::new();
            };

            rows.into_iter()
                .map(|row| PersistedLibraryScanProfile {
                    library_id: row.get::<String, _>("ID"),
                    scan_startup: row.get::<bool, _>("SCAN_STARTUP"),
                    scan_interval: row.get::<String, _>("SCAN_INTERVAL"),
                })
                .collect::<Vec<_>>()
        })
    })
    .join()
    .unwrap_or_default()
}

pub fn load_persisted_library_ids(database_file: &Path) -> Vec<String> {
    if !database_file.exists() {
        return Vec::new();
    }

    let database_file = database_file.to_path_buf();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build();
        let Ok(runtime) = runtime else {
            return Vec::new();
        };

        runtime.block_on(async move {
            let Ok(pool) = connect_pool(database_file.as_path(), 1).await else {
                return Vec::new();
            };

            let rows = sqlx::query(
                "SELECT ID \
                 FROM LIBRARY",
            )
            .fetch_all(&pool)
            .await;

            let Ok(rows) = rows else {
                return Vec::new();
            };

            rows.into_iter()
                .map(|row| row.get::<String, _>("ID"))
                .collect::<Vec<_>>()
        })
    })
    .join()
    .unwrap_or_default()
}
