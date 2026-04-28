use std::path::{Path, PathBuf};

use sqlx::Row;

use crate::sqlite::connect_private_write_pool;
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
    database_file: &Path,
    book_id: &str,
) -> Result<Option<PersistedDeleteBookDecision>, String> {
    let pool = connect_private_write_pool(database_file)
        .await
        .map_err(|error| format!("failed to open sqlite pool: {error}"))?;
    let row = sqlx::query(
        r#"
        SELECT SERIES_ID, COALESCE(oneshot, 0) AS ONESHOT
        FROM BOOK
        WHERE ID = ?
        LIMIT 1
        "#,
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("failed to resolve delete-book target for '{book_id}': {error}"))?;
    pool.close().await;

    Ok(row.map(|row| PersistedDeleteBookDecision {
        series_id: row.get::<String, _>("SERIES_ID"),
        oneshot: row.get::<i64, _>("ONESHOT") != 0,
    }))
}

pub async fn load_book_delete_sse_context(
    database_file: &Path,
    book_id: &str,
) -> Result<Option<PersistedDeleteBookSseContext>, String> {
    let pool = connect_private_write_pool(database_file)
        .await
        .map_err(|error| format!("failed to open sqlite pool: {error}"))?;
    let row = sqlx::query(
        r#"
        SELECT SERIES_ID, LIBRARY_ID
        FROM BOOK
        WHERE ID = ?
        LIMIT 1
        "#,
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("failed to load book delete SSE context for '{book_id}': {error}"))?;
    pool.close().await;

    Ok(row.map(|row| PersistedDeleteBookSseContext {
        series_id: row.get::<String, _>("SERIES_ID"),
        library_id: row.get::<String, _>("LIBRARY_ID"),
    }))
}

pub async fn load_book_delete_work(
    database_file: &Path,
    book_id: &str,
) -> Result<Option<PersistedDeleteBookWork>, String> {
    let pool = connect_private_write_pool(database_file)
        .await
        .map_err(|error| format!("failed to open sqlite pool: {error}"))?;
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
    .fetch_optional(&pool)
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
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("failed to load sidecar thumbnails for '{book_id}': {error}"))?;
    pool.close().await;

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
    database_file: &Path,
    book_id: &str,
    series_id: &str,
) -> Result<(), String> {
    let pool = connect_private_write_pool(database_file)
        .await
        .map_err(|error| format!("failed to open sqlite pool: {error}"))?;
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
    pool.close().await;

    Ok(())
}

pub async fn load_series_delete_sse_context(
    database_file: &Path,
    series_id: &str,
) -> Result<Option<PersistedDeleteSeriesSseContext>, String> {
    let pool = connect_private_write_pool(database_file)
        .await
        .map_err(|error| format!("failed to open sqlite pool: {error}"))?;
    let row = sqlx::query(
        r#"
        SELECT LIBRARY_ID
        FROM SERIES
        WHERE ID = ?
        LIMIT 1
        "#,
    )
    .bind(series_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| {
        format!("failed to load series delete SSE context for '{series_id}': {error}")
    })?;
    pool.close().await;

    Ok(row.map(|row| PersistedDeleteSeriesSseContext {
        library_id: row.get::<String, _>("LIBRARY_ID"),
    }))
}

pub async fn load_series_delete_work(
    database_file: &Path,
    series_id: &str,
) -> Result<PersistedDeleteSeriesWork, String> {
    let pool = connect_private_write_pool(database_file)
        .await
        .map_err(|error| format!("failed to open sqlite pool: {error}"))?;
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
    .fetch_all(&pool)
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
    .fetch_optional(&pool)
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
    .fetch_all(&pool)
    .await
    .map_err(|error| {
        format!("failed to load series sidecar thumbnails for '{series_id}': {error}")
    })?;
    pool.close().await;

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

pub async fn soft_delete_series_rows(database_file: &Path, series_id: &str) -> Result<(), String> {
    let pool = connect_private_write_pool(database_file)
        .await
        .map_err(|error| format!("failed to open sqlite pool: {error}"))?;
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
    pool.close().await;

    Ok(())
}

pub async fn soft_delete_series_book_rows(
    database_file: &Path,
    series_id: &str,
) -> Result<(), String> {
    let pool = connect_private_write_pool(database_file)
        .await
        .map_err(|error| format!("failed to open sqlite pool: {error}"))?;
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
    pool.close().await;

    Ok(())
}
