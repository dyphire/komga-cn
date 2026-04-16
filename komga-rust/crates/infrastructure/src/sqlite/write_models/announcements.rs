use std::path::Path;

use crate::sqlite::connect_pool;

pub async fn save_announcements_read(
    database_file: &Path,
    user_id: &str,
    announcement_ids: &[String],
) -> Result<(), sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
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
