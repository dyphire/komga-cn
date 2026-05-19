use komga_application::identity_access::{KoboSyncPointBook, KoboSyncReadListSnapshot};
use sqlx::Sqlite;

pub(super) async fn mark_books_synced(
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

pub(super) async fn mark_removed_books_synced(
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

pub(super) async fn mark_readlists_synced(
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

pub(super) async fn mark_removed_readlists_synced(
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
