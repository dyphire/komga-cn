use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use komga_application::runtime_sse::register_runtime_sse_event;
use serde_json::json;
use sqlx::{Row, SqlitePool};
use tokio::fs;

use super::*;

pub(super) async fn delete_book_task(
    runtime: &JobRuntime<'_>,
    book_id: &str,
) -> Result<(), TaskProcessingError> {
    if !runtime.database().owns_main_database() {
        return Ok(());
    }

    let target = load_book_delete_target(runtime, book_id).await?;
    let Some((series_id, oneshot)) = target else {
        return Ok(());
    };

    if oneshot {
        delete_series(runtime, &series_id).await
    } else {
        delete_book(runtime, book_id).await
    }
}

async fn load_book_delete_target(
    runtime: &JobRuntime<'_>,
    book_id: &str,
) -> Result<Option<(String, bool)>, TaskProcessingError> {
    Ok(
        load_book_delete_decision(runtime.database().read_pool(), book_id)
            .await
            .map_err(TaskProcessingError::runtime)?
            .map(|target| (target.series_id, target.oneshot)),
    )
}

fn emit_book_changed_after_file_delete(book_id: &str, series_id: &str, library_id: &str) {
    register_runtime_sse_event(
        "BookChanged",
        json!({
            "bookId": book_id,
            "seriesId": series_id,
            "libraryId": library_id,
        }),
        false,
        None,
    );
}

fn emit_series_changed_after_file_delete(series_id: &str, library_id: &str) {
    register_runtime_sse_event(
        "SeriesChanged",
        json!({
            "seriesId": series_id,
            "libraryId": library_id,
        }),
        false,
        None,
    );
}

async fn delete_book(runtime: &JobRuntime<'_>, book_id: &str) -> Result<(), TaskProcessingError> {
    let Some(context) = load_book_delete_sse_context(runtime.database().read_pool(), book_id)
        .await
        .map_err(TaskProcessingError::runtime)?
    else {
        return Ok(());
    };
    let Some(work) = load_book_delete_work(runtime.database().read_pool(), book_id)
        .await
        .map_err(TaskProcessingError::runtime)?
    else {
        return Ok(());
    };

    let book_path = work.book_path.clone();
    let sidecar_thumbnail_paths = work.sidecar_thumbnail_paths.clone();
    // Delete tasks must still reconcile database state when the target file already vanished
    // before the worker runs; only existing paths should block on writability checks.
    if (fs::metadata(&book_path).await.is_ok() && !deletion_prerequisites_met(&book_path).await)
        || !empty_parent_directory_cleanup_prerequisites_met(&book_path, &sidecar_thumbnail_paths)
            .await
    {
        return Ok(());
    }
    delete_file_if_exists(&book_path, "book file").await?;
    remove_sidecar_thumbnail_files(&sidecar_thumbnail_paths).await?;
    remove_empty_parent_directory(&book_path).await?;

    soft_delete_book_rows(runtime.database().write_pool(), book_id, &work.series_id)
        .await
        .map_err(TaskProcessingError::runtime)?;

    runtime
        .search_engine()
        .delete_book(book_id)
        .await
        .map_err(TaskProcessingError::runtime)?;

    emit_book_changed_after_file_delete(book_id, &context.series_id, &context.library_id);

    Ok(())
}

async fn remove_empty_parent_directory(target_path: &Path) -> Result<(), TaskProcessingError> {
    let Some(parent_directory) = target_path.parent() else {
        return Ok(());
    };
    remove_empty_directory(parent_directory).await
}

async fn empty_parent_directory_cleanup_prerequisites_met(
    target_path: &Path,
    sidecar_thumbnail_paths: &[std::path::PathBuf],
) -> bool {
    let Some(parent_directory) = target_path.parent() else {
        return true;
    };
    if fs::metadata(parent_directory).await.is_err() {
        return true;
    }
    let Ok(mut entries) = fs::read_dir(parent_directory).await else {
        return false;
    };

    let mut pending_deletions = sidecar_thumbnail_paths
        .iter()
        .filter(|path| path.parent() == Some(parent_directory))
        .cloned()
        .collect::<Vec<_>>();
    pending_deletions.push(target_path.to_path_buf());

    loop {
        match entries.next_entry().await {
            Ok(Some(entry)) => {
                if !pending_deletions.iter().any(|path| path == &entry.path()) {
                    return true;
                }
            }
            Ok(None) => break,
            Err(_) => return false,
        }
    }

    // A delete-book task promises to remove the now-empty parent directory too. If that final
    // directory cleanup would fail, skipping early avoids partially deleting files and then
    // bailing out on Windows readonly directories.
    directory_delete_prerequisites_met(parent_directory).await
}

async fn remove_empty_directory(target_directory: &Path) -> Result<(), TaskProcessingError> {
    if fs::metadata(target_directory).await.is_err() {
        return Ok(());
    }
    let mut entries = fs::read_dir(target_directory).await.map_err(|error| {
        TaskProcessingError::runtime(format!(
            "failed to list directory {} before deletion: {error}",
            target_directory.display()
        ))
    })?;
    if entries
        .next_entry()
        .await
        .map_err(|error| {
            TaskProcessingError::runtime(format!(
                "failed to read directory {} before deletion: {error}",
                target_directory.display()
            ))
        })?
        .is_none()
    {
        delete_directory_if_exists(target_directory).await?;
    }
    Ok(())
}

async fn deletion_prerequisites_met(target_path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(target_path).await else {
        return false;
    };
    if metadata.is_dir() {
        return directory_delete_prerequisites_met(target_path).await;
    }
    fs::OpenOptions::new()
        .write(true)
        .open(target_path)
        .await
        .is_ok()
}

async fn directory_delete_prerequisites_met(target_directory: &Path) -> bool {
    // Windows can still allow child-file creation inside a readonly directory while refusing to
    // remove the directory itself, so delete preconditions must reject readonly metadata before
    // treating the directory as safe for book/series cleanup.
    match fs::metadata(target_directory).await {
        Ok(metadata) if metadata.permissions().readonly() => false,
        Ok(_) => directory_is_writable(target_directory).await,
        Err(_) => false,
    }
}

async fn remove_sidecar_thumbnail_files<T: AsRef<Path>>(
    sidecar_thumbnail_paths: &[T],
) -> Result<(), TaskProcessingError> {
    for sidecar_thumbnail_path in sidecar_thumbnail_paths {
        let sidecar_thumbnail_path = sidecar_thumbnail_path.as_ref();
        if deletion_prerequisites_met(sidecar_thumbnail_path).await {
            delete_file_if_exists(sidecar_thumbnail_path, "sidecar thumbnail file").await?;
        }
    }
    Ok(())
}

pub(super) async fn delete_series(
    runtime: &JobRuntime<'_>,
    series_id: &str,
) -> Result<(), TaskProcessingError> {
    if !runtime.database().owns_main_database() {
        return Ok(());
    }

    let Some(context) = load_series_delete_sse_context(runtime.database().read_pool(), series_id)
        .await
        .map_err(TaskProcessingError::runtime)?
    else {
        return Ok(());
    };
    let work = load_series_delete_work(runtime.database().read_pool(), series_id)
        .await
        .map_err(TaskProcessingError::runtime)?;

    let Some(series_path) = work.series_path.clone() else {
        return Ok(());
    };
    let series_path_for_check = series_path.clone();
    // A delete-series task promises to remove the series directory itself. If that directory is
    // already missing or cannot be deleted safely, abort before cascading into child soft-deletes
    // so the database never drifts ahead of the filesystem preconditions.
    let can_delete_series = deletion_prerequisites_met(&series_path_for_check).await;
    if !can_delete_series {
        return Ok(());
    }

    for book_id in &work.book_ids {
        delete_book(runtime, book_id).await?;
    }

    let sidecar_thumbnail_paths = work.sidecar_thumbnail_paths.clone();
    remove_sidecar_thumbnail_files(&sidecar_thumbnail_paths).await?;
    remove_empty_directory(&series_path).await?;

    soft_delete_series_book_rows(runtime.database().write_pool(), series_id)
        .await
        .map_err(TaskProcessingError::runtime)?;

    soft_delete_series_rows(runtime.database().write_pool(), series_id)
        .await
        .map_err(TaskProcessingError::runtime)?;

    let search = runtime.search_engine();
    for book_id in &work.book_ids {
        search
            .delete_book(book_id)
            .await
            .map_err(TaskProcessingError::runtime)?;
    }
    search
        .delete_series(series_id)
        .await
        .map_err(TaskProcessingError::runtime)?;

    emit_series_changed_after_file_delete(series_id, &context.library_id);

    Ok(())
}

async fn delete_file_if_exists(
    target_path: &Path,
    target_kind: &str,
) -> Result<(), TaskProcessingError> {
    match fs::remove_file(target_path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(TaskProcessingError::runtime(format!(
            "failed to delete {target_kind} {}: {error}",
            target_path.display()
        ))),
    }
}

async fn delete_directory_if_exists(target_path: &Path) -> Result<(), TaskProcessingError> {
    match fs::remove_dir(target_path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(TaskProcessingError::runtime(format!(
            "failed to delete directory {}: {error}",
            target_path.display()
        ))),
    }
}

async fn directory_is_writable(target_directory: &Path) -> bool {
    for nonce in 0..3 {
        let probe_path = target_directory.join(format!(
            ".komga-delete-write-probe-{}-{}-{nonce}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|value| value.as_nanos())
                .unwrap_or_default()
        ));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&probe_path)
            .await
        {
            Ok(file) => {
                drop(file);
                let _ = fs::remove_file(probe_path).await;
                return true;
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(_) => return false,
        }
    }
    false
}

use crate::{resolve_library_item_path, resolve_optional_library_item_path};

#[derive(Clone, Debug)]
pub struct PersistedDeleteBookDecision {
    pub series_id: String,
    pub oneshot: bool,
}

#[derive(Clone, Debug)]
pub struct PersistedDeleteBookSseContext {
    pub series_id: String,
    pub library_id: String,
}

#[derive(Clone, Debug)]
pub struct PersistedDeleteBookWork {
    pub series_id: String,
    pub book_path: PathBuf,
    pub sidecar_thumbnail_paths: Vec<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct PersistedDeleteSeriesWork {
    pub book_ids: Vec<String>,
    pub series_path: Option<PathBuf>,
    pub sidecar_thumbnail_paths: Vec<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct PersistedDeleteSeriesSseContext {
    pub library_id: String,
}

pub async fn load_book_delete_decision(
    pool: &SqlitePool,
    book_id: &str,
) -> Result<Option<PersistedDeleteBookDecision>, String> {
    let row = sqlx::query(
        r#"
        SELECT SERIES_ID, COALESCE(oneshot, 0) AS ONESHOT
        FROM BOOK
        WHERE ID = ?
        LIMIT 1
        "#,
    )
    .bind(book_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("failed to resolve delete-book target for '{book_id}': {error}"))?;

    Ok(row.map(|row| PersistedDeleteBookDecision {
        series_id: row.get::<String, _>("SERIES_ID"),
        oneshot: row.get::<i64, _>("ONESHOT") != 0,
    }))
}

pub async fn load_book_delete_sse_context(
    pool: &SqlitePool,
    book_id: &str,
) -> Result<Option<PersistedDeleteBookSseContext>, String> {
    let row = sqlx::query(
        r#"
        SELECT SERIES_ID, LIBRARY_ID
        FROM BOOK
        WHERE ID = ?
        LIMIT 1
        "#,
    )
    .bind(book_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("failed to load book delete SSE context for '{book_id}': {error}"))?;

    Ok(row.map(|row| PersistedDeleteBookSseContext {
        series_id: row.get::<String, _>("SERIES_ID"),
        library_id: row.get::<String, _>("LIBRARY_ID"),
    }))
}

pub async fn load_book_delete_work(
    pool: &SqlitePool,
    book_id: &str,
) -> Result<Option<PersistedDeleteBookWork>, String> {
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
    .bind(book_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("failed to load book delete target for '{book_id}': {error}"))?;

    let sidecar_rows = sqlx::query(
        r#"
        SELECT tb.URL AS URL,
               l.ROOT AS LIBRARY_ROOT
        FROM THUMBNAIL_BOOK tb
        JOIN BOOK b ON b.ID = tb.BOOK_ID
        JOIN LIBRARY l ON l.ID = b.LIBRARY_ID
        WHERE tb.BOOK_ID = ?
          AND tb.TYPE = 'SIDECAR'
          AND tb.URL IS NOT NULL
        ORDER BY tb.ID ASC
        "#,
    )
    .bind(book_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("failed to load sidecar thumbnails for '{book_id}': {error}"))?;

    Ok(row.map(|row| PersistedDeleteBookWork {
        series_id: row.get::<String, _>("SERIES_ID"),
        book_path: resolve_library_item_path(
            row.get::<String, _>("LIBRARY_ROOT").as_str(),
            row.get::<String, _>("BOOK_URL").as_str(),
        ),
        sidecar_thumbnail_paths: sidecar_rows
            .iter()
            .filter_map(|sidecar| {
                resolve_optional_library_item_path(
                    Some(sidecar.get::<String, _>("LIBRARY_ROOT").as_str()),
                    sidecar.get::<String, _>("URL").as_str(),
                )
            })
            .collect(),
    }))
}

pub async fn soft_delete_book_rows(
    pool: &SqlitePool,
    book_id: &str,
    series_id: &str,
) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(|error| {
        format!("failed to start soft-delete-book transaction for '{book_id}': {error}")
    })?;

    sqlx::query(
        r#"
        UPDATE BOOK
        SET DELETED_DATE = CURRENT_TIMESTAMP,
            LAST_MODIFIED_DATE = STRFTIME('%Y-%m-%d %H:%M:%f', 'now')
        WHERE ID = ?
        "#,
    )
    .bind(book_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("failed to soft-delete BOOK row for '{book_id}': {error}"))?;

    sqlx::query(
        r#"
        UPDATE SERIES
        SET BOOK_COUNT = (
            SELECT COUNT(*)
            FROM BOOK
            WHERE BOOK.SERIES_ID = SERIES.ID
              AND BOOK.DELETED_DATE IS NULL
        ),
            LAST_MODIFIED_DATE = STRFTIME('%Y-%m-%d %H:%M:%f', 'now')
        WHERE ID = ?
        "#,
    )
    .bind(series_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| {
        format!(
            "failed to refresh active series count for '{series_id}' while soft-deleting book '{book_id}': {error}"
        )
    })?;

    tx.commit().await.map_err(|error| {
        format!("failed to commit soft-delete-book transaction for '{book_id}': {error}")
    })?;

    Ok(())
}

pub async fn load_series_delete_sse_context(
    pool: &SqlitePool,
    series_id: &str,
) -> Result<Option<PersistedDeleteSeriesSseContext>, String> {
    let row = sqlx::query(
        r#"
        SELECT LIBRARY_ID
        FROM SERIES
        WHERE ID = ?
        LIMIT 1
        "#,
    )
    .bind(series_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| {
        format!("failed to load series delete SSE context for '{series_id}': {error}")
    })?;

    Ok(row.map(|row| PersistedDeleteSeriesSseContext {
        library_id: row.get::<String, _>("LIBRARY_ID"),
    }))
}

pub async fn load_series_delete_work(
    pool: &SqlitePool,
    series_id: &str,
) -> Result<PersistedDeleteSeriesWork, String> {
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
    .bind(series_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("failed to load series books for delete '{series_id}': {error}"))?;

    let series_row = sqlx::query(
        r#"
        SELECT s.URL AS SERIES_URL,
               l.ROOT AS LIBRARY_ROOT
        FROM SERIES s
        JOIN LIBRARY l ON l.ID = s.LIBRARY_ID
        WHERE s.ID = ?
        LIMIT 1
        "#,
    )
    .bind(series_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("failed to load series path for delete '{series_id}': {error}"))?;

    let sidecar_rows = sqlx::query(
        r#"
        SELECT ts.URL AS URL,
               l.ROOT AS LIBRARY_ROOT
        FROM THUMBNAIL_SERIES ts
        JOIN SERIES s ON s.ID = ts.SERIES_ID
        JOIN LIBRARY l ON l.ID = s.LIBRARY_ID
        WHERE ts.SERIES_ID = ?
          AND ts.TYPE = 'SIDECAR'
          AND ts.URL IS NOT NULL
        ORDER BY ts.ID ASC
        "#,
    )
    .bind(series_id)
    .fetch_all(pool)
    .await
    .map_err(|error| {
        format!("failed to load series sidecar thumbnails for '{series_id}': {error}")
    })?;

    Ok(PersistedDeleteSeriesWork {
        book_ids: rows
            .iter()
            .map(|row| row.get::<String, _>("BOOK_ID"))
            .collect(),
        series_path: series_row.map(|row| {
            resolve_library_item_path(
                row.get::<String, _>("LIBRARY_ROOT").as_str(),
                row.get::<String, _>("SERIES_URL").as_str(),
            )
        }),
        sidecar_thumbnail_paths: sidecar_rows
            .iter()
            .filter_map(|row| {
                resolve_optional_library_item_path(
                    Some(row.get::<String, _>("LIBRARY_ROOT").as_str()),
                    row.get::<String, _>("URL").as_str(),
                )
            })
            .collect(),
    })
}

pub async fn soft_delete_series_rows(pool: &SqlitePool, series_id: &str) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(|error| {
        format!("failed to start soft-delete-series transaction for '{series_id}': {error}")
    })?;

    sqlx::query(
        r#"
        UPDATE SERIES
        SET DELETED_DATE = CURRENT_TIMESTAMP,
            LAST_MODIFIED_DATE = STRFTIME('%Y-%m-%d %H:%M:%f', 'now')
        WHERE ID = ?
        "#,
    )
    .bind(series_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("failed to soft-delete SERIES row for '{series_id}': {error}"))?;

    tx.commit().await.map_err(|error| {
        format!("failed to commit soft-delete-series transaction for '{series_id}': {error}")
    })?;

    Ok(())
}

pub async fn soft_delete_series_book_rows(
    pool: &SqlitePool,
    series_id: &str,
) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(|error| {
        format!("failed to start soft-delete-series-books transaction for '{series_id}': {error}")
    })?;

    sqlx::query(
        r#"
        UPDATE BOOK
        SET DELETED_DATE = CURRENT_TIMESTAMP,
            LAST_MODIFIED_DATE = STRFTIME('%Y-%m-%d %H:%M:%f', 'now')
        WHERE SERIES_ID = ?
        "#,
    )
    .bind(series_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| {
        format!("failed to soft-delete BOOK rows for series '{series_id}': {error}")
    })?;

    sqlx::query(
        r#"
        UPDATE SERIES
        SET BOOK_COUNT = (
            SELECT COUNT(*)
            FROM BOOK
            WHERE BOOK.SERIES_ID = SERIES.ID
              AND BOOK.DELETED_DATE IS NULL
        ),
            LAST_MODIFIED_DATE = STRFTIME('%Y-%m-%d %H:%M:%f', 'now')
        WHERE ID = ?
        "#,
    )
    .bind(series_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| {
        format!(
            "failed to refresh active series count for '{series_id}' while soft-deleting series books: {error}"
        )
    })?;

    tx.commit().await.map_err(|error| {
        format!("failed to commit soft-delete-series-books transaction for '{series_id}': {error}")
    })?;

    Ok(())
}
