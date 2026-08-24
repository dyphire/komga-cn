use komga_application::identity_access::AuthUserRole;
use komga_application::operational::{
    ClaimInitialAdminUserResult as AppClaimResult, ClaimPort, CreatedClaimedUser,
};
use sqlx::{Row, SqlitePool};

use crate::persistence::DatabaseHandle;

#[derive(Clone)]
pub struct ClaimAccess {
    db: DatabaseHandle,
}

impl ClaimAccess {
    pub fn new(db: DatabaseHandle) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl ClaimPort for ClaimAccess {
    async fn load_claim_status(&self) -> anyhow::Result<bool> {
        load_claim_status(self.db.read_pool())
            .await
            .map_err(anyhow::Error::from)
    }

    async fn claim_initial_admin_user(
        &self,
        user_id: &str,
        email: &str,
        password_hash: &str,
    ) -> anyhow::Result<AppClaimResult> {
        claim_initial_admin_user(self.db.write_pool(), user_id, email, password_hash)
            .await
            .map_err(anyhow::Error::from)
    }
}

pub async fn load_persisted_user_count(pool: &SqlitePool) -> Result<i64, sqlx::Error> {
    let count = sqlx::query(r#"SELECT COUNT(*) AS COUNT FROM USER"#)
        .fetch_one(pool)
        .await?
        .get::<i64, _>("COUNT");
    Ok(count)
}

async fn load_claim_status(pool: &SqlitePool) -> Result<bool, sqlx::Error> {
    Ok(load_persisted_user_count(pool).await? > 0)
}

async fn claim_initial_admin_user(
    pool: &SqlitePool,
    user_id: &str,
    email: &str,
    hashed_password: &str,
) -> Result<AppClaimResult, sqlx::Error> {
    if load_persisted_user_count(pool).await? > 0 {
        return Ok(AppClaimResult::AlreadyClaimed);
    }

    let mut tx = pool.begin().await?;

    sqlx::query(
        r#"INSERT INTO USER (
            ID,
            EMAIL,
            PASSWORD,
            SHARED_ALL_LIBRARIES,
            AGE_RESTRICTION,
            AGE_RESTRICTION_ALLOW_ONLY
        )
        VALUES (?, ?, ?, ?, ?, ?)"#,
    )
    .bind(user_id)
    .bind(email)
    .bind(hashed_password)
    .bind(true)
    .bind(None::<i64>)
    .bind(None::<bool>)
    .execute(&mut *tx)
    .await?;

    for role in AuthUserRole::claim_role_names() {
        sqlx::query(r#"INSERT INTO USER_ROLE (USER_ID, ROLE) VALUES (?, ?)"#)
            .bind(user_id)
            .bind(role)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;

    Ok(AppClaimResult::Created(CreatedClaimedUser {
        id: user_id.to_string(),
        email: email.to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::sqlite::{connect_test_pool, schema};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    async fn create_test_db(case: &str) -> SqlitePool {
        let root = unique_temp_dir(case);
        fs::create_dir_all(&root).expect("temp root should be created");
        let db_path = root.join("claims.sqlite");
        let pool = connect_test_pool(&db_path, 1)
            .await
            .expect("test db should open");
        schema::bootstrap_pool(&pool)
            .await
            .expect("test db should bootstrap main schema");

        pool
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
        let pool = create_test_db("status-empty").await;

        let status = load_claim_status(&pool).await.expect("status should load");

        assert!(!status);
    }

    #[tokio::test]
    async fn load_claim_status_reports_claimed_after_user_exists() {
        let pool = create_test_db("status-claimed").await;

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

        let status = load_claim_status(&pool).await.expect("status should load");

        assert!(status);
    }

    #[tokio::test]
    async fn claim_initial_admin_user_creates_user_and_admin_role_when_unclaimed() {
        let pool = create_test_db("claim-create").await;

        let result = claim_initial_admin_user(
            &pool,
            "rust-claim-1",
            "admin@example.com",
            "hashed-password",
        )
        .await
        .expect("claim should persist");

        match result {
            AppClaimResult::Created(user) => {
                assert_eq!(user.id, "rust-claim-1");
                assert_eq!(user.email, "admin@example.com");
            }
            AppClaimResult::AlreadyClaimed => {
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
        assert_eq!(role_count, 5);
    }

    #[tokio::test]
    async fn claim_initial_admin_user_reports_already_claimed_when_user_exists() {
        let pool = create_test_db("claim-existing").await;

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
            &pool,
            "rust-claim-2",
            "admin@example.com",
            "hashed-password",
        )
        .await
        .expect("claim should load");

        assert!(matches!(result, AppClaimResult::AlreadyClaimed));
    }
}
