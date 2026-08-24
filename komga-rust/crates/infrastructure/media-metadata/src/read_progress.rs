use anyhow::Context;
use std::collections::HashMap;

use komga_application::media_assets::{BookProgressionInput, BookProgressionRecord};
use komga_application::runtime_sse::{RuntimeSseEvent, RuntimeSseEventSink};
use serde_json::Value;
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
) -> anyhow::Result<()> {
    let user_exists = sqlx::query("SELECT 1 FROM USER WHERE ID = ? LIMIT 1")
        .bind(user_id_value)
        .fetch_optional(pool)
        .await
        .map_err(|error| anyhow::anyhow!(error).context(format!("query {query_context} user")))?
        .is_some();

    if !user_exists {
        return Err(anyhow::anyhow!("read-progress user not found"));
    }

    Ok(())
}

async fn load_book_series_id(
    pool: &SqlitePool,
    book_id: &str,
    query_context: &str,
) -> anyhow::Result<Option<String>> {
    sqlx::query("SELECT SERIES_ID FROM BOOK WHERE ID = ? LIMIT 1")
        .bind(book_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| {
            anyhow::anyhow!(error).context(format!("query {query_context} book series"))
        })
        .map(|row| row.map(|row| row.get::<String, _>("SERIES_ID")))
}

async fn sync_series_read_progress(
    pool: &SqlitePool,
    series_id: &str,
    user_id_value: &str,
    query_context: &str,
) -> anyhow::Result<()> {
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
    .map_err(|error| {
        anyhow::anyhow!(error).context(format!(
            "query {query_context} series read progress aggregates: "
        ))
    })?;

    let progress_count = row.get::<i64, _>("PROGRESS_COUNT");
    if progress_count == 0 {
        sqlx::query("DELETE FROM READ_PROGRESS_SERIES WHERE SERIES_ID = ? AND USER_ID = ?")
            .bind(series_id)
            .bind(user_id_value)
            .execute(pool)
            .await
            .map_err(|error| {
                anyhow::anyhow!(error)
                    .context(format!("delete {query_context} series read progress row"))
            })?;
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
    .map_err(|error| anyhow::anyhow!(error).context( format!("upsert {query_context} series read progress row")))?;

    Ok(())
}

async fn sync_series_read_progress_for_book(
    pool: &SqlitePool,
    book_id: &str,
    user_id_value: &str,
    query_context: &str,
) -> anyhow::Result<()> {
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
) -> anyhow::Result<bool> {
    sqlx::query(
        "SELECT 1 AS FOUND FROM READ_PROGRESS_SERIES WHERE SERIES_ID = ? AND USER_ID = ? LIMIT 1",
    )
    .bind(series_id)
    .bind(user_id_value)
    .fetch_optional(pool)
    .await
    .map_err(|error| {
        anyhow::anyhow!(error).context(format!("query {query_context} series read progress row"))
    })
    .map(|row| row.is_some())
}

fn emit_read_progress_changed(
    runtime_events: &dyn RuntimeSseEventSink,
    book_id: &str,
    user_id_value: &str,
) {
    runtime_events.register(RuntimeSseEvent::ReadProgressChanged {
        book_id: book_id.to_string(),
        user_id: user_id_value.to_string(),
    });
}

fn emit_read_progress_deleted(
    runtime_events: &dyn RuntimeSseEventSink,
    book_id: &str,
    user_id_value: &str,
) {
    runtime_events.register(RuntimeSseEvent::ReadProgressDeleted {
        book_id: book_id.to_string(),
        user_id: user_id_value.to_string(),
    });
}

fn emit_read_progress_series_event(
    runtime_events: &dyn RuntimeSseEventSink,
    series_id: &str,
    user_id_value: &str,
    exists: bool,
) {
    let event = if exists {
        RuntimeSseEvent::ReadProgressSeriesChanged {
            series_id: series_id.to_string(),
            user_id: user_id_value.to_string(),
        }
    } else {
        RuntimeSseEvent::ReadProgressSeriesDeleted {
            series_id: series_id.to_string(),
            user_id: user_id_value.to_string(),
        }
    };
    runtime_events.register(event);
}

pub async fn persist_read_progress(
    pool: &SqlitePool,
    runtime_events: &dyn RuntimeSseEventSink,
    book_id: &str,
    user_id_value: &str,
    page: u64,
    completed: bool,
    locator: Option<Value>,
) -> anyhow::Result<()> {
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
    .context("persist read-progress")?;

    let series_id = load_book_series_id(pool, book_id, "read-progress").await?;
    sync_series_read_progress_for_book(pool, book_id, user_id_value, "read-progress").await?;

    emit_read_progress_changed(runtime_events, book_id, user_id_value);
    if let Some(series_id) = series_id {
        let exists =
            persisted_series_read_progress_exists(pool, &series_id, user_id_value, "read-progress")
                .await?;
        emit_read_progress_series_event(runtime_events, &series_id, user_id_value, exists);
    }

    Ok(())
}

pub async fn persist_book_progression(
    pool: &SqlitePool,
    runtime_events: &dyn RuntimeSseEventSink,
    input: BookProgressionInput,
) -> anyhow::Result<()> {
    require_user_exists(pool, &input.user_id, "progression").await?;

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
    .bind(&input.book_id)
    .bind(&input.user_id)
    .bind(input.page as i64)
    .bind(input.completed)
    .bind(input.modified)
    .bind(input.device_id.unwrap_or_default())
    .bind(input.device_name.unwrap_or_default())
    .bind(serialize_locator(input.locator.as_ref()))
    .execute(pool)
    .await
    .context("persist book progression")?;

    let series_id = load_book_series_id(pool, &input.book_id, "progression").await?;
    sync_series_read_progress_for_book(pool, &input.book_id, &input.user_id, "progression").await?;

    emit_read_progress_changed(runtime_events, &input.book_id, &input.user_id);
    if let Some(series_id) = series_id {
        let exists =
            persisted_series_read_progress_exists(pool, &series_id, &input.user_id, "progression")
                .await?;
        emit_read_progress_series_event(runtime_events, &series_id, &input.user_id, exists);
    }

    Ok(())
}

pub async fn load_book_progression(
    pool: &SqlitePool,
    book_id: &str,
    user_id_value: &str,
) -> anyhow::Result<Option<BookProgressionRecord>> {
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
    .context("query persisted book progression")?;

    let Some(row) = row else {
        return Ok(None);
    };

    let locator_blob = row
        .try_get::<Option<Vec<u8>>, _>("LOCATOR")
        .or_else(|_| row.try_get::<Option<Vec<u8>>, _>("locator"))
        .context("read persisted book progression locator")?;
    let locator = decode_book_progression_locator(locator_blob.as_deref())?;
    let read_date = row
        .try_get::<String, _>("READ_DATE")
        .or_else(|_| row.try_get::<String, _>("read_date"))
        .context("read persisted book progression read_date")?;
    let modified = progression_modified_utc(read_date);
    Ok(Some(BookProgressionRecord {
        modified,
        device_id: row
            .try_get::<String, _>("DEVICE_ID")
            .or_else(|_| row.try_get::<String, _>("device_id"))
            .context("read persisted book progression device_id")?,
        device_name: row
            .try_get::<String, _>("DEVICE_NAME")
            .or_else(|_| row.try_get::<String, _>("device_name"))
            .context("read persisted book progression device_name")?,
        locator,
    }))
}

fn progression_modified_utc(read_date: String) -> String {
    let normalized = read_date.replace(' ', "T");
    let has_explicit_offset = normalized
        .split_once('T')
        .is_some_and(|(_, time)| time.contains('+') || time.contains('-'));
    if normalized.ends_with('Z') || has_explicit_offset {
        normalized
    } else {
        normalized + "Z"
    }
}

fn decode_book_progression_locator(locator: Option<&[u8]>) -> anyhow::Result<Value> {
    let Some(locator) = locator.filter(|blob| !blob.is_empty()) else {
        return Ok(serde_json::json!({}));
    };

    let payload = serde_json::from_slice::<Value>(locator)
        .context("decode persisted book progression locator")?;
    if payload.is_object() {
        Ok(payload)
    } else {
        Err(anyhow::anyhow!(
            "decode persisted book progression locator: expected JSON object"
        ))
    }
}

pub async fn load_book_read_progress_completed(
    pool: &SqlitePool,
    book_id: &str,
    user_id_value: &str,
) -> anyhow::Result<Option<bool>> {
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
    .context("query persisted book read-progress completion")?;

    Ok(row.map(|row| row.get::<bool, _>("COMPLETED")))
}

pub async fn load_book_page_count(pool: &SqlitePool, book_id: &str) -> anyhow::Result<Option<u64>> {
    let row = sqlx::query("SELECT PAGE_COUNT FROM MEDIA WHERE BOOK_ID = ? LIMIT 1")
        .bind(book_id)
        .fetch_optional(pool)
        .await
        .context("query book page-count")?;

    Ok(row.map(|row| row.get::<i64, _>("PAGE_COUNT").max(0) as u64))
}

pub async fn delete_persisted_read_progress(
    pool: &SqlitePool,
    runtime_events: &dyn RuntimeSseEventSink,
    book_id: &str,
    user_id_value: &str,
) -> anyhow::Result<()> {
    let series_id = load_book_series_id(pool, book_id, "read-progress delete").await?;

    sqlx::query("DELETE FROM READ_PROGRESS WHERE BOOK_ID = ? AND USER_ID = ?")
        .bind(book_id)
        .bind(user_id_value)
        .execute(pool)
        .await
        .context("delete read-progress")?;

    if let Some(series_id) = series_id {
        sync_series_read_progress(pool, &series_id, user_id_value, "read-progress delete").await?;
        let exists = persisted_series_read_progress_exists(
            pool,
            &series_id,
            user_id_value,
            "read-progress delete",
        )
        .await?;
        emit_read_progress_series_event(runtime_events, &series_id, user_id_value, exists);
    }

    emit_read_progress_deleted(runtime_events, book_id, user_id_value);

    Ok(())
}

pub async fn read_progress_completed_by_book_ids(
    pool: &SqlitePool,
    ordered_book_ids: &[String],
    user_id_value: &str,
) -> anyhow::Result<Vec<Option<bool>>> {
    if ordered_book_ids.is_empty() {
        return Ok(Vec::new());
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
        .context("query read progress completion states")?;

    let completed_by_book = rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("BOOK_ID"),
                row.get::<Option<i64>, _>("COMPLETED"),
            )
        })
        .collect::<HashMap<_, _>>();

    Ok(ordered_book_ids
        .iter()
        .map(|book_id| {
            completed_by_book
                .get(book_id)
                .copied()
                .flatten()
                .map(|value| value != 0)
        })
        .collect())
}
