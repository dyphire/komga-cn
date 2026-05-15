use sqlx::SqlitePool;

pub async fn upsert_page_hash(
    pool: &SqlitePool,
    page_hash: &str,
    size: Option<i64>,
    action: &str,
) -> Result<(), sqlx::Error> {
    let normalized_size = size.filter(|value| *value >= 0);
    sqlx::query(
        r#"
        INSERT INTO PAGE_HASH (HASH, SIZE, ACTION, DELETE_COUNT, CREATED_DATE, LAST_MODIFIED_DATE)
        VALUES (?, ?, ?, 0, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
        ON CONFLICT(HASH) DO UPDATE
        SET SIZE = PAGE_HASH.SIZE,
            ACTION = excluded.ACTION,
            LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
    "#,
    )
    .bind(page_hash)
    .bind(normalized_size)
    .bind(action)
    .execute(pool)
    .await?;
    Ok(())
}
