mod actuator;
mod announcements;
mod announcements_persistence;
mod client_settings;
mod filesystem_browse;
mod fonts;
mod history;
mod metrics;
mod page_hashes;
mod remote_feeds;
mod server_settings;
mod syncpoints;

pub use actuator::ActuatorSnapshotAccess;
pub use announcements::AnnouncementAccess;
pub use client_settings::ClientSettingsAccess;
pub use filesystem_browse::FilesystemBrowseAccess;
pub use fonts::FontAccess;
pub use history::HistoryAccess;
pub use metrics::OperationalMetricsAccess;
pub use page_hashes::PageHashAccess;
pub use remote_feeds::RemoteFeedAccess;
pub use server_settings::{
    RememberMeRuntimeSettings, ServerSettingsStore, load_remember_me_runtime_settings,
};
pub use syncpoints::SyncpointAccess;

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
    use komga_application::operational::{
        ClientGlobalSetting, ClientUserSetting, HistorySort, HistorySortDirection,
        HistorySortProperty, HistorySortSelection,
    };
    use komga_infrastructure_base::sqlite::connect_test_pool;
    use sqlx::Row;
    use std::collections::BTreeMap;
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
        komga_infrastructure_base::sqlite::schema::bootstrap_pool(&pool)
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
        komga_infrastructure_base::sqlite::schema::bootstrap_pool(&pool)
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
        komga_infrastructure_base::sqlite::schema::bootstrap_pool(&pool)
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
        assert_eq!(
            all.get("public.setting")
                .map(|setting| setting.value.as_str()),
            Some("public-value")
        );
        assert_eq!(
            all.get("private.setting")
                .map(|setting| setting.value.as_str()),
            Some("private-value")
        );
        assert!(!all.contains_key("webui.oauth2.hide_login"));

        let unauthorized_only = load_client_settings_global(&pool, true)
            .await
            .expect("filtered global settings should load");
        assert_eq!(
            unauthorized_only
                .get("public.setting")
                .map(|setting| setting.value.as_str()),
            Some("public-value")
        );
        assert!(!unauthorized_only.contains_key("private.setting"));
        assert!(!unauthorized_only.contains_key("webui.oauth2.hide_login"));
    }

    #[tokio::test]
    async fn client_settings_access_round_trips_global_and_user_changes() {
        let pool = create_test_db("round-trip").await;

        upsert_client_settings_global(
            &pool,
            &BTreeMap::from([
                (
                    "public.setting".to_string(),
                    ClientGlobalSetting {
                        value: "public-value".to_string(),
                        allow_unauthorized: true,
                    },
                ),
                (
                    "private.setting".to_string(),
                    ClientGlobalSetting {
                        value: "private-value".to_string(),
                        allow_unauthorized: false,
                    },
                ),
            ]),
        )
        .await
        .expect("global settings should persist");
        upsert_client_settings_user(
            &pool,
            "user-1",
            &BTreeMap::from([(
                "reader.page_size".to_string(),
                ClientUserSetting {
                    value: "42".to_string(),
                },
            )]),
        )
        .await
        .expect("user settings should persist");

        let global = load_client_settings_global(&pool, false)
            .await
            .expect("global settings should reload");
        assert_eq!(
            global
                .get("public.setting")
                .map(|setting| setting.value.as_str()),
            Some("public-value")
        );
        assert_eq!(
            global
                .get("private.setting")
                .map(|setting| setting.value.as_str()),
            Some("private-value")
        );

        let user = load_client_settings_user(&pool, "user-1")
            .await
            .expect("user settings should reload");
        assert_eq!(
            user.get("reader.page_size")
                .map(|setting| setting.value.as_str()),
            Some("42")
        );

        delete_client_settings_global(&pool, &["private.setting".to_string()])
            .await
            .expect("global setting should delete");
        delete_client_settings_user(&pool, "user-1", &["reader.page_size".to_string()])
            .await
            .expect("user setting should delete");

        let global = load_client_settings_global(&pool, false)
            .await
            .expect("global settings should reload after delete");
        assert!(!global.contains_key("private.setting"));
        assert_eq!(
            global
                .get("public.setting")
                .map(|setting| setting.value.as_str()),
            Some("public-value")
        );

        let user = load_client_settings_user(&pool, "user-1")
            .await
            .expect("user settings should reload after delete");
        assert!(user.is_empty());
    }

    #[tokio::test]
    async fn load_history_page_returns_expected_entries_and_pagination_facts() {
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

        let page = load_history_page(&pool, 0, 20, HistorySortSelection::default_timestamp_desc())
            .await
            .expect("history page should load");

        assert_eq!(page.total_elements, 2);
        assert_eq!(page.total_pages, 1);
        assert_eq!(page.page, 0);
        assert_eq!(page.size, 20);
        assert_eq!(page.number_of_elements(), 2);
        assert_eq!(page.offset(), 0);
        assert!(page.sorted);

        assert_eq!(page.content.len(), 2);
        assert_eq!(page.content[0].id, "event-2");
        assert_eq!(page.content[0].event_type, "SERIES_ADDED");
        assert_eq!(page.content[0].series_id.as_deref(), Some("series-1"));
        assert_eq!(
            page.content[0].properties.get("source").map(String::as_str),
            Some("scanner")
        );
        assert_eq!(page.content[1].id, "event-1");
        assert_eq!(page.content[1].book_id.as_deref(), Some("book-1"));
        assert!(page.content[1].properties.is_empty());
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

        let page = load_history_page(
            &pool,
            0,
            20,
            HistorySortSelection::from_requested_sorts(vec![HistorySort {
                property: HistorySortProperty::Type,
                direction: HistorySortDirection::Asc,
            }]),
        )
        .await
        .expect("history page with type sort should load");

        assert_eq!(page.content[0].id, "event-book");
        assert_eq!(page.content[1].id, "event-series");
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
