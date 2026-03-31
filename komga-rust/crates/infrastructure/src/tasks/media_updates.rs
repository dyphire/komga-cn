use std::collections::HashMap;
use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

use crate::sqlite::connect_pool;

pub fn persist_book_hash(
    database_file: &Path,
    book_id: &str,
    hash: &str,
    koreader: bool,
) -> Result<(), String> {
    let database_file = database_file.to_path_buf();
    let book_id = book_id.to_string();
    let hash = hash.to_string();

    run_database_query(database_file, move |pool| {
        let book_id = book_id.clone();
        let hash = hash.clone();
        Box::pin(async move {
            let sql = if koreader {
                r#"
                UPDATE BOOK
                SET FILE_HASH_KOREADER = ?,
                    LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
                WHERE ID = ?
                "#
            } else {
                r#"
                UPDATE BOOK
                SET FILE_HASH = ?,
                    LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
                WHERE ID = ?
                "#
            };

            sqlx::query(sql)
                .bind(hash)
                .bind(&book_id)
                .execute(&pool)
                .await
                .map_err(|error| format!("failed to persist book hash for '{book_id}': {error}"))?;

            Ok(())
        })
    })
}

pub fn persist_removed_hashed_pages(
    database_file: &Path,
    book_id: &str,
    deleted_count_by_hash: &HashMap<String, i64>,
    file_last_modified: i64,
    file_size: i64,
) -> Result<(), String> {
    let database_file = database_file.to_path_buf();
    let book_id = book_id.to_string();
    let deleted_count_by_hash = deleted_count_by_hash.clone();

    run_database_query(database_file, move |pool| {
        let book_id = book_id.clone();
        let deleted_count_by_hash = deleted_count_by_hash.clone();
        Box::pin(async move {
            let mut tx = pool.begin().await.map_err(|error| {
                format!("failed to start remove-hashed-pages transaction for '{book_id}': {error}")
            })?;

            for (hash, deleted) in deleted_count_by_hash {
                sqlx::query(
                    r#"
                    UPDATE PAGE_HASH
                    SET DELETE_COUNT = DELETE_COUNT + ?,
                        LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
                    WHERE HASH = ?
                    "#,
                )
                .bind(deleted)
                .bind(hash)
                .execute(&mut *tx)
                .await
                .map_err(|error| {
                    format!("failed to update PAGE_HASH delete count for '{book_id}': {error}")
                })?;
            }

            sqlx::query(
                r#"
                UPDATE BOOK
                SET FILE_LAST_MODIFIED = ?,
                    FILE_SIZE = ?,
                    FILE_HASH = '',
                    FILE_HASH_KOREADER = '',
                    LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
                WHERE ID = ?
                "#,
            )
            .bind(file_last_modified)
            .bind(file_size)
            .bind(&book_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| format!("failed to update BOOK metadata after hashed-page removal for '{book_id}': {error}"))?;

            tx.commit().await.map_err(|error| {
                format!("failed to commit remove-hashed-pages transaction for '{book_id}': {error}")
            })?;

            Ok(())
        })
    })
}

pub fn persist_book_extension_repair(
    database_file: &Path,
    book_id: &str,
    library_id: &str,
    book_url: &str,
    destination_url: &str,
    file_last_modified: i64,
    file_size: i64,
) -> Result<(), String> {
    let database_file = database_file.to_path_buf();
    let book_id = book_id.to_string();
    let library_id = library_id.to_string();
    let book_url = book_url.to_string();
    let destination_url = destination_url.to_string();

    run_database_query(database_file, move |pool| {
        let book_id = book_id.clone();
        let library_id = library_id.clone();
        let book_url = book_url.clone();
        let destination_url = destination_url.clone();
        Box::pin(async move {
            let mut tx = pool.begin().await.map_err(|error| {
                format!("failed to start extension-repair transaction for '{book_id}': {error}")
            })?;

            sqlx::query(
                r#"
                UPDATE BOOK
                SET URL = ?,
                    FILE_LAST_MODIFIED = ?,
                    FILE_SIZE = ?,
                    LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
                WHERE ID = ?
                "#,
            )
            .bind(&destination_url)
            .bind(file_last_modified)
            .bind(file_size)
            .bind(&book_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                format!(
                    "failed to update BOOK row during extension repair for '{book_id}': {error}"
                )
            })?;

            sqlx::query(
                r#"
                UPDATE SIDECAR
                SET PARENT_URL = ?
                WHERE LIBRARY_ID = ?
                AND PARENT_URL = ?
                "#,
            )
            .bind(&destination_url)
            .bind(&library_id)
            .bind(&book_url)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                format!(
                    "failed to update SIDECAR rows during extension repair for '{book_id}': {error}"
                )
            })?;

            tx.commit().await.map_err(|error| {
                format!("failed to commit extension-repair transaction for '{book_id}': {error}")
            })?;

            Ok(())
        })
    })
}

pub fn persist_book_conversion(
    database_file: &Path,
    book_id: &str,
    library_id: &str,
    book_url: &str,
    destination_url: &str,
    file_last_modified: i64,
    file_size: i64,
) -> Result<(), String> {
    let database_file = database_file.to_path_buf();
    let book_id = book_id.to_string();
    let library_id = library_id.to_string();
    let book_url = book_url.to_string();
    let destination_url = destination_url.to_string();

    run_database_query(database_file, move |pool| {
        let book_id = book_id.clone();
        let library_id = library_id.clone();
        let book_url = book_url.clone();
        let destination_url = destination_url.clone();
        Box::pin(async move {
            let mut tx = pool.begin().await.map_err(|error| {
                format!("failed to start convert-book transaction for '{book_id}': {error}")
            })?;

            sqlx::query(
                r#"
                UPDATE BOOK
                SET URL = ?,
                    FILE_LAST_MODIFIED = ?,
                    FILE_SIZE = ?,
                    FILE_HASH = '',
                    FILE_HASH_KOREADER = '',
                    LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
                WHERE ID = ?
                "#,
            )
            .bind(&destination_url)
            .bind(file_last_modified)
            .bind(file_size)
            .bind(&book_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                format!("failed to update BOOK row during conversion for '{book_id}': {error}")
            })?;

            sqlx::query(
                r#"
                UPDATE SIDECAR
                SET PARENT_URL = ?
                WHERE LIBRARY_ID = ?
                AND PARENT_URL = ?
                "#,
            )
            .bind(&destination_url)
            .bind(&library_id)
            .bind(&book_url)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                format!("failed to update SIDECAR rows during conversion for '{book_id}': {error}")
            })?;

            sqlx::query(
                r#"
                UPDATE MEDIA
                SET STATUS = 'OUTDATED',
                    MEDIA_TYPE = 'application/zip',
                    LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
                WHERE BOOK_ID = ?
                "#,
            )
            .bind(&book_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                format!("failed to refresh MEDIA row during conversion for '{book_id}': {error}")
            })?;

            tx.commit().await.map_err(|error| {
                format!("failed to commit convert-book transaction for '{book_id}': {error}")
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
