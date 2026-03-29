use std::path::Path;

use sqlx::Row;

use crate::sqlite::connect_pool;

pub async fn load_announcement_read_ids(
    database_file: &Path,
    user_id: &str,
) -> Result<Vec<String>, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let rows = sqlx::query(
        "SELECT ANNOUNCEMENT_ID\n         FROM ANNOUNCEMENTS_READ\n         WHERE USER_ID = ?\n         ORDER BY ANNOUNCEMENT_ID ASC",
    )
    .bind(user_id)
    .fetch_all(&pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("ANNOUNCEMENT_ID"))
        .collect())
}
