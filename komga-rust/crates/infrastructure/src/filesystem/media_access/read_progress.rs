use anyhow::Context;
use komga_application::media_assets::SeriesTachiyomiProgressBook;
use sqlx::{Row, SqlitePool};

pub(crate) async fn refresh_series_read_progress_row(
    pool: &SqlitePool,
    series_id: &str,
    user_id_value: &str,
) -> anyhow::Result<()> {
    let row = sqlx::query(
        r#"SELECT COALESCE(SUM(CASE WHEN rp.COMPLETED = 1 THEN 1 ELSE 0 END), 0) AS READ_COUNT,
               COALESCE(SUM(CASE WHEN rp.COMPLETED = 0 THEN 1 ELSE 0 END), 0) AS IN_PROGRESS_COUNT,
               MAX(rp.READ_DATE) AS MOST_RECENT_READ_DATE
        FROM BOOK b LEFT JOIN READ_PROGRESS rp ON rp.BOOK_ID = b.ID AND rp.USER_ID = ?
        WHERE b.SERIES_ID = ? AND b.DELETED_DATE IS NULL"#,
    )
    .bind(user_id_value)
    .bind(series_id)
    .fetch_one(pool)
    .await
    .context("query series read progress aggregates")?;
    sqlx::query(
        r#"INSERT INTO READ_PROGRESS_SERIES (SERIES_ID, USER_ID, READ_COUNT, IN_PROGRESS_COUNT, MOST_RECENT_READ_DATE, LAST_MODIFIED_DATE)
         VALUES (?, ?, ?, ?, ?, CURRENT_TIMESTAMP)
         ON CONFLICT(SERIES_ID, USER_ID) DO UPDATE
         SET READ_COUNT = excluded.READ_COUNT, IN_PROGRESS_COUNT = excluded.IN_PROGRESS_COUNT,
             MOST_RECENT_READ_DATE = excluded.MOST_RECENT_READ_DATE, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP"#,
    )
    .bind(series_id)
    .bind(user_id_value)
    .bind(row.get::<i64, _>("READ_COUNT"))
    .bind(row.get::<i64, _>("IN_PROGRESS_COUNT"))
    .bind(row.get::<Option<String>, _>("MOST_RECENT_READ_DATE"))
    .execute(pool)
    .await
    .context("upsert series read progress row")?;
    Ok(())
}

pub(crate) async fn delete_series_read_progress_row(
    pool: &SqlitePool,
    series_id: &str,
    user_id_value: &str,
) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM READ_PROGRESS_SERIES WHERE SERIES_ID = ? AND USER_ID = ?")
        .bind(series_id)
        .bind(user_id_value)
        .execute(pool)
        .await
        .context("delete series read progress row")?;
    Ok(())
}

pub(crate) async fn load_series_tachiyomi_progress_books(
    pool: &SqlitePool,
    series_id: &str,
    user_id_value: &str,
) -> anyhow::Result<Vec<SeriesTachiyomiProgressBook>> {
    let rows = sqlx::query(
        r#"SELECT COALESCE(bm.NUMBER_SORT, CAST(0 AS REAL)) AS NUMBER_SORT, rp.COMPLETED AS COMPLETED
         FROM BOOK b LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
         LEFT JOIN READ_PROGRESS rp ON rp.BOOK_ID = b.ID AND (rp.USER_ID = ? OR rp.USER_ID IS NULL)
         WHERE b.SERIES_ID = ?
         ORDER BY COALESCE(bm.NUMBER_SORT, CAST(0 AS REAL)) ASC, b.ID ASC"#,
    )
    .bind(user_id_value)
    .bind(series_id)
    .fetch_all(pool)
    .await
    .context("query series tachiyomi rows")?;
    Ok(rows
        .into_iter()
        .map(|row| SeriesTachiyomiProgressBook {
            number_sort: row.get::<f64, _>("NUMBER_SORT"),
            completed: row
                .get::<Option<i64>, _>("COMPLETED")
                .map(|value| value != 0),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::load_series_tachiyomi_progress_books;
    use crate::test_support::BootstrappedBookFixture;

    #[tokio::test]
    async fn load_series_tachiyomi_progress_books_defaults_number_sort_without_metadata() {
        let fixture = BootstrappedBookFixture::open("tachiyomi-progress-number-sort").await;
        fixture.insert_library_series().await;
        fixture.insert_book("book-1").await;

        let books = load_series_tachiyomi_progress_books(&fixture.pool, "series-1", "user-1")
            .await
            .expect("book without metadata should load");

        assert_eq!(books.len(), 1);
        assert_eq!(books[0].number_sort, 0.0);
        assert_eq!(books[0].completed, None);
        fixture.close().await;
    }
}
