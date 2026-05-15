use sqlx::{Row, SqlitePool};

pub async fn load_announcement_read_ids(
    pool: &SqlitePool,
    user_id: &str,
) -> Result<Vec<String>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT ANNOUNCEMENT_ID
         FROM ANNOUNCEMENTS_READ
         WHERE USER_ID = ?
         ORDER BY ANNOUNCEMENT_ID ASC"#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("ANNOUNCEMENT_ID"))
        .collect())
}
