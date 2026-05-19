use sqlx::Sqlite;

pub(super) async fn has_initial_remaining(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    to_sync_point_id: &str,
) -> Result<bool, sqlx::Error> {
    Ok(exists_unsynced_books(tx, to_sync_point_id).await?
        || exists_unsynced_readlists(tx, to_sync_point_id).await?)
}

pub(super) async fn has_incremental_remaining(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    from_sync_point_id: &str,
    to_sync_point_id: &str,
) -> Result<bool, sqlx::Error> {
    Ok(
        exists_books_added(tx, from_sync_point_id, to_sync_point_id).await?
            || exists_books_changed(tx, from_sync_point_id, to_sync_point_id).await?
            || exists_books_removed(tx, from_sync_point_id, to_sync_point_id).await?
            || exists_books_read_progress_changed(tx, from_sync_point_id, to_sync_point_id).await?
            || exists_readlists_added(tx, from_sync_point_id, to_sync_point_id).await?
            || exists_readlists_changed(tx, from_sync_point_id, to_sync_point_id).await?
            || exists_readlists_removed(tx, from_sync_point_id, to_sync_point_id).await?,
    )
}

async fn exists_unsynced_books(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    sync_point_id: &str,
) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query(
        "SELECT 1 FROM SYNC_POINT_BOOK WHERE SYNC_POINT_ID = ? AND SYNCED = FALSE LIMIT 1",
    )
    .bind(sync_point_id)
    .fetch_optional(&mut **tx)
    .await?
    .is_some())
}

async fn exists_unsynced_readlists(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    sync_point_id: &str,
) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query(
        "SELECT 1 FROM SYNC_POINT_READLIST WHERE SYNC_POINT_ID = ? AND SYNCED = FALSE LIMIT 1",
    )
    .bind(sync_point_id)
    .fetch_optional(&mut **tx)
    .await?
    .is_some())
}

async fn exists_books_added(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    from_sync_point_id: &str,
    to_sync_point_id: &str,
) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query(
        r#"
        SELECT 1
        FROM SYNC_POINT_BOOK to_spb
        WHERE to_spb.SYNC_POINT_ID = ?
          AND to_spb.SYNCED = FALSE
          AND to_spb.BOOK_ID NOT IN (
              SELECT from_spb.BOOK_ID
              FROM SYNC_POINT_BOOK from_spb
              WHERE from_spb.SYNC_POINT_ID = ?
          )
        LIMIT 1
        "#,
    )
    .bind(to_sync_point_id)
    .bind(from_sync_point_id)
    .fetch_optional(&mut **tx)
    .await?
    .is_some())
}

async fn exists_books_changed(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    from_sync_point_id: &str,
    to_sync_point_id: &str,
) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query(
        r#"
        SELECT 1
        FROM SYNC_POINT_BOOK to_spb
        JOIN SYNC_POINT_BOOK from_spb ON to_spb.BOOK_ID = from_spb.BOOK_ID
        WHERE to_spb.SYNC_POINT_ID = ?
          AND from_spb.SYNC_POINT_ID = ?
          AND to_spb.SYNCED = FALSE
          AND (
              to_spb.BOOK_FILE_LAST_MODIFIED != from_spb.BOOK_FILE_LAST_MODIFIED
              OR to_spb.BOOK_FILE_SIZE != from_spb.BOOK_FILE_SIZE
              OR (
                  to_spb.BOOK_FILE_HASH != from_spb.BOOK_FILE_HASH
                  AND from_spb.BOOK_FILE_HASH IS NOT NULL
              )
              OR to_spb.BOOK_METADATA_LAST_MODIFIED_DATE != from_spb.BOOK_METADATA_LAST_MODIFIED_DATE
              OR COALESCE(to_spb.BOOK_THUMBNAIL_ID, '') != COALESCE(from_spb.BOOK_THUMBNAIL_ID, '')
          )
        LIMIT 1
        "#,
    )
    .bind(to_sync_point_id)
    .bind(from_sync_point_id)
    .fetch_optional(&mut **tx)
    .await?
    .is_some())
}

async fn exists_books_removed(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    from_sync_point_id: &str,
    to_sync_point_id: &str,
) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query(
        r#"
        SELECT 1
        FROM SYNC_POINT_BOOK from_spb
        WHERE from_spb.SYNC_POINT_ID = ?
          AND from_spb.BOOK_ID NOT IN (
              SELECT to_spb.BOOK_ID
              FROM SYNC_POINT_BOOK to_spb
              WHERE to_spb.SYNC_POINT_ID = ?
          )
          AND from_spb.BOOK_ID NOT IN (
              SELECT removed.BOOK_ID
              FROM SYNC_POINT_BOOK_REMOVED_SYNCED removed
              WHERE removed.SYNC_POINT_ID = ?
          )
        LIMIT 1
        "#,
    )
    .bind(from_sync_point_id)
    .bind(to_sync_point_id)
    .bind(to_sync_point_id)
    .fetch_optional(&mut **tx)
    .await?
    .is_some())
}

async fn exists_books_read_progress_changed(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    from_sync_point_id: &str,
    to_sync_point_id: &str,
) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query(
        r#"
        SELECT 1
        FROM SYNC_POINT_BOOK to_spb
        JOIN SYNC_POINT_BOOK from_spb ON to_spb.BOOK_ID = from_spb.BOOK_ID
        WHERE to_spb.SYNC_POINT_ID = ?
          AND from_spb.SYNC_POINT_ID = ?
          AND to_spb.SYNCED = FALSE
          AND to_spb.BOOK_FILE_LAST_MODIFIED = from_spb.BOOK_FILE_LAST_MODIFIED
          AND to_spb.BOOK_FILE_SIZE = from_spb.BOOK_FILE_SIZE
          AND (
              to_spb.BOOK_FILE_HASH = from_spb.BOOK_FILE_HASH
              OR from_spb.BOOK_FILE_HASH IS NULL
          )
          AND to_spb.BOOK_METADATA_LAST_MODIFIED_DATE = from_spb.BOOK_METADATA_LAST_MODIFIED_DATE
          AND COALESCE(to_spb.BOOK_THUMBNAIL_ID, '') = COALESCE(from_spb.BOOK_THUMBNAIL_ID, '')
          AND (
              to_spb.BOOK_READ_PROGRESS_LAST_MODIFIED_DATE != from_spb.BOOK_READ_PROGRESS_LAST_MODIFIED_DATE
              OR (
                  to_spb.BOOK_READ_PROGRESS_LAST_MODIFIED_DATE IS NULL
                  AND from_spb.BOOK_READ_PROGRESS_LAST_MODIFIED_DATE IS NOT NULL
              )
              OR (
                  to_spb.BOOK_READ_PROGRESS_LAST_MODIFIED_DATE IS NOT NULL
                  AND from_spb.BOOK_READ_PROGRESS_LAST_MODIFIED_DATE IS NULL
              )
          )
        LIMIT 1
        "#,
    )
    .bind(to_sync_point_id)
    .bind(from_sync_point_id)
    .fetch_optional(&mut **tx)
    .await?
    .is_some())
}

async fn exists_readlists_added(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    from_sync_point_id: &str,
    to_sync_point_id: &str,
) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query(
        r#"
        SELECT 1
        FROM SYNC_POINT_READLIST to_rl
        LEFT JOIN SYNC_POINT_READLIST from_rl
            ON to_rl.READLIST_ID = from_rl.READLIST_ID
           AND from_rl.SYNC_POINT_ID = ?
        WHERE to_rl.SYNC_POINT_ID = ?
          AND to_rl.SYNCED = FALSE
          AND from_rl.READLIST_ID IS NULL
        LIMIT 1
        "#,
    )
    .bind(from_sync_point_id)
    .bind(to_sync_point_id)
    .fetch_optional(&mut **tx)
    .await?
    .is_some())
}

async fn exists_readlists_changed(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    from_sync_point_id: &str,
    to_sync_point_id: &str,
) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query(
        r#"
        SELECT 1
        FROM SYNC_POINT_READLIST to_rl
        JOIN SYNC_POINT_READLIST from_rl ON to_rl.READLIST_ID = from_rl.READLIST_ID
        WHERE to_rl.SYNC_POINT_ID = ?
          AND from_rl.SYNC_POINT_ID = ?
          AND to_rl.SYNCED = FALSE
          AND (
              to_rl.READLIST_LAST_MODIFIED_DATE != from_rl.READLIST_LAST_MODIFIED_DATE
              OR to_rl.READLIST_NAME != from_rl.READLIST_NAME
          )
        LIMIT 1
        "#,
    )
    .bind(to_sync_point_id)
    .bind(from_sync_point_id)
    .fetch_optional(&mut **tx)
    .await?
    .is_some())
}

async fn exists_readlists_removed(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    from_sync_point_id: &str,
    to_sync_point_id: &str,
) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query(
        r#"
        SELECT 1
        FROM SYNC_POINT_READLIST from_rl
        LEFT JOIN SYNC_POINT_READLIST to_rl
            ON from_rl.READLIST_ID = to_rl.READLIST_ID
           AND to_rl.SYNC_POINT_ID = ?
        WHERE from_rl.SYNC_POINT_ID = ?
          AND from_rl.READLIST_ID NOT IN (
              SELECT removed.READLIST_ID
              FROM SYNC_POINT_READLIST_REMOVED_SYNCED removed
              WHERE removed.SYNC_POINT_ID = ?
          )
          AND to_rl.READLIST_ID IS NULL
        LIMIT 1
        "#,
    )
    .bind(to_sync_point_id)
    .bind(from_sync_point_id)
    .bind(to_sync_point_id)
    .fetch_optional(&mut **tx)
    .await?
    .is_some())
}
