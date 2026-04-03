use std::path::Path;

use serde_json::{Value, json};
use sqlx::Row;

use crate::sqlite::connect_pool;

fn empty_series_tachiyomi_progress_payload() -> Value {
    json!({
        "booksCount": 0,
        "booksReadCount": 0,
        "booksUnreadCount": 0,
        "booksInProgressCount": 0,
        "lastReadContinuousNumberSort": 0.0,
        "maxNumberSort": 0.0,
    })
}

pub async fn refresh_series_read_progress_row(
    database_file: &Path,
    series_id: &str,
    user_id_value: &str,
) -> Result<(), String> {
    if !database_file.exists() {
        return Ok(());
    }
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open series read progress db: {error}"))?;
    let row = sqlx::query(
        "SELECT COALESCE(SUM(CASE WHEN rp.COMPLETED = 1 THEN 1 ELSE 0 END), 0) AS READ_COUNT, \
                COALESCE(SUM(CASE WHEN rp.COMPLETED = 0 AND rp.PAGE > 0 THEN 1 ELSE 0 END), 0) AS IN_PROGRESS_COUNT, \
                MAX(rp.READ_DATE) AS MOST_RECENT_READ_DATE \
         FROM BOOK b LEFT JOIN READ_PROGRESS rp ON rp.BOOK_ID = b.ID AND rp.USER_ID = ? \
         WHERE b.SERIES_ID = ? AND b.DELETED_DATE IS NULL",
    )
    .bind(user_id_value)
    .bind(series_id)
    .fetch_one(&pool)
    .await
    .map_err(|error| format!("query series read progress aggregates: {error}"))?;
    sqlx::query(
        "INSERT INTO READ_PROGRESS_SERIES (SERIES_ID, USER_ID, READ_COUNT, IN_PROGRESS_COUNT, MOST_RECENT_READ_DATE, LAST_MODIFIED_DATE) \
         VALUES (?, ?, ?, ?, ?, CURRENT_TIMESTAMP) \
         ON CONFLICT(SERIES_ID, USER_ID) DO UPDATE \
         SET READ_COUNT = excluded.READ_COUNT, IN_PROGRESS_COUNT = excluded.IN_PROGRESS_COUNT, \
             MOST_RECENT_READ_DATE = excluded.MOST_RECENT_READ_DATE, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP",
    )
    .bind(series_id)
    .bind(user_id_value)
    .bind(row.get::<i64, _>("READ_COUNT"))
    .bind(row.get::<i64, _>("IN_PROGRESS_COUNT"))
    .bind(row.get::<Option<String>, _>("MOST_RECENT_READ_DATE"))
    .execute(&pool)
    .await
    .map_err(|error| format!("upsert series read progress row: {error}"))?;
    Ok(())
}

pub async fn delete_series_read_progress_row(
    database_file: &Path,
    series_id: &str,
    user_id_value: &str,
) -> Result<(), String> {
    if !database_file.exists() {
        return Ok(());
    }
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open series read progress delete db: {error}"))?;
    sqlx::query("DELETE FROM READ_PROGRESS_SERIES WHERE SERIES_ID = ? AND USER_ID = ?")
        .bind(series_id)
        .bind(user_id_value)
        .execute(&pool)
        .await
        .map_err(|error| format!("delete series read progress row: {error}"))?;
    Ok(())
}

pub async fn load_series_tachiyomi_progress(
    database_file: &Path,
    series_id: &str,
    user_id_value: &str,
) -> Result<Option<Value>, String> {
    if !database_file.exists() {
        return Ok(None);
    }
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open series tachiyomi db: {error}"))?;
    let rows = sqlx::query(
        "SELECT COALESCE(bm.NUMBER_SORT, 0) AS NUMBER_SORT, rp.COMPLETED AS COMPLETED \
         FROM BOOK b LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID \
         LEFT JOIN READ_PROGRESS rp ON rp.BOOK_ID = b.ID AND (rp.USER_ID = ? OR rp.USER_ID IS NULL) \
         WHERE b.SERIES_ID = ? \
         ORDER BY COALESCE(bm.NUMBER_SORT, 0) ASC, b.ID ASC",
    )
    .bind(user_id_value)
    .bind(series_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query series tachiyomi rows: {error}"))?;
    if rows.is_empty() {
        return Ok(Some(empty_series_tachiyomi_progress_payload()));
    }
    let mut books_count = 0usize;
    let mut books_read_count = 0usize;
    let mut books_in_progress_count = 0usize;
    let mut last_read_continuous_number_sort = 0.0f64;
    let mut max_number_sort = 0.0f64;
    let mut all_previous_completed = true;
    for row in rows {
        books_count += 1;
        let number_sort = row.get::<f64, _>("NUMBER_SORT");
        let completed = row.get::<Option<i64>, _>("COMPLETED");
        if number_sort > max_number_sort {
            max_number_sort = number_sort;
        }
        match completed {
            Some(value) if value != 0 => {
                books_read_count += 1;
                if all_previous_completed {
                    last_read_continuous_number_sort = number_sort;
                }
            }
            Some(_) => {
                books_in_progress_count += 1;
                all_previous_completed = false;
            }
            None => {
                all_previous_completed = false;
            }
        }
    }
    let books_unread_count = books_count
        .saturating_sub(books_read_count)
        .saturating_sub(books_in_progress_count);
    Ok(Some(json!({
        "booksCount": books_count,
        "booksReadCount": books_read_count,
        "booksUnreadCount": books_unread_count,
        "booksInProgressCount": books_in_progress_count,
        "lastReadContinuousNumberSort": last_read_continuous_number_sort,
        "maxNumberSort": max_number_sort,
    })))
}

#[cfg(test)]
mod tests {
    use super::empty_series_tachiyomi_progress_payload;

    #[test]
    fn empty_series_tachiyomi_progress_payload_returns_zeroed_counts() {
        let payload = empty_series_tachiyomi_progress_payload();

        assert_eq!(payload["booksCount"], 0);
        assert_eq!(payload["booksReadCount"], 0);
        assert_eq!(payload["booksUnreadCount"], 0);
        assert_eq!(payload["booksInProgressCount"], 0);
        assert_eq!(payload["lastReadContinuousNumberSort"], 0.0);
        assert_eq!(payload["maxNumberSort"], 0.0);
    }
}
