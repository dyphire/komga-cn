use crate::sqlite::{SharedSqlitePoolSnapshot, shared_pool_snapshots_for_paths};
use sqlx::{Row, SqlitePool};

pub fn load_sqlite_pool_snapshots(paths: &[std::path::PathBuf]) -> Vec<SharedSqlitePoolSnapshot> {
    shared_pool_snapshots_for_paths(paths)
}

pub async fn load_task_execution_values(pool: &SqlitePool) -> Result<Vec<(String, f64)>, String> {
    let rows = sqlx::query(
        r#"SELECT SIMPLE_TYPE, CAST(COUNT(*) AS REAL) AS VALUE
        FROM TASK
        GROUP BY SIMPLE_TYPE
        ORDER BY SIMPLE_TYPE"#,
    )
    .fetch_all(pool)
    .await
    .map_err(|error| format!("query task execution metrics: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("SIMPLE_TYPE"),
                row.get::<f64, _>("VALUE"),
            )
        })
        .collect::<Vec<_>>())
}

pub async fn load_libraries_count(pool: &SqlitePool) -> Result<f64, String> {
    let row = sqlx::query(
        r#"SELECT CAST(COUNT(*) AS REAL) AS VALUE
        FROM LIBRARY"#,
    )
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("query libraries metrics: {error}"))?;

    Ok(row.map(|value| value.get::<f64, _>("VALUE")).unwrap_or(0.0))
}

pub async fn load_series_grouped_by_library(
    pool: &SqlitePool,
) -> Result<Vec<(String, f64)>, String> {
    let rows = sqlx::query(
        r#"SELECT LIBRARY_ID, CAST(COUNT(*) AS REAL) AS VALUE
        FROM SERIES
        GROUP BY LIBRARY_ID
        ORDER BY LIBRARY_ID"#,
    )
    .fetch_all(pool)
    .await
    .map_err(|error| format!("query series metrics: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("LIBRARY_ID"),
                row.get::<f64, _>("VALUE"),
            )
        })
        .collect::<Vec<_>>())
}

pub async fn load_books_grouped_by_library(
    pool: &SqlitePool,
) -> Result<Vec<(String, f64)>, String> {
    let rows = sqlx::query(
        r#"SELECT LIBRARY_ID, CAST(COUNT(*) AS REAL) AS VALUE
        FROM BOOK
        GROUP BY LIBRARY_ID
        ORDER BY LIBRARY_ID"#,
    )
    .fetch_all(pool)
    .await
    .map_err(|error| format!("query books metrics: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("LIBRARY_ID"),
                row.get::<f64, _>("VALUE"),
            )
        })
        .collect::<Vec<_>>())
}

pub async fn load_books_filesize_grouped_by_library(
    pool: &SqlitePool,
) -> Result<Vec<(String, f64)>, String> {
    let rows = sqlx::query(
        r#"SELECT LIBRARY_ID, CAST(COALESCE(SUM(FILE_SIZE), 0) AS REAL) AS VALUE
        FROM BOOK
        GROUP BY LIBRARY_ID
        ORDER BY LIBRARY_ID"#,
    )
    .fetch_all(pool)
    .await
    .map_err(|error| format!("query books filesize metrics: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("LIBRARY_ID"),
                row.get::<f64, _>("VALUE"),
            )
        })
        .collect::<Vec<_>>())
}

pub async fn load_sidecars_grouped_by_library(
    pool: &SqlitePool,
) -> Result<Vec<(String, f64)>, String> {
    let rows = sqlx::query(
        r#"SELECT LIBRARY_ID, CAST(COUNT(*) AS REAL) AS VALUE
        FROM SIDECAR
        GROUP BY LIBRARY_ID
        ORDER BY LIBRARY_ID"#,
    )
    .fetch_all(pool)
    .await
    .map_err(|error| format!("query sidecars metrics: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("LIBRARY_ID"),
                row.get::<f64, _>("VALUE"),
            )
        })
        .collect::<Vec<_>>())
}

pub async fn load_collections_count(pool: &SqlitePool) -> Result<f64, String> {
    let row = sqlx::query(
        r#"SELECT CAST(COUNT(*) AS REAL) AS VALUE
        FROM COLLECTION"#,
    )
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("query collections metrics: {error}"))?;

    Ok(row.map(|value| value.get::<f64, _>("VALUE")).unwrap_or(0.0))
}

pub async fn load_readlists_count(pool: &SqlitePool) -> Result<f64, String> {
    let row = sqlx::query(
        r#"SELECT CAST(COUNT(*) AS REAL) AS VALUE
        FROM READLIST"#,
    )
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("query readlists metrics: {error}"))?;

    Ok(row.map(|value| value.get::<f64, _>("VALUE")).unwrap_or(0.0))
}

pub async fn load_task_failure_count(pool: &SqlitePool) -> Result<f64, String> {
    let row = sqlx::query(
        r#"SELECT CAST(COUNT(*) AS REAL) AS VALUE
        FROM HISTORICAL_EVENT
        WHERE TYPE LIKE '%TASK%'
        AND TYPE LIKE '%FAIL%'"#,
    )
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("query task failure metrics: {error}"))?;

    Ok(row.map(|value| value.get::<f64, _>("VALUE")).unwrap_or(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::{SqliteTempPool, setup};

    async fn seeded_pool(case_id: &str) -> SqliteTempPool {
        let temp_pool = SqliteTempPool::new(case_id).await.expect("temp pool");
        setup::bootstrap_pool(temp_pool.pool())
            .await
            .expect("bootstrap main schema");
        temp_pool
    }

    #[tokio::test]
    async fn load_grouped_library_metrics_from_the_owned_queries() {
        let pool = seeded_pool("operational-metrics-grouped").await;

        sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES (?, ?, ?)")
            .bind("lib-a")
            .bind("Library A")
            .bind("/a")
            .execute(pool.pool())
            .await
            .expect("insert library a");
        sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES (?, ?, ?)")
            .bind("lib-b")
            .bind("Library B")
            .bind("/b")
            .execute(pool.pool())
            .await
            .expect("insert library b");

        sqlx::query(
            "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("series-a")
        .bind("2026-01-01T00:00:00Z")
        .bind("Series A")
        .bind("/series/a")
        .bind("lib-a")
        .execute(pool.pool())
        .await
        .expect("insert series a");
        sqlx::query(
            "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("series-b")
        .bind("2026-01-01T00:00:00Z")
        .bind("Series B")
        .bind("/series/b")
        .bind("lib-b")
        .execute(pool.pool())
        .await
        .expect("insert series b");

        sqlx::query(
            "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, LIBRARY_ID) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind("book-a1")
        .bind("2026-01-01T00:00:00Z")
        .bind("Book A1")
        .bind("/book/a1")
        .bind("series-a")
        .bind(10_i64)
        .bind("lib-a")
        .execute(pool.pool())
        .await
        .expect("insert book a1");
        sqlx::query(
            "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, LIBRARY_ID) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind("book-a2")
        .bind("2026-01-01T00:00:00Z")
        .bind("Book A2")
        .bind("/book/a2")
        .bind("series-a")
        .bind(20_i64)
        .bind("lib-a")
        .execute(pool.pool())
        .await
        .expect("insert book a2");
        sqlx::query(
            "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, LIBRARY_ID) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind("book-b1")
        .bind("2026-01-01T00:00:00Z")
        .bind("Book B1")
        .bind("/book/b1")
        .bind("series-b")
        .bind(30_i64)
        .bind("lib-b")
        .execute(pool.pool())
        .await
        .expect("insert book b1");

        sqlx::query("INSERT INTO SIDECAR (URL, PARENT_URL, LAST_MODIFIED_TIME, LIBRARY_ID) VALUES (?, ?, ?, ?)")
            .bind("/sidecar/a")
            .bind("/parent/a")
            .bind("2026-01-01T00:00:00Z")
            .bind("lib-a")
            .execute(pool.pool())
            .await
            .expect("insert sidecar a");
        sqlx::query("INSERT INTO SIDECAR (URL, PARENT_URL, LAST_MODIFIED_TIME, LIBRARY_ID) VALUES (?, ?, ?, ?)")
            .bind("/sidecar/b1")
            .bind("/parent/b1")
            .bind("2026-01-01T00:00:00Z")
            .bind("lib-b")
            .execute(pool.pool())
            .await
            .expect("insert sidecar b1");
        sqlx::query("INSERT INTO SIDECAR (URL, PARENT_URL, LAST_MODIFIED_TIME, LIBRARY_ID) VALUES (?, ?, ?, ?)")
            .bind("/sidecar/b2")
            .bind("/parent/b2")
            .bind("2026-01-01T00:00:00Z")
            .bind("lib-b")
            .execute(pool.pool())
            .await
            .expect("insert sidecar b2");

        assert_eq!(
            load_series_grouped_by_library(pool.pool()).await.unwrap(),
            vec![("lib-a".to_string(), 1.0), ("lib-b".to_string(), 1.0)]
        );
        assert_eq!(
            load_books_grouped_by_library(pool.pool()).await.unwrap(),
            vec![("lib-a".to_string(), 2.0), ("lib-b".to_string(), 1.0)]
        );
        assert_eq!(
            load_books_filesize_grouped_by_library(pool.pool())
                .await
                .unwrap(),
            vec![("lib-a".to_string(), 30.0), ("lib-b".to_string(), 30.0)]
        );
        assert_eq!(
            load_sidecars_grouped_by_library(pool.pool()).await.unwrap(),
            vec![("lib-a".to_string(), 1.0), ("lib-b".to_string(), 2.0)]
        );

        pool.cleanup().await;
    }

    #[tokio::test]
    async fn load_scalar_operational_metric_counts_from_the_owned_queries() {
        let pool = seeded_pool("operational-metrics-scalar").await;

        sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES (?, ?, ?)")
            .bind("lib-a")
            .bind("Library A")
            .bind("/a")
            .execute(pool.pool())
            .await
            .expect("insert library");
        sqlx::query("INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT) VALUES (?, ?, ?, ?)")
            .bind("collection-a")
            .bind("Collection A")
            .bind(false)
            .bind(0_i64)
            .execute(pool.pool())
            .await
            .expect("insert collection");
        sqlx::query("INSERT INTO READLIST (ID, NAME, BOOK_COUNT) VALUES (?, ?, ?)")
            .bind("readlist-a")
            .bind("Readlist A")
            .bind(0_i64)
            .execute(pool.pool())
            .await
            .expect("insert readlist");
        sqlx::query("INSERT INTO HISTORICAL_EVENT (ID, TYPE) VALUES (?, ?)")
            .bind("event-a")
            .bind("TASK_FAILED")
            .execute(pool.pool())
            .await
            .expect("insert matching event");
        sqlx::query("INSERT INTO HISTORICAL_EVENT (ID, TYPE) VALUES (?, ?)")
            .bind("event-b")
            .bind("SCAN_COMPLETE")
            .execute(pool.pool())
            .await
            .expect("insert non matching event");

        assert_eq!(load_libraries_count(pool.pool()).await.unwrap(), 1.0);
        assert_eq!(load_collections_count(pool.pool()).await.unwrap(), 1.0);
        assert_eq!(load_readlists_count(pool.pool()).await.unwrap(), 1.0);
        assert_eq!(load_task_failure_count(pool.pool()).await.unwrap(), 1.0);

        pool.cleanup().await;
    }
}
