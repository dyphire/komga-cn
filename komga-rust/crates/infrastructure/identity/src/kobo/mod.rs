use komga_application::identity_access::{
    AuthUser, KoboSyncPage, KoboSyncPageRequest, random_uuid_like, user_id,
};
use sqlx::{Row, SqlitePool};

mod book_state;
mod proxy;
mod seeding;
mod sync_diff;

pub(super) use book_state::load_sync_book_states;
pub(super) use proxy::execute_kobo_proxy_request;
use seeding::{seed_sync_point_books, seed_sync_point_ondeck};
use sync_diff::{load_incremental_sync_page, load_initial_sync_page};

pub(crate) const DEFAULT_KOBO_PROXY_BASE_URL: &str = "https://storeapi.kobo.com";

#[derive(Clone, Debug)]
struct PersistedSyncPoint {
    id: String,
}

pub(super) async fn load_kobo_sync_page(
    pool: &SqlitePool,
    request: KoboSyncPageRequest,
) -> Result<KoboSyncPage, sqlx::Error> {
    let user_id = user_id(&request.user);
    let mut tx = pool.begin().await?;

    let to_sync_point = if let Some(sync_point_id) = request.ongoing_sync_point_id.as_deref() {
        if let Some(sync_point) = load_sync_point_for_user(&mut tx, sync_point_id, user_id).await? {
            sync_point
        } else {
            let new_sync_point_id = random_uuid_like();
            create_sync_point(
                &mut tx,
                &new_sync_point_id,
                &request.user,
                request.current_api_key_id.as_deref(),
            )
            .await?
        }
    } else {
        let new_sync_point_id = random_uuid_like();
        create_sync_point(
            &mut tx,
            &new_sync_point_id,
            &request.user,
            request.current_api_key_id.as_deref(),
        )
        .await?
    };

    let from_sync_point =
        if let Some(sync_point_id) = request.last_successful_sync_point_id.as_deref() {
            load_sync_point_for_user(&mut tx, sync_point_id, user_id).await?
        } else {
            None
        };

    let page = if let Some(from_sync_point) = from_sync_point.as_ref() {
        load_incremental_sync_page(
            &mut tx,
            &from_sync_point.id,
            &to_sync_point.id,
            request.limit,
        )
        .await?
    } else {
        load_initial_sync_page(&mut tx, &to_sync_point.id, request.limit).await?
    };

    tx.commit().await?;
    Ok(KoboSyncPage {
        to_sync_point_id: to_sync_point.id,
        from_sync_point_id: from_sync_point.map(|value| value.id),
        ..page
    })
}

pub(super) async fn remove_sync_point(
    pool: &SqlitePool,
    sync_point_id: &str,
) -> Result<(), sqlx::Error> {
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

    use komga_application::identity_access::{AuthUser, AuthUserRole, KoboSyncPageRequest};

    use super::*;
    use komga_infrastructure_base::sqlite::{connect_test_pool, schema};

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
            roles: vec![AuthUserRole::KoboSync],
            shared_all_libraries: true,
            shared_library_ids: Vec::new(),
            labels_allow: Vec::new(),
            labels_exclude: Vec::new(),
            age_restriction: None,
        }
    }

    #[tokio::test]
    async fn sqlite_kobo_sync_state_persists_empty_sync_page() {
        let db_path = temp_db_path("empty-page");
        let pool = connect_test_pool(db_path.as_path(), 1)
            .await
            .expect("temporary sqlite db should open");
        schema::bootstrap_pool(&pool)
            .await
            .expect("temporary sqlite db should bootstrap main schema");
        sqlx::query("INSERT INTO USER (ID, EMAIL, PASSWORD) VALUES (?, ?, ?)")
            .bind("kobo-user")
            .bind("kobo-user@example.org")
            .bind("secret")
            .execute(&pool)
            .await
            .expect("sync user should be inserted");

        let page = load_kobo_sync_page(
            &pool,
            KoboSyncPageRequest {
                user: sync_user(),
                current_api_key_id: Some("api-key-1".to_string()),
                ongoing_sync_point_id: None,
                last_successful_sync_point_id: None,
                limit: 200,
            },
        )
        .await
        .expect("empty sync page should complete");

        assert!(page.books_added.is_empty());
        assert!(page.readlists_added.is_empty());
        assert!(page.from_sync_point_id.is_none());
        assert!(!page.should_continue);

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

    #[tokio::test]
    async fn sqlite_kobo_sync_diff_paginates_readlist_item_changes() {
        let db_path = temp_db_path("readlist-item-change");
        let pool = connect_test_pool(db_path.as_path(), 1)
            .await
            .expect("temporary sqlite db should open");
        schema::bootstrap_pool(&pool)
            .await
            .expect("temporary sqlite db should bootstrap main schema");
        sqlx::query("INSERT INTO USER (ID, EMAIL, PASSWORD) VALUES (?, ?, ?)")
            .bind("kobo-user")
            .bind("kobo-user@example.org")
            .bind("secret")
            .execute(&pool)
            .await
            .expect("sync user should be inserted");
        for sync_point_id in ["from-sync-point", "to-sync-point"] {
            sqlx::query("INSERT INTO SYNC_POINT (ID, USER_ID, API_KEY_ID) VALUES (?, ?, ?)")
                .bind(sync_point_id)
                .bind("kobo-user")
                .bind("api-key-1")
                .execute(&pool)
                .await
                .expect("sync point should be inserted");
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
            .bind("2026-01-01 00:00:00")
            .bind("2026-01-02 00:00:00")
            .execute(&pool)
            .await
            .expect("readlist snapshot should be inserted");
        }
        sqlx::query(
            "INSERT INTO SYNC_POINT_READLIST_BOOK (SYNC_POINT_ID, READLIST_ID, BOOK_ID) VALUES (?, ?, ?)",
        )
        .bind("from-sync-point")
        .bind("KOMGA-ONDECK")
        .bind("book-a")
        .execute(&pool)
        .await
        .expect("from readlist item should be inserted");
        sqlx::query(
            "INSERT INTO SYNC_POINT_READLIST_BOOK (SYNC_POINT_ID, READLIST_ID, BOOK_ID) VALUES (?, ?, ?)",
        )
        .bind("to-sync-point")
        .bind("KOMGA-ONDECK")
        .bind("book-b")
        .execute(&pool)
        .await
        .expect("to readlist item should be inserted");
        sqlx::query(
            r#"
            INSERT INTO SYNC_POINT_BOOK (
                SYNC_POINT_ID,
                BOOK_ID,
                BOOK_CREATED_DATE,
                BOOK_LAST_MODIFIED_DATE,
                BOOK_FILE_LAST_MODIFIED,
                BOOK_FILE_SIZE,
                BOOK_FILE_HASH,
                BOOK_METADATA_LAST_MODIFIED_DATE
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind("to-sync-point")
        .bind("book-added")
        .bind("2026-01-01 00:00:00")
        .bind("2026-01-01 00:00:00")
        .bind("2026-01-01 00:00:00")
        .bind(1_i64)
        .bind("hash-added")
        .bind("2026-01-01 00:00:00")
        .execute(&pool)
        .await
        .expect("added book should be inserted");

        let page = load_kobo_sync_page(
            &pool,
            KoboSyncPageRequest {
                user: sync_user(),
                current_api_key_id: Some("api-key-1".to_string()),
                ongoing_sync_point_id: Some("to-sync-point".to_string()),
                last_successful_sync_point_id: Some("from-sync-point".to_string()),
                limit: 1,
            },
        )
        .await
        .expect("sync page should load");

        assert_eq!(page.books_added.len(), 1);
        assert_eq!(page.books_added[0].book_id, "book-added");
        assert!(page.should_continue);

        let page = load_kobo_sync_page(
            &pool,
            KoboSyncPageRequest {
                user: sync_user(),
                current_api_key_id: Some("api-key-1".to_string()),
                ongoing_sync_point_id: Some("to-sync-point".to_string()),
                last_successful_sync_point_id: Some("from-sync-point".to_string()),
                limit: 1,
            },
        )
        .await
        .expect("next sync page should load");

        assert_eq!(page.readlists_changed.len(), 1);
        assert_eq!(page.readlists_changed[0].id, "KOMGA-ONDECK");
        assert_eq!(page.readlists_changed[0].items, vec!["book-b"]);
        assert!(!page.should_continue);

        pool.close().await;
        let _ = std::fs::remove_file(db_path);
    }
}
