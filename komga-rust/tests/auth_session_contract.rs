use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use komga_compat_testkit::contract_matrix::assert_required_target_declared;
use komga_rust::persistence::sqlite::connect_pool;
use serde_json::Value;
use std::fs;
use std::path::Path;
use tower::ServiceExt;

#[path = "compat/auth_env.rs"]
mod compat_auth_env;

#[path = "support/persistence_contract_fixture.rs"]
mod persistence_contract_fixture;

const DB_ADMIN_USER_ID: &str = "db-admin-1";
const DB_ADMIN_EMAIL: &str = "db-admin@example.org";
const DB_ADMIN_PASSWORD: &str = "db-admin-password";
const DB_ADMIN_UPDATED_PASSWORD: &str = "db-admin-password-updated";
const DB_ADMIN_PASSWORD_BCRYPT: &str =
    "$2a$10$x7NyXzncFgR/Nd/VR8eYde9njk/JaWz1X05C1wkk1G89dZnmVpw3e";
const DB_USER_ID: &str = "db-user-1";
const DB_USER_EMAIL: &str = "db-user@example.org";
const DB_USER_UPDATED_PASSWORD: &str = "db-user-password-updated";
const DB_USER_PASSWORD_BCRYPT: &str =
    "$2a$10$6uBfM3Iovphyo.x1KDYFa.kdgG/Wth9mRYP9wQDTwYF0ShEXc6/4m";
const DB_API_KEY_ID: &str = "db-api-key-1";
const DB_API_KEY: &str = "db-compat-api-key";
const DB_API_KEY_SHA512: &str = "b5938e5cff74b2ca05ea0a075501ae9f39f9cc7218691673a00cf9461d15cb85ec4abffd7b147d30543cde42ec894503594eae8140f3f500ffd1f835a649bda1";
const DB_AUTH_ACTIVITY_API_KEY_IP: &str = "203.0.113.10";
const DB_AUTH_ACTIVITY_API_KEY_USER_AGENT: &str = "Komga contract API key client";
const DB_AUTH_ACTIVITY_API_KEY_DATE_TIME: &str = "2099-01-02 03:04:05";
const DB_AUTH_ACTIVITY_PASSWORD_IP: &str = "203.0.113.11";
const DB_AUTH_ACTIVITY_PASSWORD_USER_AGENT: &str = "Komga contract password client";
const DB_AUTH_ACTIVITY_PASSWORD_ERROR: &str = "Bad credentials";
const DB_AUTH_ACTIVITY_PASSWORD_DATE_TIME: &str = "2099-01-01 02:03:04";
const DB_USER_AUTH_ACTIVITY_IP: &str = "203.0.113.12";
const DB_USER_AUTH_ACTIVITY_USER_AGENT: &str = "Komga contract secondary user client";
const DB_USER_AUTH_ACTIVITY_DATE_TIME: &str = "2098-12-31 01:02:03";

fn basic_auth(credentials: &str) -> String {
    format!(
        "Basic {}",
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, credentials),
    )
}

fn extract_cookie_value(set_cookie: &str, cookie_name: &str) -> String {
    set_cookie
        .split(';')
        .map(str::trim)
        .find_map(|part| part.strip_prefix(&format!("{cookie_name}=")))
        .unwrap_or_else(|| panic!("response should include {cookie_name} cookie"))
        .to_string()
}

fn extract_cookie_value_from_headers(
    headers: &axum::http::HeaderMap,
    cookie_name: &str,
) -> Option<String> {
    headers
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find_map(|set_cookie| {
            set_cookie
                .split(';')
                .map(str::trim)
                .find_map(|part| part.strip_prefix(&format!("{cookie_name}=")))
                .map(str::to_string)
        })
}

async fn response_json(response: axum::response::Response) -> Value {
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

async fn login_status(app: &axum::Router, credentials: &str) -> StatusCode {
    app.clone()
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me")
                .header(header::AUTHORIZATION, basic_auth(credentials))
                .header("X-Auth-Token", "")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

async fn login_header_exchange(app: &axum::Router, credentials: &str) -> (String, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me")
                .header(header::AUTHORIZATION, basic_auth(credentials))
                .header("X-Auth-Token", "")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let token = response
        .headers()
        .get("x-auth-token")
        .expect("login response should include x-auth-token")
        .to_str()
        .expect("x-auth-token should be valid UTF-8")
        .to_string();
    let json = response_json(response).await;

    (token, json)
}

async fn exchange_token_for_cookie(app: &axum::Router, token: &str) -> String {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/login/set-cookie")
                .header("X-Auth-Token", token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(response.headers().get("x-auth-token").is_none());

    let set_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .expect("login/set-cookie should issue KOMGA-SESSION cookie")
        .to_str()
        .unwrap()
        .to_string();

    assert!(set_cookie.contains("KOMGA-SESSION="));
    assert!(set_cookie.contains("Path=/"));
    assert!(set_cookie.contains("HttpOnly"));

    extract_cookie_value(&set_cookie, "KOMGA-SESSION")
}

async fn runtime_with_db_backed_auth_fixture() -> axum::Router {
    let paths = persistence_contract_fixture::new_legacy_db_paths("auth-session-db-backed")
        .expect("auth session contract db paths should be created");
    persistence_contract_fixture::seed_main_db_from_flyway(&paths.main_db)
        .await
        .expect("main db flyway fixture should be created");
    persistence_contract_fixture::seed_tasks_db_from_flyway(&paths.tasks_db)
        .await
        .expect("tasks db flyway fixture should be created");

    seed_db_backed_auth_rows(&paths.main_db).await;
    assert_db_backed_auth_rows(&paths.main_db).await;

    fs::create_dir_all(paths.config_dir.join("lucene")).unwrap();
    fs::create_dir_all(paths.config_dir.join("fonts")).unwrap();

    let mut config = komga_rust::config::RuntimeConfig::for_compat_profile(
        komga_rust::config::CompatProfile::SnapshotAligned,
    );
    config.config_dir = Some(paths.config_dir.clone());
    config.log_file = paths.config_dir.join("komga.log");
    config.database_file = paths.main_db.clone();
    config.tasks_db_file = paths.tasks_db.clone();
    config.lucene_data_directory = paths.config_dir.join("lucene");
    config.fonts_data_directory = paths.config_dir.join("fonts");

    komga_rust::app::build_router_with_config(&config)
}

async fn runtime_with_db_backed_auth_fixture_without_admin_activity() -> axum::Router {
    let paths = persistence_contract_fixture::new_legacy_db_paths(
        "auth-session-db-backed-no-admin-activity",
    )
    .expect("auth session contract db paths should be created");
    persistence_contract_fixture::seed_main_db_from_flyway(&paths.main_db)
        .await
        .expect("main db flyway fixture should be created");
    persistence_contract_fixture::seed_tasks_db_from_flyway(&paths.tasks_db)
        .await
        .expect("tasks db flyway fixture should be created");

    seed_db_backed_auth_rows(&paths.main_db).await;
    assert_db_backed_auth_rows(&paths.main_db).await;

    let pool = connect_pool(&paths.main_db, 1)
        .await
        .expect("sqlite pool should open for auth activity cleanup");
    sqlx::query("DELETE FROM AUTHENTICATION_ACTIVITY WHERE USER_ID = ?")
        .bind(DB_ADMIN_USER_ID)
        .execute(&pool)
        .await
        .expect("admin authentication activity rows should delete for no-activity fixture");
    pool.close().await;

    fs::create_dir_all(paths.config_dir.join("lucene")).unwrap();
    fs::create_dir_all(paths.config_dir.join("fonts")).unwrap();

    let mut config = komga_rust::config::RuntimeConfig::for_compat_profile(
        komga_rust::config::CompatProfile::SnapshotAligned,
    );
    config.config_dir = Some(paths.config_dir.clone());
    config.log_file = paths.config_dir.join("komga.log");
    config.database_file = paths.main_db.clone();
    config.tasks_db_file = paths.tasks_db.clone();
    config.lucene_data_directory = paths.config_dir.join("lucene");
    config.fonts_data_directory = paths.config_dir.join("fonts");

    komga_rust::app::build_router_with_config(&config)
}

async fn runtime_with_db_backed_auth_fixture_and_oauth() -> axum::Router {
    let paths = persistence_contract_fixture::new_legacy_db_paths("auth-session-db-backed-oauth")
        .expect("auth session oauth db paths should be created");
    persistence_contract_fixture::seed_main_db_from_flyway(&paths.main_db)
        .await
        .expect("main db flyway fixture should be created");
    persistence_contract_fixture::seed_tasks_db_from_flyway(&paths.tasks_db)
        .await
        .expect("tasks db flyway fixture should be created");

    seed_db_backed_auth_rows(&paths.main_db).await;
    assert_db_backed_auth_rows(&paths.main_db).await;

    fs::create_dir_all(paths.config_dir.join("lucene")).unwrap();
    fs::create_dir_all(paths.config_dir.join("fonts")).unwrap();

    let mut config = komga_rust::config::RuntimeConfig::for_compat_profile(
        komga_rust::config::CompatProfile::SnapshotAligned,
    );
    config.config_dir = Some(paths.config_dir.clone());
    config.log_file = paths.config_dir.join("komga.log");
    config.database_file = paths.main_db.clone();
    config.tasks_db_file = paths.tasks_db.clone();
    config.lucene_data_directory = paths.config_dir.join("lucene");
    config.fonts_data_directory = paths.config_dir.join("fonts");
    config.oauth2_clients = vec![komga_rust::config::OAuth2ClientConfig {
        registration_id: "oidc".to_string(),
        client_name: "Example OIDC".to_string(),
        client_id: "compat-client".to_string(),
        client_secret: "compat-secret".to_string(),
        authorization_uri: "https://id.example.org/oauth2/authorize".to_string(),
        token_uri: "https://id.example.org/oauth2/token".to_string(),
    }];

    komga_rust::app::build_router_with_config(&config)
}

async fn seed_db_backed_auth_rows(main_db: &Path) {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for auth fixture seeding");

    for (id, email, password) in [
        (DB_ADMIN_USER_ID, DB_ADMIN_EMAIL, DB_ADMIN_PASSWORD_BCRYPT),
        (DB_USER_ID, DB_USER_EMAIL, DB_USER_PASSWORD_BCRYPT),
    ] {
        sqlx::query(
            "INSERT INTO USER (ID, EMAIL, PASSWORD, SHARED_ALL_LIBRARIES) VALUES (?, ?, ?, ?)",
        )
        .bind(id)
        .bind(email)
        .bind(password)
        .bind(true)
        .execute(&pool)
        .await
        .expect("auth fixture user should insert with Kotlin-compatible columns");
    }

    for role in ["ADMIN", "FILE_DOWNLOAD", "PAGE_STREAMING", "USER"] {
        sqlx::query("INSERT INTO USER_ROLE (USER_ID, ROLE) VALUES (?, ?)")
            .bind(DB_ADMIN_USER_ID)
            .bind(role)
            .execute(&pool)
            .await
            .expect("auth fixture admin role should insert");
    }

    sqlx::query("INSERT INTO USER_ROLE (USER_ID, ROLE) VALUES (?, ?)")
        .bind(DB_USER_ID)
        .bind("USER")
        .execute(&pool)
        .await
        .expect("auth fixture secondary user role should insert");

    sqlx::query("INSERT INTO USER_API_KEY (ID, USER_ID, API_KEY, COMMENT) VALUES (?, ?, ?, ?)")
        .bind(DB_API_KEY_ID)
        .bind(DB_ADMIN_USER_ID)
        .bind(DB_API_KEY_SHA512)
        .bind(DB_API_KEY)
        .execute(&pool)
        .await
        .expect("auth fixture API key should insert with Kotlin-compatible token encoding");

    for (
        user_id,
        email,
        ip,
        user_agent,
        success,
        error,
        date_time,
        source,
        api_key_id,
        api_key_comment,
    ) in [
        (
            Some(DB_ADMIN_USER_ID),
            Some(DB_ADMIN_EMAIL),
            Some(DB_AUTH_ACTIVITY_API_KEY_IP),
            Some(DB_AUTH_ACTIVITY_API_KEY_USER_AGENT),
            true,
            None,
            DB_AUTH_ACTIVITY_API_KEY_DATE_TIME,
            Some("API_KEY"),
            Some(DB_API_KEY_ID),
            Some(DB_API_KEY),
        ),
        (
            Some(DB_ADMIN_USER_ID),
            Some(DB_ADMIN_EMAIL),
            Some(DB_AUTH_ACTIVITY_PASSWORD_IP),
            Some(DB_AUTH_ACTIVITY_PASSWORD_USER_AGENT),
            false,
            Some(DB_AUTH_ACTIVITY_PASSWORD_ERROR),
            DB_AUTH_ACTIVITY_PASSWORD_DATE_TIME,
            Some("BASIC"),
            None,
            None,
        ),
        (
            Some(DB_USER_ID),
            Some(DB_USER_EMAIL),
            Some(DB_USER_AUTH_ACTIVITY_IP),
            Some(DB_USER_AUTH_ACTIVITY_USER_AGENT),
            true,
            None,
            DB_USER_AUTH_ACTIVITY_DATE_TIME,
            Some("SESSION"),
            None,
            None,
        ),
    ] {
        sqlx::query(
            "INSERT INTO AUTHENTICATION_ACTIVITY (USER_ID, EMAIL, IP, USER_AGENT, SUCCESS, ERROR, DATE_TIME, SOURCE, API_KEY_ID, API_KEY_COMMENT) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(user_id)
        .bind(email)
        .bind(ip)
        .bind(user_agent)
        .bind(success)
        .bind(error)
        .bind(date_time)
        .bind(source)
        .bind(api_key_id)
        .bind(api_key_comment)
        .execute(&pool)
        .await
        .expect("auth fixture authentication activity row should insert");
    }

    pool.close().await;
}

async fn assert_db_backed_auth_rows(main_db: &Path) {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for auth fixture verification");

    let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM USER")
        .fetch_one(&pool)
        .await
        .expect("auth fixture users should be queryable");
    assert_eq!(user_count, 2, "auth fixture should seed exactly two users");

    let admin_email: String = sqlx::query_scalar("SELECT EMAIL FROM USER WHERE ID = ?")
        .bind(DB_ADMIN_USER_ID)
        .fetch_one(&pool)
        .await
        .expect("auth fixture admin user should exist");
    assert_eq!(admin_email, DB_ADMIN_EMAIL);

    let admin_role_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM USER_ROLE WHERE USER_ID = ? AND ROLE = 'ADMIN'")
            .bind(DB_ADMIN_USER_ID)
            .fetch_one(&pool)
            .await
            .expect("auth fixture admin role should be queryable");
    assert_eq!(
        admin_role_count, 1,
        "auth fixture should expose ADMIN via USER_ROLE rather than legacy USER columns",
    );

    let api_key_hash: String =
        sqlx::query_scalar("SELECT API_KEY FROM USER_API_KEY WHERE ID = ? AND USER_ID = ?")
            .bind(DB_API_KEY_ID)
            .bind(DB_ADMIN_USER_ID)
            .fetch_one(&pool)
            .await
            .expect("auth fixture API key should be queryable");
    assert_eq!(
        api_key_hash, DB_API_KEY_SHA512,
        "auth fixture should store the Kotlin token-encoded API key hash",
    );

    let activity_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM AUTHENTICATION_ACTIVITY")
        .fetch_one(&pool)
        .await
        .expect("auth fixture authentication activity should be queryable");
    assert_eq!(
        activity_count, 3,
        "auth fixture should seed authentication activity rows for admin and secondary users",
    );

    let latest_api_key_activity_source: String = sqlx::query_scalar(
        "SELECT SOURCE FROM AUTHENTICATION_ACTIVITY WHERE USER_ID = ? AND API_KEY_ID = ? AND DATE_TIME = ?",
    )
    .bind(DB_ADMIN_USER_ID)
    .bind(DB_API_KEY_ID)
    .bind(DB_AUTH_ACTIVITY_API_KEY_DATE_TIME)
    .fetch_one(&pool)
    .await
    .expect("auth fixture API key activity should be queryable");
    assert_eq!(latest_api_key_activity_source, "API_KEY");

    pool.close().await;
}

#[test]
fn auth_session_contract_target_is_registered() {
    assert_required_target_declared("auth/session", "auth_session_contract");

    let cleanup_paths =
        persistence_contract_fixture::new_legacy_db_paths("auth-session-cleanup-probe")
            .expect("auth session cleanup probe paths should be created");
    persistence_contract_fixture::cleanup(cleanup_paths);
}

#[tokio::test]
async fn login_cookie_and_x_auth_token_reuse_match_kotlin_session_oracle() {
    let app = runtime_with_db_backed_auth_fixture().await;

    let (token, login_json) =
        login_header_exchange(&app, &format!("{DB_ADMIN_EMAIL}:{DB_ADMIN_PASSWORD}")).await;
    assert_eq!(login_json["email"], DB_ADMIN_EMAIL);

    let token_reuse = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/libraries")
                .header("X-Auth-Token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        token_reuse.status(),
        StatusCode::OK,
        "X-Auth-Token reuse should authenticate protected API routes",
    );

    let session_cookie = exchange_token_for_cookie(&app, &token).await;

    let cookie_reuse = app
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me")
                .header(header::COOKIE, format!("KOMGA-SESSION={session_cookie}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        cookie_reuse.status(),
        StatusCode::OK,
        "KOMGA-SESSION cookie reuse should authenticate /api/v2/users/me",
    );

    let cookie_json = response_json(cookie_reuse).await;
    assert_eq!(cookie_json["email"], DB_ADMIN_EMAIL);
}

#[tokio::test]
async fn api_key_login_bootstraps_kotlin_session_cookie_for_follow_up_requests() {
    let app = runtime_with_db_backed_auth_fixture().await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me")
                .header("X-API-Key", DB_API_KEY)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let set_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .expect("API key login should issue KOMGA-SESSION cookie")
        .to_str()
        .unwrap()
        .to_string();
    assert!(set_cookie.contains("KOMGA-SESSION="));
    assert!(set_cookie.contains("HttpOnly"));

    let session_cookie = extract_cookie_value(&set_cookie, "KOMGA-SESSION");
    let json = response_json(response).await;
    assert_eq!(json["email"], DB_ADMIN_EMAIL);

    let cookie_reuse = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/libraries")
                .header(header::COOKIE, format!("KOMGA-SESSION={session_cookie}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        cookie_reuse.status(),
        StatusCode::OK,
        "API key bootstrap cookie should remain reusable on protected routes",
    );
}

#[tokio::test]
async fn logout_clears_kotlin_session_cookie_and_invalidates_token_and_cookie_reuse() {
    let app = runtime_with_db_backed_auth_fixture().await;

    let (token, _) =
        login_header_exchange(&app, &format!("{DB_ADMIN_EMAIL}:{DB_ADMIN_PASSWORD}")).await;
    let session_cookie = exchange_token_for_cookie(&app, &token).await;

    let logout = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/logout")
                .header("X-Auth-Token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(logout.status(), StatusCode::NO_CONTENT);
    let cleared_cookie = logout
        .headers()
        .get(header::SET_COOKIE)
        .expect("logout should clear KOMGA-SESSION cookie")
        .to_str()
        .unwrap();
    assert!(cleared_cookie.contains("KOMGA-SESSION=;"));
    assert!(cleared_cookie.contains("Max-Age=0"));

    let token_after_logout = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/libraries")
                .header("X-Auth-Token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(token_after_logout.status(), StatusCode::UNAUTHORIZED);

    let cookie_after_logout = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/libraries")
                .header(header::COOKIE, format!("KOMGA-SESSION={session_cookie}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cookie_after_logout.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn login_logout_cookie_lifecycle_matches_contract() {
    let app = runtime_with_db_backed_auth_fixture().await;

    let (session_token, me_json) =
        login_header_exchange(&app, &format!("{DB_ADMIN_EMAIL}:{DB_ADMIN_PASSWORD}")).await;
    assert_eq!(me_json["email"], DB_ADMIN_EMAIL);

    let session_cookie = exchange_token_for_cookie(&app, &session_token).await;

    let authenticated_with_cookie = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/libraries")
                .header(header::COOKIE, format!("KOMGA-SESSION={session_cookie}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(authenticated_with_cookie.status(), StatusCode::OK);

    let logout = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/logout")
                .header("X-Auth-Token", &session_token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(logout.status(), StatusCode::NO_CONTENT);
    let cleared_cookie = logout
        .headers()
        .get(header::SET_COOKIE)
        .expect("logout should return a clearing KOMGA-SESSION cookie")
        .to_str()
        .unwrap();
    assert!(cleared_cookie.contains("KOMGA-SESSION=;"));
    assert!(cleared_cookie.contains("Max-Age=0"));

    let cookie_after_logout = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/libraries")
                .header(header::COOKIE, format!("KOMGA-SESSION={session_cookie}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cookie_after_logout.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn remember_me_survives_expected_restart_flow() {
    let paths =
        persistence_contract_fixture::new_legacy_db_paths("auth-session-db-backed-remember-me")
            .expect("auth session remember-me db paths should be created");
    persistence_contract_fixture::seed_main_db_from_flyway(&paths.main_db)
        .await
        .expect("main db flyway fixture should be created");
    persistence_contract_fixture::seed_tasks_db_from_flyway(&paths.tasks_db)
        .await
        .expect("tasks db flyway fixture should be created");

    seed_db_backed_auth_rows(&paths.main_db).await;
    assert_db_backed_auth_rows(&paths.main_db).await;

    fs::create_dir_all(paths.config_dir.join("lucene")).unwrap();
    fs::create_dir_all(paths.config_dir.join("fonts")).unwrap();

    let mut config = komga_rust::config::RuntimeConfig::for_compat_profile(
        komga_rust::config::CompatProfile::SnapshotAligned,
    );
    config.config_dir = Some(paths.config_dir.clone());
    config.log_file = paths.config_dir.join("komga.log");
    config.database_file = paths.main_db.clone();
    config.tasks_db_file = paths.tasks_db.clone();
    config.lucene_data_directory = paths.config_dir.join("lucene");
    config.fonts_data_directory = paths.config_dir.join("fonts");

    let app = komga_rust::app::build_router_with_config(&config);

    let remember_login = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me?remember-me=true")
                .header(
                    header::AUTHORIZATION,
                    basic_auth(&format!("{DB_ADMIN_EMAIL}:{DB_ADMIN_PASSWORD}")),
                )
                .header("X-Auth-Token", "")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(remember_login.status(), StatusCode::OK);
    let remember_cookie =
        extract_cookie_value_from_headers(remember_login.headers(), "komga-remember-me")
            .expect("remember-me login should issue a remember-me cookie");
    assert!(remember_cookie.starts_with("komga-remember-me-"));

    let remember_me_store = paths.config_dir.join("remember-me-tokens.json");
    let remember_me_store_contents = fs::read_to_string(&remember_me_store)
        .expect("remember-me tokens must be persisted to disk");
    assert!(
        remember_me_store_contents.contains(&remember_cookie),
        "remember-me token persistence must survive process/router restart semantics and be scoped to fixture config dir",
    );

    let global_store = std::env::temp_dir().join("remember-me-tokens.json");
    if global_store != remember_me_store
        && let Ok(global_store_contents) = fs::read_to_string(&global_store)
    {
        assert!(
            !global_store_contents.contains(&remember_cookie),
            "remember-me token must not leak into a global process-wide store",
        );
    }

    let app_after_restart = komga_rust::app::build_router_with_config(&config);
    let remember_reuse = app_after_restart
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me")
                .header(
                    header::COOKIE,
                    format!("komga-remember-me={remember_cookie}"),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        remember_reuse.status(),
        StatusCode::OK,
        "remember-me cookie should authenticate after restart-style router rebuild",
    );
    let remember_reuse_json = response_json(remember_reuse).await;
    assert_eq!(remember_reuse_json["email"], DB_ADMIN_EMAIL);
}

#[tokio::test]
async fn oauth_entrypoints_match_current_contract_when_enabled() {
    let app = runtime_with_db_backed_auth_fixture_and_oauth().await;

    let providers = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/oauth2/providers")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(providers.status(), StatusCode::OK);
    let providers_json = response_json(providers).await;
    let providers_array = providers_json
        .as_array()
        .expect("oauth2 providers should serialize as an array");
    assert_eq!(providers_array.len(), 1);
    assert_eq!(providers_array[0]["registrationId"], "oidc");

    let authorization_redirect = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/oauth2/authorization/oidc")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(authorization_redirect.status(), StatusCode::FOUND);
    let location = authorization_redirect
        .headers()
        .get(header::LOCATION)
        .expect("oauth2 authorization should provide redirect location")
        .to_str()
        .unwrap()
        .to_string();
    assert!(location.contains("https://id.example.org/oauth2/authorize"));
    assert!(location.contains("client_id=compat-client"));
    assert!(
        location.contains("redirect_uri=http%3A%2F%2F127.0.0.1%2Flogin%2Foauth2%2Fcode%2Foidc")
    );

    let login_code_callback = app
        .oneshot(
            Request::builder()
                .uri("/login/oauth2/code/oidc?code=sample-code&state=sample-state")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        login_code_callback.status(),
        StatusCode::FOUND,
        "oauth callback entrypoint should exist and fail closed with a login redirect",
    );
    let callback_location = login_code_callback
        .headers()
        .get(header::LOCATION)
        .expect("oauth callback should provide redirect location")
        .to_str()
        .unwrap()
        .to_string();
    assert!(callback_location.starts_with("/login?server_redirect=Y&error="));
}

#[tokio::test]
async fn db_backed_runtime_rejects_placeholder_credentials_on_protected_routes() {
    compat_auth_env::ensure_compat_auth_env();
    let app = runtime_with_db_backed_auth_fixture().await;

    let basic_auth_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/libraries")
                .header(header::AUTHORIZATION, basic_auth("admin@example.org:admin"))
                .header("X-Auth-Token", "")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        basic_auth_response.status(),
        StatusCode::UNAUTHORIZED,
        "db-backed runtime should not accept env-backed placeholder basic credentials on protected routes",
    );

    let api_key_response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/libraries")
                .header("X-API-Key", "compat-api-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        api_key_response.status(),
        StatusCode::UNAUTHORIZED,
        "db-backed runtime should not accept env-backed placeholder API keys on protected routes",
    );
}

#[tokio::test]
async fn user_v2_management_entrypoints_remain_available_for_unchanged_clients() {
    let app = runtime_with_db_backed_auth_fixture().await;
    let (admin_token, _) =
        login_header_exchange(&app, &format!("{DB_ADMIN_EMAIL}:{DB_ADMIN_PASSWORD}")).await;

    let list_users = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v2/users")
                .header("X-Auth-Token", &admin_token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        list_users.status(),
        StatusCode::OK,
        "admin clients still need GET /api/v2/users",
    );

    let list_users_json = response_json(list_users).await;
    let users = list_users_json
        .as_array()
        .expect("GET /api/v2/users should return a JSON array");
    assert!(
        users.iter().any(|user| user["email"] == DB_ADMIN_EMAIL),
        "admin users list should expose the current admin account",
    );
    assert!(
        users.iter().any(|user| user["email"] == DB_USER_EMAIL),
        "admin users list should expose unchanged-client secondary users",
    );
}

#[tokio::test]
async fn user_v2_management_allows_current_users_to_change_passwords_via_me_password() {
    let app = runtime_with_db_backed_auth_fixture().await;
    let (admin_token, _) =
        login_header_exchange(&app, &format!("{DB_ADMIN_EMAIL}:{DB_ADMIN_PASSWORD}")).await;

    let update_password = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v2/users/me/password")
                .header("X-Auth-Token", &admin_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(format!(
                    r#"{{"password":"{DB_ADMIN_UPDATED_PASSWORD}"}}"#
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        update_password.status(),
        StatusCode::NO_CONTENT,
        "unchanged clients should still be able to PATCH /api/v2/users/me/password",
    );

    assert_eq!(
        login_status(&app, &format!("{DB_ADMIN_EMAIL}:{DB_ADMIN_PASSWORD}")).await,
        StatusCode::UNAUTHORIZED,
        "old basic-auth credentials should stop working after current-user password changes",
    );

    let (_, login_json) = login_header_exchange(
        &app,
        &format!("{DB_ADMIN_EMAIL}:{DB_ADMIN_UPDATED_PASSWORD}"),
    )
    .await;
    assert_eq!(login_json["email"], DB_ADMIN_EMAIL);
}

#[tokio::test]
async fn user_v2_management_allows_admins_to_change_other_users_passwords_via_user_id_password() {
    let app = runtime_with_db_backed_auth_fixture().await;
    let (admin_token, _) =
        login_header_exchange(&app, &format!("{DB_ADMIN_EMAIL}:{DB_ADMIN_PASSWORD}")).await;

    let update_password = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/v2/users/{DB_USER_ID}/password"))
                .header("X-Auth-Token", &admin_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(format!(
                    r#"{{"password":"{DB_USER_UPDATED_PASSWORD}"}}"#
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        update_password.status(),
        StatusCode::NO_CONTENT,
        "admin clients should still be able to PATCH /api/v2/users/{{id}}/password for managed users",
    );

    let (_, login_json) =
        login_header_exchange(&app, &format!("{DB_USER_EMAIL}:{DB_USER_UPDATED_PASSWORD}")).await;
    assert_eq!(login_json["email"], DB_USER_EMAIL);
}

#[tokio::test]
async fn user_v2_management_supports_api_key_create_list_delete_round_trips_for_current_users() {
    let app = runtime_with_db_backed_auth_fixture().await;
    let (admin_token, _) =
        login_header_exchange(&app, &format!("{DB_ADMIN_EMAIL}:{DB_ADMIN_PASSWORD}")).await;

    let create_api_key = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v2/users/me/api-keys")
                .header("X-Auth-Token", &admin_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"comment":"contract api key"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        create_api_key.status(),
        StatusCode::OK,
        "unchanged clients should still be able to POST /api/v2/users/me/api-keys",
    );
    let create_api_key_json = response_json(create_api_key).await;
    let created_api_key_id = create_api_key_json["id"]
        .as_str()
        .expect("created API key should expose an id")
        .to_string();
    let created_api_key = create_api_key_json["key"]
        .as_str()
        .expect("created API key should expose the raw key once")
        .to_string();
    assert_eq!(create_api_key_json["userId"], DB_ADMIN_USER_ID);
    assert_eq!(create_api_key_json["comment"], "contract api key");
    assert!(
        !created_api_key.is_empty() && created_api_key.chars().any(|c| c != '*'),
        "new API keys should be returned in plain text on creation",
    );

    let api_key_login = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me")
                .header("X-API-Key", &created_api_key)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        api_key_login.status(),
        StatusCode::OK,
        "created API keys should authenticate unchanged clients immediately",
    );
    let api_key_login_json = response_json(api_key_login).await;
    assert_eq!(api_key_login_json["email"], DB_ADMIN_EMAIL);

    let list_api_keys = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me/api-keys")
                .header("X-Auth-Token", &admin_token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        list_api_keys.status(),
        StatusCode::OK,
        "unchanged clients should still be able to GET /api/v2/users/me/api-keys",
    );
    let list_api_keys_json = response_json(list_api_keys).await;
    let api_keys = list_api_keys_json
        .as_array()
        .expect("GET /api/v2/users/me/api-keys should return a JSON array");
    let created_api_key_entry = api_keys
        .iter()
        .find(|api_key| api_key["id"] == created_api_key_id)
        .expect("API key list should include the just-created API key");
    assert_eq!(created_api_key_entry["comment"], "contract api key");
    assert_eq!(created_api_key_entry["userId"], DB_ADMIN_USER_ID);
    assert_eq!(created_api_key_entry["key"], "******");

    let delete_api_key = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v2/users/me/api-keys/{created_api_key_id}"))
                .header("X-Auth-Token", &admin_token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        delete_api_key.status(),
        StatusCode::NO_CONTENT,
        "unchanged clients should still be able to DELETE /api/v2/users/me/api-keys/{{keyId}}",
    );

    let list_after_delete = app
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me/api-keys")
                .header("X-Auth-Token", &admin_token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list_after_delete.status(), StatusCode::OK);
    let list_after_delete_json = response_json(list_after_delete).await;
    let api_keys_after_delete = list_after_delete_json
        .as_array()
        .expect("API key list after delete should remain a JSON array");
    assert!(
        api_keys_after_delete
            .iter()
            .all(|api_key| api_key["id"] != created_api_key_id),
        "deleted API keys should disappear from GET /api/v2/users/me/api-keys",
    );
}

#[tokio::test]
async fn user_v2_management_lists_current_user_authentication_activity_for_unchanged_clients() {
    let app = runtime_with_db_backed_auth_fixture().await;
    let (admin_token, _) =
        login_header_exchange(&app, &format!("{DB_ADMIN_EMAIL}:{DB_ADMIN_PASSWORD}")).await;

    let activity = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me/authentication-activity?unpaged=true")
                .header("X-Auth-Token", &admin_token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        activity.status(),
        StatusCode::OK,
        "unchanged clients should still be able to GET /api/v2/users/me/authentication-activity",
    );

    let activity_json = response_json(activity).await;
    let activity_rows = activity_json["content"]
        .as_array()
        .expect("current-user authentication activity should serialize a page content array");
    assert_eq!(activity_json["pageable"]["unpaged"], true);
    assert_eq!(activity_rows[0]["userId"], DB_ADMIN_USER_ID);
    assert_eq!(activity_rows[0]["email"], DB_ADMIN_EMAIL);
    assert_eq!(activity_rows[0]["apiKeyId"], DB_API_KEY_ID);
    assert_eq!(activity_rows[0]["apiKeyComment"], DB_API_KEY);
    assert_eq!(activity_rows[0]["ip"], DB_AUTH_ACTIVITY_API_KEY_IP);
    assert_eq!(
        activity_rows[0]["userAgent"],
        DB_AUTH_ACTIVITY_API_KEY_USER_AGENT
    );
    assert_eq!(activity_rows[0]["success"], true);
    assert_eq!(activity_rows[0]["error"], Value::Null);
    assert_eq!(activity_rows[0]["source"], "API_KEY");
    assert_eq!(activity_rows[0]["dateTime"], "2099-01-02T03:04:05Z");
    assert!(
        activity_rows
            .iter()
            .all(|row| row["email"] != DB_USER_EMAIL),
        "current-user authentication activity should not leak other users' rows",
    );
}

#[tokio::test]
async fn user_v2_management_lists_admin_authentication_activity_for_unchanged_clients() {
    let app = runtime_with_db_backed_auth_fixture().await;
    let (admin_token, _) =
        login_header_exchange(&app, &format!("{DB_ADMIN_EMAIL}:{DB_ADMIN_PASSWORD}")).await;

    let activity = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/authentication-activity?unpaged=true")
                .header("X-Auth-Token", &admin_token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        activity.status(),
        StatusCode::OK,
        "admin clients should still be able to GET /api/v2/users/authentication-activity",
    );

    let activity_json = response_json(activity).await;
    let activity_rows = activity_json["content"]
        .as_array()
        .expect("admin authentication activity should serialize a page content array");
    assert_eq!(activity_json["pageable"]["unpaged"], true);
    assert_eq!(activity_rows[0]["dateTime"], "2099-01-02T03:04:05Z");
    assert!(
        activity_rows
            .iter()
            .any(|row| row["email"] == DB_ADMIN_EMAIL),
        "admin authentication activity should include admin rows",
    );
    assert!(
        activity_rows
            .iter()
            .any(|row| row["email"] == DB_USER_EMAIL),
        "admin authentication activity should include secondary-user rows",
    );
}

#[tokio::test]
async fn user_v2_management_returns_latest_authentication_activity_by_user_and_api_key() {
    let app = runtime_with_db_backed_auth_fixture().await;
    let (admin_token, _) =
        login_header_exchange(&app, &format!("{DB_ADMIN_EMAIL}:{DB_ADMIN_PASSWORD}")).await;

    let latest_activity = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/v2/users/{DB_ADMIN_USER_ID}/authentication-activity/latest?apikey_id={DB_API_KEY_ID}",
                ))
                .header("X-Auth-Token", &admin_token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        latest_activity.status(),
        StatusCode::OK,
        "unchanged clients should still be able to GET /api/v2/users/{{id}}/authentication-activity/latest with api key filters",
    );

    let latest_activity_json = response_json(latest_activity).await;
    assert_eq!(latest_activity_json["userId"], DB_ADMIN_USER_ID);
    assert_eq!(latest_activity_json["email"], DB_ADMIN_EMAIL);
    assert_eq!(latest_activity_json["apiKeyId"], DB_API_KEY_ID);
    assert_eq!(latest_activity_json["apiKeyComment"], DB_API_KEY);
    assert_eq!(latest_activity_json["ip"], DB_AUTH_ACTIVITY_API_KEY_IP);
    assert_eq!(
        latest_activity_json["userAgent"],
        DB_AUTH_ACTIVITY_API_KEY_USER_AGENT
    );
    assert_eq!(latest_activity_json["success"], true);
    assert_eq!(latest_activity_json["error"], Value::Null);
    assert_eq!(latest_activity_json["source"], "API_KEY");
    assert_eq!(latest_activity_json["dateTime"], "2099-01-02T03:04:05Z");
}

#[tokio::test]
async fn user_v2_management_returns_latest_authentication_activity_without_api_key_filter() {
    let app = runtime_with_db_backed_auth_fixture().await;
    let (admin_token, _) =
        login_header_exchange(&app, &format!("{DB_ADMIN_EMAIL}:{DB_ADMIN_PASSWORD}")).await;

    let latest_activity = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/v2/users/{DB_ADMIN_USER_ID}/authentication-activity/latest",
                ))
                .header("X-Auth-Token", &admin_token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        latest_activity.status(),
        StatusCode::OK,
        "unchanged clients should still be able to GET /api/v2/users/{{id}}/authentication-activity/latest without apikey_id",
    );

    let latest_activity_json = response_json(latest_activity).await;
    assert_eq!(latest_activity_json["userId"], DB_ADMIN_USER_ID);
    assert_eq!(latest_activity_json["email"], DB_ADMIN_EMAIL);
    assert_eq!(latest_activity_json["ip"], DB_AUTH_ACTIVITY_API_KEY_IP);
    assert_eq!(
        latest_activity_json["userAgent"],
        DB_AUTH_ACTIVITY_API_KEY_USER_AGENT
    );
    assert_eq!(latest_activity_json["success"], true);
    assert_eq!(latest_activity_json["error"], Value::Null);
    assert_eq!(latest_activity_json["source"], "API_KEY");
    assert_eq!(latest_activity_json["dateTime"], "2099-01-02T03:04:05Z");
}

#[tokio::test]
async fn user_v2_management_records_login_activity_for_latest_without_api_key_filter_when_seed_has_none()
 {
    let app = runtime_with_db_backed_auth_fixture_without_admin_activity().await;
    let (admin_token, _) =
        login_header_exchange(&app, &format!("{DB_ADMIN_EMAIL}:{DB_ADMIN_PASSWORD}")).await;

    let latest_activity = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/v2/users/{DB_ADMIN_USER_ID}/authentication-activity/latest",
                ))
                .header("X-Auth-Token", &admin_token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        latest_activity.status(),
        StatusCode::OK,
        "after successful basic login, latest authentication activity should be available without apikey_id",
    );

    let latest_activity_json = response_json(latest_activity).await;
    assert_eq!(latest_activity_json["userId"], DB_ADMIN_USER_ID);
    assert_eq!(latest_activity_json["email"], DB_ADMIN_EMAIL);
    assert_eq!(latest_activity_json["success"], true);
    assert_eq!(latest_activity_json["error"], Value::Null);
    assert_eq!(latest_activity_json["source"], "BASIC");
}
