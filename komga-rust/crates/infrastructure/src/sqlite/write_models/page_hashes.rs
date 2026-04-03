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
            "UPDATE PAGE_HASH \
             SET DELETE_COUNT = DELETE_COUNT + ?, \
                 LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
             WHERE HASH = ?",
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
        "DELETE \
         FROM MEDIA_PAGE \
         WHERE FILE_HASH = ? \
         AND BOOK_ID = ? \
         AND NUMBER = ?",
    )
    .bind(page_hash)
    .bind(book_id)
    .bind(page_number.saturating_sub(1) as i64)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    if deleted > 0 {
        sqlx::query(
            "UPDATE PAGE_HASH \
             SET DELETE_COUNT = DELETE_COUNT + ?, \
                 LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
             WHERE HASH = ?",
        )
        .bind(deleted as i64)
        .bind(page_hash)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(deleted)
}
