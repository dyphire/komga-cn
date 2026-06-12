use komga_application::operational::PageHashUpsertCommand;
use sqlx::SqlitePool;

use super::super::page_hash_action::persisted_page_hash_action;

pub(crate) async fn upsert_page_hash(
    pool: &SqlitePool,
    command: &PageHashUpsertCommand,
) -> Result<(), sqlx::Error> {
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
    .bind(command.hash.as_str())
    .bind(command.size)
    .bind(persisted_page_hash_action(command.action))
    .execute(pool)
    .await?;
    Ok(())
}
