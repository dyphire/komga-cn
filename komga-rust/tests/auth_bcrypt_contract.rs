use bcrypt::{DEFAULT_COST, hash as hash_bcrypt_password};
use komga_application::identity_access::{AuthOutcome, AuthenticationPort};
use komga_infrastructure::{
    identity::IdentityAccess, identity::persisted_update_password_by_user_id,
    media::ContentResolver, persistence::DatabaseHandle, persistence::bootstrap_pool,
};
use std::sync::Arc;
use tempfile::TempDir;

#[path = "support/sqlite.rs"]
mod sqlite_support;
use sqlite_support::connect_test_pool;

async fn create_test_db(case: &str) -> (TempDir, sqlx::Pool<sqlx::Sqlite>) {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let db_path = temp_dir.path().join(format!("{case}.sqlite"));
    let pool = connect_test_pool(&db_path, 1)
        .await
        .expect("test db should open");
    bootstrap_pool(&pool)
        .await
        .expect("test db should bootstrap main schema");

    (temp_dir, pool)
}

fn identity_access(
    temp_dir: &TempDir,
    case: &str,
    pool: &sqlx::Pool<sqlx::Sqlite>,
) -> IdentityAccess {
    let db_path = temp_dir.path().join(format!("{case}.sqlite"));
    IdentityAccess::new(
        DatabaseHandle::single_pool(db_path, pool.clone()),
        Arc::new(ContentResolver),
    )
}

async fn insert_test_user(
    pool: &sqlx::Pool<sqlx::Sqlite>,
    user_id: &str,
    email: &str,
    password: &str,
) {
    sqlx::query(
        "INSERT INTO USER (ID, EMAIL, PASSWORD, SHARED_ALL_LIBRARIES, AGE_RESTRICTION, AGE_RESTRICTION_ALLOW_ONLY) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(email)
    .bind(password)
    .bind(false)
    .bind(None::<i64>)
    .bind(None::<bool>)
    .execute(pool)
    .await
    .expect("user row should be inserted");
}

fn kotlin_style_bcrypt_hash(password: &str) -> String {
    // Kotlin's BCryptPasswordEncoder uses the historical $2a$ envelope; the verifier must keep
    // accepting that format even when the underlying bcrypt body is identical.
    hash_bcrypt_password(password, DEFAULT_COST)
        .expect("bcrypt hash should be generated")
        .replacen("$2b$", "$2a$", 1)
}

async fn persisted_password(pool: &sqlx::Pool<sqlx::Sqlite>, user_id: &str) -> String {
    sqlx::query_scalar::<_, String>("SELECT PASSWORD FROM USER WHERE ID = ?")
        .bind(user_id)
        .fetch_one(pool)
        .await
        .expect("password should load")
}

#[tokio::test]
async fn kotlin_bcrypt_hashes_verify_in_rust() {
    let (temp_dir, pool) = create_test_db("legacy-bcrypt").await;
    let identity = identity_access(&temp_dir, "legacy-bcrypt", &pool);
    let raw_password = "kotlin-password";
    let legacy_hash = kotlin_style_bcrypt_hash(raw_password);

    insert_test_user(&pool, "user-1", "admin@example.com", &legacy_hash).await;

    let outcome = identity
        .authenticate_basic("admin@example.com", raw_password)
        .await;

    match outcome {
        Ok(AuthOutcome::Valid(user)) => {
            assert_eq!(user.email, "admin@example.com");
        }
        other => panic!("legacy bcrypt hash should authenticate, got {other:?}"),
    }
}

#[tokio::test]
async fn password_updates_emit_bcrypt_hashes() {
    let (temp_dir, pool) = create_test_db("password-update").await;
    let identity = identity_access(&temp_dir, "password-update", &pool);
    insert_test_user(&pool, "user-1", "admin@example.com", "old-password-hash").await;

    let updated = persisted_update_password_by_user_id(&pool, "user-1", "new-password").await;

    assert!(matches!(updated, Ok(true)));

    let stored_password = persisted_password(&pool, "user-1").await;
    assert!(stored_password.starts_with("$2"));
    assert_eq!(stored_password.len(), 60);

    let outcome = identity
        .authenticate_basic("admin@example.com", "new-password")
        .await;

    match outcome {
        Ok(AuthOutcome::Valid(user)) => {
            assert_eq!(user.id, "user-1");
            assert_eq!(user.email, "admin@example.com");
        }
        other => panic!("updated bcrypt hash should still authenticate, got {other:?}"),
    }
}

#[tokio::test]
async fn malformed_persisted_bcrypt_hash_fails_as_storage_error() {
    let (temp_dir, pool) = create_test_db("malformed-bcrypt").await;
    let identity = identity_access(&temp_dir, "malformed-bcrypt", &pool);

    insert_test_user(&pool, "user-1", "admin@example.com", "not-a-bcrypt-hash").await;

    let outcome = identity
        .authenticate_basic("admin@example.com", "password")
        .await;

    assert!(
        matches!(outcome, Err(ref error) if error.to_string().contains("failed to verify persisted password hash")),
        "malformed persisted hash should be a storage error, got {outcome:?}"
    );
}
