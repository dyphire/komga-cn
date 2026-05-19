use komga_application::identity_access::KoboSyncPage;
use sqlx::Sqlite;

use super::exists::{has_incremental_remaining, has_initial_remaining};
use super::queries::{
    take_books_added, take_books_by_sync_point, take_books_changed,
    take_books_read_progress_changed, take_books_removed, take_readlists_added,
    take_readlists_by_sync_point, take_readlists_changed, take_readlists_removed,
};

pub(super) async fn load_initial_sync_page(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    to_sync_point_id: &str,
    limit: usize,
) -> Result<KoboSyncPage, sqlx::Error> {
    let mut remaining = limit;
    let mut books_added = Vec::new();
    let mut readlists_added = Vec::new();

    if remaining > 0 {
        books_added = take_books_by_sync_point(tx, to_sync_point_id, remaining).await?;
        remaining = remaining.saturating_sub(books_added.len());
    }
    if remaining > 0 {
        readlists_added = take_readlists_by_sync_point(tx, to_sync_point_id, remaining).await?;
    }

    let should_continue = has_initial_remaining(tx, to_sync_point_id).await?;
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
        should_continue,
    })
}

pub(super) async fn load_incremental_sync_page(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    from_sync_point_id: &str,
    to_sync_point_id: &str,
    limit: usize,
) -> Result<KoboSyncPage, sqlx::Error> {
    let mut remaining = limit;
    let mut books_added = Vec::new();
    let mut books_changed = Vec::new();
    let mut books_removed = Vec::new();
    let mut books_read_progress_changed = Vec::new();
    let mut readlists_added = Vec::new();
    let mut readlists_changed = Vec::new();
    let mut readlists_removed = Vec::new();

    if remaining > 0 {
        books_added = take_books_added(tx, from_sync_point_id, to_sync_point_id, remaining).await?;
        remaining = remaining.saturating_sub(books_added.len());
    }
    if remaining > 0 {
        books_changed =
            take_books_changed(tx, from_sync_point_id, to_sync_point_id, remaining).await?;
        remaining = remaining.saturating_sub(books_changed.len());
    }
    if remaining > 0 {
        books_removed =
            take_books_removed(tx, from_sync_point_id, to_sync_point_id, remaining).await?;
        remaining = remaining.saturating_sub(books_removed.len());
    }
    if remaining > 0 {
        books_read_progress_changed =
            take_books_read_progress_changed(tx, from_sync_point_id, to_sync_point_id, remaining)
                .await?;
        remaining = remaining.saturating_sub(books_read_progress_changed.len());
    }
    if remaining > 0 {
        readlists_added =
            take_readlists_added(tx, from_sync_point_id, to_sync_point_id, remaining).await?;
        remaining = remaining.saturating_sub(readlists_added.len());
    }
    if remaining > 0 {
        readlists_changed =
            take_readlists_changed(tx, from_sync_point_id, to_sync_point_id, remaining).await?;
        remaining = remaining.saturating_sub(readlists_changed.len());
    }
    if remaining > 0 {
        readlists_removed =
            take_readlists_removed(tx, from_sync_point_id, to_sync_point_id, remaining).await?;
    }

    let should_continue =
        has_incremental_remaining(tx, from_sync_point_id, to_sync_point_id).await?;
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
        should_continue,
    })
}
