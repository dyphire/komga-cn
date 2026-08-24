use std::collections::HashMap;

use komga_application::identity_access::{
    KoboSyncPage, KoboSyncPointBook, KoboSyncReadListSnapshot,
};
use sqlx::{Row, Sqlite};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DiffKind {
    BooksAdded,
    BooksChanged,
    BooksRemoved,
    BooksReadProgressChanged,
    ReadlistsAdded,
    ReadlistsChanged,
    ReadlistsRemoved,
}

#[derive(Clone, Debug)]
struct DiffCandidate {
    kind: DiffKind,
    id: String,
}

#[derive(Clone, Debug)]
struct DiffSelection {
    candidates: Vec<DiffCandidate>,
    should_continue: bool,
}

pub(super) async fn load_initial_sync_page(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    to_sync_point_id: &str,
    limit: usize,
) -> Result<KoboSyncPage, sqlx::Error> {
    let selection = select_initial_candidates(tx, to_sync_point_id, limit).await?;
    let books_added = hydrate_books(
        tx,
        to_sync_point_id,
        &candidate_ids(&selection.candidates, DiffKind::BooksAdded),
    )
    .await?;
    mark_books_synced(tx, to_sync_point_id, &books_added).await?;

    let readlists_added = hydrate_readlists(
        tx,
        to_sync_point_id,
        &candidate_ids(&selection.candidates, DiffKind::ReadlistsAdded),
        true,
    )
    .await?;
    mark_readlists_synced(tx, to_sync_point_id, &readlists_added).await?;

    Ok(KoboSyncPage {
        to_sync_point_id: String::new(),
        from_sync_point_id: None,
        books_added,
        books_changed: Vec::new(),
        books_removed: Vec::new(),
        books_read_progress_changed: Vec::new(),
        readlists_added,
        readlists_changed: Vec::new(),
        readlists_removed: Vec::new(),
        should_continue: selection.should_continue,
    })
}

pub(super) async fn load_incremental_sync_page(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    from_sync_point_id: &str,
    to_sync_point_id: &str,
    limit: usize,
) -> Result<KoboSyncPage, sqlx::Error> {
    let selection =
        select_incremental_candidates(tx, from_sync_point_id, to_sync_point_id, limit).await?;

    let books_added = hydrate_books(
        tx,
        to_sync_point_id,
        &candidate_ids(&selection.candidates, DiffKind::BooksAdded),
    )
    .await?;
    mark_books_synced(tx, to_sync_point_id, &books_added).await?;

    let books_changed = hydrate_books(
        tx,
        to_sync_point_id,
        &candidate_ids(&selection.candidates, DiffKind::BooksChanged),
    )
    .await?;
    mark_books_synced(tx, to_sync_point_id, &books_changed).await?;

    let books_removed = hydrate_books(
        tx,
        from_sync_point_id,
        &candidate_ids(&selection.candidates, DiffKind::BooksRemoved),
    )
    .await?;
    mark_removed_books_synced(tx, to_sync_point_id, &books_removed).await?;

    let books_read_progress_changed = hydrate_books(
        tx,
        to_sync_point_id,
        &candidate_ids(&selection.candidates, DiffKind::BooksReadProgressChanged),
    )
    .await?;
    mark_books_synced(tx, to_sync_point_id, &books_read_progress_changed).await?;

    let readlists_added = hydrate_readlists(
        tx,
        to_sync_point_id,
        &candidate_ids(&selection.candidates, DiffKind::ReadlistsAdded),
        true,
    )
    .await?;
    mark_readlists_synced(tx, to_sync_point_id, &readlists_added).await?;

    let readlists_changed = hydrate_readlists(
        tx,
        to_sync_point_id,
        &candidate_ids(&selection.candidates, DiffKind::ReadlistsChanged),
        true,
    )
    .await?;
    mark_readlists_synced(tx, to_sync_point_id, &readlists_changed).await?;

    let readlists_removed = hydrate_readlists(
        tx,
        from_sync_point_id,
        &candidate_ids(&selection.candidates, DiffKind::ReadlistsRemoved),
        false,
    )
    .await?;
    mark_removed_readlists_synced(tx, to_sync_point_id, &readlists_removed).await?;

    Ok(KoboSyncPage {
        to_sync_point_id: String::new(),
        from_sync_point_id: None,
        books_added,
        books_changed,
        books_removed,
        books_read_progress_changed,
        readlists_added,
        readlists_changed,
        readlists_removed,
        should_continue: selection.should_continue,
    })
}

async fn select_initial_candidates(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    to_sync_point_id: &str,
    limit: usize,
) -> Result<DiffSelection, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT KIND, ENTITY_ID
        FROM (
            SELECT 0 AS KIND, BOOK_ID AS ENTITY_ID
            FROM SYNC_POINT_BOOK
            WHERE SYNC_POINT_ID = ?
              AND SYNCED = FALSE

            UNION ALL

            SELECT 4 AS KIND, READLIST_ID AS ENTITY_ID
            FROM SYNC_POINT_READLIST
            WHERE SYNC_POINT_ID = ?
              AND SYNCED = FALSE
        )
        ORDER BY KIND ASC, ENTITY_ID ASC
        LIMIT ?
        "#,
    )
    .bind(to_sync_point_id)
    .bind(to_sync_point_id)
    .bind(limit.saturating_add(1) as i64)
    .fetch_all(&mut **tx)
    .await?;

    Ok(selection_from_rows(rows, limit))
}

async fn select_incremental_candidates(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    from_sync_point_id: &str,
    to_sync_point_id: &str,
    limit: usize,
) -> Result<DiffSelection, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT KIND, ENTITY_ID
        FROM (
            SELECT 0 AS KIND, to_spb.BOOK_ID AS ENTITY_ID
            FROM SYNC_POINT_BOOK to_spb
            WHERE to_spb.SYNC_POINT_ID = ?
              AND to_spb.SYNCED = FALSE
              AND to_spb.BOOK_ID NOT IN (
                  SELECT from_spb.BOOK_ID
                  FROM SYNC_POINT_BOOK from_spb
                  WHERE from_spb.SYNC_POINT_ID = ?
              )

            UNION ALL

            SELECT 1 AS KIND, to_spb.BOOK_ID AS ENTITY_ID
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

            UNION ALL

            SELECT 2 AS KIND, from_spb.BOOK_ID AS ENTITY_ID
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

            UNION ALL

            SELECT 3 AS KIND, to_spb.BOOK_ID AS ENTITY_ID
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

            UNION ALL

            SELECT 4 AS KIND, to_rl.READLIST_ID AS ENTITY_ID
            FROM SYNC_POINT_READLIST to_rl
            LEFT JOIN SYNC_POINT_READLIST from_rl
                ON to_rl.READLIST_ID = from_rl.READLIST_ID
               AND from_rl.SYNC_POINT_ID = ?
            WHERE to_rl.SYNC_POINT_ID = ?
              AND to_rl.SYNCED = FALSE
              AND from_rl.READLIST_ID IS NULL

            UNION ALL

            SELECT 5 AS KIND, to_rl.READLIST_ID AS ENTITY_ID
            FROM SYNC_POINT_READLIST to_rl
            JOIN SYNC_POINT_READLIST from_rl ON to_rl.READLIST_ID = from_rl.READLIST_ID
            WHERE to_rl.SYNC_POINT_ID = ?
              AND from_rl.SYNC_POINT_ID = ?
              AND to_rl.SYNCED = FALSE
              AND (
                  to_rl.READLIST_LAST_MODIFIED_DATE != from_rl.READLIST_LAST_MODIFIED_DATE
                  OR to_rl.READLIST_NAME != from_rl.READLIST_NAME
                  OR EXISTS (
                      SELECT 1
                      FROM SYNC_POINT_READLIST_BOOK to_item
                      WHERE to_item.SYNC_POINT_ID = to_rl.SYNC_POINT_ID
                        AND to_item.READLIST_ID = to_rl.READLIST_ID
                        AND NOT EXISTS (
                            SELECT 1
                            FROM SYNC_POINT_READLIST_BOOK from_item
                            WHERE from_item.SYNC_POINT_ID = from_rl.SYNC_POINT_ID
                              AND from_item.READLIST_ID = from_rl.READLIST_ID
                              AND from_item.BOOK_ID = to_item.BOOK_ID
                        )
                  )
                  OR EXISTS (
                      SELECT 1
                      FROM SYNC_POINT_READLIST_BOOK from_item
                      WHERE from_item.SYNC_POINT_ID = from_rl.SYNC_POINT_ID
                        AND from_item.READLIST_ID = from_rl.READLIST_ID
                        AND NOT EXISTS (
                            SELECT 1
                            FROM SYNC_POINT_READLIST_BOOK to_item
                            WHERE to_item.SYNC_POINT_ID = to_rl.SYNC_POINT_ID
                              AND to_item.READLIST_ID = to_rl.READLIST_ID
                              AND to_item.BOOK_ID = from_item.BOOK_ID
                        )
                  )
              )

            UNION ALL

            SELECT 6 AS KIND, from_rl.READLIST_ID AS ENTITY_ID
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
        )
        ORDER BY KIND ASC, ENTITY_ID ASC
        LIMIT ?
        "#,
    )
    .bind(to_sync_point_id)
    .bind(from_sync_point_id)
    .bind(to_sync_point_id)
    .bind(from_sync_point_id)
    .bind(from_sync_point_id)
    .bind(to_sync_point_id)
    .bind(to_sync_point_id)
    .bind(to_sync_point_id)
    .bind(from_sync_point_id)
    .bind(from_sync_point_id)
    .bind(to_sync_point_id)
    .bind(to_sync_point_id)
    .bind(from_sync_point_id)
    .bind(to_sync_point_id)
    .bind(from_sync_point_id)
    .bind(to_sync_point_id)
    .bind(limit.saturating_add(1) as i64)
    .fetch_all(&mut **tx)
    .await?;

    Ok(selection_from_rows(rows, limit))
}

fn selection_from_rows(rows: Vec<sqlx::sqlite::SqliteRow>, limit: usize) -> DiffSelection {
    let should_continue = rows.len() > limit;
    let candidates = rows
        .into_iter()
        .take(limit)
        .map(|row| DiffCandidate {
            kind: diff_kind(row.get::<i64, _>("KIND")),
            id: row.get::<String, _>("ENTITY_ID"),
        })
        .collect();

    DiffSelection {
        candidates,
        should_continue,
    }
}

fn diff_kind(value: i64) -> DiffKind {
    match value {
        0 => DiffKind::BooksAdded,
        1 => DiffKind::BooksChanged,
        2 => DiffKind::BooksRemoved,
        3 => DiffKind::BooksReadProgressChanged,
        4 => DiffKind::ReadlistsAdded,
        5 => DiffKind::ReadlistsChanged,
        6 => DiffKind::ReadlistsRemoved,
        _ => unreachable!("diff kind is selected from static SQL"),
    }
}

fn candidate_ids(candidates: &[DiffCandidate], kind: DiffKind) -> Vec<String> {
    candidates
        .iter()
        .filter(|candidate| candidate.kind == kind)
        .map(|candidate| candidate.id.clone())
        .collect()
}

async fn hydrate_books(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    sync_point_id: &str,
    book_ids: &[String],
) -> Result<Vec<KoboSyncPointBook>, sqlx::Error> {
    if book_ids.is_empty() {
        return Ok(Vec::new());
    }

    let mut query = sqlx::QueryBuilder::<Sqlite>::new(
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
        WHERE SYNC_POINT_ID =
        "#,
    );
    query.push_bind(sync_point_id);
    query.push(" AND BOOK_ID IN (");
    let mut separated = query.separated(", ");
    for book_id in book_ids {
        separated.push_bind(book_id.as_str());
    }
    separated.push_unseparated(") ORDER BY BOOK_ID ASC");

    let rows = query.build().fetch_all(&mut **tx).await?;
    Ok(rows.into_iter().map(map_sync_point_book).collect())
}

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

async fn hydrate_readlists(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    sync_point_id: &str,
    readlist_ids: &[String],
    include_items: bool,
) -> Result<Vec<KoboSyncReadListSnapshot>, sqlx::Error> {
    if readlist_ids.is_empty() {
        return Ok(Vec::new());
    }

    let mut query = sqlx::QueryBuilder::<Sqlite>::new(
        r#"
        SELECT
            READLIST_ID,
            READLIST_NAME,
            READLIST_CREATED_DATE,
            READLIST_LAST_MODIFIED_DATE
        FROM SYNC_POINT_READLIST
        WHERE SYNC_POINT_ID =
        "#,
    );
    query.push_bind(sync_point_id);
    query.push(" AND READLIST_ID IN (");
    let mut separated = query.separated(", ");
    for readlist_id in readlist_ids {
        separated.push_bind(readlist_id.as_str());
    }
    separated.push_unseparated(") ORDER BY READLIST_ID ASC");

    let mut readlists = query
        .build()
        .fetch_all(&mut **tx)
        .await?
        .into_iter()
        .map(map_readlist_row)
        .collect::<Vec<_>>();
    if !include_items {
        return Ok(readlists);
    }

    let mut item_query = sqlx::QueryBuilder::<Sqlite>::new(
        "SELECT READLIST_ID, BOOK_ID FROM SYNC_POINT_READLIST_BOOK WHERE SYNC_POINT_ID = ",
    );
    item_query.push_bind(sync_point_id);
    item_query.push(" AND READLIST_ID IN (");
    let mut separated = item_query.separated(", ");
    for readlist_id in readlist_ids {
        separated.push_bind(readlist_id.as_str());
    }
    separated.push_unseparated(") ORDER BY READLIST_ID ASC, BOOK_ID ASC");

    let rows = item_query.build().fetch_all(&mut **tx).await?;
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

fn map_readlist_row(row: sqlx::sqlite::SqliteRow) -> KoboSyncReadListSnapshot {
    KoboSyncReadListSnapshot {
        id: row.get::<String, _>("READLIST_ID"),
        name: row.get::<String, _>("READLIST_NAME"),
        created: row.get::<String, _>("READLIST_CREATED_DATE"),
        last_modified: row.get::<String, _>("READLIST_LAST_MODIFIED_DATE"),
        items: Vec::new(),
    }
}

async fn mark_books_synced(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    sync_point_id: &str,
    books: &[KoboSyncPointBook],
) -> Result<(), sqlx::Error> {
    if books.is_empty() {
        return Ok(());
    }
    let mut query = sqlx::QueryBuilder::<Sqlite>::new(
        "UPDATE SYNC_POINT_BOOK SET SYNCED = TRUE WHERE SYNC_POINT_ID = ",
    );
    query.push_bind(sync_point_id);
    query.push(" AND BOOK_ID IN (");
    let mut separated = query.separated(", ");
    for book in books {
        separated.push_bind(book.book_id.as_str());
    }
    separated.push_unseparated(")");
    query.build().execute(&mut **tx).await?;
    Ok(())
}

async fn mark_removed_books_synced(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    sync_point_id: &str,
    books: &[KoboSyncPointBook],
) -> Result<(), sqlx::Error> {
    for book in books {
        sqlx::query(
            "INSERT OR IGNORE INTO SYNC_POINT_BOOK_REMOVED_SYNCED (SYNC_POINT_ID, BOOK_ID) VALUES (?, ?)",
        )
        .bind(sync_point_id)
        .bind(book.book_id.as_str())
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn mark_readlists_synced(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    sync_point_id: &str,
    readlists: &[KoboSyncReadListSnapshot],
) -> Result<(), sqlx::Error> {
    if readlists.is_empty() {
        return Ok(());
    }
    let mut query = sqlx::QueryBuilder::<Sqlite>::new(
        "UPDATE SYNC_POINT_READLIST SET SYNCED = TRUE WHERE SYNC_POINT_ID = ",
    );
    query.push_bind(sync_point_id);
    query.push(" AND READLIST_ID IN (");
    let mut separated = query.separated(", ");
    for readlist in readlists {
        separated.push_bind(readlist.id.as_str());
    }
    separated.push_unseparated(")");
    query.build().execute(&mut **tx).await?;
    Ok(())
}

async fn mark_removed_readlists_synced(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    sync_point_id: &str,
    readlists: &[KoboSyncReadListSnapshot],
) -> Result<(), sqlx::Error> {
    for readlist in readlists {
        sqlx::query(
            "INSERT OR IGNORE INTO SYNC_POINT_READLIST_REMOVED_SYNCED (SYNC_POINT_ID, READLIST_ID) VALUES (?, ?)",
        )
        .bind(sync_point_id)
        .bind(readlist.id.as_str())
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}
