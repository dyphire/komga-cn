use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use bcrypt::{DEFAULT_COST, hash as hash_bcrypt_password};
use komga_infrastructure::sqlite::connect_pool;
use tower::util::ServiceExt;

use super::RuntimeDbPaths;

pub fn basic_authorization_header_value(email: &str, password: &str) -> String {
    format!("Basic {}", STANDARD.encode(format!("{email}:{password}")))
}

pub async fn seed_router_age_exclude_user_with_roles(
    paths: &RuntimeDbPaths,
    user_id: &str,
    email: &str,
    password: &str,
    age_restriction: i64,
    roles: &[&str],
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract restricted-user db should open");

    let hashed_password =
        hash_bcrypt_password(password, DEFAULT_COST).expect("bcrypt hash should be computed");

    sqlx::query(
        "INSERT INTO USER (ID, EMAIL, PASSWORD, SHARED_ALL_LIBRARIES, AGE_RESTRICTION, AGE_RESTRICTION_ALLOW_ONLY) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(email)
    .bind(hashed_password)
    .bind(true)
    .bind(age_restriction)
    .bind(false)
    .execute(&pool)
    .await
    .expect("restricted user should be inserted");

    for role in roles {
        sqlx::query("INSERT INTO USER_ROLE (USER_ID, ROLE) VALUES (?, ?)")
            .bind(user_id)
            .bind(*role)
            .execute(&pool)
            .await
            .expect("restricted role should be inserted");
    }

    pool.close().await;
}

pub async fn seed_router_library_restricted_user(
    paths: &RuntimeDbPaths,
    user_id: &str,
    email: &str,
    password: &str,
    library_ids: &[&str],
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract library-restricted-user db should open");

    let hashed_password = hash_bcrypt_password(password, DEFAULT_COST)
        .expect("restricted user password hash should be computed");

    sqlx::query(
        "INSERT INTO USER (ID, EMAIL, PASSWORD, SHARED_ALL_LIBRARIES) \
         VALUES (?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(email)
    .bind(hashed_password)
    .bind(false)
    .execute(&pool)
    .await
    .expect("library-restricted user should be inserted");

    for role in ["USER", "PAGE_STREAMING"] {
        sqlx::query("INSERT INTO USER_ROLE (USER_ID, ROLE) VALUES (?, ?)")
            .bind(user_id)
            .bind(role)
            .execute(&pool)
            .await
            .expect("library-restricted role should be inserted");
    }

    for library_id in library_ids {
        sqlx::query("INSERT INTO USER_LIBRARY_SHARING (USER_ID, LIBRARY_ID) VALUES (?, ?)")
            .bind(user_id)
            .bind(library_id)
            .execute(&pool)
            .await
            .expect("library sharing row should be inserted");
    }

    pool.close().await;
}

pub async fn login_with_basic_and_get_token(app: axum::Router) -> String {
    login_with_basic_credentials_and_get_token(app, "admin@example.org", "router-contract-admin-123")
        .await
}

pub async fn login_with_basic_credentials_and_get_token(
    app: axum::Router,
    email: &str,
    password: &str,
) -> String {
    let basic_token = STANDARD.encode(format!("{email}:{password}"));
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v2/users/me")
                .header(header::AUTHORIZATION, format!("Basic {basic_token}"))
                .header("x-auth-token", "")
                .body(Body::empty())
                .expect("users/me request should build"),
        )
        .await
        .expect("users/me request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    response
        .headers()
        .get("x-auth-token")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .expect("users/me login should return x-auth-token")
}
