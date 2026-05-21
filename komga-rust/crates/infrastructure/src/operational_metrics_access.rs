use std::path::PathBuf;

use async_trait::async_trait;
use komga_application::operational::{OperationalMetricsPort, SqlitePoolSnapshot};
use sqlx::{Row, SqlitePool};

use crate::database_handle::DatabaseHandle;
use crate::sqlite::shared_pool_snapshots_for_paths;

#[derive(Clone)]
pub struct OperationalMetricsAccess {
    main_db: DatabaseHandle,
    tasks_db: DatabaseHandle,
}

impl OperationalMetricsAccess {
    pub fn new(main_db: DatabaseHandle, tasks_db: DatabaseHandle) -> Self {
        Self { main_db, tasks_db }
    }
}

#[async_trait]
impl OperationalMetricsPort for OperationalMetricsAccess {
    async fn load_task_execution_values(&self) -> Result<Vec<(String, f64)>, String> {
        load_task_execution_values(self.tasks_db.read_pool()).await
    }

    async fn load_libraries_count(&self) -> Result<f64, String> {
        load_libraries_count(self.main_db.read_pool()).await
    }

    async fn load_series_grouped_by_library(&self) -> Result<Vec<(String, f64)>, String> {
        load_series_grouped_by_library(self.main_db.read_pool()).await
    }

    async fn load_books_grouped_by_library(&self) -> Result<Vec<(String, f64)>, String> {
        load_books_grouped_by_library(self.main_db.read_pool()).await
    }

    async fn load_books_filesize_grouped_by_library(&self) -> Result<Vec<(String, f64)>, String> {
        load_books_filesize_grouped_by_library(self.main_db.read_pool()).await
    }

    async fn load_sidecars_grouped_by_library(&self) -> Result<Vec<(String, f64)>, String> {
        load_sidecars_grouped_by_library(self.main_db.read_pool()).await
    }

    async fn load_collections_count(&self) -> Result<f64, String> {
        load_collections_count(self.main_db.read_pool()).await
    }

    async fn load_readlists_count(&self) -> Result<f64, String> {
        load_readlists_count(self.main_db.read_pool()).await
    }

    async fn load_task_failure_count(&self) -> Result<f64, String> {
        load_task_failure_count(self.main_db.read_pool()).await
    }

    async fn load_sqlite_pool_snapshots(
        &self,
        paths: &[PathBuf],
    ) -> Result<Vec<SqlitePoolSnapshot>, String> {
        Ok(shared_pool_snapshots_for_paths(paths)
            .into_iter()
            .map(|s| SqlitePoolSnapshot {
                path: s.path,
                max_connections: s.max_connections,
                min_connections: s.min_connections,
                total_connections: s.total_connections,
                idle_connections: s.idle_connections,
                in_use_connections: s.in_use_connections,
                is_closed: s.is_closed,
            })
            .collect())
    }
}

pub fn load_sqlite_pool_snapshots(paths: &[PathBuf]) -> Vec<SqlitePoolSnapshot> {
    shared_pool_snapshots_for_paths(paths)
        .into_iter()
        .map(|s| SqlitePoolSnapshot {
            path: s.path,
            max_connections: s.max_connections,
            min_connections: s.min_connections,
            total_connections: s.total_connections,
            idle_connections: s.idle_connections,
            in_use_connections: s.in_use_connections,
            is_closed: s.is_closed,
        })
        .collect()
}

pub async fn load_task_execution_values(pool: &SqlitePool) -> Result<Vec<(String, f64)>, String> {
    let rows = sqlx::query(
        r#"SELECT TASK_TYPE, COUNT(*) AS COUNT
FROM TASK_EXECUTION
GROUP BY TASK_TYPE"#,
    )
    .fetch_all(pool)
    .await
    .map_err(|error| format!("query task execution values: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("TASK_TYPE"),
                row.get::<i64, _>("COUNT") as f64,
            )
        })
        .collect())
}

pub async fn load_libraries_count(pool: &SqlitePool) -> Result<f64, String> {
    let row = sqlx::query(
        r#"SELECT COUNT(*) AS COUNT
FROM LIBRARY"#,
    )
    .fetch_one(pool)
    .await
    .map_err(|error| format!("query libraries count: {error}"))?;

    Ok(row.get::<i64, _>("COUNT") as f64)
}

pub async fn load_series_grouped_by_library(
    pool: &SqlitePool,
) -> Result<Vec<(String, f64)>, String> {
    let rows = sqlx::query(
        r#"SELECT l.NAME AS LIBRARY_NAME, COUNT(s.ID) AS COUNT
FROM SERIES s
JOIN LIBRARY l ON l.ID = s.LIBRARY_ID
GROUP BY l.NAME"#,
    )
    .fetch_all(pool)
    .await
    .map_err(|error| format!("query series grouped by library: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("LIBRARY_NAME"),
                row.get::<i64, _>("COUNT") as f64,
            )
        })
        .collect())
}

pub async fn load_books_grouped_by_library(
    pool: &SqlitePool,
) -> Result<Vec<(String, f64)>, String> {
    let rows = sqlx::query(
        r#"SELECT l.NAME AS LIBRARY_NAME, COUNT(b.ID) AS COUNT
FROM BOOK b
JOIN LIBRARY l ON l.ID = b.LIBRARY_ID
GROUP BY l.NAME"#,
    )
    .fetch_all(pool)
    .await
    .map_err(|error| format!("query books grouped by library: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("LIBRARY_NAME"),
                row.get::<i64, _>("COUNT") as f64,
            )
        })
        .collect())
}

pub async fn load_books_filesize_grouped_by_library(
    pool: &SqlitePool,
) -> Result<Vec<(String, f64)>, String> {
    let rows = sqlx::query(
        r#"SELECT l.NAME AS LIBRARY_NAME, COALESCE(SUM(b.FILE_SIZE), 0) AS TOTAL_SIZE
FROM BOOK b
JOIN LIBRARY l ON l.ID = b.LIBRARY_ID
GROUP BY l.NAME"#,
    )
    .fetch_all(pool)
    .await
    .map_err(|error| format!("query books filesize grouped by library: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("LIBRARY_NAME"),
                row.get::<i64, _>("TOTAL_SIZE") as f64,
            )
        })
        .collect())
}

pub async fn load_sidecars_grouped_by_library(
    pool: &SqlitePool,
) -> Result<Vec<(String, f64)>, String> {
    let rows = sqlx::query(
        r#"SELECT l.NAME AS LIBRARY_NAME, COUNT(sc.ID) AS COUNT
FROM SIDECAR sc
JOIN BOOK b ON b.ID = sc.BOOK_ID
JOIN LIBRARY l ON l.ID = b.LIBRARY_ID
GROUP BY l.NAME"#,
    )
    .fetch_all(pool)
    .await
    .map_err(|error| format!("query sidecars grouped by library: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("LIBRARY_NAME"),
                row.get::<i64, _>("COUNT") as f64,
            )
        })
        .collect())
}

pub async fn load_collections_count(pool: &SqlitePool) -> Result<f64, String> {
    let row = sqlx::query(
        r#"SELECT COUNT(*) AS COUNT
FROM COLLECTION"#,
    )
    .fetch_one(pool)
    .await
    .map_err(|error| format!("query collections count: {error}"))?;

    Ok(row.get::<i64, _>("COUNT") as f64)
}

pub async fn load_readlists_count(pool: &SqlitePool) -> Result<f64, String> {
    let row = sqlx::query(
        r#"SELECT COUNT(*) AS COUNT
FROM READLIST"#,
    )
    .fetch_one(pool)
    .await
    .map_err(|error| format!("query readlists count: {error}"))?;

    Ok(row.get::<i64, _>("COUNT") as f64)
}

pub async fn load_task_failure_count(pool: &SqlitePool) -> Result<f64, String> {
    let row = sqlx::query(
        r#"SELECT COUNT(*) AS COUNT
FROM TASK_EXECUTION
WHERE STATUS = 'FAILED'"#,
    )
    .fetch_one(pool)
    .await
    .map_err(|error| format!("query task failure count: {error}"))?;

    Ok(row.get::<i64, _>("COUNT") as f64)
}
