use axum::http::{HeaderMap, header};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use bcrypt::{DEFAULT_COST, hash as hash_bcrypt_password, verify as verify_bcrypt_password};
use komga_persistence::sqlite::connect_pool;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha512};
use sqlx::Row;
use std::fmt::Write as _;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
const AUTH_USER_ID_ENV: &str = "KOMGA_RUST_AUTH_USER_ID";
const AUTH_USER_EMAIL_ENV: &str = "KOMGA_RUST_AUTH_USER_EMAIL";
const AUTH_USER_PASSWORD_ENV: &str = "KOMGA_RUST_AUTH_USER_PASSWORD";
const AUTH_USER_ROLES_ENV: &str = "KOMGA_RUST_AUTH_USER_ROLES";
const AUTH_USER_SHARED_ALL_ENV: &str = "KOMGA_RUST_AUTH_USER_SHARED_ALL_LIBRARIES";
const AUTH_USER_SHARED_IDS_ENV: &str = "KOMGA_RUST_AUTH_USER_SHARED_LIBRARY_IDS";
const AUTH_USER_LABELS_ALLOW_ENV: &str = "KOMGA_RUST_AUTH_USER_LABELS_ALLOW";
const AUTH_USER_LABELS_EXCLUDE_ENV: &str = "KOMGA_RUST_AUTH_USER_LABELS_EXCLUDE";
const AUTH_USER2_ID_ENV: &str = "KOMGA_RUST_AUTH_USER2_ID";
const AUTH_USER2_EMAIL_ENV: &str = "KOMGA_RUST_AUTH_USER2_EMAIL";
const AUTH_USER2_PASSWORD_ENV: &str = "KOMGA_RUST_AUTH_USER2_PASSWORD";
const AUTH_USER2_ROLES_ENV: &str = "KOMGA_RUST_AUTH_USER2_ROLES";
const AUTH_USER2_SHARED_ALL_ENV: &str = "KOMGA_RUST_AUTH_USER2_SHARED_ALL_LIBRARIES";
const AUTH_USER2_SHARED_IDS_ENV: &str = "KOMGA_RUST_AUTH_USER2_SHARED_LIBRARY_IDS";
const AUTH_USER2_LABELS_ALLOW_ENV: &str = "KOMGA_RUST_AUTH_USER2_LABELS_ALLOW";
const AUTH_USER2_LABELS_EXCLUDE_ENV: &str = "KOMGA_RUST_AUTH_USER2_LABELS_EXCLUDE";
const API_KEY_ENV: &str = "KOMGA_COMPAT_API_KEY";
static API_KEY_NONCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone)]
pub(in crate::app) struct PersistedApiKey {
    pub(in crate::app) id: String,
    pub(in crate::app) user_id: String,
    pub(in crate::app) key: String,
    pub(in crate::app) comment: String,
}

#[derive(Clone)]
pub(in crate::app) struct PersistedAuthenticationActivity {
    pub(in crate::app) user_id: Option<String>,
    pub(in crate::app) email: Option<String>,
    pub(in crate::app) ip: Option<String>,
    pub(in crate::app) user_agent: Option<String>,
    pub(in crate::app) success: bool,
    pub(in crate::app) error: Option<String>,
    pub(in crate::app) date_time: String,
    pub(in crate::app) source: Option<String>,
    pub(in crate::app) api_key_id: Option<String>,
    pub(in crate::app) api_key_comment: Option<String>,
}

#[derive(Clone)]
pub(in crate::app) struct PersistedApiKeyMetadata {
    pub(in crate::app) id: String,
    pub(in crate::app) comment: String,
}

#[derive(Clone)]
pub(in crate::app) struct PlaceholderUser {
    id: String,
    email: String,
    password: String,
    roles: Vec<String>,
    shared_all_libraries: bool,
    shared_library_ids: Vec<String>,
    labels_allow: Vec<String>,
    labels_exclude: Vec<String>,
    age_restriction: Option<UserAgeRestriction>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(in crate::app) struct PlaceholderUserSessionSnapshot {
    pub(in crate::app) id: String,
    pub(in crate::app) email: String,
    pub(in crate::app) roles: Vec<String>,
    pub(in crate::app) shared_all_libraries: bool,
    pub(in crate::app) shared_library_ids: Vec<String>,
    pub(in crate::app) labels_allow: Vec<String>,
    pub(in crate::app) labels_exclude: Vec<String>,
    pub(in crate::app) age_restriction: Option<PlaceholderUserAgeRestrictionSnapshot>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(in crate::app) struct PlaceholderUserAgeRestrictionSnapshot {
    pub(in crate::app) age: i64,
    pub(in crate::app) restriction: String,
}

#[derive(Clone)]
struct UserAgeRestriction {
    age: i64,
    restriction: &'static str,
}

impl PlaceholderUser {
    pub(super) fn id(&self) -> &str {
        self.id.as_str()
    }

    fn email(&self) -> &str {
        self.email.as_str()
    }

    fn password(&self) -> &str {
        self.password.as_str()
    }
}

pub(in crate::app) fn user_id(user: &PlaceholderUser) -> &str {
    user.id()
}

pub(in crate::app) enum AuthOutcome {
    Valid(PlaceholderUser),
    Invalid,
    Missing,
}

pub(in crate::app) fn basic_user(headers: &HeaderMap) -> AuthOutcome {
    let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    else {
        return AuthOutcome::Missing;
    };

    let value = value.trim();
    if value.is_empty() {
        return AuthOutcome::Missing;
    }

    let Some(encoded) = value.strip_prefix("Basic ") else {
        return AuthOutcome::Invalid;
    };

    let decoded = match STANDARD.decode(encoded) {
        Ok(decoded) => decoded,
        Err(_) => return AuthOutcome::Invalid,
    };

    let credentials = match String::from_utf8(decoded) {
        Ok(credentials) => credentials,
        Err(_) => return AuthOutcome::Invalid,
    };

    let Some((username, password)) = credentials.split_once(':') else {
        return AuthOutcome::Invalid;
    };

    let users = configured_users();
    if users.is_empty() {
        return AuthOutcome::Missing;
    }

    for user in users {
        if user.email() == username && user.password() == password {
            return AuthOutcome::Valid(user);
        }
    }

    AuthOutcome::Invalid
}

pub(in crate::app) fn api_key_user(headers: &HeaderMap) -> AuthOutcome {
    let Some(value) = headers
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())
    else {
        return AuthOutcome::Missing;
    };

    let value = value.trim();
    if value.is_empty() {
        return AuthOutcome::Invalid;
    }

    let Some(user) = configured_primary_user() else {
        return AuthOutcome::Missing;
    };

    match configured_api_key() {
        Some(api_key) if value == api_key => AuthOutcome::Valid(user),
        Some(_) => AuthOutcome::Invalid,
        None => AuthOutcome::Missing,
    }
}

pub(in crate::app) async fn persisted_basic_user(
    headers: &HeaderMap,
    database_file: &Path,
) -> Option<AuthOutcome> {
    let Some((username, password)) = basic_credentials(headers) else {
        return Some(basic_user(headers));
    };

    let mut users = open_persisted_users(database_file).await?;
    let Some(user) = users
        .iter_mut()
        .find(|user| user.email.eq_ignore_ascii_case(&username))
    else {
        return Some(AuthOutcome::Invalid);
    };

    match verify_bcrypt_password(&password, &user.password) {
        Ok(true) => Some(AuthOutcome::Valid(user.clone())),
        Ok(false) | Err(_) => Some(AuthOutcome::Invalid),
    }
}

pub(in crate::app) async fn persisted_api_key_user(
    headers: &HeaderMap,
    database_file: &Path,
) -> Option<AuthOutcome> {
    let Some(api_key) = api_key_header_value(headers) else {
        return Some(api_key_user(headers));
    };

    let mut users = open_persisted_users(database_file).await?;
    let api_key_hash = sha512_hex(&api_key);
    let pool = match connect_pool(database_file, 1).await {
        Ok(pool) => pool,
        Err(_) => return None,
    };

    let row = sqlx::query("SELECT USER_ID FROM USER_API_KEY WHERE API_KEY = ? LIMIT 1")
        .bind(api_key_hash)
        .fetch_optional(&pool)
        .await;

    pool.close().await;

    let Ok(row) = row else {
        return None;
    };
    let Some(row) = row else {
        return Some(AuthOutcome::Invalid);
    };

    let user_id = row.get::<String, _>("USER_ID");
    let Some(user) = users.iter_mut().find(|user| user.id == user_id) else {
        return Some(AuthOutcome::Invalid);
    };

    Some(AuthOutcome::Valid(user.clone()))
}

pub(in crate::app) async fn persisted_api_key_metadata(
    headers: &HeaderMap,
    database_file: &Path,
) -> Option<PersistedApiKeyMetadata> {
    if !database_file.exists() {
        return None;
    }

    let api_key = api_key_header_value(headers)?;
    let api_key_hash = sha512_hex(&api_key);
    let pool = connect_pool(database_file, 1).await.ok()?;
    let row = sqlx::query("SELECT ID, COMMENT FROM USER_API_KEY WHERE API_KEY = ? LIMIT 1")
        .bind(api_key_hash)
        .fetch_optional(&pool)
        .await;

    pool.close().await;

    let row = row.ok()??;
    Some(PersistedApiKeyMetadata {
        id: row.get::<String, _>("ID"),
        comment: row.get::<String, _>("COMMENT"),
    })
}

pub(in crate::app) fn user_is_admin(user: &PlaceholderUser) -> bool {
    user.roles.iter().any(|role| role == "ADMIN")
}

pub(in crate::app) fn user_shared_all_libraries(user: &PlaceholderUser) -> bool {
    user.shared_all_libraries
}

pub(in crate::app) fn user_shared_library_ids(user: &PlaceholderUser) -> &[String] {
    user.shared_library_ids.as_slice()
}

pub(in crate::app) fn placeholder_user_json(user: &PlaceholderUser) -> Value {
    json!({
        "id": user.id,
        "email": user.email,
        "roles": user.roles,
        "sharedAllLibraries": user.shared_all_libraries,
        "sharedLibrariesIds": user.shared_library_ids,
        "labelsAllow": user.labels_allow,
        "labelsExclude": user.labels_exclude,
        "ageRestriction": user.age_restriction.as_ref().map(|age_restriction| {
            json!({
                "age": age_restriction.age,
                "restriction": age_restriction.restriction,
            })
        }),
    })
}

pub(in crate::app) fn user_session_snapshot(
    user: &PlaceholderUser,
) -> PlaceholderUserSessionSnapshot {
    PlaceholderUserSessionSnapshot {
        id: user.id.clone(),
        email: user.email.clone(),
        roles: user.roles.clone(),
        shared_all_libraries: user.shared_all_libraries,
        shared_library_ids: user.shared_library_ids.clone(),
        labels_allow: user.labels_allow.clone(),
        labels_exclude: user.labels_exclude.clone(),
        age_restriction: user.age_restriction.as_ref().map(|age_restriction| {
            PlaceholderUserAgeRestrictionSnapshot {
                age: age_restriction.age,
                restriction: age_restriction.restriction.to_string(),
            }
        }),
    }
}

pub(in crate::app) fn user_from_session_snapshot(
    snapshot: &PlaceholderUserSessionSnapshot,
) -> PlaceholderUser {
    PlaceholderUser {
        id: snapshot.id.clone(),
        email: snapshot.email.clone(),
        password: String::new(),
        roles: snapshot.roles.clone(),
        shared_all_libraries: snapshot.shared_all_libraries,
        shared_library_ids: snapshot.shared_library_ids.clone(),
        labels_allow: snapshot.labels_allow.clone(),
        labels_exclude: snapshot.labels_exclude.clone(),
        age_restriction: snapshot
            .age_restriction
            .as_ref()
            .and_then(|age_restriction| {
                let restriction = match age_restriction.restriction.as_str() {
                    "ALLOW_ONLY" => "ALLOW_ONLY",
                    "EXCLUDE" => "EXCLUDE",
                    _ => return None,
                };
                Some(UserAgeRestriction {
                    age: age_restriction.age,
                    restriction,
                })
            }),
    }
}

pub(in crate::app) async fn persisted_users(database_file: &Path) -> Option<Vec<PlaceholderUser>> {
    open_persisted_users(database_file).await
}

pub(in crate::app) async fn persisted_update_password_by_user_id(
    database_file: &Path,
    user_id: &str,
    password: &str,
) -> Option<bool> {
    if !database_file.exists() {
        return None;
    }

    let hashed_password = hash_bcrypt_password(password, DEFAULT_COST).ok()?;
    let pool = connect_pool(database_file, 1).await.ok()?;
    let update = sqlx::query("UPDATE USER SET PASSWORD = ? WHERE ID = ?")
        .bind(hashed_password)
        .bind(user_id)
        .execute(&pool)
        .await;

    pool.close().await;
    update.ok().map(|result| result.rows_affected() > 0)
}

pub(in crate::app) async fn persisted_create_api_key(
    database_file: &Path,
    user_id: &str,
    comment: &str,
) -> Option<PersistedApiKey> {
    if !database_file.exists() {
        return None;
    }

    let generated_key = generated_api_key_secret(user_id);
    let generated_key_hash = sha512_hex(&generated_key);
    let generated_id = generated_api_key_id(user_id);
    let pool = connect_pool(database_file, 1).await.ok()?;

    let insert =
        sqlx::query("INSERT INTO USER_API_KEY (ID, USER_ID, API_KEY, COMMENT) VALUES (?, ?, ?, ?)")
            .bind(&generated_id)
            .bind(user_id)
            .bind(generated_key_hash)
            .bind(comment)
            .execute(&pool)
            .await;

    pool.close().await;
    insert.ok()?;

    Some(PersistedApiKey {
        id: generated_id,
        user_id: user_id.to_string(),
        key: generated_key,
        comment: comment.to_string(),
    })
}

pub(in crate::app) async fn persisted_list_api_keys(
    database_file: &Path,
    user_id: &str,
) -> Option<Vec<PersistedApiKey>> {
    if !database_file.exists() {
        return None;
    }

    let pool = connect_pool(database_file, 1).await.ok()?;
    let rows = sqlx::query("SELECT ID, USER_ID, COMMENT FROM USER_API_KEY WHERE USER_ID = ? ORDER BY CREATED_DATE DESC, ID DESC")
        .bind(user_id)
        .fetch_all(&pool)
        .await;

    pool.close().await;

    let rows = rows.ok()?;
    Some(
        rows.into_iter()
            .map(|row| PersistedApiKey {
                id: row.get::<String, _>("ID"),
                user_id: row.get::<String, _>("USER_ID"),
                key: "******".to_string(),
                comment: row.get::<String, _>("COMMENT"),
            })
            .collect(),
    )
}

pub(in crate::app) async fn persisted_delete_api_key_by_id(
    database_file: &Path,
    user_id: &str,
    api_key_id: &str,
) -> Option<bool> {
    if !database_file.exists() {
        return None;
    }

    let pool = connect_pool(database_file, 1).await.ok()?;
    let delete = sqlx::query("DELETE FROM USER_API_KEY WHERE ID = ? AND USER_ID = ?")
        .bind(api_key_id)
        .bind(user_id)
        .execute(&pool)
        .await;

    pool.close().await;
    delete.ok().map(|result| result.rows_affected() > 0)
}

pub(in crate::app) async fn persisted_list_authentication_activity(
    database_file: &Path,
    user_id: Option<&str>,
) -> Option<Vec<PersistedAuthenticationActivity>> {
    if !database_file.exists() {
        return None;
    }

    let pool = connect_pool(database_file, 1).await.ok()?;
    let rows = if let Some(user_id) = user_id {
        sqlx::query(
            "SELECT USER_ID, EMAIL, IP, USER_AGENT, SUCCESS, ERROR, DATE_TIME, SOURCE, API_KEY_ID, API_KEY_COMMENT FROM AUTHENTICATION_ACTIVITY WHERE USER_ID = ? ORDER BY DATE_TIME DESC",
        )
        .bind(user_id)
        .fetch_all(&pool)
        .await
    } else {
        sqlx::query(
            "SELECT USER_ID, EMAIL, IP, USER_AGENT, SUCCESS, ERROR, DATE_TIME, SOURCE, API_KEY_ID, API_KEY_COMMENT FROM AUTHENTICATION_ACTIVITY ORDER BY DATE_TIME DESC",
        )
        .fetch_all(&pool)
        .await
    };

    pool.close().await;

    let rows = rows.ok()?;
    Some(
        rows.into_iter()
            .map(|row| PersistedAuthenticationActivity {
                user_id: row.get::<Option<String>, _>("USER_ID"),
                email: row.get::<Option<String>, _>("EMAIL"),
                ip: row.get::<Option<String>, _>("IP"),
                user_agent: row.get::<Option<String>, _>("USER_AGENT"),
                success: row.get::<bool, _>("SUCCESS"),
                error: row.get::<Option<String>, _>("ERROR"),
                date_time: row.get::<String, _>("DATE_TIME"),
                source: row.get::<Option<String>, _>("SOURCE"),
                api_key_id: row.get::<Option<String>, _>("API_KEY_ID"),
                api_key_comment: row.get::<Option<String>, _>("API_KEY_COMMENT"),
            })
            .collect(),
    )
}

pub(in crate::app) async fn persisted_latest_authentication_activity_by_user_and_api_key(
    database_file: &Path,
    user_id: &str,
    api_key_id: &str,
) -> Option<PersistedAuthenticationActivity> {
    if !database_file.exists() {
        return None;
    }

    let pool = connect_pool(database_file, 1).await.ok()?;
    let row = sqlx::query(
        "SELECT USER_ID, EMAIL, IP, USER_AGENT, SUCCESS, ERROR, DATE_TIME, SOURCE, API_KEY_ID, API_KEY_COMMENT FROM AUTHENTICATION_ACTIVITY WHERE USER_ID = ? AND API_KEY_ID = ? ORDER BY DATE_TIME DESC LIMIT 1",
    )
    .bind(user_id)
    .bind(api_key_id)
    .fetch_optional(&pool)
    .await;

    pool.close().await;
    let row = row.ok()??;

    Some(PersistedAuthenticationActivity {
        user_id: row.get::<Option<String>, _>("USER_ID"),
        email: row.get::<Option<String>, _>("EMAIL"),
        ip: row.get::<Option<String>, _>("IP"),
        user_agent: row.get::<Option<String>, _>("USER_AGENT"),
        success: row.get::<bool, _>("SUCCESS"),
        error: row.get::<Option<String>, _>("ERROR"),
        date_time: row.get::<String, _>("DATE_TIME"),
        source: row.get::<Option<String>, _>("SOURCE"),
        api_key_id: row.get::<Option<String>, _>("API_KEY_ID"),
        api_key_comment: row.get::<Option<String>, _>("API_KEY_COMMENT"),
    })
}

pub(in crate::app) async fn persisted_record_successful_authentication_activity(
    database_file: &Path,
    user: &PlaceholderUser,
    source: &str,
    api_key_id: Option<&str>,
    api_key_comment: Option<&str>,
) -> Option<()> {
    if !database_file.exists() {
        return None;
    }

    let pool = connect_pool(database_file, 1).await.ok()?;
    let insert_with_user_id = sqlx::query(
        "INSERT INTO AUTHENTICATION_ACTIVITY (USER_ID, EMAIL, IP, USER_AGENT, SUCCESS, ERROR, DATE_TIME, SOURCE, API_KEY_ID, API_KEY_COMMENT) VALUES (?, ?, ?, ?, ?, ?, datetime('now'), ?, ?, ?)",
    )
    .bind(user.id.as_str())
    .bind(user.email.as_str())
    .bind(Option::<String>::None)
    .bind(Option::<String>::None)
    .bind(true)
    .bind(Option::<String>::None)
    .bind(source)
    .bind(api_key_id)
    .bind(api_key_comment)
    .execute(&pool)
    .await;

    let insert = match insert_with_user_id {
        Ok(result) => Ok(result),
        Err(_) => {
            sqlx::query(
                "INSERT INTO AUTHENTICATION_ACTIVITY (USER_ID, EMAIL, IP, USER_AGENT, SUCCESS, ERROR, DATE_TIME, SOURCE, API_KEY_ID, API_KEY_COMMENT) VALUES (?, ?, ?, ?, ?, ?, datetime('now'), ?, ?, ?)",
            )
            .bind(Option::<String>::None)
            .bind(user.email.as_str())
            .bind(Option::<String>::None)
            .bind(Option::<String>::None)
            .bind(true)
            .bind(Option::<String>::None)
            .bind(source)
            .bind(api_key_id)
            .bind(api_key_comment)
            .execute(&pool)
            .await
        }
    };

    pool.close().await;
    insert.ok().map(|_| ())
}

fn configured_primary_user() -> Option<PlaceholderUser> {
    load_user(
        AUTH_USER_ID_ENV,
        AUTH_USER_EMAIL_ENV,
        AUTH_USER_PASSWORD_ENV,
        AUTH_USER_ROLES_ENV,
        AUTH_USER_SHARED_ALL_ENV,
        AUTH_USER_SHARED_IDS_ENV,
        AUTH_USER_LABELS_ALLOW_ENV,
        AUTH_USER_LABELS_EXCLUDE_ENV,
        &["ADMIN", "FILE_DOWNLOAD", "PAGE_STREAMING", "USER"],
        true,
    )
}

fn configured_secondary_user() -> Option<PlaceholderUser> {
    load_user(
        AUTH_USER2_ID_ENV,
        AUTH_USER2_EMAIL_ENV,
        AUTH_USER2_PASSWORD_ENV,
        AUTH_USER2_ROLES_ENV,
        AUTH_USER2_SHARED_ALL_ENV,
        AUTH_USER2_SHARED_IDS_ENV,
        AUTH_USER2_LABELS_ALLOW_ENV,
        AUTH_USER2_LABELS_EXCLUDE_ENV,
        &["USER"],
        true,
    )
}

pub(in crate::app) fn configured_users() -> Vec<PlaceholderUser> {
    let mut users = Vec::with_capacity(2);
    if let Some(primary) = configured_primary_user() {
        users.push(primary);
    }
    if let Some(secondary) = configured_secondary_user() {
        users.push(secondary);
    }
    users
}

fn configured_api_key() -> Option<String> {
    load_configured_api_key()
}

fn load_user(
    id_env: &str,
    email_env: &str,
    password_env: &str,
    roles_env: &str,
    shared_all_env: &str,
    shared_ids_env: &str,
    labels_allow_env: &str,
    labels_exclude_env: &str,
    default_roles: &[&str],
    default_shared_all: bool,
) -> Option<PlaceholderUser> {
    let id = env_required(id_env)?;
    let email = env_required(email_env)?;
    let password = env_required(password_env)?;

    let fallback_roles = default_roles
        .iter()
        .map(|role| (*role).to_string())
        .collect::<Vec<_>>();

    Some(PlaceholderUser {
        id,
        email,
        password,
        roles: env_csv(roles_env)
            .filter(|roles| !roles.is_empty())
            .unwrap_or(fallback_roles),
        shared_all_libraries: env_bool_or_default(shared_all_env, default_shared_all),
        shared_library_ids: env_csv(shared_ids_env).unwrap_or_default(),
        labels_allow: env_csv(labels_allow_env).unwrap_or_default(),
        labels_exclude: env_csv(labels_exclude_env).unwrap_or_default(),
        age_restriction: None,
    })
}

fn load_configured_api_key() -> Option<String> {
    std::env::var(API_KEY_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn basic_credentials(headers: &HeaderMap) -> Option<(String, String)> {
    let value = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())?
        .trim();
    if value.is_empty() {
        return None;
    }

    let encoded = value.strip_prefix("Basic ")?;
    let decoded = STANDARD.decode(encoded).ok()?;
    let credentials = String::from_utf8(decoded).ok()?;
    credentials
        .split_once(':')
        .map(|(username, password)| (username.to_string(), password.to_string()))
}

fn api_key_header_value(headers: &HeaderMap) -> Option<String> {
    let value = headers
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())?;
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

async fn open_persisted_users(database_file: &Path) -> Option<Vec<PlaceholderUser>> {
    if !database_file.exists() {
        return None;
    }

    let pool = connect_pool(database_file, 1).await.ok()?;
    let user_rows = sqlx::query(
        "SELECT ID, EMAIL, PASSWORD, SHARED_ALL_LIBRARIES, AGE_RESTRICTION, AGE_RESTRICTION_ALLOW_ONLY FROM USER ORDER BY EMAIL",
    )
    .fetch_all(&pool)
    .await;

    let Ok(user_rows) = user_rows else {
        pool.close().await;
        return None;
    };

    if user_rows.is_empty() {
        pool.close().await;
        return None;
    }

    let mut users = Vec::with_capacity(user_rows.len());
    for row in user_rows {
        let user_id = row.get::<String, _>("ID");
        let roles = sqlx::query("SELECT ROLE FROM USER_ROLE WHERE USER_ID = ? ORDER BY ROLE")
            .bind(&user_id)
            .fetch_all(&pool)
            .await
            .ok()?
            .into_iter()
            .map(|row| row.get::<String, _>("ROLE"))
            .collect::<Vec<_>>();

        let shared_library_ids = sqlx::query(
            "SELECT LIBRARY_ID FROM USER_LIBRARY_SHARING WHERE USER_ID = ? ORDER BY LIBRARY_ID",
        )
        .bind(&user_id)
        .fetch_all(&pool)
        .await
        .ok()?
        .into_iter()
        .map(|row| row.get::<String, _>("LIBRARY_ID"))
        .collect::<Vec<_>>();

        let sharing_rows = sqlx::query(
            "SELECT LABEL, ALLOW FROM USER_SHARING WHERE USER_ID = ? ORDER BY ALLOW DESC, LABEL",
        )
        .bind(&user_id)
        .fetch_all(&pool)
        .await
        .ok()?;

        let labels_allow = sharing_rows
            .iter()
            .filter(|row| row.get::<bool, _>("ALLOW"))
            .map(|row| row.get::<String, _>("LABEL"))
            .collect::<Vec<_>>();

        let labels_exclude = sharing_rows
            .iter()
            .filter(|row| !row.get::<bool, _>("ALLOW"))
            .map(|row| row.get::<String, _>("LABEL"))
            .collect::<Vec<_>>();

        let age_restriction = match (
            row.get::<Option<i64>, _>("AGE_RESTRICTION"),
            row.get::<Option<bool>, _>("AGE_RESTRICTION_ALLOW_ONLY"),
        ) {
            (Some(age), Some(true)) => Some(UserAgeRestriction {
                age,
                restriction: "ALLOW_ONLY",
            }),
            (Some(age), Some(false)) => Some(UserAgeRestriction {
                age,
                restriction: "EXCLUDE",
            }),
            _ => None,
        };

        users.push(PlaceholderUser {
            id: user_id,
            email: row.get::<String, _>("EMAIL"),
            password: row.get::<String, _>("PASSWORD"),
            roles,
            shared_all_libraries: row.get::<bool, _>("SHARED_ALL_LIBRARIES"),
            shared_library_ids,
            labels_allow,
            labels_exclude,
            age_restriction,
        });
    }

    pool.close().await;
    Some(users)
}

fn sha512_hex(value: &str) -> String {
    let digest = Sha512::digest(value.as_bytes());
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(&mut encoded, "{byte:02x}");
    }
    encoded
}

fn generated_api_key_seed(user_id: &str) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let nonce = API_KEY_NONCE.fetch_add(1, Ordering::Relaxed);
    format!("{user_id}:{now}:{nonce}")
}

fn generated_api_key_secret(user_id: &str) -> String {
    sha512_hex(&format!(
        "komga-api-key:{}",
        generated_api_key_seed(user_id)
    ))
}

fn generated_api_key_id(user_id: &str) -> String {
    let digest = sha512_hex(&format!(
        "komga-api-key-id:{}",
        generated_api_key_seed(user_id)
    ));
    format!("rust-api-key-{}", &digest[..24])
}

fn env_required(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn env_csv(name: &str) -> Option<Vec<String>> {
    std::env::var(name).ok().map(|value| {
        value
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>()
    })
}

fn env_bool_or_default(name: &str, default: bool) -> bool {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .and_then(|value| match value.as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => None,
        })
        .unwrap_or(default)
}
