use std::collections::HashMap;

use komga_application::identity_access::{KoboSyncPointBook, KoboSyncReadListSnapshot};
use sqlx::{Row, Sqlite};

use super::mark_synced::{
    mark_books_synced, mark_readlists_synced, mark_removed_books_synced,
    mark_removed_readlists_synced,
};

fn map_sync_point_book(row: sqlx::sqlite::SqliteRow) -> KoboSyncPointBook {
    KoboSyncPointBook {
        book_id: row.get::<String, _>("BOOK_ID"),
        created: row.get::<String, _>("BOOK_CREATED_DATE"),
        file_last_modified: row.get::<String, _>("BOOK_FILE_LAST_MODIFIED"),
        file_size: row.get::<i64, _>("BOOK_FILE_SIZE").max(0) as u64,
        file_hash: row.get::<String, _>("BOOK_FILE_HASH"),
        metadata_last_modified: row.get::<String, _>("BOOK_METADATA_LAST_MODIFIED_DATE"),
        read_progress_last_modified: row
            .get::<Option<String>, _>("BOOK_READ_PROGRESS_LAST_MODIFIED_DATE"),
        cover_image_id: row.get::<Option<String>, _>("BOOK_THUMBNAIL_ID"),
    }
}

fn map_readlist_row(row: sqlx::sqlite::SqliteRow) -> KoboSyncReadListSnapshot {
    KoboSyncReadListSnapshot {
        id: row.get::<String, _>("READLIST_ID"),
        name: row.get::<String, _>("READLIST_NAME"),
        created: row.get::<String, _>("READLIST_CREATED_DATE"),
        last_modified: row.get::<String, _>("READLIST_LAST_MODIFIED_DATE"),
        items: Vec::new(),
    }
}

async fn hydrate_readlists(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    sync_point_id: &str,
    mut readlists: Vec<KoboSyncReadListSnapshot>,
    include_items: bool,
) -> Result<Vec<KoboSyncReadListSnapshot>, sqlx::Error> {
    if !include_items || readlists.is_empty() {
        return Ok(readlists);
    }

    let mut query = sqlx::QueryBuilder::<Sqlite>::new(
        "SELECT READLIST_ID, BOOK_ID FROM SYNC_POINT_READLIST_BOOK WHERE SYNC_POINT_ID = ",
    );
    query.push_bind(sync_point_id);
    query.push(" AND READLIST_ID IN (");
    let mut separated = query.separated(", ");
    for readlist in &readlists {
        separated.push_bind(readlist.id.as_str());
    }
    separated.push_unseparated(") ORDER BY READLIST_ID ASC, BOOK_ID ASC");

    let rows = query.build().fetch_all(&mut **tx).await?;
    let mut items_by_readlist = HashMap::<String, Vec<String>>::new();
    for row in rows {
        items_by_readlist
            .entry(row.get::<String, _>("READLIST_ID"))
            .or_default()
            .push(row.get::<String, _>("BOOK_ID"));
    }

    for readlist in &mut readlists {
        readlist.items = items_by_readlist.remove(&readlist.id).unwrap_or_default();
    }
    Ok(readlists)
}

pub(super) async fn take_books_by_sync_point(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    sync_point_id: &str,
    limit: usize,
) -> Result<Vec<KoboSyncPointBook>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT
            BOOK_ID,
            BOOK_CREATED_DATE,
            BOOK_FILE_LAST_MODIFIED,
            BOOK_FILE_SIZE,
            BOOK_FILE_HASH,
            BOOK_METADATA_LAST_MODIFIED_DATE,
            BOOK_READ_PROGRESS_LAST_MODIFIED_DATE,
            BOOK_THUMBNAIL_ID
        FROM SYNC_POINT_BOOK
        WHERE SYNC_POINT_ID = ?
          AND SYNCED = FALSE
        ORDER BY BOOK_ID ASC
        LIMIT ?
        "#,
    )
    .bind(sync_point_id)
    .bind(limit as i64)
    .fetch_all(&mut **tx)
    .await?;

    let books = rows
        .into_iter()
        .map(map_sync_point_book)
        .collect::<Vec<_>>();
    mark_books_synced(tx, sync_point_id, &books).await?;
    Ok(books)
}

pub(super) async fn take_books_added(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    from_sync_point_id: &str,
    to_sync_point_id: &str,
    limit: usize,
) -> Result<Vec<KoboSyncPointBook>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT
            to_spb.BOOK_ID,
            to_spb.BOOK_CREATED_DATE,
            to_spb.BOOK_FILE_LAST_MODIFIED,
            to_spb.BOOK_FILE_SIZE,
            to_spb.BOOK_FILE_HASH,
            to_spb.BOOK_METADATA_LAST_MODIFIED_DATE,
            to_spb.BOOK_READ_PROGRESS_LAST_MODIFIED_DATE,
            to_spb.BOOK_THUMBNAIL_ID
        FROM SYNC_POINT_BOOK to_spb
        WHERE to_spb.SYNC_POINT_ID = ?
          AND to_spb.SYNCED = FALSE
          AND to_spb.BOOK_ID NOT IN (
              SELECT from_spb.BOOK_ID
              FROM SYNC_POINT_BOOK from_spb
              WHERE from_spb.SYNC_POINT_ID = ?
          )
        ORDER BY to_spb.BOOK_ID ASC
        LIMIT ?
        "#,
    )
    .bind(to_sync_point_id)
    .bind(from_sync_point_id)
    .bind(limit as i64)
    .fetch_all(&mut **tx)
    .await?;

    let books = rows
        .into_iter()
        .map(map_sync_point_book)
        .collect::<Vec<_>>();
    mark_books_synced(tx, to_sync_point_id, &books).await?;
    Ok(books)
}

pub(super) async fn take_books_changed(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    from_sync_point_id: &str,
    to_sync_point_id: &str,
    limit: usize,
) -> Result<Vec<KoboSyncPointBook>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT
            to_spb.BOOK_ID,
            to_spb.BOOK_CREATED_DATE,
            to_spb.BOOK_FILE_LAST_MODIFIED,
            to_spb.BOOK_FILE_SIZE,
            to_spb.BOOK_FILE_HASH,
            to_spb.BOOK_METADATA_LAST_MODIFIED_DATE,
            to_spb.BOOK_READ_PROGRESS_LAST_MODIFIED_DATE,
            to_spb.BOOK_THUMBNAIL_ID
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
        ORDER BY to_spb.BOOK_ID ASC
        LIMIT ?
        "#,
    )
    .bind(to_sync_point_id)
    .bind(from_sync_point_id)
    .bind(limit as i64)
    .fetch_all(&mut **tx)
    .await?;

    let books = rows
        .into_iter()
        .map(map_sync_point_book)
        .collect::<Vec<_>>();
    mark_books_synced(tx, to_sync_point_id, &books).await?;
    Ok(books)
}

pub(super) async fn take_books_removed(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    from_sync_point_id: &str,
    to_sync_point_id: &str,
    limit: usize,
) -> Result<Vec<KoboSyncPointBook>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT
            from_spb.BOOK_ID,
            from_spb.BOOK_CREATED_DATE,
            from_spb.BOOK_FILE_LAST_MODIFIED,
            from_spb.BOOK_FILE_SIZE,
            from_spb.BOOK_FILE_HASH,
            from_spb.BOOK_METADATA_LAST_MODIFIED_DATE,
            from_spb.BOOK_READ_PROGRESS_LAST_MODIFIED_DATE,
            from_spb.BOOK_THUMBNAIL_ID
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
        ORDER BY from_spb.BOOK_ID ASC
        LIMIT ?
        "#,
    )
    .bind(from_sync_point_id)
    .bind(to_sync_point_id)
    .bind(to_sync_point_id)
    .bind(limit as i64)
    .fetch_all(&mut **tx)
    .await?;

    let books = rows
        .into_iter()
        .map(map_sync_point_book)
        .collect::<Vec<_>>();
    mark_removed_books_synced(tx, to_sync_point_id, &books).await?;
    Ok(books)
}

pub(super) async fn take_books_read_progress_changed(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    from_sync_point_id: &str,
    to_sync_point_id: &str,
    limit: usize,
) -> Result<Vec<KoboSyncPointBook>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT
            to_spb.BOOK_ID,
            to_spb.BOOK_CREATED_DATE,
            to_spb.BOOK_FILE_LAST_MODIFIED,
            to_spb.BOOK_FILE_SIZE,
            to_spb.BOOK_FILE_HASH,
            to_spb.BOOK_METADATA_LAST_MODIFIED_DATE,
            to_spb.BOOK_READ_PROGRESS_LAST_MODIFIED_DATE,
            to_spb.BOOK_THUMBNAIL_ID
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
        ORDER BY to_spb.BOOK_ID ASC
        LIMIT ?
        "#,
    )
    .bind(to_sync_point_id)
    .bind(from_sync_point_id)
    .bind(limit as i64)
    .fetch_all(&mut **tx)
    .await?;

    let books = rows
        .into_iter()
        .map(map_sync_point_book)
        .collect::<Vec<_>>();
    mark_books_synced(tx, to_sync_point_id, &books).await?;
    Ok(books)
}

pub(super) async fn take_readlists_by_sync_point(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    sync_point_id: &str,
    limit: usize,
) -> Result<Vec<KoboSyncReadListSnapshot>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT
            READLIST_ID,
            READLIST_NAME,
            READLIST_CREATED_DATE,
            READLIST_LAST_MODIFIED_DATE
        FROM SYNC_POINT_READLIST
        WHERE SYNC_POINT_ID = ?
          AND SYNCED = FALSE
        ORDER BY READLIST_ID ASC
        LIMIT ?
        "#,
    )
    .bind(sync_point_id)
    .bind(limit as i64)
    .fetch_all(&mut **tx)
    .await?;

    let readlists = hydrate_readlists(
        tx,
        sync_point_id,
        rows.into_iter().map(map_readlist_row).collect::<Vec<_>>(),
        true,
    )
    .await?;
    mark_readlists_synced(tx, sync_point_id, &readlists).await?;
    Ok(readlists)
}

pub(super) async fn take_readlists_added(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    from_sync_point_id: &str,
    to_sync_point_id: &str,
    limit: usize,
) -> Result<Vec<KoboSyncReadListSnapshot>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT
            to_rl.READLIST_ID,
            to_rl.READLIST_NAME,
            to_rl.READLIST_CREATED_DATE,
            to_rl.READLIST_LAST_MODIFIED_DATE
        FROM SYNC_POINT_READLIST to_rl
        LEFT JOIN SYNC_POINT_READLIST from_rl
            ON to_rl.READLIST_ID = from_rl.READLIST_ID
           AND from_rl.SYNC_POINT_ID = ?
        WHERE to_rl.SYNC_POINT_ID = ?
          AND to_rl.SYNCED = FALSE
          AND from_rl.READLIST_ID IS NULL
        ORDER BY to_rl.READLIST_ID ASC
        LIMIT ?
        "#,
    )
    .bind(from_sync_point_id)
    .bind(to_sync_point_id)
    .bind(limit as i64)
    .fetch_all(&mut **tx)
    .await?;

    let readlists = hydrate_readlists(
        tx,
        to_sync_point_id,
        rows.into_iter().map(map_readlist_row).collect::<Vec<_>>(),
        true,
    )
    .await?;
    mark_readlists_synced(tx, to_sync_point_id, &readlists).await?;
    Ok(readlists)
}

pub(super) async fn take_readlists_changed(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    from_sync_point_id: &str,
    to_sync_point_id: &str,
    limit: usize,
) -> Result<Vec<KoboSyncReadListSnapshot>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT
            to_rl.READLIST_ID,
            to_rl.READLIST_NAME,
            to_rl.READLIST_CREATED_DATE,
            to_rl.READLIST_LAST_MODIFIED_DATE
        FROM SYNC_POINT_READLIST to_rl
        JOIN SYNC_POINT_READLIST from_rl ON to_rl.READLIST_ID = from_rl.READLIST_ID
        WHERE to_rl.SYNC_POINT_ID = ?
          AND from_rl.SYNC_POINT_ID = ?
          AND to_rl.SYNCED = FALSE
          AND (
              to_rl.READLIST_LAST_MODIFIED_DATE != from_rl.READLIST_LAST_MODIFIED_DATE
              OR to_rl.READLIST_NAME != from_rl.READLIST_NAME
          )
        ORDER BY to_rl.READLIST_ID ASC
        LIMIT ?
        "#,
    )
    .bind(to_sync_point_id)
    .bind(from_sync_point_id)
    .bind(limit as i64)
    .fetch_all(&mut **tx)
    .await?;

    let readlists = hydrate_readlists(
        tx,
        to_sync_point_id,
        rows.into_iter().map(map_readlist_row).collect::<Vec<_>>(),
        true,
    )
    .await?;
    mark_readlists_synced(tx, to_sync_point_id, &readlists).await?;
    Ok(readlists)
}

pub(super) async fn take_readlists_removed(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    from_sync_point_id: &str,
    to_sync_point_id: &str,
    limit: usize,
) -> Result<Vec<KoboSyncReadListSnapshot>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT
            from_rl.READLIST_ID,
            from_rl.READLIST_NAME,
            from_rl.READLIST_CREATED_DATE,
            from_rl.READLIST_LAST_MODIFIED_DATE
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
        ORDER BY from_rl.READLIST_ID ASC
        LIMIT ?
        "#,
    )
    .bind(to_sync_point_id)
    .bind(from_sync_point_id)
    .bind(to_sync_point_id)
    .bind(limit as i64)
    .fetch_all(&mut **tx)
    .await?;

    let readlists = rows.into_iter().map(map_readlist_row).collect::<Vec<_>>();
    mark_removed_readlists_synced(tx, to_sync_point_id, &readlists).await?;
    Ok(readlists)
}
