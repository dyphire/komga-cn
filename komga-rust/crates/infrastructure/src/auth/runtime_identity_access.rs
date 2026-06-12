use std::fmt::Write as _;
use std::path::Path;

use bcrypt::{DEFAULT_COST, hash as hash_bcrypt_password, verify as verify_bcrypt_password};
use komga_application::identity_access::{
    AuthOutcome, AuthUser, AuthUserRole, AuthenticationActivityApiKey, PersistedApiKey,
    PersistedApiKeyMetadata, PersistedAuthenticationActivity, ResolvedAuthToken,
    invalidate_remember_me_token as invalidate_remember_me_session_token,
    invalidate_session_token as invalidate_active_session_token,
    invalidate_user_sessions as invalidate_all_user_sessions, issue_remember_me_token,
    issue_session_token, resolve_authenticated_token, user_age_restriction_from_persisted_columns,
    user_roles_from_persisted_names,
};
use sha2::{Digest, Sha512};
use sqlx::{Row, SqlitePool};

use super::session_store::RememberMeRuntimeSettings;
use super::session_store::session_token_store;
use crate::random_hex_token;

pub(crate) fn auth_token_user_from_tokens(
    session_token: Option<&str>,
    remember_me_token: Option<&str>,
) -> Result<Option<AuthUser>, String> {
    auth_token_resolution_from_tokens(session_token, remember_me_token)
        .map(|resolved| resolved.map(|resolved| resolved.user))
}

pub(crate) fn auth_token_resolution_from_tokens(
    session_token: Option<&str>,
    remember_me_token: Option<&str>,
) -> Result<Option<ResolvedAuthToken>, String> {
    resolve_authenticated_token(
        session_token_store(),
        session_token_store(),
        session_token,
        remember_me_token,
    )
}

pub(crate) fn session_token_for_user_with_runtime_key(
    user: &AuthUser,
    runtime_key: &str,
) -> String {
    issue_session_token(session_token_store(), user, runtime_key)
}

pub(crate) fn remember_me_token_for_user_with_runtime_key(
    user: &AuthUser,
    runtime_key: &str,
) -> Option<String> {
    issue_remember_me_token(session_token_store(), user, runtime_key)
}

pub(crate) fn sync_session_runtime_settings(runtime_key: &str, max_inactive_seconds: u64) {
    session_token_store().sync_session_settings(runtime_key, max_inactive_seconds);
}

pub(crate) fn sync_remember_me_runtime_database_file(runtime_key: &str, database_file: &Path) {
    session_token_store().sync_remember_me_database_path(runtime_key, database_file);
}

pub(crate) fn sync_remember_me_runtime_settings(
    runtime_key: &str,
    settings: RememberMeRuntimeSettings,
) {
    session_token_store().sync_remember_me_settings(
        runtime_key,
        settings.key.as_str(),
        settings.duration_days,
    );
}

pub(crate) fn remember_me_max_age_seconds(runtime_key: &str) -> u64 {
    session_token_store().remember_me_max_age_seconds(runtime_key)
}

pub fn invalidate_user_sessions(user_id: &str) {
    invalidate_all_user_sessions(session_token_store(), user_id)
}

pub(crate) fn invalidate_user_sessions_with_runtime_key(user_id: &str, runtime_key: &str) {
    session_token_store().invalidate_user_sessions_for_runtime_key(runtime_key, user_id);
}

pub(crate) fn invalidate_session_token(token: &str) {
    invalidate_active_session_token(session_token_store(), token)
}

pub(crate) fn invalidate_remember_me_token(token: &str) {
    invalidate_remember_me_session_token(session_token_store(), token)
}

pub(crate) fn store_oauth2_authorization_state(
    runtime_key: &str,
    session_token: &str,
    registration_id: &str,
    state: &str,
) {
    session_token_store().store_oauth2_authorization_state(
        runtime_key,
        session_token,
        registration_id,
        state,
    );
}

pub(crate) fn take_oauth2_authorization_state(
    runtime_key: &str,
    session_token: &str,
    registration_id: &str,
) -> Option<String> {
    session_token_store().take_oauth2_authorization_state(
        runtime_key,
        session_token,
        registration_id,
    )
}

pub(crate) async fn authenticate_basic_credentials(
    pool: &SqlitePool,
    username: &str,
    password: &str,
) -> Result<AuthOutcome, String> {
    let mut users = load_persisted_users(pool).await?;
    let Some(user) = users
        .iter_mut()
        .find(|user| user.email.eq_ignore_ascii_case(username))
    else {
        return Ok(AuthOutcome::Invalid);
    };

    match verify_bcrypt_password(password, &user.password) {
        Ok(true) => Ok(AuthOutcome::Valid(Box::new(user.clone()))),
        Ok(false) => Ok(AuthOutcome::Invalid),
        Err(error) => Err(format!(
            "failed to verify persisted password hash for user {}: {error}",
            user.id
        )),
    }
}

pub(crate) async fn persisted_api_key_user_by_token(
    api_key: &str,
    pool: &SqlitePool,
) -> Result<AuthOutcome, String> {
    let api_key = api_key.trim();
    if api_key.is_empty() {
        return Ok(AuthOutcome::Missing);
    }

    let api_key_hash = sha512_hex(api_key);

    let row = sqlx::query("SELECT USER_ID FROM USER_API_KEY WHERE API_KEY = ? LIMIT 1")
        .bind(api_key_hash)
        .fetch_optional(pool)
        .await
        .map_err(|error| error.to_string())?;
    let Some(row) = row else {
        return Ok(AuthOutcome::Invalid);
    };

    let mut users = load_persisted_users(pool).await?;
    let user_id = row.get::<String, _>("USER_ID");
    let Some(user) = users.iter_mut().find(|user| user.id == user_id) else {
        return Ok(AuthOutcome::Invalid);
    };

    Ok(AuthOutcome::Valid(Box::new(user.clone())))
}

pub(crate) async fn persisted_api_key_metadata_by_token(
    api_key: &str,
    pool: &SqlitePool,
) -> Result<Option<PersistedApiKeyMetadata>, String> {
    let api_key_hash = sha512_hex(api_key);
    let row = sqlx::query("SELECT ID, COMMENT FROM USER_API_KEY WHERE API_KEY = ? LIMIT 1")
        .bind(api_key_hash)
        .fetch_optional(pool)
        .await
        .map_err(|error| error.to_string())?;

    let Some(row) = row else {
        return Ok(None);
    };

    Ok(Some(PersistedApiKeyMetadata {
        id: row.get::<String, _>("ID"),
        comment: row.get::<String, _>("COMMENT"),
    }))
}

pub(crate) async fn persisted_users(pool: &SqlitePool) -> Result<Vec<AuthUser>, String> {
    load_persisted_users(pool).await
}

pub async fn persisted_update_password_by_user_id(
    pool: &SqlitePool,
    user_id: &str,
    password: &str,
) -> Result<bool, String> {
    let hashed_password = hash_bcrypt_password(password, DEFAULT_COST)
        .map_err(|error| format!("failed to hash password: {error}"))?;
    let update = sqlx::query("UPDATE USER SET PASSWORD = ? WHERE ID = ?")
        .bind(hashed_password)
        .bind(user_id)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;

    Ok(update.rows_affected() > 0)
}

pub(crate) async fn persisted_create_api_key(
    pool: &SqlitePool,
    user_id: &str,
    comment: &str,
) -> Result<PersistedApiKey, String> {
    let generated_key = generated_api_key_secret();
    let generated_key_hash = sha512_hex(&generated_key);
    let generated_id = generated_api_key_id();
    let normalized_comment = comment.trim();
    if normalized_comment.is_empty() {
        return Err("api key comment must not be blank".to_string());
    }

    sqlx::query("INSERT INTO USER_API_KEY (ID, USER_ID, API_KEY, COMMENT) VALUES (?, ?, ?, ?)")
        .bind(&generated_id)
        .bind(user_id)
        .bind(generated_key_hash)
        .bind(normalized_comment)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;

    let row = sqlx::query(
        "SELECT CREATED_DATE, LAST_MODIFIED_DATE FROM USER_API_KEY WHERE ID = ? AND USER_ID = ? LIMIT 1",
    )
    .bind(&generated_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| "created api key row was not found".to_string())?;

    Ok(PersistedApiKey {
        id: generated_id,
        user_id: user_id.to_string(),
        key: generated_key,
        comment: normalized_comment.to_string(),
        created_date: Some(row.get::<String, _>("CREATED_DATE")),
        last_modified_date: Some(row.get::<String, _>("LAST_MODIFIED_DATE")),
    })
}

pub(crate) async fn persisted_api_key_comment_exists(
    pool: &SqlitePool,
    user_id: &str,
    comment: &str,
) -> Result<bool, String> {
    let normalized_comment = comment.trim();
    if normalized_comment.is_empty() {
        return Err("api key comment must not be blank".to_string());
    }

    let row = sqlx::query(
        "SELECT 1 FROM USER_API_KEY WHERE USER_ID = ? AND LOWER(COMMENT) = LOWER(?) LIMIT 1",
    )
    .bind(user_id)
    .bind(normalized_comment)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?;

    Ok(row.is_some())
}

pub(crate) async fn persisted_list_api_keys(
    pool: &SqlitePool,
    user_id: &str,
) -> Result<Vec<PersistedApiKey>, String> {
    let rows = sqlx::query(
        "SELECT ID, USER_ID, COMMENT, CREATED_DATE, LAST_MODIFIED_DATE FROM USER_API_KEY WHERE USER_ID = ? ORDER BY CREATED_DATE DESC, ID DESC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;

    Ok(rows
        .into_iter()
        .map(|row| PersistedApiKey {
            id: row.get::<String, _>("ID"),
            user_id: row.get::<String, _>("USER_ID"),
            key: "******".to_string(),
            comment: row.get::<String, _>("COMMENT"),
            created_date: Some(row.get::<String, _>("CREATED_DATE")),
            last_modified_date: Some(row.get::<String, _>("LAST_MODIFIED_DATE")),
        })
        .collect())
}

pub(crate) async fn persisted_delete_api_key_by_id(
    pool: &SqlitePool,
    user_id: &str,
    api_key_id: &str,
) -> Result<bool, String> {
    let delete = sqlx::query("DELETE FROM USER_API_KEY WHERE ID = ? AND USER_ID = ?")
        .bind(api_key_id)
        .bind(user_id)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;

    Ok(delete.rows_affected() > 0)
}

pub(crate) async fn persisted_list_authentication_activity(
    pool: &SqlitePool,
    user_id: Option<&str>,
) -> Result<Vec<PersistedAuthenticationActivity>, String> {
    let rows = if let Some(user_id) = user_id {
        sqlx::query(
            r#"
            SELECT
                USER_ID,
                EMAIL,
                IP,
                USER_AGENT,
                SUCCESS,
                ERROR,
                DATE_TIME,
                SOURCE,
                API_KEY_ID,
                API_KEY_COMMENT
            FROM AUTHENTICATION_ACTIVITY
            WHERE USER_ID = ?
            ORDER BY DATE_TIME DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(pool)
        .await
    } else {
        sqlx::query(
            r#"
            SELECT
                USER_ID,
                EMAIL,
                IP,
                USER_AGENT,
                SUCCESS,
                ERROR,
                DATE_TIME,
                SOURCE,
                API_KEY_ID,
                API_KEY_COMMENT
            FROM AUTHENTICATION_ACTIVITY
            ORDER BY DATE_TIME DESC
            "#,
        )
        .fetch_all(pool)
        .await
    };

    let rows = rows.map_err(|error| error.to_string())?;
    Ok(rows
        .into_iter()
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
        .collect())
}

pub(crate) async fn persisted_cleanup_authentication_activity(
    pool: &SqlitePool,
) -> Result<u64, String> {
    let deleted = sqlx::query(
        "DELETE FROM AUTHENTICATION_ACTIVITY WHERE datetime(DATE_TIME) < datetime('now', '-1 month')",
    )
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;

    Ok(deleted.rows_affected())
}

pub(crate) async fn persisted_record_successful_authentication_activity(
    pool: &SqlitePool,
    user: &AuthUser,
    source: &str,
    api_key: AuthenticationActivityApiKey<'_>,
    ip: Option<&str>,
    user_agent: Option<&str>,
) -> Option<()> {
    let insert_with_user_id = sqlx::query(
        r#"
        INSERT INTO AUTHENTICATION_ACTIVITY (
            USER_ID,
            EMAIL,
            IP,
            USER_AGENT,
            SUCCESS,
            ERROR,
            DATE_TIME,
            SOURCE,
            API_KEY_ID,
            API_KEY_COMMENT
        ) VALUES (?, ?, ?, ?, ?, ?, datetime('now'), ?, ?, ?)
        "#,
    )
    .bind(user.id.as_str())
    .bind(user.email.as_str())
    .bind(ip)
    .bind(user_agent)
    .bind(true)
    .bind(Option::<String>::None)
    .bind(source)
    .bind(api_key.id)
    .bind(api_key.comment)
    .execute(pool)
    .await;

    let insert = match insert_with_user_id {
        Ok(result) => Ok(result),
        Err(_) => {
            sqlx::query(
                r#"
            INSERT INTO AUTHENTICATION_ACTIVITY (
                USER_ID,
                EMAIL,
                IP,
                USER_AGENT,
                SUCCESS,
                ERROR,
                DATE_TIME,
                SOURCE,
                API_KEY_ID,
                API_KEY_COMMENT
            ) VALUES (?, ?, ?, ?, ?, ?, datetime('now'), ?, ?, ?)
            "#,
            )
            .bind(Option::<String>::None)
            .bind(user.email.as_str())
            .bind(ip)
            .bind(user_agent)
            .bind(true)
            .bind(Option::<String>::None)
            .bind(source)
            .bind(api_key.id)
            .bind(api_key.comment)
            .execute(pool)
            .await
        }
    };

    insert.ok().map(|_| ())
}

pub(crate) async fn persisted_record_failed_authentication_activity(
    pool: &SqlitePool,
    email: Option<&str>,
    source: &str,
    error: &str,
    ip: Option<&str>,
    user_agent: Option<&str>,
) -> Option<()> {
    let normalized_email = email
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let user_id = if let Some(email) = normalized_email.as_deref() {
        sqlx::query("SELECT ID FROM USER WHERE lower(EMAIL) = lower(?) LIMIT 1")
            .bind(email)
            .fetch_optional(pool)
            .await
            .ok()?
            .map(|row| row.get::<String, _>("ID"))
    } else {
        None
    };

    sqlx::query(
        r#"
        INSERT INTO AUTHENTICATION_ACTIVITY (
            USER_ID,
            EMAIL,
            IP,
            USER_AGENT,
            SUCCESS,
            ERROR,
            DATE_TIME,
            SOURCE,
            API_KEY_ID,
            API_KEY_COMMENT
        ) VALUES (?, ?, ?, ?, ?, ?, datetime('now'), ?, ?, ?)
        "#,
    )
    .bind(user_id)
    .bind(normalized_email)
    .bind(ip)
    .bind(user_agent)
    .bind(false)
    .bind(Some(error.to_string()))
    .bind(source)
    .bind(Option::<String>::None)
    .bind(Option::<String>::None)
    .execute(pool)
    .await
    .ok()
    .map(|_| ())
}

pub(crate) async fn ensure_oauth_user(
    pool: &SqlitePool,
    email: &str,
    allow_create: bool,
) -> Result<Option<AuthUser>, sqlx::Error> {
    if let Some(user) = persisted_users(pool)
        .await
        .map_err(sqlx::Error::Protocol)?
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
    let generated_password = random_hex_token(32);
    let password_hash =
        hash_bcrypt_password(generated_password.as_str(), DEFAULT_COST).map_err(|error| {
            sqlx::Error::Protocol(format!("failed to hash OAuth password: {error}"))
        })?;

    let insert_result = sqlx::query(
        "INSERT OR IGNORE INTO USER (ID, EMAIL, PASSWORD, SHARED_ALL_LIBRARIES) VALUES (?, ?, ?, ?)",
    )
    .bind(&user_id_value)
    .bind(email)
    .bind(password_hash)
    .bind(true)
    .execute(pool)
    .await?;

    let created = insert_result.rows_affected() > 0;

    let persisted_user_id =
        sqlx::query("SELECT ID FROM USER WHERE lower(EMAIL) = lower(?) LIMIT 1")
            .bind(email)
            .fetch_optional(pool)
            .await?
            .map(|row| row.get::<String, _>("ID"));

    if created && let Some(persisted_user_id) = persisted_user_id {
        for role in [
            AuthUserRole::FileDownload.persisted_name(),
            AuthUserRole::PageStreaming.persisted_name(),
        ] {
            sqlx::query("INSERT OR IGNORE INTO USER_ROLE (USER_ID, ROLE) VALUES (?, ?)")
                .bind(&persisted_user_id)
                .bind(role)
                .execute(pool)
                .await?;
        }
    }

    Ok(persisted_users(pool)
        .await
        .map_err(sqlx::Error::Protocol)?
        .into_iter()
        .find(|user| auth_user_email_equals(user, email)))
}

async fn load_persisted_users(pool: &SqlitePool) -> Result<Vec<AuthUser>, String> {
    let user_rows = sqlx::query(
        "SELECT ID, EMAIL, PASSWORD, SHARED_ALL_LIBRARIES, AGE_RESTRICTION, AGE_RESTRICTION_ALLOW_ONLY FROM USER ORDER BY EMAIL",
    )
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;

    let mut users = Vec::with_capacity(user_rows.len());
    for row in user_rows {
        let user_id = row.get::<String, _>("ID");
        let roles = sqlx::query("SELECT ROLE FROM USER_ROLE WHERE USER_ID = ? ORDER BY ROLE")
            .bind(&user_id)
            .fetch_all(pool)
            .await
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|row| row.get::<String, _>("ROLE"))
            .collect::<Vec<_>>();

        let shared_library_ids = sqlx::query(
            "SELECT LIBRARY_ID FROM USER_LIBRARY_SHARING WHERE USER_ID = ? ORDER BY LIBRARY_ID",
        )
        .bind(&user_id)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|row| row.get::<String, _>("LIBRARY_ID"))
        .collect::<Vec<_>>();

        let sharing_rows = sqlx::query(
            "SELECT LABEL, ALLOW FROM USER_SHARING WHERE USER_ID = ? ORDER BY ALLOW DESC, LABEL",
        )
        .bind(&user_id)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;

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

        let age_restriction = user_age_restriction_from_persisted_columns(
            row.get::<Option<i64>, _>("AGE_RESTRICTION"),
            row.get::<Option<bool>, _>("AGE_RESTRICTION_ALLOW_ONLY"),
        );

        users.push(AuthUser {
            id: user_id,
            email: row.get::<String, _>("EMAIL"),
            password: row.get::<String, _>("PASSWORD"),
            roles: user_roles_from_persisted_names(roles),
            shared_all_libraries: row.get::<bool, _>("SHARED_ALL_LIBRARIES"),
            shared_library_ids,
            labels_allow,
            labels_exclude,
            age_restriction,
        });
    }

    Ok(users)
}

fn auth_user_email_equals(user: &AuthUser, email: &str) -> bool {
    user.email.eq_ignore_ascii_case(email)
}

fn sha512_hex(value: &str) -> String {
    let digest = Sha512::digest(value.as_bytes());
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(&mut encoded, "{byte:02x}");
    }
    encoded
}

fn generated_api_key_secret() -> String {
    random_hex_token(64)
}

fn generated_api_key_id() -> String {
    random_hex_token(12)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::sqlite::connect_test_pool;
    use crate::test_support::BootstrappedBookFixture;
    use sqlx::SqlitePool;

    fn temp_db_path(case_id: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("komga-rust-auth-roles-{case_id}-{nanos}.sqlite"))
    }

    #[test]
    fn generated_api_key_credentials_are_opaque_hex_tokens() {
        let secret = generated_api_key_secret();
        let id = generated_api_key_id();

        assert_eq!(secret.len(), 128);
        assert_eq!(id.len(), 24);
        assert!(
            secret
                .chars()
                .all(|character| character.is_ascii_hexdigit())
        );
        assert!(id.chars().all(|character| character.is_ascii_hexdigit()));
    }

    async fn closed_test_pool(case_id: &str) -> (PathBuf, SqlitePool) {
        let db_path = temp_db_path(case_id);
        let pool = connect_test_pool(db_path.as_path(), 1)
            .await
            .expect("temporary sqlite db should open");
        pool.close().await;
        (db_path, pool)
    }

    #[tokio::test]
    async fn persisted_users_reports_storage_errors() {
        let (db_path, pool) = closed_test_pool("persisted-users-error").await;

        let result = persisted_users(&pool).await;

        assert!(
            result.is_err(),
            "storage failures must not be collapsed into an empty user list"
        );

        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn user_admin_password_update_reports_storage_errors() {
        let (db_path, pool) = closed_test_pool("password-update-error").await;

        let result = persisted_update_password_by_user_id(&pool, "user-1", "new-password").await;

        assert!(
            result.is_err(),
            "storage failures must not be collapsed into a missing user"
        );

        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn user_admin_api_key_delete_reports_storage_errors() {
        let (db_path, pool) = closed_test_pool("api-key-delete-error").await;

        let result = persisted_delete_api_key_by_id(&pool, "user-1", "api-key-1").await;

        assert!(
            result.is_err(),
            "storage failures must not be collapsed into a missing api key"
        );

        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn user_admin_api_key_list_reports_storage_errors() {
        let (db_path, pool) = closed_test_pool("api-key-list-error").await;

        let result = persisted_list_api_keys(&pool, "user-1").await;

        assert!(
            result.is_err(),
            "storage failures must not be collapsed into an empty api-key list"
        );

        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn user_admin_api_key_create_reports_storage_errors() {
        let (db_path, pool) = closed_test_pool("api-key-create-error").await;

        let result = persisted_create_api_key(&pool, "user-1", "device").await;

        assert!(
            result.is_err(),
            "storage failures must not be collapsed into an absent api key"
        );

        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn user_admin_api_key_comment_exists_reports_storage_errors() {
        let (db_path, pool) = closed_test_pool("api-key-comment-exists-error").await;

        let result = persisted_api_key_comment_exists(&pool, "user-1", "device").await;

        assert!(
            result.is_err(),
            "storage failures must not be collapsed into a non-existing comment"
        );

        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn user_admin_api_key_comment_exists_rejects_blank_comment() {
        let (db_path, pool) = closed_test_pool("api-key-comment-blank").await;

        let result = persisted_api_key_comment_exists(&pool, "user-1", " ").await;

        assert!(
            result.is_err(),
            "transport validation should own blank api-key comments"
        );

        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn ensure_oauth_user_does_not_use_predictable_basic_password_prefix() {
        let fixture = BootstrappedBookFixture::open("oauth-user-password-prefix").await;
        let email = format!("{}@example.com", "a".repeat(90));

        ensure_oauth_user(&fixture.pool, &email, true)
            .await
            .expect("OAuth user creation should not fail")
            .expect("OAuth user should be created");
        let predictable_prefix = format!("oauth2:{email}");
        let guessed_password = &predictable_prefix[..72];
        let outcome = authenticate_basic_credentials(&fixture.pool, &email, guessed_password)
            .await
            .expect("basic authentication should not fail");

        assert_eq!(outcome, AuthOutcome::Invalid);

        fixture.close().await;
    }

    #[tokio::test]
    async fn auth_activity_list_reports_storage_errors() {
        let (db_path, pool) = closed_test_pool("auth-activity-list-error").await;

        let result = persisted_list_authentication_activity(&pool, None).await;

        assert!(
            result.is_err(),
            "storage failures must not be collapsed into an empty activity list"
        );

        let _ = std::fs::remove_file(db_path);
    }
}
