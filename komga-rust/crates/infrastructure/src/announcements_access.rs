use sqlx::SqlitePool;

use crate::sqlite::read_models::announcements::load_announcement_read_ids as load_announcement_read_ids_model;
use crate::sqlite::write_models::announcements::save_announcements_read as save_announcements_read_model;

pub async fn load_announcement_read_ids(
    pool: &SqlitePool,
    user_id: &str,
) -> Result<Vec<String>, sqlx::Error> {
    load_announcement_read_ids_model(pool, user_id).await
}

pub async fn save_announcements_read(
    pool: &SqlitePool,
    user_id: &str,
    announcement_ids: &[String],
) -> Result<(), sqlx::Error> {
    save_announcements_read_model(pool, user_id, announcement_ids).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::{connect_test_pool, setup};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    async fn create_test_db(case: &str) -> sqlx::Pool<sqlx::Sqlite> {
        let root = unique_temp_dir(case);
        fs::create_dir_all(&root).expect("temp root should be created");
        let db_path = root.join("announcements.sqlite");
        let pool = connect_test_pool(&db_path, 1)
            .await
            .expect("test db should open");
        setup::bootstrap_pool(&pool)
            .await
            .expect("test db should bootstrap main schema");

        sqlx::query(
            "INSERT INTO USER (ID, EMAIL, PASSWORD, SHARED_ALL_LIBRARIES) VALUES (?, ?, ?, ?)",
        )
        .bind("user-1")
        .bind("user-1@example.org")
        .bind("hashed-password")
        .bind(true)
        .execute(&pool)
        .await
        .expect("user row should be inserted");

        pool
    }

    fn unique_temp_dir(case: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "komga-announcements-{case}-{nanos}-{}",
            std::process::id()
        ))
    }

    #[tokio::test]
    async fn load_announcement_read_ids_returns_read_ids_in_sorted_order() {
        let pool = create_test_db("sorted-load").await;

        sqlx::query("INSERT INTO ANNOUNCEMENTS_READ (USER_ID, ANNOUNCEMENT_ID) VALUES (?, ?)")
            .bind("user-1")
            .bind("announcement-b")
            .execute(&pool)
            .await
            .expect("announcement b should be inserted");
        sqlx::query("INSERT INTO ANNOUNCEMENTS_READ (USER_ID, ANNOUNCEMENT_ID) VALUES (?, ?)")
            .bind("user-1")
            .bind("announcement-a")
            .execute(&pool)
            .await
            .expect("announcement a should be inserted");

        let read_ids = load_announcement_read_ids(&pool, "user-1")
            .await
            .expect("read ids should load");

        assert_eq!(
            read_ids,
            vec!["announcement-a".to_string(), "announcement-b".to_string()]
        );
    }

    #[tokio::test]
    async fn save_announcements_read_persists_unique_ids_for_user() {
        let pool = create_test_db("save-round-trip").await;
        let announcement_ids = vec![
            "announcement-c".to_string(),
            "announcement-a".to_string(),
            "announcement-c".to_string(),
        ];

        save_announcements_read(&pool, "user-1", &announcement_ids)
            .await
            .expect("announcement reads should persist");

        let read_ids = load_announcement_read_ids(&pool, "user-1")
            .await
            .expect("read ids should reload");

        assert_eq!(
            read_ids,
            vec!["announcement-a".to_string(), "announcement-c".to_string()]
        );
    }
}
