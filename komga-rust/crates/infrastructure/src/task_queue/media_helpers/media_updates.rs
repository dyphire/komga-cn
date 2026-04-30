use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use komga_application::runtime_sse::register_runtime_sse_event;
use serde_json::json;
use sqlx::{Row, Sqlite, SqlitePool};

use super::media_queries::PersistedHashedPageToDelete;
use crate::sqlite::connect_private_write_pool;

#[derive(Clone)]
struct BookSseContext {
    series_id: String,
    library_id: String,
}

fn emit_book_changed(book_id: &str, context: &BookSseContext) {
    register_runtime_sse_event(
        "BookChanged",
        json!({
            "bookId": book_id,
            "seriesId": context.series_id,
            "libraryId": context.library_id,
        }),
        false,
        None,
    );
}

async fn load_book_sse_context(
    pool: &SqlitePool,
    book_id: &str,
) -> Result<Option<BookSseContext>, String> {
    sqlx::query("SELECT SERIES_ID, LIBRARY_ID FROM BOOK WHERE ID = ? LIMIT 1")
        .bind(book_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| format!("failed to load book SSE context for '{book_id}': {error}"))
        .map(|row| {
            row.map(|row| BookSseContext {
                series_id: row.get::<String, _>("SERIES_ID"),
                library_id: row.get::<String, _>("LIBRARY_ID"),
            })
        })
}

pub async fn persist_book_hash(
    database_file: &Path,
    book_id: &str,
    hash: &str,
    koreader: bool,
) -> Result<(), String> {
    let database_file = database_file.to_path_buf();
    let pool = connect_private_write_pool(database_file.as_path())
        .await
        .map_err(|error| format!("failed to open sqlite pool: {error}"))?;
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
        .bind(book_id)
        .execute(&pool)
        .await
        .map_err(|error| format!("failed to persist book hash for '{book_id}': {error}"))?;
    pool.close().await;

    Ok(())
}

pub async fn persist_removed_hashed_pages(
    database_file: &Path,
    book_id: &str,
    deleted_count_by_hash: &HashMap<String, i64>,
    file_last_modified: i64,
    file_size: i64,
) -> Result<(), String> {
    let pool = connect_private_write_pool(database_file)
        .await
        .map_err(|error| format!("failed to open sqlite pool: {error}"))?;
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
        SET FILE_LAST_MODIFIED = datetime(?, 'unixepoch'),
            FILE_SIZE = ?,
            FILE_HASH = '',
            FILE_HASH_KOREADER = '',
            LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
        WHERE ID = ?
        "#,
    )
    .bind(file_last_modified)
    .bind(file_size)
    .bind(book_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| {
        format!("failed to update BOOK metadata after hashed-page removal for '{book_id}': {error}")
    })?;

    tx.commit().await.map_err(|error| {
        format!("failed to commit remove-hashed-pages transaction for '{book_id}': {error}")
    })?;

    let book_context = load_book_sse_context(&pool, book_id).await?;
    pool.close().await;

    if let Some(book_context) = book_context {
        emit_book_changed(book_id, &book_context);
    }
    Ok(())
}

pub async fn persist_book_extension_repair(
    database_file: &Path,
    book_id: &str,
    library_id: &str,
    book_url: &str,
    destination_url: &str,
    file_last_modified: i64,
    file_size: i64,
) -> Result<(), String> {
    let pool = connect_private_write_pool(database_file)
        .await
        .map_err(|error| format!("failed to open sqlite pool: {error}"))?;
    let mut tx = pool.begin().await.map_err(|error| {
        format!("failed to start extension-repair transaction for '{book_id}': {error}")
    })?;

    sqlx::query(
        r#"
        UPDATE BOOK
        SET URL = ?,
            FILE_LAST_MODIFIED = datetime(?, 'unixepoch'),
            FILE_SIZE = ?,
            LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
        WHERE ID = ?
        "#,
    )
    .bind(destination_url)
    .bind(file_last_modified)
    .bind(file_size)
    .bind(book_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| {
        format!("failed to update BOOK row during extension repair for '{book_id}': {error}")
    })?;

    sqlx::query(
        r#"
        UPDATE SIDECAR
        SET PARENT_URL = ?
        WHERE LIBRARY_ID = ?
        AND PARENT_URL = ?
        "#,
    )
    .bind(destination_url)
    .bind(library_id)
    .bind(book_url)
    .execute(&mut *tx)
    .await
    .map_err(|error| {
        format!("failed to update SIDECAR rows during extension repair for '{book_id}': {error}")
    })?;

    tx.commit().await.map_err(|error| {
        format!("failed to commit extension-repair transaction for '{book_id}': {error}")
    })?;
    pool.close().await;

    Ok(())
}

pub async fn persist_book_conversion(
    database_file: &Path,
    book_id: &str,
    library_id: &str,
    book_url: &str,
    destination_url: &str,
    file_last_modified: i64,
    file_size: i64,
) -> Result<(), String> {
    let pool = connect_private_write_pool(database_file)
        .await
        .map_err(|error| format!("failed to open sqlite pool: {error}"))?;
    let mut tx = pool.begin().await.map_err(|error| {
        format!("failed to start convert-book transaction for '{book_id}': {error}")
    })?;

    sqlx::query(
        r#"
        UPDATE BOOK
        SET URL = ?,
            FILE_LAST_MODIFIED = datetime(?, 'unixepoch'),
            FILE_SIZE = ?,
            FILE_HASH = '',
            FILE_HASH_KOREADER = '',
            LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
        WHERE ID = ?
        "#,
    )
    .bind(destination_url)
    .bind(file_last_modified)
    .bind(file_size)
    .bind(book_id)
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
    .bind(destination_url)
    .bind(library_id)
    .bind(book_url)
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
    .bind(book_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| {
        format!("failed to refresh MEDIA row during conversion for '{book_id}': {error}")
    })?;

    tx.commit().await.map_err(|error| {
        format!("failed to commit convert-book transaction for '{book_id}': {error}")
    })?;

    let book_context = load_book_sse_context(&pool, book_id).await?;
    pool.close().await;

    if let Some(book_context) = book_context {
        emit_book_changed(book_id, &book_context);
    }
    Ok(())
}

pub async fn adjust_analyzed_book_read_progress(
    database_file: &Path,
    book_id: &str,
    series_id: &str,
    previous_media_status: &str,
    previous_page_count: i64,
    current_page_count: i64,
) -> Result<(), String> {
    if !previous_media_status.eq_ignore_ascii_case("OUTDATED")
        || previous_page_count == current_page_count
    {
        return Ok(());
    }

    let current_page_count = current_page_count.max(0);
    let pool = connect_private_write_pool(database_file)
        .await
        .map_err(|error| format!("failed to open sqlite pool: {error}"))?;
    let mut tx = pool.begin().await.map_err(|error| {
        format!("failed to start analyze-book read-progress adjustment for '{book_id}': {error}")
    })?;

    let progress_rows = sqlx::query(
        "SELECT USER_ID, COMPLETED FROM READ_PROGRESS WHERE BOOK_ID = ?",
    )
    .bind(book_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| {
        format!(
            "failed to load READ_PROGRESS rows for analyze-book adjustment '{book_id}': {error}"
        )
    })?;

    if progress_rows.is_empty() {
        tx.commit().await.map_err(|error| {
            format!("failed to commit empty analyze-book read-progress adjustment for '{book_id}': {error}")
        })?;
        pool.close().await;
        return Ok(());
    }

    let mut affected_user_ids = HashSet::new();
    for row in progress_rows {
        let user_id = row.get::<String, _>("USER_ID");
        let completed = row
            .get::<Option<i64>, _>("COMPLETED")
            .is_some_and(|value| value != 0);
        let adjusted_page = if completed { current_page_count } else { 1_i64 };
        sqlx::query(
            "UPDATE READ_PROGRESS SET PAGE = ?, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP WHERE BOOK_ID = ? AND USER_ID = ?",
        )
        .bind(adjusted_page)
        .bind(book_id)
        .bind(&user_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| {
            format!(
                "failed to update READ_PROGRESS for analyze-book adjustment '{book_id}' user '{user_id}': {error}"
            )
        })?;
        affected_user_ids.insert(user_id);
    }

    if !series_id.is_empty() {
        for user_id in &affected_user_ids {
            upsert_series_read_progress_row(&mut tx, series_id, user_id)
                .await
                .map_err(|error| {
                    format!(
                        "failed to refresh READ_PROGRESS_SERIES during analyze-book adjustment '{book_id}' for user '{user_id}': {error}"
                    )
                })?;
        }
    }

    tx.commit().await.map_err(|error| {
        format!("failed to commit analyze-book read-progress adjustment for '{book_id}': {error}")
    })?;
    pool.close().await;

    Ok(())
}

pub async fn persist_book_conversion_events(
    database_file: &Path,
    book_id: &str,
    series_id: &str,
    source_path: &Path,
    destination_path: &Path,
    source_deleted: bool,
) -> Result<(), String> {
    let source_name = source_path.to_string_lossy().to_string();
    let destination_name = destination_path.to_string_lossy().to_string();
    let pool = connect_private_write_pool(database_file)
        .await
        .map_err(|error| format!("failed to open sqlite pool: {error}"))?;
    let mut tx = pool.begin().await.map_err(|error| {
        format!("failed to start historical conversion-event transaction for '{book_id}': {error}")
    })?;

    if source_deleted {
        insert_historical_event(
            &mut tx,
            "BookFileDeleted",
            book_id,
            series_id,
            vec![
                (
                    "reason",
                    "File was deleted after conversion to CBZ".to_string(),
                ),
                ("name", source_name.clone()),
            ],
        )
        .await
        .map_err(|error| {
            format!(
                "failed to insert BookFileDeleted event for converted book '{book_id}': {error}"
            )
        })?;
    }

    insert_historical_event(
        &mut tx,
        "BookConverted",
        book_id,
        series_id,
        vec![("name", destination_name), ("former file", source_name)],
    )
    .await
    .map_err(|error| format!("failed to insert BookConverted event for '{book_id}': {error}"))?;

    tx.commit().await.map_err(|error| {
        format!("failed to commit historical conversion-event transaction for '{book_id}': {error}")
    })?;
    pool.close().await;

    Ok(())
}

pub async fn persist_book_page_hashes(
    database_file: &Path,
    book_id: &str,
    page_hashes: &[(i64, String)],
) -> Result<(), String> {
    if page_hashes.is_empty() {
        return Ok(());
    }

    let pool = connect_private_write_pool(database_file)
        .await
        .map_err(|error| format!("failed to open sqlite pool: {error}"))?;
    let mut tx = pool.begin().await.map_err(|error| {
        format!("failed to start restore-page-hash transaction for '{book_id}': {error}")
    })?;

    for (page_number, file_hash) in page_hashes {
        sqlx::query("UPDATE MEDIA_PAGE SET FILE_HASH = ? WHERE BOOK_ID = ? AND NUMBER = ?")
            .bind(file_hash)
            .bind(book_id)
            .bind(page_number.saturating_sub(1))
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                format!(
                    "failed to restore MEDIA_PAGE hash for '{book_id}' page {}: {error}",
                    page_number
                )
            })?;
    }

    tx.commit().await.map_err(|error| {
        format!("failed to commit restore-page-hash transaction for '{book_id}': {error}")
    })?;
    pool.close().await;

    Ok(())
}

pub async fn persist_duplicate_page_deleted_events(
    database_file: &Path,
    book_id: &str,
    series_id: &str,
    book_path: &Path,
    removed_pages: &[PersistedHashedPageToDelete],
) -> Result<(), String> {
    if removed_pages.is_empty() {
        return Ok(());
    }

    let book_name = book_path.to_string_lossy().to_string();
    let pool = connect_private_write_pool(database_file)
        .await
        .map_err(|error| format!("failed to open sqlite pool: {error}"))?;
    let mut tx = pool.begin().await.map_err(|error| {
        format!("failed to start duplicate-page-deleted transaction for '{book_id}': {error}")
    })?;

    for removed_page in removed_pages {
        insert_historical_event(
            &mut tx,
            "DuplicatePageDeleted",
            book_id,
            series_id,
            vec![
                ("name", book_name.clone()),
                ("page number", removed_page.page_number.to_string()),
                ("page file name", removed_page.file_name.clone()),
                ("page file hash", removed_page.file_hash.clone()),
                ("page file size", removed_page.file_size.to_string()),
                ("page media type", removed_page.media_type.clone()),
            ],
        )
        .await
        .map_err(|error| {
            format!("failed to insert DuplicatePageDeleted event for '{book_id}': {error}")
        })?;
    }

    tx.commit().await.map_err(|error| {
        format!("failed to commit duplicate-page-deleted transaction for '{book_id}': {error}")
    })?;
    pool.close().await;

    Ok(())
}

async fn insert_historical_event(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    event_type: &str,
    book_id: &str,
    series_id: &str,
    properties: Vec<(&str, String)>,
) -> Result<(), sqlx::Error> {
    let event_id = generated_historical_event_id();
    sqlx::query("INSERT INTO HISTORICAL_EVENT (ID, TYPE, BOOK_ID, SERIES_ID) VALUES (?, ?, ?, ?)")
        .bind(&event_id)
        .bind(event_type)
        .bind(book_id)
        .bind(series_id)
        .execute(&mut **tx)
        .await?;

    for (key, value) in properties {
        sqlx::query(
            "INSERT INTO HISTORICAL_EVENT_PROPERTIES (ID, \"KEY\", VALUE) VALUES (?, ?, ?)",
        )
        .bind(&event_id)
        .bind(key)
        .bind(value)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

async fn upsert_series_read_progress_row(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    series_id: &str,
    user_id: &str,
) -> Result<(), sqlx::Error> {
    let row = sqlx::query(
        r#"SELECT
            COALESCE(SUM(CASE WHEN rp.COMPLETED = 1 THEN 1 ELSE 0 END), 0) AS READ_COUNT,
            COALESCE(SUM(CASE WHEN rp.COMPLETED = 0 THEN 1 ELSE 0 END), 0) AS IN_PROGRESS_COUNT,
            MAX(rp.READ_DATE) AS MOST_RECENT_READ_DATE
        FROM BOOK b
        LEFT JOIN READ_PROGRESS rp ON rp.BOOK_ID = b.ID AND rp.USER_ID = ?
        WHERE b.SERIES_ID = ? AND b.DELETED_DATE IS NULL"#,
    )
    .bind(user_id)
    .bind(series_id)
    .fetch_one(&mut **tx)
    .await?;

    sqlx::query(
        r#"INSERT INTO READ_PROGRESS_SERIES (
            SERIES_ID,
            USER_ID,
            READ_COUNT,
            IN_PROGRESS_COUNT,
            MOST_RECENT_READ_DATE,
            LAST_MODIFIED_DATE
        ) VALUES (?, ?, ?, ?, ?, CURRENT_TIMESTAMP)
        ON CONFLICT(SERIES_ID, USER_ID) DO UPDATE
        SET READ_COUNT = excluded.READ_COUNT,
            IN_PROGRESS_COUNT = excluded.IN_PROGRESS_COUNT,
            MOST_RECENT_READ_DATE = excluded.MOST_RECENT_READ_DATE,
            LAST_MODIFIED_DATE = CURRENT_TIMESTAMP"#,
    )
    .bind(series_id)
    .bind(user_id)
    .bind(row.get::<i64, _>("READ_COUNT"))
    .bind(row.get::<i64, _>("IN_PROGRESS_COUNT"))
    .bind(row.get::<Option<String>, _>("MOST_RECENT_READ_DATE"))
    .execute(&mut **tx)
    .await?;

    Ok(())
}

fn generated_historical_event_id() -> String {
    random_prefixed_id("historical-event")
}

fn random_prefixed_id(prefix: &str) -> String {
    format!("{prefix}-{}", random_hex_token(12))
}

fn random_hex_token(byte_len: usize) -> String {
    let mut bytes = vec![0u8; byte_len];
    if let Ok(mut file) = std::fs::File::open("/dev/urandom") {
        let _ = file.read_exact(&mut bytes);
    } else {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = ((seed >> ((index % 8) * 8)) as u8) ^ (index as u8).wrapping_mul(29);
        }
    }

    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
