use std::path::{Path, PathBuf};

use sqlx::{Row, SqlitePool};

use crate::sql::task_queue::{EMPTY_TRASH_BOOK_DEPENDENCY_SQL, EMPTY_TRASH_SERIES_DEPENDENCY_SQL};
use crate::sqlite::connect_pool;

#[derive(Clone, Debug)]
struct PersistedCleanupEmptySetsFlags {
    delete_collections: bool,
    delete_readlists: bool,
}

pub fn empty_trash_rows(database_file: &Path, library_id: &str) -> Result<(), String> {
    let database_file = database_file.to_path_buf();
    let library_id = library_id.to_string();

    run_database_query(database_file, move |pool| {
        let library_id = library_id.clone();
        Box::pin(async move {
            let mut tx = pool
                .begin()
                .await
                .map_err(|error| format!("failed to start empty-trash transaction: {error}"))?;

            for sql in EMPTY_TRASH_BOOK_DEPENDENCY_SQL {
                sqlx::query(sql)
                    .bind(&library_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|error| format!("failed to delete empty-trash dependent rows for library '{library_id}': {error}"))?;
            }

            sqlx::query(
                r#"
                DELETE FROM BOOK
                WHERE LIBRARY_ID = ?
                AND DELETED_DATE IS NOT NULL
                "#,
            )
            .bind(&library_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                format!("failed to delete trashed BOOK rows for library '{library_id}': {error}")
            })?;

            sqlx::query(
                r#"
                UPDATE SERIES
                SET BOOK_COUNT = (
                SELECT COUNT(*)
                FROM BOOK
                WHERE BOOK.SERIES_ID = SERIES.ID
                )
                WHERE LIBRARY_ID = ?
                "#,
            )
            .bind(&library_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                format!("failed to refresh SERIES book counts for library '{library_id}': {error}")
            })?;

            for sql in EMPTY_TRASH_SERIES_DEPENDENCY_SQL {
                sqlx::query(sql)
                    .bind(&library_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|error| format!("failed to delete empty-trash SERIES dependents for library '{library_id}': {error}"))?;
            }

            sqlx::query(
                r#"
                DELETE FROM SERIES
                WHERE LIBRARY_ID = ?
                AND (
                    DELETED_DATE IS NOT NULL
                    OR BOOK_COUNT = 0
                )
                "#,
            )
            .bind(&library_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                format!("failed to delete trashed SERIES rows for library '{library_id}': {error}")
            })?;

            tx.commit().await.map_err(|error| {
                format!(
                    "failed to commit empty-trash transaction for library '{library_id}': {error}"
                )
            })?;

            Ok(())
        })
    })
}

pub fn cleanup_empty_sets_rows(database_file: &Path) -> Result<(), String> {
    let database_file = database_file.to_path_buf();

    run_database_query(database_file, move |pool| {
        Box::pin(async move {
            let flags = load_cleanup_empty_sets_flags_from_pool(&pool).await?;
            let mut tx = pool.begin().await.map_err(|error| {
                format!("failed to start cleanup-empty-sets transaction: {error}")
            })?;

            let mut deletes = Vec::<&str>::new();
            if flags.delete_collections {
                deletes.push(
                    "DELETE FROM COLLECTION WHERE ID NOT IN (SELECT COLLECTION_ID FROM COLLECTION_SERIES)",
                );
                deletes.push(
                    "DELETE FROM THUMBNAIL_COLLECTION WHERE COLLECTION_ID NOT IN (SELECT ID FROM COLLECTION)",
                );
            }
            if flags.delete_readlists {
                deletes.push(
                    "DELETE FROM READLIST WHERE ID NOT IN (SELECT READLIST_ID FROM READLIST_BOOK)",
                );
                deletes.push(
                    "DELETE FROM THUMBNAIL_READLIST WHERE READLIST_ID NOT IN (SELECT ID FROM READLIST)",
                );
            }

            for sql in deletes {
                sqlx::query(sql)
                    .execute(&mut *tx)
                    .await
                    .map_err(|error| format!("failed to cleanup empty sets rows: {error}"))?;
            }

            tx.commit().await.map_err(|error| {
                format!("failed to commit cleanup-empty-sets transaction: {error}")
            })?;

            Ok(())
        })
    })
}

async fn load_cleanup_empty_sets_flags_from_pool(
    pool: &SqlitePool,
) -> Result<PersistedCleanupEmptySetsFlags, String> {
    let rows = sqlx::query(
        r#"
        SELECT KEY, VALUE
        FROM SERVER_SETTINGS
        WHERE KEY IN ('DELETE_EMPTY_COLLECTIONS', 'DELETE_EMPTY_READLISTS')
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|error| {
        format!("failed to load cleanup-empty-sets flags from server settings: {error}")
    })?;

    let mut delete_collections = false;
    let mut delete_readlists = false;

    for row in rows {
        let key = row.get::<String, _>("KEY");
        let value = row.get::<Option<String>, _>("VALUE").unwrap_or_default();
        let enabled = value.trim().eq_ignore_ascii_case("true");
        match key.as_str() {
            "DELETE_EMPTY_COLLECTIONS" => delete_collections = enabled,
            "DELETE_EMPTY_READLISTS" => delete_readlists = enabled,
            _ => {}
        }
    }

    Ok(PersistedCleanupEmptySetsFlags {
        delete_collections,
        delete_readlists,
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
