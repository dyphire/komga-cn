use komga_application::identity_access::{
    AuthUser, KoboLibrarySyncRequest, KoboLibrarySyncResponse, KoboStoreSyncMergeResult,
    KoboSyncBookSnapshot, KoboSyncPage, KoboSyncReadProgressSnapshot,
    build_kobo_changed_entitlement_removed, build_kobo_changed_product_metadata,
    build_kobo_changed_reading_state, build_kobo_changed_tag, build_kobo_deleted_tag,
    build_kobo_new_entitlement, build_kobo_new_tag, build_komga_sync_token_payload,
    is_kobo_store_sync_token_candidate, parse_komga_sync_token_payload, random_uuid_like, user_id,
};
use sqlx::{Row, SqlitePool};

use super::device_auth::{
    KoboMetadataRecord, PersistedReadProgressRecord, load_kobo_metadata_record, load_read_progress,
};

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

pub async fn load_kobo_library_sync(
    pool: &SqlitePool,
    request: KoboLibrarySyncRequest,
) -> Result<KoboLibrarySyncResponse, sqlx::Error> {
    let user_id_value = user_id(&request.user);
    let sync_token_payload = request
        .sync_token_raw
        .as_deref()
        .and_then(parse_komga_sync_token_payload);
    let sync_page = load_kobo_sync_page(
        pool,
        &request.user,
        user_id_value,
        request.current_api_key_id.as_deref(),
        sync_token_payload
            .as_ref()
            .and_then(|token| token.ongoing_sync_point_id.as_deref()),
        sync_token_payload
            .as_ref()
            .and_then(|token| token.last_successful_sync_point_id.as_deref()),
        request.limit,
    )
    .await?;
    let response_events = build_kobo_sync_events_page(
        pool,
        &sync_page,
        user_id_value,
        request.base_url.as_str(),
        request.auth_token.as_str(),
    )
    .await?;

    let from_sync_point_id = sync_page.from_sync_point_id.clone();
    let to_sync_point_id = sync_page.to_sync_point_id.clone();
    let mut merged_events = response_events;
    let mut merged_should_continue = sync_page.should_continue;
    let mut merged_raw_kobo_sync_token = sync_token_payload
        .as_ref()
        .map(|payload| payload.raw_kobo_sync_token.clone())
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            request
                .sync_token_raw
                .as_deref()
                .filter(|value| is_kobo_store_sync_token_candidate(value))
                .map(str::to_string)
        });

    if !sync_page.should_continue
        && request.store_sync_enabled
        && let Some(raw_store_sync_token) = merged_raw_kobo_sync_token
            .as_deref()
            .filter(|value| is_kobo_store_sync_token_candidate(value))
        && let Ok(store_response) = proxy_kobo_store_library_sync(
            &request.forwarded_headers,
            request.query.as_deref(),
            raw_store_sync_token,
        )
        .await
    {
        merged_events.extend(store_response.events);
        merged_should_continue = store_response.should_continue;
        if let Some(raw_store_sync_token) = store_response.raw_sync_token
            && !raw_store_sync_token.trim().is_empty()
        {
            merged_raw_kobo_sync_token = Some(raw_store_sync_token);
        }
    }

    if !merged_should_continue
        && let Some(from_sync_point_id) = from_sync_point_id.as_deref()
        && from_sync_point_id != to_sync_point_id
    {
        remove_sync_point(pool, from_sync_point_id).await?;
    }

    let sync_token_payload_sanitized = sync_token_payload.map(|mut payload| {
        payload.ongoing_sync_point_id = sync_page.should_continue.then(|| to_sync_point_id.clone());
        if let Some(raw) = merged_raw_kobo_sync_token.as_ref() {
            payload.raw_kobo_sync_token = raw.clone();
        }
        payload
    });
    let sync_token_payload = build_komga_sync_token_payload(
        sync_token_payload_sanitized,
        merged_raw_kobo_sync_token,
        to_sync_point_id.as_str(),
        merged_should_continue,
    );

    Ok(KoboLibrarySyncResponse {
        events: merged_events,
        sync_token_payload,
        should_continue: merged_should_continue,
    })
}

async fn load_kobo_sync_page(
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

async fn remove_sync_point(pool: &SqlitePool, sync_point_id: &str) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    delete_sync_point_children(&mut tx, sync_point_id).await?;
    sqlx::query("DELETE FROM SYNC_POINT WHERE ID = ?")
        .bind(sync_point_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

async fn proxy_kobo_store_library_sync(
    forwarded_headers: &[(String, String)],
    query: Option<&str>,
    raw_sync_token: &str,
) -> Result<KoboStoreSyncMergeResult, ()> {
    proxy::proxy_kobo_store_library_sync(forwarded_headers, query, raw_sync_token).await
}

fn sync_book_snapshot_from_metadata(
    book_id: &str,
    created: &str,
    file_last_modified: &str,
    metadata: &KoboMetadataRecord,
) -> KoboSyncBookSnapshot {
    KoboSyncBookSnapshot {
        id: book_id.to_string(),
        title: metadata.title.clone(),
        summary: metadata.summary.clone(),
        release_date: metadata.release_date.clone(),
        language: metadata.language.clone(),
        file_size: metadata.file_size,
        page_count: 1,
        created: metadata
            .created_date
            .clone()
            .unwrap_or_else(|| created.to_string()),
        last_modified: file_last_modified.to_string(),
        contributor_names: metadata.contributor_names.clone(),
        isbn: metadata.isbn.clone(),
        publisher_name: metadata.publisher_name.clone(),
        cover_image_id: metadata.cover_image_id.clone(),
        series_id: metadata.series_id.clone(),
        series_name: metadata.series_name.clone(),
        series_number: metadata.series_number.clone(),
        series_number_float: metadata.series_number_float,
        oneshot: metadata.oneshot,
    }
}

fn removed_book_snapshot(
    book_id: &str,
    created: &str,
    file_last_modified: &str,
) -> KoboSyncBookSnapshot {
    KoboSyncBookSnapshot {
        id: book_id.to_string(),
        title: book_id.to_string(),
        summary: String::new(),
        release_date: None,
        language: "en".to_string(),
        file_size: 0,
        page_count: 1,
        created: created.to_string(),
        last_modified: file_last_modified.to_string(),
        contributor_names: Vec::new(),
        isbn: None,
        publisher_name: None,
        cover_image_id: Some(book_id.to_string()),
        series_id: None,
        series_name: None,
        series_number: None,
        series_number_float: None,
        oneshot: true,
    }
}

fn progress_snapshot(record: &PersistedReadProgressRecord) -> KoboSyncReadProgressSnapshot {
    KoboSyncReadProgressSnapshot {
        page: record.page,
        completed: record.completed,
        created: record.created.clone(),
        last_modified: record.last_modified.clone(),
        locator: record.locator.clone(),
    }
}

async fn build_kobo_sync_events_page(
    pool: &SqlitePool,
    page: &KoboSyncPage,
    user_id: &str,
    base_url: &str,
    auth_token: &str,
) -> Result<Vec<serde_json::Value>, sqlx::Error> {
    let mut events = Vec::new();

    for book in &page.books_added {
        let metadata = load_kobo_metadata_record(pool, &book.book_id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;
        let progress = load_read_progress(pool, &book.book_id, user_id)
            .await?
            .as_ref()
            .map(progress_snapshot);
        let snapshot = sync_book_snapshot_from_metadata(
            &book.book_id,
            &book.created,
            &book.file_last_modified,
            &metadata,
        );
        events.push(build_kobo_new_entitlement(
            &snapshot,
            progress.as_ref(),
            base_url,
            auth_token,
        ));
    }

    for book in &page.books_changed {
        let metadata = load_kobo_metadata_record(pool, &book.book_id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;
        let progress = load_read_progress(pool, &book.book_id, user_id)
            .await?
            .as_ref()
            .map(progress_snapshot);
        let snapshot = sync_book_snapshot_from_metadata(
            &book.book_id,
            &book.created,
            &book.file_last_modified,
            &metadata,
        );
        events.push(build_kobo_new_entitlement(
            &snapshot,
            progress.as_ref(),
            base_url,
            auth_token,
        ));
        events.push(build_kobo_changed_product_metadata(
            &snapshot, base_url, auth_token,
        ));
        if let Some(progress) = progress.as_ref() {
            events.push(build_kobo_changed_reading_state(&snapshot, progress));
        }
    }

    for book in &page.books_removed {
        let snapshot =
            removed_book_snapshot(&book.book_id, &book.created, &book.file_last_modified);
        events.push(build_kobo_changed_entitlement_removed(
            &snapshot, base_url, auth_token,
        ));
    }

    for book in &page.books_read_progress_changed {
        if let Some(progress) = load_read_progress(pool, &book.book_id, user_id).await? {
            let metadata = load_kobo_metadata_record(pool, &book.book_id)
                .await?
                .ok_or(sqlx::Error::RowNotFound)?;
            let snapshot = sync_book_snapshot_from_metadata(
                &book.book_id,
                &book.created,
                &book.file_last_modified,
                &metadata,
            );
            let progress = progress_snapshot(&progress);
            events.push(build_kobo_changed_reading_state(&snapshot, &progress));
        }
    }

    for readlist in &page.readlists_added {
        events.push(build_kobo_new_tag(readlist));
    }
    for readlist in &page.readlists_changed {
        events.push(build_kobo_changed_tag(readlist));
    }
    for readlist in &page.readlists_removed {
        events.push(build_kobo_deleted_tag(readlist));
    }

    Ok(events)
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
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(sync_point_id)
            .execute(&mut **tx)
            .await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use komga_application::identity_access::{
        AuthUser, KOBO_SYNC_ITEM_LIMIT, KoboLibrarySyncRequest, parse_komga_sync_token_payload,
    };

    use super::*;
    use crate::sqlite::{connect_test_pool, setup};

    fn temp_db_path(case_id: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("komga-rust-kobo-sync-{case_id}-{nanos}.sqlite"))
    }

    fn sync_user() -> AuthUser {
        AuthUser {
            id: "kobo-user".to_string(),
            email: "kobo-user@example.org".to_string(),
            password: "secret".to_string(),
            roles: vec!["USER".to_string(), "KOBO_SYNC".to_string()],
            shared_all_libraries: true,
            shared_library_ids: Vec::new(),
            labels_allow: Vec::new(),
            labels_exclude: Vec::new(),
            age_restriction: None,
        }
    }

    #[tokio::test]
    async fn load_kobo_library_sync_finalizes_empty_page_behind_pipeline_boundary() {
        let db_path = temp_db_path("empty-page");
        let pool = connect_test_pool(db_path.as_path(), 1)
            .await
            .expect("temporary sqlite db should open");
        setup::bootstrap_pool(&pool)
            .await
            .expect("temporary sqlite db should bootstrap main schema");
        sqlx::query("INSERT INTO USER (ID, EMAIL, PASSWORD) VALUES (?, ?, ?)")
            .bind("kobo-user")
            .bind("kobo-user@example.org")
            .bind("secret")
            .execute(&pool)
            .await
            .expect("sync user should be inserted");

        let response = load_kobo_library_sync(
            &pool,
            KoboLibrarySyncRequest {
                user: sync_user(),
                current_api_key_id: Some("api-key-1".to_string()),
                sync_token_raw: None,
                store_sync_enabled: false,
                forwarded_headers: Vec::new(),
                query: None,
                base_url: "http://localhost:8080".to_string(),
                auth_token: "kobo-token".to_string(),
                limit: KOBO_SYNC_ITEM_LIMIT,
            },
        )
        .await
        .expect("empty sync page should complete");

        assert!(response.events.is_empty());
        assert!(!response.should_continue);
        let token = parse_komga_sync_token_payload(response.sync_token_payload.as_str())
            .expect("pipeline response should include a valid Komga sync token payload");
        assert!(token.ongoing_sync_point_id.is_none());
        assert!(token.last_successful_sync_point_id.is_some());

        let sync_point = sqlx::query("SELECT USER_ID, API_KEY_ID FROM SYNC_POINT LIMIT 1")
            .fetch_one(&pool)
            .await
            .expect("sync point should be persisted");
        assert_eq!(sync_point.get::<String, _>("USER_ID"), "kobo-user");
        assert_eq!(
            sync_point.get::<Option<String>, _>("API_KEY_ID").as_deref(),
            Some("api-key-1"),
        );

        pool.close().await;
        let _ = std::fs::remove_file(db_path);
    }
}
