use sqlx::SqlitePool;

pub async fn save_announcements_read(
    pool: &SqlitePool,
    user_id: &str,
    announcement_ids: &[String],
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;

    for announcement_id in announcement_ids {
        sqlx::query(
            r#"INSERT OR IGNORE INTO ANNOUNCEMENTS_READ (USER_ID, ANNOUNCEMENT_ID)
               VALUES (?, ?)"#,
        )
        .bind(user_id)
        .bind(announcement_id)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}
