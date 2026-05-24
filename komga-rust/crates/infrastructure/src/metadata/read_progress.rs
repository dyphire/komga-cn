use std::collections::HashMap;

use komga_application::runtime_sse::register_runtime_sse_event;
use serde_json::Value;
use serde_json::json;
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};

fn serialize_locator(locator: Option<&Value>) -> Vec<u8> {
    locator
        .and_then(|value| serde_json::to_vec(value).ok())
        .unwrap_or_default()
}

async fn require_user_exists(
    pool: &SqlitePool,
    user_id_value: &str,
    query_context: &str,
) -> Result<(), String> {
    let user_exists = sqlx::query("SELECT 1 FROM USER WHERE ID = ? LIMIT 1")
        .bind(user_id_value)
        .fetch_optional(pool)
        .await
        .map_err(|error| format!("query {query_context} user: {error}"))?
        .is_some();

    if !user_exists {
        return Err("read-progress user not found".to_string());
    }

    Ok(())
}

async fn load_book_series_id(
    pool: &SqlitePool,
    book_id: &str,
    query_context: &str,
) -> Result<Option<String>, String> {
    sqlx::query("SELECT SERIES_ID FROM BOOK WHERE ID = ? LIMIT 1")
        .bind(book_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| format!("query {query_context} book series: {error}"))
        .map(|row| row.map(|row| row.get::<String, _>("SERIES_ID")))
}

async fn sync_series_read_progress(
    pool: &SqlitePool,
    series_id: &str,
    user_id_value: &str,
    query_context: &str,
) -> Result<(), String> {
    let row = sqlx::query(
        r#"
        SELECT COUNT(rp.BOOK_ID) AS PROGRESS_COUNT,
               COALESCE(SUM(CASE WHEN rp.COMPLETED = 1 THEN 1 ELSE 0 END), 0) AS READ_COUNT,
               COALESCE(SUM(CASE WHEN rp.COMPLETED = 0 THEN 1 ELSE 0 END), 0) AS IN_PROGRESS_COUNT,
               MAX(rp.READ_DATE) AS MOST_RECENT_READ_DATE
        FROM BOOK b
        LEFT JOIN READ_PROGRESS rp ON rp.BOOK_ID = b.ID AND rp.USER_ID = ?
        WHERE b.SERIES_ID = ? AND b.DELETED_DATE IS NULL
        "#,
    )
    .bind(user_id_value)
    .bind(series_id)
    .fetch_one(pool)
    .await
    .map_err(|error| format!("query {query_context} series read progress aggregates: {error}"))?;

    let progress_count = row.get::<i64, _>("PROGRESS_COUNT");
    if progress_count == 0 {
        sqlx::query("DELETE FROM READ_PROGRESS_SERIES WHERE SERIES_ID = ? AND USER_ID = ?")
            .bind(series_id)
            .bind(user_id_value)
            .execute(pool)
            .await
            .map_err(|error| format!("delete {query_context} series read progress row: {error}"))?;
        return Ok(());
    }

    sqlx::query(
        r#"
        INSERT INTO READ_PROGRESS_SERIES (SERIES_ID, USER_ID, READ_COUNT, IN_PROGRESS_COUNT, MOST_RECENT_READ_DATE, LAST_MODIFIED_DATE)
        VALUES (?, ?, ?, ?, ?, CURRENT_TIMESTAMP)
        ON CONFLICT(SERIES_ID, USER_ID) DO UPDATE
        SET READ_COUNT = excluded.READ_COUNT, IN_PROGRESS_COUNT = excluded.IN_PROGRESS_COUNT,
            MOST_RECENT_READ_DATE = excluded.MOST_RECENT_READ_DATE, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
        "#,
    )
    .bind(series_id)
    .bind(user_id_value)
    .bind(row.get::<i64, _>("READ_COUNT"))
    .bind(row.get::<i64, _>("IN_PROGRESS_COUNT"))
    .bind(row.get::<Option<String>, _>("MOST_RECENT_READ_DATE"))
    .execute(pool)
    .await
    .map_err(|error| format!("upsert {query_context} series read progress row: {error}"))?;

    Ok(())
}

async fn sync_series_read_progress_for_book(
    pool: &SqlitePool,
    book_id: &str,
    user_id_value: &str,
    query_context: &str,
) -> Result<(), String> {
    let Some(series_id) = load_book_series_id(pool, book_id, query_context).await? else {
        return Ok(());
    };

    sync_series_read_progress(pool, &series_id, user_id_value, query_context).await
}

async fn persisted_series_read_progress_exists(
    pool: &SqlitePool,
    series_id: &str,
    user_id_value: &str,
    query_context: &str,
) -> Result<bool, String> {
    sqlx::query(
        "SELECT 1 AS FOUND FROM READ_PROGRESS_SERIES WHERE SERIES_ID = ? AND USER_ID = ? LIMIT 1",
    )
    .bind(series_id)
    .bind(user_id_value)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("query {query_context} series read progress row: {error}"))
    .map(|row| row.is_some())
}

fn emit_read_progress_changed(book_id: &str, user_id_value: &str) {
    register_runtime_sse_event(
        "ReadProgressChanged",
        json!({
            "bookId": book_id,
            "userId": user_id_value,
        }),
        false,
        Some(user_id_value.to_string()),
    );
}

fn emit_read_progress_deleted(book_id: &str, user_id_value: &str) {
    register_runtime_sse_event(
        "ReadProgressDeleted",
        json!({
            "bookId": book_id,
            "userId": user_id_value,
        }),
        false,
        Some(user_id_value.to_string()),
    );
}

fn emit_read_progress_series_event(series_id: &str, user_id_value: &str, exists: bool) {
    register_runtime_sse_event(
        if exists {
            "ReadProgressSeriesChanged"
        } else {
            "ReadProgressSeriesDeleted"
        },
        json!({
            "seriesId": series_id,
            "userId": user_id_value,
        }),
        false,
        Some(user_id_value.to_string()),
    );
}

pub async fn persist_read_progress(
    pool: &SqlitePool,
    book_id: &str,
    user_id_value: &str,
    page: u64,
    completed: bool,
    locator: Option<Value>,
) -> Result<(), String> {
    require_user_exists(pool, user_id_value, "read-progress").await?;

    sqlx::query(
        r#"
        INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED, READ_DATE, DEVICE_ID, DEVICE_NAME, LOCATOR)
        VALUES (?, ?, ?, ?, CURRENT_TIMESTAMP, '', '', ?)
        ON CONFLICT(BOOK_ID, USER_ID) DO UPDATE
        SET PAGE = excluded.PAGE, COMPLETED = excluded.COMPLETED, READ_DATE = CURRENT_TIMESTAMP,
            DEVICE_ID = excluded.DEVICE_ID, DEVICE_NAME = excluded.DEVICE_NAME, LOCATOR = excluded.LOCATOR,
            LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
        "#,
    )
    .bind(book_id)
    .bind(user_id_value)
    .bind(page as i64)
    .bind(completed)
    .bind(serialize_locator(locator.as_ref()))
    .execute(pool)
    .await
    .map_err(|error| format!("persist read-progress: {error}"))?;

    let series_id = load_book_series_id(pool, book_id, "read-progress").await?;
    sync_series_read_progress_for_book(pool, book_id, user_id_value, "read-progress").await?;

    emit_read_progress_changed(book_id, user_id_value);
    if let Some(series_id) = series_id {
        let exists =
            persisted_series_read_progress_exists(pool, &series_id, user_id_value, "read-progress")
                .await?;
        emit_read_progress_series_event(&series_id, user_id_value, exists);
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn persist_book_progression(
    pool: &SqlitePool,
    book_id: &str,
    user_id_value: &str,
    progression: f64,
    use_locator_position_for_page: bool,
    modified: Option<String>,
    device_id: Option<String>,
    device_name: Option<String>,
    locator: Option<Value>,
) -> Result<(), String> {
    let page_count = load_book_page_count(pool, book_id)
        .await?
        .unwrap_or(1)
        .max(1);
    let total_progression = locator
        .as_ref()
        .and_then(|value| value.get("locations"))
        .and_then(|value| value.get("totalProgression"))
        .and_then(Value::as_f64);
    let effective_progression = total_progression.unwrap_or(progression);
    let page_from_progression = (effective_progression * page_count as f64)
        .round()
        .clamp(0.0, page_count as f64) as u64;
    let page = if use_locator_position_for_page {
        locator
            .as_ref()
            .and_then(|value| value.get("locations"))
            .and_then(|value| value.get("position"))
            .and_then(Value::as_u64)
            .filter(|value| *value >= 1)
            .unwrap_or(page_from_progression)
    } else {
        page_from_progression
    };
    let completed = effective_progression >= 0.99;
    require_user_exists(pool, user_id_value, "progression").await?;

    sqlx::query(
        r#"
        INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED, READ_DATE, DEVICE_ID, DEVICE_NAME, LOCATOR)
        VALUES (?, ?, ?, ?, COALESCE(?, CURRENT_TIMESTAMP), ?, ?, ?)
        ON CONFLICT(BOOK_ID, USER_ID) DO UPDATE
        SET PAGE = excluded.PAGE, COMPLETED = excluded.COMPLETED, READ_DATE = excluded.READ_DATE,
            DEVICE_ID = excluded.DEVICE_ID, DEVICE_NAME = excluded.DEVICE_NAME, LOCATOR = excluded.LOCATOR,
            LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
        "#,
    )
    .bind(book_id)
    .bind(user_id_value)
    .bind(page as i64)
    .bind(completed)
    .bind(modified)
    .bind(device_id.unwrap_or_default())
    .bind(device_name.unwrap_or_default())
    .bind(serialize_locator(locator.as_ref()))
    .execute(pool)
    .await
    .map_err(|error| format!("persist book progression: {error}"))?;

    let series_id = load_book_series_id(pool, book_id, "progression").await?;
    sync_series_read_progress_for_book(pool, book_id, user_id_value, "progression").await?;

    emit_read_progress_changed(book_id, user_id_value);
    if let Some(series_id) = series_id {
        let exists =
            persisted_series_read_progress_exists(pool, &series_id, user_id_value, "progression")
                .await?;
        emit_read_progress_series_event(&series_id, user_id_value, exists);
    }

    Ok(())
}

pub async fn load_book_progression(
    pool: &SqlitePool,
    book_id: &str,
    user_id_value: &str,
) -> Result<Option<Value>, String> {
    let row = sqlx::query(
        r#"
        SELECT PAGE, READ_DATE, DEVICE_ID, DEVICE_NAME, LOCATOR
        FROM READ_PROGRESS
        WHERE BOOK_ID = ? AND USER_ID = ?
        LIMIT 1
        "#,
    )
    .bind(book_id)
    .bind(user_id_value)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("query persisted book progression: {error}"))?;

    let Some(row) = row else {
        return Ok(None);
    };

    let locator_blob = row
        .try_get::<Option<Vec<u8>>, _>("LOCATOR")
        .or_else(|_| row.try_get::<Option<Vec<u8>>, _>("locator"))
        .map_err(|error| format!("read persisted book progression locator: {error}"))?;
    let locator = locator_blob
        .as_deref()
        .filter(|blob| !blob.is_empty())
        .map(serde_json::from_slice::<Value>)
        .transpose()
        .map_err(|error| format!("decode persisted book progression locator: {error}"))?
        .unwrap_or_else(|| serde_json::json!({}));
    let read_date = row
        .try_get::<String, _>("READ_DATE")
        .or_else(|_| row.try_get::<String, _>("read_date"))
        .map_err(|error| format!("read persisted book progression read_date: {error}"))?;
    let modified = if read_date.contains('T') {
        read_date
    } else {
        read_date.replace(' ', "T") + "Z"
    };
    Ok(Some(serde_json::json!({
        "modified": modified,
        "device": {
            "id": row
                .try_get::<String, _>("DEVICE_ID")
                .or_else(|_| row.try_get::<String, _>("device_id"))
                .map_err(|error| format!("read persisted book progression device_id: {error}"))?,
            "name": row
                .try_get::<String, _>("DEVICE_NAME")
                .or_else(|_| row.try_get::<String, _>("device_name"))
                .map_err(|error| format!("read persisted book progression device_name: {error}"))?,
        },
        "locator": locator,
    })))
}

pub async fn load_book_read_progress_completed(
    pool: &SqlitePool,
    book_id: &str,
    user_id_value: &str,
) -> Result<Option<bool>, String> {
    let row = sqlx::query(
        r#"
        SELECT COMPLETED
        FROM READ_PROGRESS
        WHERE BOOK_ID = ? AND USER_ID = ?
        LIMIT 1
        "#,
    )
    .bind(book_id)
    .bind(user_id_value)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("query persisted book read-progress completion: {error}"))?;

    Ok(row.map(|row| row.get::<bool, _>("COMPLETED")))
}

pub async fn load_book_page_count(pool: &SqlitePool, book_id: &str) -> Result<Option<u64>, String> {
    let row = sqlx::query("SELECT PAGE_COUNT FROM MEDIA WHERE BOOK_ID = ? LIMIT 1")
        .bind(book_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| format!("query book page-count: {error}"))?;

    Ok(row.map(|row| row.get::<i64, _>("PAGE_COUNT").max(0) as u64))
}

pub async fn delete_persisted_read_progress(
    pool: &SqlitePool,
    book_id: &str,
    user_id_value: &str,
) -> Result<(), String> {
    let series_id = load_book_series_id(pool, book_id, "read-progress delete").await?;

    sqlx::query("DELETE FROM READ_PROGRESS WHERE BOOK_ID = ? AND USER_ID = ?")
        .bind(book_id)
        .bind(user_id_value)
        .execute(pool)
        .await
        .map_err(|error| format!("delete read-progress: {error}"))?;

    if let Some(series_id) = series_id {
        sync_series_read_progress(pool, &series_id, user_id_value, "read-progress delete").await?;
        let exists = persisted_series_read_progress_exists(
            pool,
            &series_id,
            user_id_value,
            "read-progress delete",
        )
        .await?;
        emit_read_progress_series_event(&series_id, user_id_value, exists);
    }

    emit_read_progress_deleted(book_id, user_id_value);

    Ok(())
}

pub async fn readlist_tachiyomi_counters(
    pool: &SqlitePool,
    ordered_book_ids: &[String],
    user_id_value: &str,
) -> Result<(u64, u64, u64, u64, u64), String> {
    if ordered_book_ids.is_empty() {
        return Ok((0, 0, 0, 0, 0));
    }

    let mut query = QueryBuilder::<Sqlite>::new(
        "SELECT BOOK_ID, COMPLETED FROM READ_PROGRESS WHERE USER_ID = ",
    );
    query.push_bind(user_id_value);
    query.push(" AND BOOK_ID IN (");
    let mut separated = query.separated(",");
    for book_id in ordered_book_ids {
        separated.push_bind(book_id);
    }
    separated.push_unseparated(")");

    let rows = query
        .build()
        .fetch_all(pool)
        .await
        .map_err(|error| format!("query readlist tachiyomi counters: {error}"))?;

    let completed_by_book = rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("BOOK_ID"),
                row.get::<Option<i64>, _>("COMPLETED"),
            )
        })
        .collect::<HashMap<_, _>>();

    let completed_states = ordered_book_ids
        .iter()
        .map(|book_id| completed_by_book.get(book_id).copied().flatten())
        .collect::<Vec<_>>();

    let books_count = ordered_book_ids.len() as u64;
    let books_read_count = completed_states
        .iter()
        .filter(|completed| **completed == Some(1))
        .count() as u64;
    let books_in_progress_count = completed_states
        .iter()
        .filter(|completed| **completed == Some(0))
        .count() as u64;
    let books_unread_count = completed_states
        .iter()
        .filter(|completed| completed.is_none())
        .count() as u64;

    let mut last_read_continuous_index = 0_u64;
    for completed in completed_states {
        if completed == Some(1) {
            last_read_continuous_index += 1;
        } else {
            break;
        }
    }

    Ok((
        books_count,
        books_read_count,
        books_unread_count,
        books_in_progress_count,
        last_read_continuous_index,
    ))
}

pub async fn persist_readlist_tachiyomi_progress(
    pool: &SqlitePool,
    ordered_book_ids: &[String],
    user_id_value: &str,
    last_book_read: usize,
) -> Result<Option<()>, String> {
    for book_id in ordered_book_ids.iter().take(last_book_read) {
        let already_completed = sqlx::query(
            "SELECT COMPLETED FROM READ_PROGRESS WHERE BOOK_ID = ? AND USER_ID = ? LIMIT 1",
        )
        .bind(book_id)
        .bind(user_id_value)
        .fetch_optional(pool)
        .await
        .map_err(|error| format!("query existing completion for tachiyomi write: {error}"))?
        .is_some_and(|row| row.get::<i64, _>("COMPLETED") != 0);
        if already_completed {
            continue;
        }

        let page_count = sqlx::query(
            "SELECT COALESCE(PAGE_COUNT, 0) AS PAGE_COUNT FROM MEDIA WHERE BOOK_ID = ? LIMIT 1",
        )
        .bind(book_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| format!("query page count for tachiyomi write: {error}"))?
        .map(|row| row.get::<i64, _>("PAGE_COUNT").max(0) as u64)
        .unwrap_or(1)
        .max(1);

        persist_read_progress(pool, book_id, user_id_value, page_count, true, None)
            .await
            .map_err(|error| format!("persist read progress for tachiyomi write: {error}"))?;
    }

    Ok(Some(()))
}
