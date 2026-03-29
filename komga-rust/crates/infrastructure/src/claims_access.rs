use std::path::Path;

use crate::sqlite::write_models::{
    CreatedClaimedUser, load_persisted_user_count, persist_initial_admin_user,
};

#[derive(Clone, Debug)]
pub enum ClaimInitialAdminUserResult {
    Created(CreatedClaimedUser),
    AlreadyClaimed,
}

pub async fn load_claim_status(database_file: &Path) -> Result<bool, sqlx::Error> {
    Ok(load_persisted_user_count(database_file).await? > 0)
}

pub async fn claim_initial_admin_user(
    database_file: &Path,
    user_id: &str,
    email: &str,
    hashed_password: &str,
) -> Result<ClaimInitialAdminUserResult, sqlx::Error> {
    if load_persisted_user_count(database_file).await? > 0 {
        return Ok(ClaimInitialAdminUserResult::AlreadyClaimed);
    }

    let created_user =
        persist_initial_admin_user(database_file, user_id, email, hashed_password).await?;

    Ok(ClaimInitialAdminUserResult::Created(created_user))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::connect_pool;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    async fn create_test_db(case: &str) -> (PathBuf, sqlx::Pool<sqlx::Sqlite>) {
        let root = unique_temp_dir(case);
        fs::create_dir_all(&root).expect("temp root should be created");
        let db_path = root.join("claims.sqlite");
        let pool = connect_pool(&db_path, 1)
            .await
            .expect("test db should open");

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS USER (ID varchar NOT NULL PRIMARY KEY, EMAIL varchar NOT NULL, PASSWORD varchar NOT NULL, SHARED_ALL_LIBRARIES boolean NOT NULL, AGE_RESTRICTION int8 NULL, AGE_RESTRICTION_ALLOW_ONLY boolean NULL)",
        )
        .execute(&pool)
        .await
        .expect("user table should be created");
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS USER_ROLE (USER_ID varchar NOT NULL, ROLE varchar NOT NULL)",
        )
        .execute(&pool)
        .await
        .expect("user role table should be created");

        (db_path, pool)
    }

    fn unique_temp_dir(case: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "komga-claims-{case}-{nanos}-{}",
            std::process::id()
        ))
    }

    #[tokio::test]
    async fn load_claim_status_reports_unclaimed_when_no_users_exist() {
        let (db_path, _pool) = create_test_db("status-empty").await;

        let status = load_claim_status(db_path.as_path())
            .await
            .expect("status should load");

        assert!(!status);
    }

    #[tokio::test]
    async fn load_claim_status_reports_claimed_after_user_exists() {
        let (db_path, pool) = create_test_db("status-claimed").await;

        sqlx::query(
            "INSERT INTO USER (ID, EMAIL, PASSWORD, SHARED_ALL_LIBRARIES, AGE_RESTRICTION, AGE_RESTRICTION_ALLOW_ONLY) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("user-1")
        .bind("admin@example.com")
        .bind("hashed-password")
        .bind(true)
        .bind(None::<i64>)
        .bind(None::<bool>)
        .execute(&pool)
        .await
        .expect("user row should be inserted");

        let status = load_claim_status(db_path.as_path())
            .await
            .expect("status should load");

        assert!(status);
    }

    #[tokio::test]
    async fn claim_initial_admin_user_creates_user_and_admin_role_when_unclaimed() {
        let (db_path, pool) = create_test_db("claim-create").await;

        let result = claim_initial_admin_user(
            db_path.as_path(),
            "rust-claim-1",
            "admin@example.com",
            "hashed-password",
        )
        .await
        .expect("claim should persist");

        match result {
            ClaimInitialAdminUserResult::Created(user) => {
                assert_eq!(user.id, "rust-claim-1");
                assert_eq!(user.email, "admin@example.com");
            }
            ClaimInitialAdminUserResult::AlreadyClaimed => {
                panic!("claim should have created the initial admin user")
            }
        }

        let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM USER")
            .fetch_one(&pool)
            .await
            .expect("user count should load");
        let role_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM USER_ROLE")
            .fetch_one(&pool)
            .await
            .expect("role count should load");

        assert_eq!(user_count, 1);
        assert_eq!(role_count, 1);
    }

    #[tokio::test]
    async fn claim_initial_admin_user_reports_already_claimed_when_user_exists() {
        let (db_path, pool) = create_test_db("claim-existing").await;

        sqlx::query(
            "INSERT INTO USER (ID, EMAIL, PASSWORD, SHARED_ALL_LIBRARIES, AGE_RESTRICTION, AGE_RESTRICTION_ALLOW_ONLY) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("user-1")
        .bind("admin@example.com")
        .bind("hashed-password")
        .bind(true)
        .bind(None::<i64>)
        .bind(None::<bool>)
        .execute(&pool)
        .await
        .expect("user row should be inserted");

        let result = claim_initial_admin_user(
            db_path.as_path(),
            "rust-claim-2",
            "admin@example.com",
            "hashed-password",
        )
        .await
        .expect("claim should load");

        assert!(matches!(
            result,
            ClaimInitialAdminUserResult::AlreadyClaimed
        ));
    }
}
