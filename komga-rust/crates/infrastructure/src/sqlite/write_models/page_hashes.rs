use std::path::Path;

use crate::sqlite::connect_pool;

pub async fn upsert_page_hash(
    database_file: &Path,
    page_hash: &str,
    size: Option<i64>,
    action: &str,
) -> Result<(), sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    sqlx::query(
        "INSERT INTO PAGE_HASH (HASH, SIZE, ACTION, DELETE_COUNT, CREATED_DATE, LAST_MODIFIED_DATE)\n         VALUES (?, ?, ?, 0, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)\n         ON CONFLICT(HASH) DO UPDATE\n         SET SIZE = excluded.SIZE,\n             ACTION = excluded.ACTION,\n             LAST_MODIFIED_DATE = CURRENT_TIMESTAMP",
    )
    .bind(page_hash)
    .bind(size)
    .bind(action)
    .execute(&pool)
    .await?;
    Ok(())
}

pub async fn delete_all_page_hash_matches(
    database_file: &Path,
    page_hash: &str,
) -> Result<u64, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let mut tx = pool.begin().await?;

    let deleted = sqlx::query("DELETE FROM MEDIA_PAGE WHERE FILE_HASH = ?")
        .bind(page_hash)
        .execute(&mut *tx)
        .await?
        .rows_affected();

    if deleted > 0 {
        sqlx::query(
            "UPDATE PAGE_HASH\n             SET DELETE_COUNT = DELETE_COUNT + ?,\n                 LAST_MODIFIED_DATE = CURRENT_TIMESTAMP\n             WHERE HASH = ?",
        )
        .bind(deleted as i64)
        .bind(page_hash)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(deleted)
}

pub async fn delete_page_hash_match(
    database_file: &Path,
    page_hash: &str,
    book_id: &str,
    page_number: u64,
) -> Result<u64, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let mut tx = pool.begin().await?;

    let deleted = sqlx::query(
        "DELETE\n         FROM MEDIA_PAGE\n         WHERE FILE_HASH = ?\n         AND BOOK_ID = ?\n         AND NUMBER = ?",
    )
    .bind(page_hash)
    .bind(book_id)
    .bind(page_number.saturating_sub(1) as i64)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    if deleted > 0 {
        sqlx::query(
            "UPDATE PAGE_HASH\n             SET DELETE_COUNT = DELETE_COUNT + ?,\n                 LAST_MODIFIED_DATE = CURRENT_TIMESTAMP\n             WHERE HASH = ?",
        )
        .bind(deleted as i64)
        .bind(page_hash)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(deleted)
}
