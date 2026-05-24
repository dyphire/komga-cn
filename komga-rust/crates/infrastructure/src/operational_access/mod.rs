mod announcements;
mod claims;
mod client_settings;
mod filesystem;
mod history;
mod page_hashes;
mod remote_feeds;
mod server_settings;
mod syncpoints;
mod transient_books;

pub use announcements::AnnouncementAccess;
pub use claims::ClaimAccess;
pub use client_settings::ClientSettingsAccess;
pub use filesystem::{FilesystemBrowseAccess, FontAccess};
pub use history::HistoryAccess;
pub use page_hashes::PageHashAccess;
pub use remote_feeds::RemoteFeedAccess;
pub use server_settings::{load_remember_me_runtime_settings, load_server_settings};
pub use syncpoints::SyncpointAccess;
pub use transient_books::TransientBookAccess;

#[cfg(test)]
use client_settings::{
    delete_client_settings_global, delete_client_settings_user, load_client_settings_global,
    load_client_settings_user, upsert_client_settings_global, upsert_client_settings_user,
};
#[cfg(test)]
use history::load_history_page;
#[cfg(test)]
use syncpoints::{delete_syncpoints_by_user, delete_syncpoints_by_user_and_key_ids};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::connect_test_pool;
    use sqlx::Row;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    async fn create_test_db(case: &str) -> sqlx::Pool<sqlx::Sqlite> {
        let root = unique_temp_dir(case);
        fs::create_dir_all(&root).expect("temp root should be created");
        let db_path = root.join("operational-settings.sqlite");
        let pool = connect_test_pool(&db_path, 1)
            .await
            .expect("test db should open");
        crate::sqlite::setup::bootstrap_pool(&pool)
            .await
            .expect("test db should bootstrap main schema");

        sqlx::query("INSERT INTO USER (ID, EMAIL, PASSWORD) VALUES (?, ?, ?)")
            .bind("user-1")
            .bind("user-1@example.org")
            .bind("test-password")
            .execute(&pool)
            .await
            .expect("user row should be inserted");

        pool
    }

    async fn create_history_test_db(case: &str) -> sqlx::Pool<sqlx::Sqlite> {
        let root = unique_temp_dir(case);
        fs::create_dir_all(&root).expect("temp root should be created");
        let db_path = root.join("operational-settings-history.sqlite");
        let pool = connect_test_pool(&db_path, 1)
            .await
            .expect("test db should open");
        crate::sqlite::setup::bootstrap_pool(&pool)
            .await
            .expect("history test db should bootstrap main schema");

        pool
    }

    async fn create_syncpoint_test_db(case: &str) -> sqlx::Pool<sqlx::Sqlite> {
        let root = unique_temp_dir(case);
        fs::create_dir_all(&root).expect("temp root should be created");
        let db_path = root.join("operational-settings-syncpoints.sqlite");
        let pool = connect_test_pool(&db_path, 1)
            .await
            .expect("test db should open");
        crate::sqlite::setup::bootstrap_pool(&pool)
            .await
            .expect("sync point test db should bootstrap main schema");
        for user_id in ["user-1", "user-2"] {
            sqlx::query("INSERT INTO USER (ID, EMAIL, PASSWORD) VALUES (?, ?, ?)")
                .bind(user_id)
                .bind(format!("{user_id}@example.org"))
                .bind("test-password")
                .execute(&pool)
                .await
                .expect("sync point fixture user should be inserted");
        }

        pool
    }

    fn unique_temp_dir(case: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "komga-operational-settings-{case}-{nanos}-{}",
            std::process::id()
        ))
    }

    #[tokio::test]
    async fn load_client_settings_global_filters_unauthorized_only_without_injecting_defaults() {
        let pool = create_test_db("load-global").await;

        sqlx::query(
            "INSERT INTO CLIENT_SETTINGS_GLOBAL (KEY, VALUE, ALLOW_UNAUTHORIZED) VALUES (?, ?, ?)",
        )
        .bind("public.setting")
        .bind("public-value")
        .bind(true)
        .execute(&pool)
        .await
        .expect("public setting should be inserted");
        sqlx::query(
            "INSERT INTO CLIENT_SETTINGS_GLOBAL (KEY, VALUE, ALLOW_UNAUTHORIZED) VALUES (?, ?, ?)",
        )
        .bind("private.setting")
        .bind("private-value")
        .bind(false)
        .execute(&pool)
        .await
        .expect("private setting should be inserted");

        let all = load_client_settings_global(&pool, false)
            .await
            .expect("global settings should load");
        let all = all
            .as_object()
            .expect("global settings should be an object");
        assert_eq!(all["public.setting"]["value"], "public-value");
        assert_eq!(all["private.setting"]["value"], "private-value");
        assert!(all.get("webui.oauth2.hide_login").is_none());

        let unauthorized_only = load_client_settings_global(&pool, true)
            .await
            .expect("filtered global settings should load");
        let unauthorized_only = unauthorized_only
            .as_object()
            .expect("filtered global settings should be an object");
        assert_eq!(unauthorized_only["public.setting"]["value"], "public-value");
        assert!(unauthorized_only.get("private.setting").is_none());
        assert!(unauthorized_only.get("webui.oauth2.hide_login").is_none());
    }

    #[tokio::test]
    async fn client_settings_access_round_trips_global_and_user_changes() {
        let pool = create_test_db("round-trip").await;

        upsert_client_settings_global(
            &pool,
            &[
                (
                    "public.setting".to_string(),
                    "public-value".to_string(),
                    true,
                ),
                (
                    "private.setting".to_string(),
                    "private-value".to_string(),
                    false,
                ),
            ],
        )
        .await
        .expect("global settings should persist");
        upsert_client_settings_user(
            &pool,
            "user-1",
            &[("reader.page_size".to_string(), "42".to_string())],
        )
        .await
        .expect("user settings should persist");

        let global = load_client_settings_global(&pool, false)
            .await
            .expect("global settings should reload");
        let global = global
            .as_object()
            .expect("global settings should be an object");
        assert_eq!(global["public.setting"]["value"], "public-value");
        assert_eq!(global["private.setting"]["value"], "private-value");

        let user = load_client_settings_user(&pool, "user-1")
            .await
            .expect("user settings should reload");
        let user = user.as_object().expect("user settings should be an object");
        assert_eq!(user["reader.page_size"]["value"], "42");

        delete_client_settings_global(&pool, &["private.setting".to_string()])
            .await
            .expect("global setting should delete");
        delete_client_settings_user(&pool, "user-1", &["reader.page_size".to_string()])
            .await
            .expect("user setting should delete");

        let global = load_client_settings_global(&pool, false)
            .await
            .expect("global settings should reload after delete");
        let global = global
            .as_object()
            .expect("global settings should be an object");
        assert!(global.get("private.setting").is_none());
        assert_eq!(global["public.setting"]["value"], "public-value");

        let user = load_client_settings_user(&pool, "user-1")
            .await
            .expect("user settings should reload after delete");
        assert!(
            user.as_object()
                .expect("user settings should be an object")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn load_history_page_returns_expected_shape_and_order() {
        let pool = create_history_test_db("history-page").await;

        sqlx::query(
            "INSERT INTO HISTORICAL_EVENT (ID, TYPE, BOOK_ID, SERIES_ID, TIMESTAMP) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("event-1")
        .bind("BOOK_ADDED")
        .bind(Some("book-1"))
        .bind(None::<&str>)
        .bind("2024-01-01T00:00:00Z")
        .execute(&pool)
        .await
        .expect("older event should be inserted");
        sqlx::query(
            "INSERT INTO HISTORICAL_EVENT (ID, TYPE, BOOK_ID, SERIES_ID, TIMESTAMP) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("event-2")
        .bind("SERIES_ADDED")
        .bind(None::<&str>)
        .bind(Some("series-1"))
        .bind("2024-02-01T00:00:00Z")
        .execute(&pool)
        .await
        .expect("newer event should be inserted");
        sqlx::query(
            "INSERT INTO HISTORICAL_EVENT_PROPERTIES (ID, \"KEY\", VALUE) VALUES (?, ?, ?)",
        )
        .bind("event-2")
        .bind("source")
        .bind("scanner")
        .execute(&pool)
        .await
        .expect("event property should be inserted");

        let page = load_history_page(&pool, 0, 20, &[])
            .await
            .expect("history page should load");
        let page = page.as_object().expect("history page should be an object");

        assert_eq!(page["totalElements"], 2);
        assert_eq!(page["totalPages"], 1);
        assert_eq!(page["number"], 0);
        assert_eq!(page["size"], 20);
        assert_eq!(page["numberOfElements"], 2);
        assert_eq!(page["first"], true);
        assert_eq!(page["last"], true);
        assert_eq!(page["empty"], false);

        let pageable = page["pageable"]
            .as_object()
            .expect("pageable should be an object");
        assert_eq!(pageable["pageNumber"], 0);
        assert_eq!(pageable["pageSize"], 20);
        assert_eq!(pageable["offset"], 0);
        assert_eq!(pageable["paged"], true);
        assert_eq!(pageable["unpaged"], false);

        let content = page["content"]
            .as_array()
            .expect("content should be an array");
        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["id"], "event-2");
        assert_eq!(content[0]["type"], "SERIES_ADDED");
        assert_eq!(content[0]["seriesId"], "series-1");
        assert_eq!(content[0]["properties"]["source"], "scanner");
        assert_eq!(content[1]["id"], "event-1");
        assert_eq!(content[1]["bookId"], "book-1");
        assert_eq!(content[1]["properties"], serde_json::json!({}));
    }

    #[tokio::test]
    async fn load_history_page_honors_supported_sort_override() {
        let pool = create_history_test_db("history-page-type-sort").await;

        sqlx::query(
            "INSERT INTO HISTORICAL_EVENT (ID, TYPE, BOOK_ID, SERIES_ID, TIMESTAMP) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("event-series")
        .bind("SERIES_ADDED")
        .bind(None::<&str>)
        .bind(Some("series-1"))
        .bind("2024-02-01T00:00:00Z")
        .execute(&pool)
        .await
        .expect("series event should be inserted");
        sqlx::query(
            "INSERT INTO HISTORICAL_EVENT (ID, TYPE, BOOK_ID, SERIES_ID, TIMESTAMP) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("event-book")
        .bind("BOOK_ADDED")
        .bind(Some("book-1"))
        .bind(None::<&str>)
        .bind("2024-01-01T00:00:00Z")
        .execute(&pool)
        .await
        .expect("book event should be inserted");

        let page = load_history_page(&pool, 0, 20, &["type,asc".to_string()])
            .await
            .expect("history page with type sort should load");
        let content = page["content"]
            .as_array()
            .expect("history content should be an array");

        assert_eq!(content[0]["id"], "event-book");
        assert_eq!(content[1]["id"], "event-series");
    }

    #[tokio::test]
    async fn load_history_page_marks_unknown_sort_as_unsorted() {
        let pool = create_history_test_db("history-page-unknown-sort").await;

        sqlx::query(
            "INSERT INTO HISTORICAL_EVENT (ID, TYPE, BOOK_ID, SERIES_ID, TIMESTAMP) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("event-1")
        .bind("BOOK_ADDED")
        .bind(Some("book-1"))
        .bind(None::<&str>)
        .bind("2024-01-01T00:00:00Z")
        .execute(&pool)
        .await
        .expect("event should be inserted");

        let page = load_history_page(&pool, 0, 20, &["unknown,asc".to_string()])
            .await
            .expect("history page with unknown sort should load");

        assert_eq!(
            page["sort"],
            serde_json::json!({
                "empty": true,
                "sorted": false,
                "unsorted": true,
            })
        );
        assert_eq!(
            page["pageable"]["sort"],
            serde_json::json!({
                "empty": true,
                "sorted": false,
                "unsorted": true,
            })
        );
    }

    #[tokio::test]
    async fn delete_syncpoints_by_user_removes_all_rows_for_user() {
        let pool = create_syncpoint_test_db("syncpoints-delete-all").await;

        for (id, user_id, key_id) in [
            ("sp-1", "user-1", "key-1"),
            ("sp-2", "user-1", "key-2"),
            ("sp-3", "user-2", "key-1"),
        ] {
            sqlx::query("INSERT INTO SYNC_POINT (ID, USER_ID, API_KEY_ID) VALUES (?, ?, ?)")
                .bind(id)
                .bind(user_id)
                .bind(key_id)
                .execute(&pool)
                .await
                .expect("sync point should be inserted");
        }

        delete_syncpoints_by_user(&pool, "user-1")
            .await
            .expect("all sync points for user should delete");

        let rows = sqlx::query("SELECT ID FROM SYNC_POINT ORDER BY ID")
            .fetch_all(&pool)
            .await
            .expect("remaining sync points should load");
        let ids = rows
            .iter()
            .map(|row| row.get::<String, _>("ID"))
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["sp-3".to_string()]);
    }

    #[tokio::test]
    async fn delete_syncpoints_by_user_and_key_ids_removes_matching_key_set() {
        let pool = create_syncpoint_test_db("syncpoints-delete-many").await;

        for (id, user_id, key_id) in [
            ("sp-1", "user-1", "key-1"),
            ("sp-2", "user-1", "key-2"),
            ("sp-3", "user-1", "key-3"),
            ("sp-4", "user-2", "key-1"),
        ] {
            sqlx::query("INSERT INTO SYNC_POINT (ID, USER_ID, API_KEY_ID) VALUES (?, ?, ?)")
                .bind(id)
                .bind(user_id)
                .bind(key_id)
                .execute(&pool)
                .await
                .expect("sync point should be inserted");
        }

        delete_syncpoints_by_user_and_key_ids(
            &pool,
            "user-1",
            &["key-1".to_string(), "key-3".to_string()],
        )
        .await
        .expect("matching sync points for key set should delete");

        let rows = sqlx::query("SELECT ID FROM SYNC_POINT ORDER BY ID")
            .fetch_all(&pool)
            .await
            .expect("remaining sync points should load");
        let ids = rows
            .iter()
            .map(|row| row.get::<String, _>("ID"))
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["sp-2".to_string(), "sp-4".to_string()]);
    }
}
