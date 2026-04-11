use std::fmt::Write as _;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::http::{HeaderMap, header};
use axum_extra::extract::cookie::CookieJar;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use bcrypt::{DEFAULT_COST, hash as hash_bcrypt_password, verify as verify_bcrypt_password};
use komga_application::identity_access::{
    AuthOutcome, AuthUser, PersistedApiKey, PersistedApiKeyMetadata,
    PersistedAuthenticationActivity,
    invalidate_remember_me_token as invalidate_remember_me_session_token,
    invalidate_session_token as invalidate_active_session_token,
    invalidate_user_sessions as invalidate_all_user_sessions, issue_remember_me_token,
    issue_session_token, resolve_authenticated_user, user_payload_json,
};
use serde_json::Value;
use sha2::{Digest, Sha512};
use sqlx::Row;

use super::session_store::RememberMeRuntimeSettings;
use super::session_store::session_token_store;
use crate::sqlite::connect_pool;

static API_KEY_NONCE: AtomicU64 = AtomicU64::new(0);

pub fn auth_token_user(headers: &HeaderMap) -> Option<AuthUser> {
    let session_token = session_token_from_headers(headers);
    let remember_me_token = remember_me_token_from_headers(headers);
    resolve_authenticated_user(
        session_token_store(),
        session_token_store(),
        session_token.as_deref(),
        remember_me_token.as_deref(),
    )
}

pub fn session_token_for_user_with_runtime_key(user: &AuthUser, runtime_key: &str) -> String {
    issue_session_token(session_token_store(), user, runtime_key)
}

pub fn remember_me_token_for_user_with_runtime_key(
    user: &AuthUser,
    runtime_key: &str,
) -> Option<String> {
    issue_remember_me_token(session_token_store(), user, runtime_key)
}

pub fn sync_remember_me_runtime_database_file(runtime_key: &str, database_file: &Path) {
    session_token_store().sync_remember_me_database_path(runtime_key, database_file);
}

pub fn sync_remember_me_runtime_settings(runtime_key: &str, settings: RememberMeRuntimeSettings) {
    session_token_store().sync_remember_me_settings(
        runtime_key,
        settings.key.as_str(),
        settings.duration_days,
    );
}

pub fn remember_me_max_age_seconds(runtime_key: &str) -> u64 {
    session_token_store().remember_me_max_age_seconds(runtime_key)
}

pub fn invalidate_user_sessions(user_id: &str) {
    invalidate_all_user_sessions(session_token_store(), user_id)
}

pub fn invalidate_user_sessions_with_runtime_key(user_id: &str, runtime_key: &str) {
    session_token_store().invalidate_user_sessions_for_runtime_key(runtime_key, user_id);
}

pub fn invalidate_session_token(token: &str) {
    invalidate_active_session_token(session_token_store(), token)
}

pub fn invalidate_remember_me_token(token: &str) {
    invalidate_remember_me_session_token(session_token_store(), token)
}

pub async fn persisted_basic_user(
    headers: &HeaderMap,
    database_file: &Path,
) -> Option<AuthOutcome> {
    let Some((username, password)) = basic_credentials(headers) else {
        return Some(AuthOutcome::Missing);
    };

    let mut users = open_persisted_users(database_file).await?;
    let Some(user) = users
        .iter_mut()
        .find(|user| user.email.eq_ignore_ascii_case(&username))
    else {
        return Some(AuthOutcome::Invalid);
    };

    match verify_bcrypt_password(&password, &user.password) {
        Ok(true) => Some(AuthOutcome::Valid(Box::new(user.clone()))),
        Ok(false) | Err(_) => Some(AuthOutcome::Invalid),
    }
}

pub async fn persisted_api_key_user(
    headers: &HeaderMap,
    database_file: &Path,
) -> Option<AuthOutcome> {
    let Some(api_key) = api_key_header_value(headers) else {
        return Some(AuthOutcome::Missing);
    };

    persisted_api_key_user_by_token(api_key.as_str(), database_file).await
}

pub async fn persisted_api_key_user_by_token(
    api_key: &str,
    database_file: &Path,
) -> Option<AuthOutcome> {
    let api_key = api_key.trim();
    if api_key.is_empty() {
        return Some(AuthOutcome::Missing);
    }

    let mut users = open_persisted_users(database_file).await?;
    let api_key_hash = sha512_hex(api_key);
    let pool = match connect_pool(database_file, 1).await {
        Ok(pool) => pool,
        Err(_) => return None,
    };

    let row = sqlx::query("SELECT USER_ID FROM USER_API_KEY WHERE API_KEY = ? LIMIT 1")
        .bind(api_key_hash)
        .fetch_optional(&pool)
        .await;

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

    Some(AuthOutcome::Valid(Box::new(user.clone())))
}

pub async fn persisted_api_key_metadata(
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

    let row = row.ok()??;
    Some(PersistedApiKeyMetadata {
        id: row.get::<String, _>("ID"),
        comment: row.get::<String, _>("COMMENT"),
    })
}

pub async fn persisted_users(database_file: &Path) -> Option<Vec<AuthUser>> {
    open_persisted_users(database_file).await
}

pub async fn persisted_update_password_by_user_id(
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

    update.ok().map(|result| result.rows_affected() > 0)
}

pub async fn persisted_create_api_key(
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
    let normalized_comment = comment.trim();
    if normalized_comment.is_empty() {
        return None;
    }
    let pool = connect_pool(database_file, 1).await.ok()?;

    let insert =
        sqlx::query("INSERT INTO USER_API_KEY (ID, USER_ID, API_KEY, COMMENT) VALUES (?, ?, ?, ?)")
            .bind(&generated_id)
            .bind(user_id)
            .bind(generated_key_hash)
            .bind(normalized_comment)
            .execute(&pool)
            .await;

    insert.ok()?;

    let row = sqlx::query(
        "SELECT CREATED_DATE, LAST_MODIFIED_DATE FROM USER_API_KEY WHERE ID = ? AND USER_ID = ? LIMIT 1",
    )
    .bind(&generated_id)
    .bind(user_id)
    .fetch_optional(&pool)
    .await
    .ok()?;

    Some(PersistedApiKey {
        id: generated_id,
        user_id: user_id.to_string(),
        key: generated_key,
        comment: normalized_comment.to_string(),
        created_date: row.as_ref().map(|row| row.get::<String, _>("CREATED_DATE")),
        last_modified_date: row
            .as_ref()
            .map(|row| row.get::<String, _>("LAST_MODIFIED_DATE")),
    })
}

pub async fn persisted_api_key_comment_exists(
    database_file: &Path,
    user_id: &str,
    comment: &str,
) -> Option<bool> {
    if !database_file.exists() {
        return None;
    }

    let normalized_comment = comment.trim();
    if normalized_comment.is_empty() {
        return Some(false);
    }

    let pool = connect_pool(database_file, 1).await.ok()?;
    let row = sqlx::query(
        "SELECT 1 FROM USER_API_KEY WHERE USER_ID = ? AND LOWER(COMMENT) = LOWER(?) LIMIT 1",
    )
    .bind(user_id)
    .bind(normalized_comment)
    .fetch_optional(&pool)
    .await
    .ok()?;

    Some(row.is_some())
}

pub async fn persisted_list_api_keys(
    database_file: &Path,
    user_id: &str,
) -> Option<Vec<PersistedApiKey>> {
    if !database_file.exists() {
        return None;
    }

    let pool = connect_pool(database_file, 1).await.ok()?;
    let rows = sqlx::query(
        "SELECT ID, USER_ID, COMMENT, CREATED_DATE, LAST_MODIFIED_DATE FROM USER_API_KEY WHERE USER_ID = ? ORDER BY CREATED_DATE DESC, ID DESC",
    )
    .bind(user_id)
    .fetch_all(&pool)
    .await;

    let rows = rows.ok()?;
    Some(
        rows.into_iter()
            .map(|row| PersistedApiKey {
                id: row.get::<String, _>("ID"),
                user_id: row.get::<String, _>("USER_ID"),
                key: "******".to_string(),
                comment: row.get::<String, _>("COMMENT"),
                created_date: Some(row.get::<String, _>("CREATED_DATE")),
                last_modified_date: Some(row.get::<String, _>("LAST_MODIFIED_DATE")),
            })
            .collect(),
    )
}

pub async fn persisted_delete_api_key_by_id(
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

    delete.ok().map(|result| result.rows_affected() > 0)
}

pub async fn persisted_list_authentication_activity(
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

pub async fn persisted_cleanup_authentication_activity(database_file: &Path) -> Option<u64> {
    if !database_file.exists() {
        return Some(0);
    }

    let pool = connect_pool(database_file, 1).await.ok()?;
    let deleted = sqlx::query(
        "DELETE FROM AUTHENTICATION_ACTIVITY WHERE datetime(DATE_TIME) < datetime('now', '-1 month')",
    )
    .execute(&pool)
    .await;

    deleted.ok().map(|result| result.rows_affected())
}

pub async fn persisted_latest_authentication_activity_by_user_and_api_key(
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

pub async fn persisted_record_successful_authentication_activity(
    database_file: &Path,
    user: &AuthUser,
    source: &str,
    api_key_id: Option<&str>,
    api_key_comment: Option<&str>,
    ip: Option<&str>,
    user_agent: Option<&str>,
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
    .bind(ip)
    .bind(user_agent)
    .bind(true)
    .bind(Option::<String>::None)
    .bind(source)
    .bind(api_key_id)
    .bind(api_key_comment)
    .execute(&pool)
    .await;

    let insert = match insert_with_user_id {
        Ok(result) => Ok(result),
        Err(_) => sqlx::query(
            "INSERT INTO AUTHENTICATION_ACTIVITY (USER_ID, EMAIL, IP, USER_AGENT, SUCCESS, ERROR, DATE_TIME, SOURCE, API_KEY_ID, API_KEY_COMMENT) VALUES (?, ?, ?, ?, ?, ?, datetime('now'), ?, ?, ?)",
        )
        .bind(Option::<String>::None)
        .bind(user.email.as_str())
        .bind(ip)
        .bind(user_agent)
        .bind(true)
        .bind(Option::<String>::None)
        .bind(source)
        .bind(api_key_id)
        .bind(api_key_comment)
        .execute(&pool)
        .await,
    };

    insert.ok().map(|_| ())
}

pub async fn ensure_oauth_user(
    database_file: &Path,
    email: &str,
    allow_create: bool,
) -> Result<Option<AuthUser>, sqlx::Error> {
    if let Some(users) = persisted_users(database_file).await
        && let Some(user) = users
            .into_iter()
            .find(|user| auth_user_email_equals(user, email))
    {
        return Ok(Some(user));
    }

    if !allow_create {
        return Ok(None);
    }

    let normalized = email.trim().to_ascii_lowercase();
    let digest = <sha2::Sha256 as sha2::Digest>::digest(normalized.as_bytes());
    let digest_hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let user_id_value = format!("oauth2-{digest_hex}");
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let generated_password = format!("oauth2:{normalized}:{seed}");
    let password_hash = hash_bcrypt_password(generated_password.as_str(), DEFAULT_COST)
        .unwrap_or_else(|_| "oauth2-disabled-password".to_string());

    let pool = connect_pool(database_file, 1).await?;
    let insert_result = sqlx::query(
        "INSERT OR IGNORE INTO USER (ID, EMAIL, PASSWORD, SHARED_ALL_LIBRARIES) VALUES (?, ?, ?, ?)",
    )
    .bind(&user_id_value)
    .bind(email)
    .bind(password_hash)
    .bind(false)
    .execute(&pool)
    .await?;

    let created = insert_result.rows_affected() > 0;

    let persisted_user_id =
        sqlx::query("SELECT ID FROM USER WHERE lower(EMAIL) = lower(?) LIMIT 1")
            .bind(email)
            .fetch_optional(&pool)
            .await?
            .map(|row| row.get::<String, _>("ID"));

    if created && let Some(persisted_user_id) = persisted_user_id {
        for role in ["FILE_DOWNLOAD", "PAGE_STREAMING"] {
            sqlx::query("INSERT OR IGNORE INTO USER_ROLE (USER_ID, ROLE) VALUES (?, ?)")
                .bind(&persisted_user_id)
                .bind(role)
                .execute(&pool)
                .await?;
        }
    }

    Ok(persisted_users(database_file).await.and_then(|users| {
        users
            .into_iter()
            .find(|user| auth_user_email_equals(user, email))
    }))
}

pub async fn open_auth_pool(database_file: &Path) -> Result<sqlx::SqlitePool, sqlx::Error> {
    connect_pool(database_file, 1).await
}

fn session_token_from_headers(headers: &HeaderMap) -> Option<String> {
    x_auth_token(headers).or_else(|| session_cookie_token(headers))
}

fn remember_me_token_from_headers(headers: &HeaderMap) -> Option<String> {
    let jar = CookieJar::from_headers(headers);
    jar.get("komga-remember-me")
        .map(|cookie| cookie.value().trim().to_string())
        .filter(|value| !value.is_empty())
}

fn x_auth_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("X-Auth-Token")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.to_string())
}

fn session_cookie_token(headers: &HeaderMap) -> Option<String> {
    let jar = CookieJar::from_headers(headers);
    jar.get("KOMGA-SESSION")
        .map(|cookie| cookie.value().trim().to_string())
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

async fn open_persisted_users(database_file: &Path) -> Option<Vec<AuthUser>> {
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
        return None;
    };

    if user_rows.is_empty() {
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
            .filter(|role| role != "USER")
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
            (Some(age), Some(true)) => {
                Some(komga_application::identity_access::AuthUserAgeRestriction {
                    age,
                    restriction: "ALLOW_ONLY".to_string(),
                })
            }
            (Some(age), Some(false)) => {
                Some(komga_application::identity_access::AuthUserAgeRestriction {
                    age,
                    restriction: "EXCLUDE".to_string(),
                })
            }
            _ => None,
        };

        users.push(AuthUser {
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

    Some(users)
}

fn auth_user_email_equals(user: &AuthUser, email: &str) -> bool {
    user_payload_json(user)
        .get("email")
        .and_then(Value::as_str)
        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(email))
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
    digest[..24].to_string()
}
