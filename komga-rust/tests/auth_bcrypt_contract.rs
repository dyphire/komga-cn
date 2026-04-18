use axum::http::{HeaderMap, header};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use bcrypt::{DEFAULT_COST, hash as hash_bcrypt_password};
use komga_application::identity_access::AuthOutcome;
use komga_infrastructure::runtime_identity_access::{
    persisted_basic_user, persisted_update_password_by_user_id,
};
use komga_infrastructure::sqlite::{connect_test_pool, setup};
use std::path::PathBuf;
use tempfile::TempDir;

async fn create_test_db(case: &str) -> (TempDir, PathBuf, sqlx::Pool<sqlx::Sqlite>) {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let db_path = temp_dir.path().join(format!("{case}.sqlite"));
    let pool = connect_test_pool(&db_path, 1)
        .await
        .expect("test db should open");
    setup::bootstrap_pool(&pool)
        .await
        .expect("test db should bootstrap main schema");

    (temp_dir, db_path, pool)
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

fn basic_auth_headers(email: &str, password: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    let credentials = STANDARD.encode(format!("{email}:{password}"));
    headers.insert(
        header::AUTHORIZATION,
        header::HeaderValue::from_str(&format!("Basic {credentials}"))
            .expect("basic auth header should be valid"),
    );
    headers
}

fn kotlin_style_bcrypt_hash(password: &str) -> String {
    // Kotlin's BCryptPasswordEncoder uses the historical $2a$ envelope; the verifier must keep
    // accepting that format even when the underlying bcrypt body is identical.
    hash_bcrypt_password(password, DEFAULT_COST)
        .expect("bcrypt hash should be generated")
        .replacen("$2b$", "$2a$", 1)
}

async fn persisted_password(db_path: &PathBuf, user_id: &str) -> String {
    let pool = connect_test_pool(db_path, 1)
        .await
        .expect("test db should reopen");
    sqlx::query_scalar::<_, String>("SELECT PASSWORD FROM USER WHERE ID = ?")
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .expect("password should load")
}

#[tokio::test]
async fn kotlin_bcrypt_hashes_verify_in_rust() {
    let (_temp_dir, db_path, pool) = create_test_db("legacy-bcrypt").await;
    let raw_password = "kotlin-password";
    let legacy_hash = kotlin_style_bcrypt_hash(raw_password);

    insert_test_user(&pool, "user-1", "admin@example.com", &legacy_hash).await;

    let outcome = persisted_basic_user(
        &basic_auth_headers("admin@example.com", raw_password),
        db_path.as_path(),
    )
    .await;

    match outcome {
        Some(AuthOutcome::Valid(user)) => {
            assert_eq!(user.email, "admin@example.com");
        }
        other => panic!("legacy bcrypt hash should authenticate, got {other:?}"),
    }
}

#[tokio::test]
async fn password_updates_emit_bcrypt_hashes() {
    let (_temp_dir, db_path, pool) = create_test_db("password-update").await;
    insert_test_user(&pool, "user-1", "admin@example.com", "old-password-hash").await;

    let updated =
        persisted_update_password_by_user_id(db_path.as_path(), "user-1", "new-password").await;

    assert_eq!(updated, Some(true));

    let stored_password = persisted_password(&db_path, "user-1").await;
    assert!(stored_password.starts_with("$2"));
    assert_eq!(stored_password.len(), 60);

    let outcome = persisted_basic_user(
        &basic_auth_headers("admin@example.com", "new-password"),
        db_path.as_path(),
    )
    .await;

    match outcome {
        Some(AuthOutcome::Valid(user)) => {
            assert_eq!(user.id, "user-1");
            assert_eq!(user.email, "admin@example.com");
        }
        other => panic!("updated bcrypt hash should still authenticate, got {other:?}"),
    }
}
