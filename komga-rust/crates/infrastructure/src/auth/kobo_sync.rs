use std::collections::HashMap;
use std::path::Path;

use komga_application::identity_access::{
    AuthUser, KoboStoreSyncMergeResult, KoboSyncPage, KoboSyncPointBook, KoboSyncReadListSnapshot,
    decode_or_passthrough_sync_token, now_sync_marker, random_uuid_like,
};
use reqwest::header::{HeaderName, HeaderValue};
use serde_json::Value;
use sqlx::Row;

use crate::sqlite::connect_private_write_pool;

#[derive(Clone, Debug)]
struct PersistedSyncPoint {
    id: String,
}

#[derive(Clone)]
struct SyncPointBookSeedRow {
    book_id: String,
    created_date: String,
    last_modified_date: String,
    file_last_modified: String,
    file_size: i64,
    file_hash: Option<String>,
    metadata_last_modified_date: String,
    read_progress_last_modified_date: Option<String>,
    thumbnail_id: Option<String>,
    library_id: String,
    age_rating: Option<u16>,
    sharing_labels: Vec<String>,
}

#[derive(Clone)]
struct OnDeckSeedRow {
    book_id: String,
    library_id: String,
    age_rating: Option<u16>,
    sharing_labels: Vec<String>,
    most_recent_read_date: Option<String>,
}

pub async fn load_kobo_sync_page(
    database_file: &Path,
    user: &AuthUser,
    user_id: &str,
    current_api_key_id: Option<&str>,
    ongoing_sync_point_id: Option<&str>,
    last_successful_sync_point_id: Option<&str>,
    limit: usize,
) -> Result<KoboSyncPage, sqlx::Error> {
    let pool = connect_private_write_pool(database_file).await?;
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

pub async fn remove_sync_point(
    database_file: &Path,
    sync_point_id: &str,
) -> Result<(), sqlx::Error> {
    let pool = connect_private_write_pool(database_file).await?;
    let mut tx = pool.begin().await?;
    delete_sync_point_children(&mut tx, sync_point_id).await?;
    sqlx::query("DELETE FROM SYNC_POINT WHERE ID = ?")
        .bind(sync_point_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
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

async fn seed_sync_point_books(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    sync_point_id: &str,
    user: &AuthUser,
) -> Result<(), sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT
            b.ID AS BOOK_ID,
            COALESCE(b.CREATED_DATE, CURRENT_TIMESTAMP) AS BOOK_CREATED_DATE,
            COALESCE(b.LAST_MODIFIED_DATE, b.CREATED_DATE, CURRENT_TIMESTAMP) AS BOOK_LAST_MODIFIED_DATE,
            CASE
                WHEN typeof(b.FILE_LAST_MODIFIED) = 'integer'
                    THEN datetime(b.FILE_LAST_MODIFIED, 'unixepoch')
                ELSE b.FILE_LAST_MODIFIED
            END AS BOOK_FILE_LAST_MODIFIED,
            COALESCE(b.FILE_SIZE, 0) AS BOOK_FILE_SIZE,
            b.FILE_HASH AS BOOK_FILE_HASH,
            COALESCE(
                bm.LAST_MODIFIED_DATE,
                bm.CREATED_DATE,
                b.LAST_MODIFIED_DATE,
                b.CREATED_DATE,
                CURRENT_TIMESTAMP
            ) AS BOOK_METADATA_LAST_MODIFIED_DATE,
            rp.LAST_MODIFIED_DATE AS BOOK_READ_PROGRESS_LAST_MODIFIED_DATE,
            tb.ID AS BOOK_THUMBNAIL_ID,
            b.LIBRARY_ID AS LIBRARY_ID,
            sm.AGE_RATING AS AGE_RATING,
            COALESCE(
                (
                    SELECT GROUP_CONCAT(DISTINCT sms.LABEL)
                    FROM SERIES_METADATA_SHARING sms
                    WHERE sms.SERIES_ID = b.SERIES_ID
                ),
                ''
            ) AS SHARING_LABELS
        FROM BOOK b
        JOIN MEDIA m ON m.BOOK_ID = b.ID
        JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
        JOIN SERIES_METADATA sm ON sm.SERIES_ID = b.SERIES_ID
        LEFT JOIN READ_PROGRESS rp ON rp.BOOK_ID = b.ID AND rp.USER_ID = ?
        LEFT JOIN THUMBNAIL_BOOK tb ON tb.BOOK_ID = b.ID AND tb.SELECTED = TRUE
        WHERE b.DELETED_DATE IS NULL
          AND m.STATUS = 'READY'
          AND m.MEDIA_TYPE = 'application/epub+zip'
        "#,
    )
    .bind(user.id.as_str())
    .fetch_all(&mut **tx)
    .await?;

    let books = rows
        .into_iter()
        .map(|row| SyncPointBookSeedRow {
            book_id: row.get::<String, _>("BOOK_ID"),
            created_date: row.get::<String, _>("BOOK_CREATED_DATE"),
            last_modified_date: row.get::<String, _>("BOOK_LAST_MODIFIED_DATE"),
            file_last_modified: row.get::<String, _>("BOOK_FILE_LAST_MODIFIED"),
            file_size: row.get::<i64, _>("BOOK_FILE_SIZE"),
            file_hash: row.get::<Option<String>, _>("BOOK_FILE_HASH"),
            metadata_last_modified_date: row.get::<String, _>("BOOK_METADATA_LAST_MODIFIED_DATE"),
            read_progress_last_modified_date: row
                .get::<Option<String>, _>("BOOK_READ_PROGRESS_LAST_MODIFIED_DATE"),
            thumbnail_id: row.get::<Option<String>, _>("BOOK_THUMBNAIL_ID"),
            library_id: row.get::<String, _>("LIBRARY_ID"),
            age_rating: row
                .get::<Option<i64>, _>("AGE_RATING")
                .and_then(|value| u16::try_from(value).ok()),
            sharing_labels: normalized_sharing_labels(
                row.get::<String, _>("SHARING_LABELS").as_str(),
            ),
        })
        .filter(|row| {
            user_can_access_sync_book(user, &row.library_id, row.age_rating, &row.sharing_labels)
        })
        .collect::<Vec<_>>();

    if books.is_empty() {
        return Ok(());
    }

    let mut query = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
        r#"
        INSERT INTO SYNC_POINT_BOOK (
            SYNC_POINT_ID,
            BOOK_ID,
            BOOK_CREATED_DATE,
            BOOK_LAST_MODIFIED_DATE,
            BOOK_FILE_LAST_MODIFIED,
            BOOK_FILE_SIZE,
            BOOK_FILE_HASH,
            BOOK_METADATA_LAST_MODIFIED_DATE,
            BOOK_READ_PROGRESS_LAST_MODIFIED_DATE,
            BOOK_THUMBNAIL_ID
        )
        "#,
    );
    query.push_values(books.iter(), |mut builder, book| {
        builder
            .push_bind(sync_point_id)
            .push_bind(book.book_id.as_str())
            .push_bind(book.created_date.as_str())
            .push_bind(book.last_modified_date.as_str())
            .push_bind(book.file_last_modified.as_str())
            .push_bind(book.file_size)
            .push_bind(book.file_hash.as_deref())
            .push_bind(book.metadata_last_modified_date.as_str())
            .push_bind(book.read_progress_last_modified_date.as_deref())
            .push_bind(book.thumbnail_id.as_deref());
    });
    query.build().execute(&mut **tx).await?;

    Ok(())
}

async fn seed_sync_point_ondeck(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    sync_point_id: &str,
    user: &AuthUser,
) -> Result<(), sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT
            b.ID AS BOOK_ID,
            b.LIBRARY_ID AS LIBRARY_ID,
            sm.AGE_RATING AS AGE_RATING,
            COALESCE(
                (
                    SELECT GROUP_CONCAT(DISTINCT sms.LABEL)
                    FROM SERIES_METADATA_SHARING sms
                    WHERE sms.SERIES_ID = b.SERIES_ID
                ),
                ''
            ) AS SHARING_LABELS,
            rps.MOST_RECENT_READ_DATE AS MOST_RECENT_READ_DATE
        FROM READ_PROGRESS_SERIES rps
        JOIN SERIES s ON s.ID = rps.SERIES_ID
        JOIN BOOK b ON b.SERIES_ID = s.ID
        JOIN MEDIA m ON m.BOOK_ID = b.ID
        JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
        JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
        LEFT JOIN READ_PROGRESS rp ON rp.BOOK_ID = b.ID AND rp.USER_ID = rps.USER_ID
        WHERE rps.USER_ID = ?
          AND b.DELETED_DATE IS NULL
          AND m.STATUS = 'READY'
          AND m.MEDIA_TYPE = 'application/epub+zip'
          AND rps.IN_PROGRESS_COUNT = 0
          AND rps.READ_COUNT != s.BOOK_COUNT
          AND rp.COMPLETED IS NULL
          AND NOT EXISTS (
              SELECT 1
              FROM BOOK b_prev
              JOIN BOOK_METADATA bm_prev ON bm_prev.BOOK_ID = b_prev.ID
              LEFT JOIN READ_PROGRESS rp_prev ON rp_prev.BOOK_ID = b_prev.ID
                                           AND rp_prev.USER_ID = rps.USER_ID
              WHERE b_prev.SERIES_ID = b.SERIES_ID
                AND rp_prev.COMPLETED IS NULL
                AND (
                    COALESCE(bm_prev.NUMBER_SORT, 0) < COALESCE(bm.NUMBER_SORT, 0)
                    OR (
                        COALESCE(bm_prev.NUMBER_SORT, 0) = COALESCE(bm.NUMBER_SORT, 0)
                        AND b_prev.ID < b.ID
                    )
                )
          )
        "#,
    )
    .bind(user.id.as_str())
    .fetch_all(&mut **tx)
    .await?;

    let items = rows
        .into_iter()
        .map(|row| OnDeckSeedRow {
            book_id: row.get::<String, _>("BOOK_ID"),
            library_id: row.get::<String, _>("LIBRARY_ID"),
            age_rating: row
                .get::<Option<i64>, _>("AGE_RATING")
                .and_then(|value| u16::try_from(value).ok()),
            sharing_labels: normalized_sharing_labels(
                row.get::<String, _>("SHARING_LABELS").as_str(),
            ),
            most_recent_read_date: row.get::<Option<String>, _>("MOST_RECENT_READ_DATE"),
        })
        .filter(|row| {
            user_can_access_sync_book(user, &row.library_id, row.age_rating, &row.sharing_labels)
        })
        .collect::<Vec<_>>();

    if items.is_empty() {
        return Ok(());
    }

    let created_date = now_sync_marker();
    let last_modified = items
        .iter()
        .filter_map(|item| item.most_recent_read_date.as_deref())
        .max()
        .unwrap_or(created_date.as_str())
        .to_string();

    sqlx::query(
        r#"
        INSERT INTO SYNC_POINT_READLIST (
            SYNC_POINT_ID,
            READLIST_ID,
            READLIST_NAME,
            READLIST_CREATED_DATE,
            READLIST_LAST_MODIFIED_DATE
        ) VALUES (?, ?, ?, ?, ?)
        "#,
    )
    .bind(sync_point_id)
    .bind("KOMGA-ONDECK")
    .bind("On Deck")
    .bind(created_date)
    .bind(last_modified)
    .execute(&mut **tx)
    .await?;

    let mut query = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
        "INSERT INTO SYNC_POINT_READLIST_BOOK (SYNC_POINT_ID, READLIST_ID, BOOK_ID) ",
    );
    query.push_values(items.iter(), |mut builder, item| {
        builder
            .push_bind(sync_point_id)
            .push_bind("KOMGA-ONDECK")
            .push_bind(item.book_id.as_str());
    });
    query.build().execute(&mut **tx).await?;

    Ok(())
}

fn user_can_access_sync_book(
    user: &AuthUser,
    library_id: &str,
    age_rating: Option<u16>,
    sharing_labels: &[String],
) -> bool {
    user_can_access_library(user, library_id)
        && user_allows_content(user, age_rating, sharing_labels)
}

fn user_can_access_library(user: &AuthUser, library_id: &str) -> bool {
    user.shared_all_libraries
        || user.roles.iter().any(|role| role == "ADMIN")
        || user
            .shared_library_ids
            .iter()
            .any(|shared_library_id| shared_library_id == library_id)
}

fn user_allows_content(
    user: &AuthUser,
    age_rating: Option<u16>,
    sharing_labels: &[String],
) -> bool {
    if user.age_restriction.is_none()
        && user.labels_allow.is_empty()
        && user.labels_exclude.is_empty()
    {
        return true;
    }

    let labels = sharing_labels
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();

    let age_allowed = user.age_restriction.as_ref().and_then(|restriction| {
        restriction
            .restriction
            .eq_ignore_ascii_case("ALLOW_ONLY")
            .then(|| age_rating.is_some_and(|age| age <= restriction.age as u16))
    });
    let label_allowed = if user.labels_allow.is_empty() {
        None
    } else {
        Some(
            user.labels_allow
                .iter()
                .map(|value| value.trim().to_ascii_lowercase())
                .filter(|value| !value.is_empty())
                .any(|candidate| labels.contains(&candidate)),
        )
    };

    let allowed = match (age_allowed, label_allowed) {
        (None, label_allowed) => label_allowed != Some(false),
        (age_allowed, None) => age_allowed != Some(false),
        (age_allowed, label_allowed) => age_allowed != Some(false) || label_allowed != Some(false),
    };
    if !allowed {
        return false;
    }

    let age_denied = user.age_restriction.as_ref().is_some_and(|restriction| {
        restriction.restriction.eq_ignore_ascii_case("EXCLUDE")
            && age_rating.is_some_and(|age| age >= restriction.age as u16)
    });
    let label_denied = if user.labels_exclude.is_empty() {
        false
    } else {
        user.labels_exclude
            .iter()
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty())
            .any(|candidate| labels.contains(&candidate))
    };

    !age_denied && !label_denied
}

fn normalized_sharing_labels(labels: &str) -> Vec<String> {
    labels
        .split(',')
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .map(|label| label.to_ascii_lowercase())
        .collect()
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

async fn load_initial_sync_page(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
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

async fn load_incremental_sync_page(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
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
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    sync_point_id: &str,
    mut readlists: Vec<KoboSyncReadListSnapshot>,
    include_items: bool,
) -> Result<Vec<KoboSyncReadListSnapshot>, sqlx::Error> {
    if !include_items || readlists.is_empty() {
        return Ok(readlists);
    }

    let mut query = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
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

async fn take_books_by_sync_point(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
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

async fn take_books_added(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
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

async fn take_books_changed(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
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

async fn take_books_removed(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
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

async fn take_books_read_progress_changed(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
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

async fn take_readlists_by_sync_point(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
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

async fn take_readlists_added(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
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

async fn take_readlists_changed(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
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

async fn take_readlists_removed(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
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

async fn mark_books_synced(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    sync_point_id: &str,
    books: &[KoboSyncPointBook],
) -> Result<(), sqlx::Error> {
    if books.is_empty() {
        return Ok(());
    }
    let mut query = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
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
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
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
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    sync_point_id: &str,
    readlists: &[KoboSyncReadListSnapshot],
) -> Result<(), sqlx::Error> {
    if readlists.is_empty() {
        return Ok(());
    }
    let mut query = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
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
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
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

async fn has_initial_remaining(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    to_sync_point_id: &str,
) -> Result<bool, sqlx::Error> {
    Ok(exists_unsynced_books(tx, to_sync_point_id).await?
        || exists_unsynced_readlists(tx, to_sync_point_id).await?)
}

async fn has_incremental_remaining(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
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
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
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
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
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
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
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
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
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
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
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
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
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
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
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
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
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
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
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

pub async fn proxy_kobo_store_library_sync(
    forwarded_headers: &[(String, String)],
    query: Option<&str>,
    raw_sync_token: &str,
) -> Result<KoboStoreSyncMergeResult, ()> {
    let mut target = String::from("https://storeapi.kobo.com/v1/library/sync");
    if let Some(query) = query.map(str::trim).filter(|query| !query.is_empty()) {
        target.push('?');
        target.push_str(query);
    }

    let client = reqwest::Client::builder().build().map_err(|_| ())?;
    let mut request = client.get(target);
    for (name, value) in forwarded_headers {
        let lower = name.to_ascii_lowercase();
        if lower == "host" || lower == "content-length" || lower == "x-kobo-synctoken" {
            continue;
        }
        let Ok(header_name) = HeaderName::from_bytes(name.as_bytes()) else {
            continue;
        };
        let Ok(header_value) = HeaderValue::from_str(value) else {
            continue;
        };
        request = request.header(header_name, header_value);
    }
    request = request.header("x-kobo-synctoken", raw_sync_token);

    let response = request.send().await.map_err(|_| ())?;
    if !response.status().is_success() {
        return Err(());
    }

    let headers = response.headers().clone();
    let body = response.json::<Value>().await.map_err(|_| ())?;
    let events = body.as_array().cloned().unwrap_or_default();
    let should_continue = headers
        .get("x-kobo-sync")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .is_some_and(|value| value.eq_ignore_ascii_case("continue"));
    let raw_sync_token = headers
        .get("x-kobo-synctoken")
        .and_then(|value| value.to_str().ok())
        .and_then(decode_or_passthrough_sync_token);

    Ok(KoboStoreSyncMergeResult {
        events,
        raw_sync_token,
        should_continue,
    })
}
