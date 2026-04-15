use std::path::Path;

use komga_application::task_processing::LibraryScanProfile;
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
) -> Result<Vec<PersistedLibraryScanProfile>, String> {
    if !database_file.exists() {
        return Ok(Vec::new());
    }

    let database_file = database_file.to_path_buf();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build();
        let runtime = runtime.map_err(|error| format!("build scan profile runtime: {error}"))?;

        runtime.block_on(async move {
            let pool = connect_pool(database_file.as_path(), 1)
                .await
                .map_err(|error| format!("open scan profile db: {error}"))?;

            let rows = sqlx::query(
                "SELECT ID, SCAN_STARTUP, SCAN_INTERVAL \
                 FROM LIBRARY \
                 ORDER BY ID ASC",
            )
            .fetch_all(&pool)
            .await
            .map_err(|error| format!("query scan profiles: {error}"))?;

            Ok(rows
                .into_iter()
                .map(|row| PersistedLibraryScanProfile {
                    library_id: row.get::<String, _>("ID"),
                    scan_startup: row.get::<bool, _>("SCAN_STARTUP"),
                    scan_interval: row.get::<String, _>("SCAN_INTERVAL"),
                })
                .collect::<Vec<_>>())
        })
    })
    .join()
    .map_err(|_| "join scan profile loader thread".to_string())?
}

pub fn load_library_scan_profiles(database_file: &Path) -> Result<Vec<LibraryScanProfile>, String> {
    load_persisted_library_scan_profiles(database_file).map(|profiles| {
        profiles
            .into_iter()
            .map(|profile| LibraryScanProfile {
                library_id: profile.library_id,
                scan_startup: profile.scan_startup,
                scan_interval: profile.scan_interval,
            })
            .collect()
    })
}

pub fn load_persisted_library_ids(database_file: &Path) -> Result<Vec<String>, String> {
    if !database_file.exists() {
        return Ok(Vec::new());
    }

    let database_file = database_file.to_path_buf();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build();
        let runtime = runtime.map_err(|error| format!("build library id runtime: {error}"))?;

        runtime.block_on(async move {
            let pool = connect_pool(database_file.as_path(), 1)
                .await
                .map_err(|error| format!("open library id db: {error}"))?;

            let rows = sqlx::query(
                "SELECT ID \
                 FROM LIBRARY",
            )
            .fetch_all(&pool)
            .await
            .map_err(|error| format!("query library ids: {error}"))?;

            Ok(rows
                .into_iter()
                .map(|row| row.get::<String, _>("ID"))
                .collect::<Vec<_>>())
        })
    })
    .join()
    .map_err(|_| "join library id loader thread".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::setup;

    fn temp_db_path(case_id: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("komga-rust-{case_id}-{nanos}.sqlite"))
    }

    #[tokio::test]
    async fn scan_profile_loader_returns_error_for_invalid_schema() {
        let db_path = temp_db_path("scan-profile-invalid-schema");
        let pool = connect_pool(db_path.as_path(), 1)
            .await
            .expect("temporary sqlite db should open");
        pool.close().await;

        let error = load_persisted_library_scan_profiles(db_path.as_path())
            .expect_err("missing library table should return error");
        assert!(error.contains("query scan profiles"));

        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn library_scan_profile_loader_maps_to_application_profiles() {
        let db_path = temp_db_path("scan-profile-application-mapping");
        let pool = connect_pool(db_path.as_path(), 1)
            .await
            .expect("temporary sqlite db should open");
        setup::bootstrap_pool(&pool)
            .await
            .expect("temporary sqlite db should bootstrap main schema");
        sqlx::query(
            "INSERT INTO LIBRARY (ID, NAME, ROOT, SCAN_STARTUP, SCAN_INTERVAL) VALUES (?, ?, ?, ?, ?), (?, ?, ?, ?, ?)",
        )
        .bind("library-2")
        .bind("Library 2")
        .bind(std::env::temp_dir().to_string_lossy().to_string())
        .bind(false)
        .bind("DAILY")
        .bind("library-1")
        .bind("Library 1")
        .bind(std::env::temp_dir().to_string_lossy().to_string())
        .bind(true)
        .bind("DISABLED")
        .execute(&pool)
        .await
        .expect("library rows should be inserted");
        pool.close().await;

        let profiles = load_library_scan_profiles(db_path.as_path())
            .expect("application scan profiles should load");
        assert_eq!(profiles.len(), 2);
        assert_eq!(profiles[0].library_id, "library-1");
        assert!(profiles[0].scan_startup);
        assert_eq!(profiles[0].scan_interval, "DISABLED");
        assert_eq!(profiles[1].library_id, "library-2");
        assert!(!profiles[1].scan_startup);
        assert_eq!(profiles[1].scan_interval, "DAILY");

        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn library_id_loader_returns_error_for_invalid_schema() {
        let db_path = temp_db_path("library-id-invalid-schema");
        let pool = connect_pool(db_path.as_path(), 1)
            .await
            .expect("temporary sqlite db should open");
        pool.close().await;

        let error = load_persisted_library_ids(db_path.as_path())
            .expect_err("missing library table should return error");
        assert!(error.contains("query library ids"));

        let _ = std::fs::remove_file(db_path);
    }
}
