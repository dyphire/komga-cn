use std::path::Path;

use sqlx::Row;

use crate::sqlite::connect_pool;

pub async fn persist_read_progress(
    database_file: &Path,
    book_id: &str,
    user_id_value: &str,
    page: u64,
    completed: bool,
) -> Result<(), String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open read-progress db: {error}"))?;

    let user_exists = sqlx::query("SELECT 1 FROM USER WHERE ID = ? LIMIT 1")
        .bind(user_id_value)
        .fetch_optional(&pool)
        .await
        .map_err(|error| format!("query read-progress user: {error}"))?
        .is_some();

    if !user_exists {
        return Err("read-progress user not found".to_string());
    }

    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED) \
         VALUES (?, ?, ?, ?) \
         ON CONFLICT(BOOK_ID, USER_ID) DO UPDATE \
         SET PAGE = excluded.PAGE, COMPLETED = excluded.COMPLETED, \
             LAST_MODIFIED_DATE = CURRENT_TIMESTAMP",
    )
    .bind(book_id)
    .bind(user_id_value)
    .bind(page as i64)
    .bind(completed)
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
) -> Result<(), String> {
    let page_count = load_book_page_count(database_file, book_id)
        .await?
        .unwrap_or(1)
        .max(1);
    let page = ((progression * page_count as f64).ceil() as u64).clamp(0, page_count as u64);
    let completed = progression >= 1.0;

    persist_read_progress(database_file, book_id, user_id_value, page, completed).await
}

pub async fn load_book_progression(
    database_file: &Path,
    book_id: &str,
    user_id_value: &str,
) -> Result<Option<f64>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open book progression db: {error}"))?;

    let row =
        sqlx::query("SELECT PAGE FROM READ_PROGRESS WHERE BOOK_ID = ? AND USER_ID = ? LIMIT 1")
            .bind(book_id)
            .bind(user_id_value)
            .fetch_optional(&pool)
            .await
            .map_err(|error| format!("query persisted book progression: {error}"))?;

    let Some(row) = row else {
        return Ok(None);
    };

    let page = row.get::<i64, _>("PAGE").max(0) as u64;
    let page_count = load_book_page_count(database_file, book_id)
        .await?
        .unwrap_or(1)
        .max(1) as f64;
    Ok(Some((page as f64 / page_count).clamp(0.0, 1.0)))
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
        persist_read_progress(database_file, &book_id, user_id_value, 10, true)
            .await
            .map_err(|error| format!("persist read progress for tachiyomi write: {error}"))?;
    }

    Ok(Some(()))
}
