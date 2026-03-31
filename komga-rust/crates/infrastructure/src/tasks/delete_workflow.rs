use std::path::{Path, PathBuf};

use sqlx::{Row, SqlitePool};

use crate::sql::task_queue::{
    DELETE_BOOK_DEPENDENCY_SQL, DELETE_SERIES_BOOK_DEPENDENCY_SQL, DELETE_SERIES_DEPENDENCY_SQL,
};
use crate::sqlite::connect_pool;

#[derive(Clone, Debug)]
pub struct PersistedDeleteBookDecision {
    pub series_id: String,
    pub oneshot: bool,
}

#[derive(Clone, Debug)]
pub struct PersistedDeleteBookWork {
    pub series_id: String,
    pub book_path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct PersistedDeleteSeriesWork {
    pub book_ids: Vec<String>,
    pub book_paths: Vec<PathBuf>,
}

pub fn load_book_delete_decision(
    database_file: &Path,
    book_id: &str,
) -> Result<Option<PersistedDeleteBookDecision>, String> {
    let database_file = database_file.to_path_buf();
    let book_id = book_id.to_string();

    run_database_query(database_file, move |pool| {
        Box::pin(async move {
            let row = sqlx::query(
                r#"
                SELECT SERIES_ID, COALESCE(oneshot, 0) AS ONESHOT
                FROM BOOK
                WHERE ID = ?
                LIMIT 1
                "#,
            )
            .bind(&book_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                format!("failed to resolve delete-book target for '{book_id}': {error}")
            })?;

            Ok(row.map(|row| PersistedDeleteBookDecision {
                series_id: row.get::<String, _>("SERIES_ID"),
                oneshot: row.get::<i64, _>("ONESHOT") != 0,
            }))
        })
    })
}

pub fn load_book_delete_work(
    database_file: &Path,
    book_id: &str,
) -> Result<Option<PersistedDeleteBookWork>, String> {
    let database_file = database_file.to_path_buf();
    let book_id = book_id.to_string();

    run_database_query(database_file, move |pool| {
        Box::pin(async move {
            let row = sqlx::query(
                r#"
                SELECT
                b.SERIES_ID AS SERIES_ID,
                b.URL AS BOOK_URL,
                l.ROOT AS LIBRARY_ROOT
                FROM BOOK b
                JOIN LIBRARY l ON l.ID = b.LIBRARY_ID
                WHERE b.ID = ?
                LIMIT 1
                "#,
            )
            .bind(&book_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                format!("failed to load book delete target for '{book_id}': {error}")
            })?;

            Ok(row.map(|row| PersistedDeleteBookWork {
                series_id: row.get::<String, _>("SERIES_ID"),
                book_path: PathBuf::from(row.get::<String, _>("LIBRARY_ROOT"))
                    .join(row.get::<String, _>("BOOK_URL")),
            }))
        })
    })
}

pub fn delete_book_rows(
    database_file: &Path,
    book_id: &str,
    series_id: &str,
) -> Result<(), String> {
    let database_file = database_file.to_path_buf();
    let book_id = book_id.to_string();
    let series_id = series_id.to_string();

    run_database_query(database_file, move |pool| {
        let book_id = book_id.clone();
        let series_id = series_id.clone();
        Box::pin(async move {
            let mut tx = pool.begin().await.map_err(|error| {
                format!("failed to start delete-book transaction for '{book_id}': {error}")
            })?;

            for sql in DELETE_BOOK_DEPENDENCY_SQL {
                sqlx::query(sql)
                    .bind(&book_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|error| format!("failed to delete dependent rows while deleting book '{book_id}': {error}"))?;
            }

            sqlx::query(
                r#"
                DELETE FROM BOOK
                WHERE ID = ?
                "#,
            )
            .bind(&book_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| format!("failed to delete BOOK row for '{book_id}': {error}"))?;

            sqlx::query(
                r#"
                UPDATE SERIES
                SET BOOK_COUNT = (
                SELECT COUNT(*)
                FROM BOOK
                WHERE BOOK.SERIES_ID = SERIES.ID
                )
                WHERE ID = ?
                "#,
            )
            .bind(&series_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| format!("failed to refresh series count for '{series_id}' while deleting book '{book_id}': {error}"))?;

            tx.commit().await.map_err(|error| {
                format!("failed to commit delete-book transaction for '{book_id}': {error}")
            })?;

            Ok(())
        })
    })
}

pub fn load_series_delete_work(
    database_file: &Path,
    series_id: &str,
) -> Result<PersistedDeleteSeriesWork, String> {
    let database_file = database_file.to_path_buf();
    let series_id = series_id.to_string();

    run_database_query(database_file, move |pool| {
        Box::pin(async move {
            let rows = sqlx::query(
                r#"
                SELECT
                b.ID AS BOOK_ID,
                b.URL AS BOOK_URL,
                l.ROOT AS LIBRARY_ROOT
                FROM BOOK b
                JOIN LIBRARY l ON l.ID = b.LIBRARY_ID
                WHERE b.SERIES_ID = ?
                "#,
            )
            .bind(&series_id)
            .fetch_all(&pool)
            .await
            .map_err(|error| {
                format!("failed to load series books for delete '{series_id}': {error}")
            })?;

            Ok(PersistedDeleteSeriesWork {
                book_ids: rows
                    .iter()
                    .map(|row| row.get::<String, _>("BOOK_ID"))
                    .collect(),
                book_paths: rows
                    .iter()
                    .map(|row| {
                        PathBuf::from(row.get::<String, _>("LIBRARY_ROOT"))
                            .join(row.get::<String, _>("BOOK_URL"))
                    })
                    .collect(),
            })
        })
    })
}

pub fn delete_series_rows(
    database_file: &Path,
    series_id: &str,
    book_ids: &[String],
) -> Result<(), String> {
    let database_file = database_file.to_path_buf();
    let series_id = series_id.to_string();
    let book_ids = book_ids.to_vec();

    run_database_query(database_file, move |pool| {
        let series_id = series_id.clone();
        let book_ids = book_ids.clone();
        Box::pin(async move {
            let mut tx = pool.begin().await.map_err(|error| {
                format!("failed to start delete-series transaction for '{series_id}': {error}")
            })?;

            for book_id in &book_ids {
                for sql in DELETE_SERIES_BOOK_DEPENDENCY_SQL {
                    sqlx::query(sql)
                        .bind(book_id)
                        .execute(&mut *tx)
                        .await
                        .map_err(|error| format!("failed to delete dependent rows while deleting series '{series_id}': {error}"))?;
                }
            }

            sqlx::query(
                r#"
                DELETE FROM BOOK
                WHERE SERIES_ID = ?
                "#,
            )
            .bind(&series_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                format!("failed to delete BOOK rows for series '{series_id}': {error}")
            })?;

            for sql in DELETE_SERIES_DEPENDENCY_SQL {
                sqlx::query(sql)
                    .bind(&series_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|error| {
                        format!("failed to delete series dependent rows for '{series_id}': {error}")
                    })?;
            }

            sqlx::query(
                r#"
                DELETE FROM SERIES
                WHERE ID = ?
                "#,
            )
            .bind(&series_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| format!("failed to delete SERIES row '{series_id}': {error}"))?;

            tx.commit().await.map_err(|error| {
                format!("failed to commit delete-series transaction for '{series_id}': {error}")
            })?;

            Ok(())
        })
    })
}

type BoxFuture<T> = std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, String>> + Send>>;

fn run_database_query<T>(
    database_file: PathBuf,
    operation: impl FnOnce(SqlitePool) -> BoxFuture<T> + Send + 'static,
) -> Result<T, String>
where
    T: Send + 'static,
{
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| format!("failed to build task runtime: {error}"))?;

        runtime.block_on(async move {
            let pool = connect_pool(&database_file, 1)
                .await
                .map_err(|error| format!("failed to open sqlite pool: {error}"))?;
            operation(pool).await
        })
    })
    .join()
    .map_err(|_| "database operation worker thread panicked".to_string())?
}
