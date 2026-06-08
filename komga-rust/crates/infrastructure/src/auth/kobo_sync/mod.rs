use async_trait::async_trait;
use komga_application::identity_access::{
    AuthUser, KoboStoreSyncMergeResult, KoboStoreSyncPort, KoboSyncPage, KoboSyncPageRequest,
    KoboSyncStatePort, random_uuid_like, user_id,
};
use sqlx::{Row, SqlitePool};

use super::device_auth::{
    KoboMetadataRecord, PersistedReadProgressRecord, load_kobo_metadata_record, load_read_progress,
};

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

pub struct SqliteKoboSyncState<'a> {
    pool: &'a SqlitePool,
}

impl<'a> SqliteKoboSyncState<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl KoboSyncStatePort for SqliteKoboSyncState<'_> {
    async fn load_sync_page(&self, request: KoboSyncPageRequest) -> Result<KoboSyncPage, String> {
        let user_id_value = user_id(&request.user);
        load_kobo_sync_page(
            self.pool,
            &request.user,
            user_id_value,
            request.current_api_key_id.as_deref(),
            request.ongoing_sync_point_id.as_deref(),
            request.last_successful_sync_point_id.as_deref(),
            request.limit,
        )
        .await
        .map_err(|error| error.to_string())
    }

    async fn load_kobo_metadata_record(
        &self,
        book_id: &str,
    ) -> Result<Option<KoboMetadataRecord>, String> {
        load_kobo_metadata_record(self.pool, book_id)
            .await
            .map_err(|error| error.to_string())
    }

    async fn load_read_progress(
        &self,
        book_id: &str,
        user_id: &str,
    ) -> Result<Option<PersistedReadProgressRecord>, String> {
        load_read_progress(self.pool, book_id, user_id)
            .await
            .map_err(|error| error.to_string())
    }

    async fn remove_sync_point(&self, sync_point_id: &str) -> Result<(), String> {
        remove_sync_point(self.pool, sync_point_id)
            .await
            .map_err(|error| error.to_string())
    }
}

pub struct HttpKoboStoreSync;

#[async_trait]
impl KoboStoreSyncPort for HttpKoboStoreSync {
    async fn sync_store_library(
        &self,
        forwarded_headers: &[(String, String)],
        query: Option<&str>,
        raw_sync_token: &str,
    ) -> Result<KoboStoreSyncMergeResult, String> {
        proxy::proxy_kobo_store_library_sync(forwarded_headers, query, raw_sync_token)
            .await
            .map_err(|_| "kobo store sync proxy failed".to_string())
    }
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
        AuthUser, KOBO_SYNC_ITEM_LIMIT, KoboLibrarySyncRequest, KoboLibrarySyncService,
        parse_komga_sync_token_payload,
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
    async fn sqlite_kobo_sync_state_persists_empty_page_for_pipeline() {
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

        let state = SqliteKoboSyncState::new(&pool);
        let response = KoboLibrarySyncService::new(&state, &HttpKoboStoreSync)
            .sync_library(KoboLibrarySyncRequest {
                user: sync_user(),
                current_api_key_id: Some("api-key-1".to_string()),
                sync_token: None,
                store_sync_enabled: false,
                forwarded_headers: Vec::new(),
                query: None,
                base_url: "http://localhost:8080".to_string(),
                auth_token: "kobo-token".to_string(),
                limit: KOBO_SYNC_ITEM_LIMIT,
            })
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
