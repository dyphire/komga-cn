use komga_application::identity_access::{
    AuthUser, KoboStoreSyncMergeResult, KoboSyncPage, random_uuid_like,
};
use sqlx::{Row, SqlitePool};

mod access_control;
mod exists;
mod mark_synced;
mod page_loading;
mod proxy;
mod queries;
mod seeding;

use page_loading::{load_incremental_sync_page, load_initial_sync_page};
use seeding::{seed_sync_point_books, seed_sync_point_ondeck};

#[derive(Clone, Debug)]
struct PersistedSyncPoint {
    id: String,
}

pub async fn load_kobo_sync_page(
    pool: &SqlitePool,
    user: &AuthUser,
    user_id: &str,
    current_api_key_id: Option<&str>,
    ongoing_sync_point_id: Option<&str>,
    last_successful_sync_point_id: Option<&str>,
    limit: usize,
) -> Result<KoboSyncPage, sqlx::Error> {
    let mut tx = pool.begin().await?;

    let to_sync_point = if let Some(sync_point_id) = ongoing_sync_point_id {
        if let Some(sync_point) = load_sync_point_for_user(&mut tx, sync_point_id, user_id).await? {
            sync_point
        } else {
            let new_sync_point_id = random_uuid_like();
            create_sync_point(&mut tx, &new_sync_point_id, user, current_api_key_id).await?
        }
    } else {
        let new_sync_point_id = random_uuid_like();
        create_sync_point(&mut tx, &new_sync_point_id, user, current_api_key_id).await?
    };

    let from_sync_point = if let Some(sync_point_id) = last_successful_sync_point_id {
        load_sync_point_for_user(&mut tx, sync_point_id, user_id).await?
    } else {
        None
    };

    let page = if let Some(from_sync_point) = from_sync_point.as_ref() {
        load_incremental_sync_page(&mut tx, &from_sync_point.id, &to_sync_point.id, limit).await?
    } else {
        load_initial_sync_page(&mut tx, &to_sync_point.id, limit).await?
    };

    tx.commit().await?;
    Ok(KoboSyncPage {
        to_sync_point_id: to_sync_point.id,
        from_sync_point_id: from_sync_point.map(|value| value.id),
        ..page
    })
}

pub async fn remove_sync_point(pool: &SqlitePool, sync_point_id: &str) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    delete_sync_point_children(&mut tx, sync_point_id).await?;
    sqlx::query("DELETE FROM SYNC_POINT WHERE ID = ?")
        .bind(sync_point_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn proxy_kobo_store_library_sync(
    forwarded_headers: &[(String, String)],
    query: Option<&str>,
    raw_sync_token: &str,
) -> Result<KoboStoreSyncMergeResult, ()> {
    proxy::proxy_kobo_store_library_sync(forwarded_headers, query, raw_sync_token).await
}

async fn load_sync_point_for_user(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    sync_point_id: &str,
    user_id: &str,
) -> Result<Option<PersistedSyncPoint>, sqlx::Error> {
    sqlx::query("SELECT ID FROM SYNC_POINT WHERE ID = ? AND USER_ID = ? LIMIT 1")
        .bind(sync_point_id)
        .bind(user_id)
        .fetch_optional(&mut **tx)
        .await
        .map(|row| {
            row.map(|row| PersistedSyncPoint {
                id: row.get::<String, _>("ID"),
            })
        })
}

async fn create_sync_point(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    sync_point_id: &str,
    user: &AuthUser,
    api_key_id: Option<&str>,
) -> Result<PersistedSyncPoint, sqlx::Error> {
    sqlx::query("INSERT INTO SYNC_POINT (ID, USER_ID, API_KEY_ID) VALUES (?, ?, ?)")
        .bind(sync_point_id)
        .bind(user.id.as_str())
        .bind(api_key_id)
        .execute(&mut **tx)
        .await?;

    seed_sync_point_books(tx, sync_point_id, user).await?;
    seed_sync_point_ondeck(tx, sync_point_id, user).await?;

    Ok(PersistedSyncPoint {
        id: sync_point_id.to_string(),
    })
}

async fn delete_sync_point_children(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    sync_point_id: &str,
) -> Result<(), sqlx::Error> {
    for sql in [
        "DELETE FROM SYNC_POINT_READLIST_REMOVED_SYNCED WHERE SYNC_POINT_ID = ?",
        "DELETE FROM SYNC_POINT_READLIST_BOOK WHERE SYNC_POINT_ID = ?",
        "DELETE FROM SYNC_POINT_READLIST WHERE SYNC_POINT_ID = ?",
        "DELETE FROM SYNC_POINT_BOOK_REMOVED_SYNCED WHERE SYNC_POINT_ID = ?",
        "DELETE FROM SYNC_POINT_BOOK WHERE SYNC_POINT_ID = ?",
    ] {
        sqlx::query(sql)
            .bind(sync_point_id)
            .execute(&mut **tx)
            .await?;
    }

    Ok(())
}
