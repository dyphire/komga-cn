use std::path::Path;

use serde_json::Value;
use sqlx::{Row, SqlitePool};

use crate::sqlite::connect_pool;

fn serialize_locator(locator: Option<&Value>) -> Vec<u8> {
    locator
        .and_then(|value| serde_json::to_vec(value).ok())
        .unwrap_or_default()
}

async fn open_pool_and_require_user(
    database_file: &Path,
    user_id_value: &str,
    open_context: &str,
    query_context: &str,
) -> Result<SqlitePool, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open {open_context} db: {error}"))?;

    let user_exists = sqlx::query("SELECT 1 FROM USER WHERE ID = ? LIMIT 1")
        .bind(user_id_value)
        .fetch_optional(&pool)
        .await
        .map_err(|error| format!("query {query_context} user: {error}"))?
        .is_some();

    if !user_exists {
        return Err("read-progress user not found".to_string());
    }

    Ok(pool)
}

pub async fn persist_read_progress(
    database_file: &Path,
    book_id: &str,
    user_id_value: &str,
    page: u64,
    completed: bool,
    locator: Option<Value>,
) -> Result<(), String> {
    let pool = open_pool_and_require_user(
        database_file,
        user_id_value,
        "read-progress",
        "read-progress",
    )
    .await?;

    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED, LOCATOR) \
         VALUES (?, ?, ?, ?, ?) \
         ON CONFLICT(BOOK_ID, USER_ID) DO UPDATE \
         SET PAGE = excluded.PAGE, COMPLETED = excluded.COMPLETED, LOCATOR = excluded.LOCATOR, \
             READ_DATE = CASE \
                 WHEN READ_PROGRESS.COMPLETED = 0 AND excluded.COMPLETED = 1 THEN CURRENT_TIMESTAMP \
                 ELSE READ_PROGRESS.READ_DATE \
             END, \
             LAST_MODIFIED_DATE = CURRENT_TIMESTAMP",
    )
    .bind(book_id)
    .bind(user_id_value)
    .bind(page as i64)
    .bind(completed)
    .bind(serialize_locator(locator.as_ref()))
    .execute(&pool)
    .await
    .map_err(|error| format!("persist read-progress: {error}"))?;

    Ok(())
}

pub async fn persist_book_progression(
    database_file: &Path,
    book_id: &str,
    user_id_value: &str,
    progression: f64,
    use_locator_position_for_page: bool,
    modified: Option<String>,
    device_id: Option<String>,
    device_name: Option<String>,
    locator: Option<Value>,
) -> Result<(), String> {
    let page_count = load_book_page_count(database_file, book_id)
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
    let pool =
        open_pool_and_require_user(database_file, user_id_value, "progression", "progression")
            .await?;

    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED, READ_DATE, DEVICE_ID, DEVICE_NAME, LOCATOR) \
         VALUES (?, ?, ?, ?, COALESCE(?, CURRENT_TIMESTAMP), ?, ?, ?) \
         ON CONFLICT(BOOK_ID, USER_ID) DO UPDATE \
         SET PAGE = excluded.PAGE, COMPLETED = excluded.COMPLETED, READ_DATE = excluded.READ_DATE, \
             DEVICE_ID = excluded.DEVICE_ID, DEVICE_NAME = excluded.DEVICE_NAME, LOCATOR = excluded.LOCATOR, \
             LAST_MODIFIED_DATE = CURRENT_TIMESTAMP",
    )
    .bind(book_id)
    .bind(user_id_value)
    .bind(page as i64)
    .bind(completed)
    .bind(modified)
    .bind(device_id.unwrap_or_default())
    .bind(device_name.unwrap_or_default())
    .bind(serialize_locator(locator.as_ref()))
    .execute(&pool)
    .await
    .map_err(|error| format!("persist book progression: {error}"))?;
    Ok(())
}

pub async fn load_book_progression(
    database_file: &Path,
    book_id: &str,
    user_id_value: &str,
) -> Result<Option<Value>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open book progression db: {error}"))?;

    let row = sqlx::query(
        "SELECT PAGE, READ_DATE, DEVICE_ID, DEVICE_NAME, LOCATOR \
         FROM READ_PROGRESS WHERE BOOK_ID = ? AND USER_ID = ? LIMIT 1",
    )
    .bind(book_id)
    .bind(user_id_value)
    .fetch_optional(&pool)
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

pub async fn load_book_page_count(
    database_file: &Path,
    book_id: &str,
) -> Result<Option<u64>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open book page-count db: {error}"))?;

    let row = sqlx::query("SELECT PAGE_COUNT FROM MEDIA WHERE BOOK_ID = ? LIMIT 1")
        .bind(book_id)
        .fetch_optional(&pool)
        .await
        .map_err(|error| format!("query book page-count: {error}"))?;

    Ok(row.map(|row| row.get::<i64, _>("PAGE_COUNT").max(0) as u64))
}

pub async fn delete_persisted_read_progress(
    database_file: &Path,
    book_id: &str,
    user_id_value: &str,
) -> Result<(), String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open read-progress delete db: {error}"))?;

    sqlx::query("DELETE FROM READ_PROGRESS WHERE BOOK_ID = ? AND USER_ID = ?")
        .bind(book_id)
        .bind(user_id_value)
        .execute(&pool)
        .await
        .map_err(|error| format!("delete read-progress: {error}"))?;

    Ok(())
}

pub async fn readlist_tachiyomi_counters(
    database_file: &Path,
    readlist_id: &str,
    user_id_value: &str,
) -> Result<Option<(u64, u64, u64, u64, u64)>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open readlist tachiyomi db: {error}"))?;
    let exists = sqlx::query("SELECT 1 AS FOUND FROM READLIST WHERE ID = ? LIMIT 1")
        .bind(readlist_id)
        .fetch_optional(&pool)
        .await
        .map_err(|error| format!("query readlist exists for tachiyomi counters: {error}"))?
        .is_some();
    if !exists {
        return Ok(None);
    }

    let rows = sqlx::query(
        "SELECT rb.NUMBER AS ORDINAL, COALESCE(rp.PAGE, 0) AS PAGE, COALESCE(rp.COMPLETED, 0) AS COMPLETED \
         FROM READLIST_BOOK rb \
         LEFT JOIN READ_PROGRESS rp ON rp.BOOK_ID = rb.BOOK_ID AND rp.USER_ID = ? \
         WHERE rb.READLIST_ID = ? \
         ORDER BY rb.NUMBER ASC",
    )
    .bind(user_id_value)
    .bind(readlist_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query readlist tachiyomi counters: {error}"))?;

    let books_count = rows.len() as u64;
    let books_read_count = rows
        .iter()
        .filter(|row| row.get::<i64, _>("COMPLETED") != 0)
        .count() as u64;
    let books_in_progress_count = rows
        .iter()
        .filter(|row| row.get::<i64, _>("COMPLETED") == 0 && row.get::<i64, _>("PAGE") > 0)
        .count() as u64;
    let books_unread_count = books_count.saturating_sub(books_read_count + books_in_progress_count);

    let mut last_read_continuous_index = 0_u64;
    for row in rows {
        if row.get::<i64, _>("COMPLETED") != 0 {
            last_read_continuous_index += 1;
        } else {
            break;
        }
    }

    Ok(Some((
        books_count,
        books_read_count,
        books_unread_count,
        books_in_progress_count,
        last_read_continuous_index,
    )))
}

pub async fn persist_readlist_tachiyomi_progress(
    database_file: &Path,
    readlist_id: &str,
    user_id_value: &str,
    last_book_read: usize,
) -> Result<Option<()>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open readlist tachiyomi write db: {error}"))?;
    let exists = sqlx::query("SELECT 1 AS FOUND FROM READLIST WHERE ID = ? LIMIT 1")
        .bind(readlist_id)
        .fetch_optional(&pool)
        .await
        .map_err(|error| format!("query readlist exists for tachiyomi write: {error}"))?
        .is_some();
    if !exists {
        return Ok(None);
    }

    let rows =
        sqlx::query("SELECT BOOK_ID FROM READLIST_BOOK WHERE READLIST_ID = ? ORDER BY NUMBER ASC")
            .bind(readlist_id)
            .fetch_all(&pool)
            .await
            .map_err(|error| format!("query readlist books for tachiyomi write: {error}"))?;

    for (index, row) in rows.into_iter().enumerate() {
        if index >= last_book_read {
            break;
        }

        let book_id = row.get::<String, _>("BOOK_ID");
        let page_count = sqlx::query(
            "SELECT COALESCE(PAGE_COUNT, 0) AS PAGE_COUNT FROM MEDIA WHERE BOOK_ID = ? LIMIT 1",
        )
        .bind(&book_id)
        .fetch_optional(&pool)
        .await
        .map_err(|error| format!("query page count for tachiyomi write: {error}"))?
        .map(|row| row.get::<i64, _>("PAGE_COUNT").max(0) as u64)
        .unwrap_or(1)
        .max(1);

        persist_read_progress(
            database_file,
            &book_id,
            user_id_value,
            page_count,
            true,
            None,
        )
        .await
        .map_err(|error| format!("persist read progress for tachiyomi write: {error}"))?;
    }

    Ok(Some(()))
}
