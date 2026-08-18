use std::path::PathBuf;

use anyhow::Context;
use komga_application::operational::{
    DatabasePoolSnapshot, LibraryMetricValue, OperationalMetricsPort, TaskExecutionMetricValue,
};
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

#[async_trait::async_trait]
impl OperationalMetricsPort for OperationalMetricsAccess {
    async fn load_task_execution_values(&self) -> anyhow::Result<Vec<TaskExecutionMetricValue>> {
        load_task_execution_values(self.tasks_db.read_pool()).await
    }

    async fn load_libraries_count(&self) -> anyhow::Result<f64> {
        load_libraries_count(self.main_db.read_pool()).await
    }

    async fn load_series_grouped_by_library(&self) -> anyhow::Result<Vec<LibraryMetricValue>> {
        load_series_grouped_by_library(self.main_db.read_pool()).await
    }

    async fn load_books_grouped_by_library(&self) -> anyhow::Result<Vec<LibraryMetricValue>> {
        load_books_grouped_by_library(self.main_db.read_pool()).await
    }

    async fn load_books_filesize_grouped_by_library(
        &self,
    ) -> anyhow::Result<Vec<LibraryMetricValue>> {
        load_books_filesize_grouped_by_library(self.main_db.read_pool()).await
    }

    async fn load_sidecars_grouped_by_library(&self) -> anyhow::Result<Vec<LibraryMetricValue>> {
        load_sidecars_grouped_by_library(self.main_db.read_pool()).await
    }

    async fn load_collections_count(&self) -> anyhow::Result<f64> {
        load_collections_count(self.main_db.read_pool()).await
    }

    async fn load_readlists_count(&self) -> anyhow::Result<f64> {
        load_readlists_count(self.main_db.read_pool()).await
    }

    async fn load_task_failure_count(&self) -> anyhow::Result<f64> {
        load_task_failure_count(self.main_db.read_pool()).await
    }

    async fn load_database_pool_snapshots(
        &self,
        paths: &[PathBuf],
    ) -> anyhow::Result<Vec<DatabasePoolSnapshot>> {
        Ok(shared_pool_snapshots_for_paths(paths)
            .into_iter()
            .map(|s| DatabasePoolSnapshot {
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

async fn load_task_execution_values(
    pool: &SqlitePool,
) -> anyhow::Result<Vec<TaskExecutionMetricValue>> {
    let rows = sqlx::query(
        r#"SELECT SIMPLE_TYPE, CAST(COUNT(*) AS REAL) AS VALUE
FROM TASK
GROUP BY SIMPLE_TYPE
ORDER BY SIMPLE_TYPE"#,
    )
    .fetch_all(pool)
    .await
    .context("query task execution values")?;

    Ok(rows
        .into_iter()
        .map(|row| TaskExecutionMetricValue {
            task_type: row.get::<String, _>("SIMPLE_TYPE"),
            count: row.get::<f64, _>("VALUE"),
        })
        .collect())
}

async fn load_libraries_count(pool: &SqlitePool) -> anyhow::Result<f64> {
    let row = sqlx::query(
        r#"SELECT COUNT(*) AS COUNT
FROM LIBRARY"#,
    )
    .fetch_one(pool)
    .await
    .context("query libraries count")?;

    Ok(row.get::<i64, _>("COUNT") as f64)
}

async fn load_series_grouped_by_library(
    pool: &SqlitePool,
) -> anyhow::Result<Vec<LibraryMetricValue>> {
    let rows = sqlx::query(
        r#"SELECT l.NAME AS LIBRARY_NAME, COUNT(s.ID) AS COUNT
FROM SERIES s
JOIN LIBRARY l ON l.ID = s.LIBRARY_ID
GROUP BY l.NAME"#,
    )
    .fetch_all(pool)
    .await
    .context("query series grouped by library")?;

    Ok(rows
        .into_iter()
        .map(|row| LibraryMetricValue {
            library_name: row.get::<String, _>("LIBRARY_NAME"),
            value: row.get::<i64, _>("COUNT") as f64,
        })
        .collect())
}

async fn load_books_grouped_by_library(
    pool: &SqlitePool,
) -> anyhow::Result<Vec<LibraryMetricValue>> {
    let rows = sqlx::query(
        r#"SELECT l.NAME AS LIBRARY_NAME, COUNT(b.ID) AS COUNT
FROM BOOK b
JOIN LIBRARY l ON l.ID = b.LIBRARY_ID
GROUP BY l.NAME"#,
    )
    .fetch_all(pool)
    .await
    .context("query books grouped by library")?;

    Ok(rows
        .into_iter()
        .map(|row| LibraryMetricValue {
            library_name: row.get::<String, _>("LIBRARY_NAME"),
            value: row.get::<i64, _>("COUNT") as f64,
        })
        .collect())
}

async fn load_books_filesize_grouped_by_library(
    pool: &SqlitePool,
) -> anyhow::Result<Vec<LibraryMetricValue>> {
    let rows = sqlx::query(
        r#"SELECT l.NAME AS LIBRARY_NAME, COALESCE(SUM(b.FILE_SIZE), 0) AS TOTAL_SIZE
FROM BOOK b
JOIN LIBRARY l ON l.ID = b.LIBRARY_ID
GROUP BY l.NAME"#,
    )
    .fetch_all(pool)
    .await
    .context("query books filesize grouped by library")?;

    Ok(rows
        .into_iter()
        .map(|row| LibraryMetricValue {
            library_name: row.get::<String, _>("LIBRARY_NAME"),
            value: row.get::<i64, _>("TOTAL_SIZE") as f64,
        })
        .collect())
}

async fn load_sidecars_grouped_by_library(
    pool: &SqlitePool,
) -> anyhow::Result<Vec<LibraryMetricValue>> {
    let rows = sqlx::query(
        r#"SELECT l.NAME AS LIBRARY_NAME, COUNT(sc.URL) AS COUNT
FROM SIDECAR sc
JOIN LIBRARY l ON l.ID = sc.LIBRARY_ID
GROUP BY l.NAME"#,
    )
    .fetch_all(pool)
    .await
    .context("query sidecars grouped by library")?;

    Ok(rows
        .into_iter()
        .map(|row| LibraryMetricValue {
            library_name: row.get::<String, _>("LIBRARY_NAME"),
            value: row.get::<i64, _>("COUNT") as f64,
        })
        .collect())
}

async fn load_collections_count(pool: &SqlitePool) -> anyhow::Result<f64> {
    let row = sqlx::query(
        r#"SELECT COUNT(*) AS COUNT
FROM COLLECTION"#,
    )
    .fetch_one(pool)
    .await
    .context("query collections count")?;

    Ok(row.get::<i64, _>("COUNT") as f64)
}

async fn load_readlists_count(pool: &SqlitePool) -> anyhow::Result<f64> {
    let row = sqlx::query(
        r#"SELECT COUNT(*) AS COUNT
FROM READLIST"#,
    )
    .fetch_one(pool)
    .await
    .context("query readlists count")?;

    Ok(row.get::<i64, _>("COUNT") as f64)
}

async fn load_task_failure_count(pool: &SqlitePool) -> anyhow::Result<f64> {
    let row = sqlx::query(
        r#"SELECT CAST(COUNT(*) AS REAL) AS VALUE
FROM HISTORICAL_EVENT
WHERE TYPE LIKE '%TASK%'
AND TYPE LIKE '%FAIL%'"#,
    )
    .fetch_optional(pool)
    .await
    .context("query task failure count")?;

    Ok(row.map(|r| r.get::<f64, _>("VALUE")).unwrap_or(0.0))
}
