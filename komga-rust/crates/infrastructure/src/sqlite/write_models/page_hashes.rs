use std::path::Path;

use crate::sqlite::connect_pool;

pub async fn upsert_page_hash(
    database_file: &Path,
    page_hash: &str,
    size: Option<i64>,
    action: &str,
) -> Result<(), sqlx::Error> {
    let normalized_size = size.filter(|value| *value >= 0);
    let pool = connect_pool(database_file, 1).await?;
    sqlx::query(
        "INSERT INTO PAGE_HASH (HASH, SIZE, ACTION, DELETE_COUNT, CREATED_DATE, LAST_MODIFIED_DATE) \
         VALUES (?, ?, ?, 0, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) \
         ON CONFLICT(HASH) DO UPDATE \
         SET SIZE = PAGE_HASH.SIZE, \
             ACTION = excluded.ACTION, \
             LAST_MODIFIED_DATE = CURRENT_TIMESTAMP",
    )
    .bind(page_hash)
    .bind(normalized_size)
    .bind(action)
    .execute(&pool)
    .await?;
    Ok(())
}
