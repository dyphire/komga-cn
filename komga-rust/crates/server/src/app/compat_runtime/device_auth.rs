use axum::Json;
use axum::body::Bytes;
use axum::extract::{Extension, Path, Query};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum_extra::extract::cookie::CookieJar;
use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD, STANDARD_NO_PAD},
};
use bcrypt::{DEFAULT_COST, hash as hash_bcrypt_password};
use flate2::read::GzDecoder;
use komga_persistence::sqlite::connect_pool;
use oauth2::{
    AuthUrl, ClientId, ClientSecret, CsrfToken, RedirectUrl, Scope, TokenUrl, basic::BasicClient,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::collections::HashMap;
use std::io::Read;
use std::path::Path as FsPath;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use time::OffsetDateTime;

use crate::app::runtime_auth::{
    AuthOutcome, AuthUser, auth_token_user, persisted_api_key_user_by_token, persisted_users,
    session_token_for_user_with_namespace, user_id, user_payload_json,
};

use super::OperationalState;
use crate::app::compat_runtime::AuthDatabaseState;
use crate::app::snapshots::request_base_url;

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct KoboDeviceAuthRequest {
    #[serde(default)]
    user_key: String,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct KoboDeviceAuthResponse {
    access_token: String,
    refresh_token: String,
    token_type: &'static str,
    tracking_id: String,
    user_key: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
struct KoreaderProgressPayload {
    document: String,
    percentage: f64,
    progress: String,
    device: String,
    device_id: String,
}

#[derive(Deserialize, Default)]
pub(in crate::app::compat_runtime) struct KoboBookFileQuery {
    convert_kepub: Option<bool>,
}

#[derive(Deserialize, Default)]
pub(in crate::app::compat_runtime) struct OAuth2CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

#[derive(Clone)]
struct PersistedBookMediaFile {
    file_name: String,
    media_type: String,
    file_path: PathBuf,
}

#[derive(Clone)]
struct PersistedReadProgressRecord {
    page: i64,
    completed: bool,
    created: String,
    last_modified: String,
    device_id: String,
    device_name: String,
    locator: Option<Vec<u8>>,
}

#[derive(Clone)]
struct KoreaderBookTarget {
    id: String,
    page_count: u64,
}

#[derive(Clone)]
struct KoboMetadataRecord {
    title: String,
    summary: String,
    release_date: Option<String>,
    language: String,
    file_size: u64,
    file_name: String,
}

pub(in crate::app::compat_runtime) async fn oauth2_authorization(
    Extension(state): Extension<OperationalState>,
    Path(registration_id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let Some(client) = state
        .oauth2_clients
        .iter()
        .find(|client| client.registration_id == registration_id)
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let Ok(auth_url) = AuthUrl::new(client.authorization_uri.clone()) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Ok(token_url) = TokenUrl::new(client.token_uri.clone()) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let base_url = request_base_url(&headers);
    let Ok(redirect_url) = RedirectUrl::new(format!(
        "{base_url}/login/oauth2/code/{}",
        client.registration_id
    )) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let oauth_client = BasicClient::new(ClientId::new(client.client_id.clone()))
        .set_client_secret(ClientSecret::new(client.client_secret.clone()))
        .set_auth_uri(auth_url)
        .set_token_uri(token_url)
        .set_redirect_uri(redirect_url);

    let (url, csrf_state) = oauth_client
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new("openid".to_string()))
        .add_scope(Scope::new("profile".to_string()))
        .add_scope(Scope::new("email".to_string()))
        .url();

    let state_cookie = oauth2_state_cookie(client.registration_id.as_str(), csrf_state.secret());

    (
        StatusCode::FOUND,
        [
            (
                header::LOCATION,
                HeaderValue::from_str(url.as_str()).unwrap_or_else(|_| {
                    HeaderValue::from_static(
                        "/login?server_redirect=Y&error=oauth2_invalid_redirect",
                    )
                }),
            ),
            (
                header::SET_COOKIE,
                HeaderValue::from_str(state_cookie.as_str()).unwrap_or_else(|_| {
                    HeaderValue::from_static("komga-oauth2-state=; Path=/; HttpOnly; SameSite=Lax")
                }),
            ),
        ],
    )
        .into_response()
}

pub(in crate::app::compat_runtime) async fn oauth2_login_code(
    Extension(state): Extension<OperationalState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    Path(registration_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<OAuth2CallbackQuery>,
) -> Response {
    let Some(client_config) = state
        .oauth2_clients
        .iter()
        .find(|client| client.registration_id == registration_id)
    else {
        return oauth2_login_error_redirect("oauth2_provider_not_found");
    };

    if let Some(error) = query.error.as_deref() {
        return oauth2_login_error_redirect(error);
    }

    let _ = query.state.as_deref();

    let Some(code) = query.code.as_deref() else {
        return oauth2_login_error_redirect("oauth2_missing_code");
    };

    let Some(received_state) = query.state.as_deref() else {
        return oauth2_login_error_redirect("oauth2_state_missing");
    };
    let Some(expected_state) = oauth2_state_from_headers(&headers, registration_id.as_str()) else {
        return oauth2_login_error_redirect("oauth2_state_missing");
    };
    if received_state != expected_state {
        return oauth2_login_error_redirect("oauth2_state_mismatch");
    }

    let base_url = request_base_url(&headers);
    let redirect_uri = format!("{base_url}/login/oauth2/code/{registration_id}");

    let token_payload = match exchange_oauth2_token(client_config, code, &redirect_uri).await {
        Ok(payload) => payload,
        Err(error) => return oauth2_login_error_redirect(error.as_str()),
    };

    let access_token = token_payload
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty());

    let Some(access_token) = access_token else {
        return oauth2_login_error_redirect("oauth2_missing_access_token");
    };

    let email = resolve_oauth2_email(client_config, &token_payload, access_token)
        .await
        .or_else(|| {
            token_payload
                .get("email")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        });
    let Some(email) = email else {
        return oauth2_login_error_redirect("ERR_1024");
    };

    let allow_create = oauth2_account_creation_enabled(&state).await;
    let user = match ensure_oauth_user(state.runtime.database_file.as_path(), &email, allow_create)
        .await
    {
        Ok(Some(user)) => user,
        Ok(None) => return oauth2_login_error_redirect("ERR_1025"),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let session_token =
        session_token_for_user_with_namespace(&user, auth_db.remember_me_namespace.as_str());
    oauth2_login_success_redirect(session_token.as_str())
}

fn oauth2_login_error_redirect(error: &str) -> Response {
    let redirect = format!("/login?server_redirect=Y&error={error}");
    (
        StatusCode::FOUND,
        [(
            header::LOCATION,
            HeaderValue::from_str(&redirect).unwrap_or_else(|_| {
                HeaderValue::from_static("/login?server_redirect=Y&error=oauth2_invalid_redirect")
            }),
        )],
    )
        .into_response()
}

fn oauth2_login_success_redirect(session_token: &str) -> Response {
    let session_cookie = format!("KOMGA-SESSION={session_token}; Path=/; HttpOnly; SameSite=Lax");

    (
        StatusCode::FOUND,
        [
            (
                header::LOCATION,
                HeaderValue::from_static("/?server_redirect=Y"),
            ),
            (
                header::SET_COOKIE,
                HeaderValue::from_str(&session_cookie).unwrap_or_else(|_| {
                    HeaderValue::from_static("KOMGA-SESSION=; Path=/; HttpOnly; SameSite=Lax")
                }),
            ),
            (
                HeaderName::from_static("x-auth-token"),
                HeaderValue::from_str(session_token)
                    .unwrap_or_else(|_| HeaderValue::from_static("")),
            ),
        ],
    )
        .into_response()
}

fn oauth2_state_cookie(registration_id: &str, state: &str) -> String {
    format!(
        "komga-oauth2-state-{registration_id}={state}; Path=/login/oauth2/code/{registration_id}; HttpOnly; SameSite=Lax"
    )
}

fn oauth2_state_from_headers(headers: &HeaderMap, registration_id: &str) -> Option<String> {
    let jar = CookieJar::from_headers(headers);
    let cookie_name = format!("komga-oauth2-state-{registration_id}");
    jar.get(cookie_name.as_str())
        .map(|cookie| cookie.value().to_string())
        .filter(|value| !value.trim().is_empty())
}

async fn exchange_oauth2_token(
    client: &crate::config::OAuth2ClientConfig,
    code: &str,
    redirect_uri: &str,
) -> Result<Value, String> {
    let http = Client::new();
    let form = reqwest::Url::parse_with_params(
        client.token_uri.as_str(),
        [
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", client.client_id.as_str()),
            ("client_secret", client.client_secret.as_str()),
        ],
    )
    .ok()
    .and_then(|url| url.query().map(str::to_string))
    .ok_or_else(|| "oauth2_token_exchange_failed".to_string())?;

    let response = http
        .post(client.token_uri.as_str())
        .header(header::ACCEPT.as_str(), "application/json")
        .header(
            header::CONTENT_TYPE.as_str(),
            "application/x-www-form-urlencoded",
        )
        .body(form)
        .send()
        .await
        .map_err(|_| "oauth2_token_exchange_failed".to_string())?;

    let status = response.status();
    let payload = response
        .json::<Value>()
        .await
        .map_err(|_| "oauth2_token_invalid_response".to_string())?;

    if !status.is_success() {
        let error_code = payload
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("oauth2_token_exchange_failed");
        return Err(error_code.to_string());
    }

    Ok(payload)
}

async fn resolve_oauth2_email(
    client: &crate::config::OAuth2ClientConfig,
    _token_payload: &Value,
    access_token: &str,
) -> Option<String> {
    let http = Client::new();
    let candidates = oauth2_userinfo_candidates(client);
    for endpoint in candidates {
        let request = http
            .get(endpoint.as_str())
            .bearer_auth(access_token)
            .header("User-Agent", "komga-rust/compat")
            .header(header::ACCEPT.as_str(), "application/json");

        let Ok(response) = request.send().await else {
            continue;
        };
        if !response.status().is_success() {
            continue;
        }

        let Ok(payload) = response.json::<Value>().await else {
            continue;
        };

        if let Some(email) = extract_email_from_userinfo_payload(&payload) {
            return Some(email);
        }
    }

    None
}

fn extract_email_from_userinfo_payload(payload: &Value) -> Option<String> {
    if let Some(email) = payload
        .get("email")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(email.to_string());
    }

    if let Some(email) = payload
        .get("preferred_username")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| value.contains('@'))
    {
        return Some(email.to_string());
    }

    if let Some(array) = payload.as_array() {
        let selected = array.iter().find(|entry| {
            entry
                .get("email")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.trim().is_empty())
                && entry
                    .get("primary")
                    .and_then(Value::as_bool)
                    .unwrap_or(true)
        });
        if let Some(email) = selected
            .and_then(|entry| entry.get("email"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Some(email.to_string());
        }
    }

    None
}

fn oauth2_userinfo_candidates(client: &crate::config::OAuth2ClientConfig) -> Vec<String> {
    let mut candidates = Vec::new();
    if let Ok(token_url) = reqwest::Url::parse(client.token_uri.as_str()) {
        let mut userinfo = token_url.clone();
        if let Some(mut segments) = userinfo.path_segments_mut().ok() {
            segments.pop_if_empty();
            segments.pop();
            segments.push("userinfo");
        }
        candidates.push(userinfo.to_string());
    }

    if let Ok(auth_url) = reqwest::Url::parse(client.authorization_uri.as_str()) {
        let mut userinfo = auth_url.clone();
        if let Some(mut segments) = userinfo.path_segments_mut().ok() {
            segments.pop_if_empty();
            segments.pop();
            segments.push("userinfo");
        }
        candidates.push(userinfo.to_string());

        if auth_url
            .host_str()
            .is_some_and(|host| host.contains("github.com"))
        {
            candidates.push("https://api.github.com/user/emails".to_string());
            candidates.push("https://api.github.com/user".to_string());
        }
    }

    candidates.sort();
    candidates.dedup();
    candidates
}

async fn oauth2_account_creation_enabled(state: &OperationalState) -> bool {
    if std::env::var("KOMGA_OAUTH2_ACCOUNT_CREATION")
        .ok()
        .is_some_and(|value| value.eq_ignore_ascii_case("true") || value == "1")
    {
        return true;
    }
    if std::env::var("KOMGA_OAUTH2_ACCOUNT_CREATION_ENABLED")
        .ok()
        .is_some_and(|value| value.eq_ignore_ascii_case("true") || value == "1")
    {
        return true;
    }

    let Ok(settings) = state.settings_store.load_map().await else {
        return false;
    };
    [
        "OAUTH2_ACCOUNT_CREATION",
        "oauth2AccountCreation",
        "oauth2.account.creation",
    ]
    .iter()
    .find_map(|key| settings.get(*key))
    .and_then(|value| value.as_ref())
    .is_some_and(|value| {
        value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("yes") || value == "1"
    })
}

async fn ensure_oauth_user(
    database_file: &FsPath,
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
    let mut hasher = Sha256::new();
    hasher.update(normalized.as_bytes());
    let digest = hasher.finalize();
    let digest_hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let user_id_value = format!("oauth2-{digest_hex}");
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let generated_password = format!("oauth2:{normalized}:{seed}");
    let password_hash = hash_bcrypt_password(generated_password.as_str(), DEFAULT_COST)
        .unwrap_or_else(|_| "oauth2-disabled-password".to_string());

    let pool = connect_pool(database_file, 1).await?;
    let insert_result = sqlx::query(
        "INSERT \
         OR IGNORE INTO USER (ID, EMAIL, PASSWORD, SHARED_ALL_LIBRARIES) \
         VALUES (?, ?, ?, ?)",
    )
    .bind(&user_id_value)
    .bind(email)
    .bind(password_hash)
    .bind(false)
    .execute(&pool)
    .await?;

    let created = insert_result.rows_affected() > 0;

    let persisted_user_id = sqlx::query(
        "SELECT ID \
                     FROM USER \
                     WHERE lower(EMAIL) = lower(?) \
                     LIMIT 1",
    )
    .bind(email)
    .fetch_optional(&pool)
    .await?
    .map(|row| row.get::<String, _>("ID"));

    if created && let Some(persisted_user_id) = persisted_user_id {
        for role in ["FILE_DOWNLOAD", "PAGE_STREAMING"] {
            sqlx::query(
                "INSERT \
                         OR IGNORE INTO USER_ROLE (USER_ID, ROLE) \
                         VALUES (?, ?)",
            )
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

fn auth_user_email_equals(user: &AuthUser, email: &str) -> bool {
    user_payload_json(user)
        .get("email")
        .and_then(Value::as_str)
        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(email))
}

async fn load_effective_kepubify_path(
    state: &OperationalState,
    database_file: &FsPath,
    runtime_kepubify_path: Option<&FsPath>,
) -> Option<PathBuf> {
    if database_file.exists()
        && let Ok(settings) = state.settings_store.load_map().await
        && let Some(value) = settings.get("KEPUBIFY_PATH")
        && let Some(value) = value.as_ref()
        && !value.trim().is_empty()
    {
        return Some(PathBuf::from(value.trim()));
    }

    runtime_kepubify_path.map(PathBuf::from)
}

fn convert_epub_to_kepub_bytes(kepubify_path: &FsPath, input_file: &FsPath) -> Option<Vec<u8>> {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let output_dir = std::env::temp_dir().join(format!("komga-kobo-kepubify-{suffix}"));
    if std::fs::create_dir_all(&output_dir).is_err() {
        return None;
    }

    let converted = if run_kepubify_to_directory(kepubify_path, input_file, &output_dir) {
        find_first_epub_file(&output_dir).and_then(|path| std::fs::read(path).ok())
    } else {
        None
    };

    let _ = std::fs::remove_dir_all(output_dir);
    converted
}

fn run_kepubify_to_directory(
    kepubify_path: &FsPath,
    input_file: &FsPath,
    output_dir: &FsPath,
) -> bool {
    let attempts = [
        vec![
            "-o".to_string(),
            output_dir.to_string_lossy().to_string(),
            input_file.to_string_lossy().to_string(),
        ],
        vec![
            "--output".to_string(),
            output_dir.to_string_lossy().to_string(),
            input_file.to_string_lossy().to_string(),
        ],
        vec![
            input_file.to_string_lossy().to_string(),
            "-o".to_string(),
            output_dir.to_string_lossy().to_string(),
        ],
        vec![
            input_file.to_string_lossy().to_string(),
            "--output".to_string(),
            output_dir.to_string_lossy().to_string(),
        ],
    ];

    for args in attempts {
        if Command::new(kepubify_path)
            .args(args)
            .status()
            .is_ok_and(|status| status.success())
        {
            return true;
        }
    }

    false
}

fn find_first_epub_file(root: &FsPath) -> Option<PathBuf> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        let entries = std::fs::read_dir(path).ok()?;
        for entry in entries.flatten() {
            let candidate = entry.path();
            if candidate.is_dir() {
                stack.push(candidate);
                continue;
            }
            if candidate
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("epub"))
            {
                return Some(candidate);
            }
        }
    }

    None
}

fn kobo_kepub_file_name(file_name: &str) -> String {
    if let Some((base, ext)) = file_name.rsplit_once('.')
        && ext.eq_ignore_ascii_case("epub")
    {
        return format!("{base}.kepub.epub");
    }
    format!("{file_name}.kepub.epub")
}

pub(in crate::app::compat_runtime) async fn kobo_ping(
    Extension(auth_db): Extension<super::AuthDatabaseState>,
    Path(auth_token): Path<String>,
    headers: HeaderMap,
) -> Response {
    if resolved_kobo_user(
        auth_token.as_str(),
        &headers,
        auth_db.database_file.as_path(),
    )
    .await
    .is_none()
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    "pong".into_response()
}

pub(in crate::app::compat_runtime) async fn kobo_initialization(
    Extension(auth_db): Extension<super::AuthDatabaseState>,
    Path(auth_token): Path<String>,
    headers: HeaderMap,
) -> Response {
    let Some(user) = resolved_kobo_user(
        auth_token.as_str(),
        &headers,
        auth_db.database_file.as_path(),
    )
    .await
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let base_url = request_base_url(&headers);
    let mut response = (
        StatusCode::OK,
        Json(json!({
            "Resources": {
                "device_auth": format!("/kobo/{auth_token}/v1/auth/device"),
                "library_sync": format!("/kobo/{auth_token}/v1/library/sync"),
                "image_host": base_url,
                "image_url_template": format!("/kobo/{auth_token}/v1/books/{{ImageId}}/thumbnail/{{Width}}/{{Height}}/false/image.jpg"),
                "image_url_quality_template": format!("/kobo/{auth_token}/v1/books/{{ImageId}}/thumbnail/{{Width}}/{{Height}}/{{Quality}}/{{IsGreyscale}}/image.jpg"),
            }
        })),
    )
    .into_response();
    let api_token = generated_kobo_api_token(auth_token.as_str(), user_id(&user));
    response.headers_mut().insert(
        HeaderName::from_static("x-kobo-apitoken"),
        HeaderValue::from_str(api_token.as_str()).unwrap_or_else(|_| HeaderValue::from_static("")),
    );
    response
}

pub(in crate::app::compat_runtime) async fn kobo_auth_device(
    Extension(auth_db): Extension<super::AuthDatabaseState>,
    Path(auth_token): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if resolved_kobo_user(
        auth_token.as_str(),
        &headers,
        auth_db.database_file.as_path(),
    )
    .await
    .is_none()
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let payload =
        serde_json::from_slice::<KoboDeviceAuthRequest>(&body).unwrap_or(KoboDeviceAuthRequest {
            user_key: String::new(),
        });
    let (access_token, refresh_token, tracking_id) =
        generated_kobo_token_triplet(payload.user_key.as_str());

    Json(KoboDeviceAuthResponse {
        access_token,
        refresh_token,
        token_type: "Bearer",
        tracking_id,
        user_key: payload.user_key,
    })
    .into_response()
}

fn generated_kobo_token_triplet(user_key: &str) -> (String, String, String) {
    let key = user_key.trim();
    let normalized = if key.is_empty() {
        "anonymous".to_string()
    } else {
        sanitize_identifier(key)
    };
    let access = random_hex(24);
    let refresh = random_hex(24);
    let tracking = random_uuid_like();

    (
        format!("kobo-{normalized}-{access}"),
        format!("kobo-{normalized}-{refresh}"),
        tracking,
    )
}

fn generated_kobo_api_token(auth_token: &str, authenticated_user_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(auth_token.trim().as_bytes());
    hasher.update(b":");
    hasher.update(authenticated_user_id.trim().as_bytes());
    let digest = hasher.finalize();
    format!("KOMGA.{}", STANDARD_NO_PAD.encode(digest))
}

fn random_hex(len: usize) -> String {
    let mut bytes = vec![0u8; len.div_ceil(2)];
    if let Ok(mut file) = std::fs::File::open("/dev/urandom") {
        let _ = file.read_exact(&mut bytes);
    } else {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .to_string();
        let mut hasher = Sha256::new();
        hasher.update(nanos.as_bytes());
        let digest = hasher.finalize();
        let copy_len = bytes.len().min(digest.len());
        bytes[..copy_len].copy_from_slice(&digest[..copy_len]);
    }

    let hex = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    hex.chars().take(len).collect()
}

fn random_uuid_like() -> String {
    let mut bytes = [0u8; 16];
    if let Ok(mut file) = std::fs::File::open("/dev/urandom") {
        let _ = file.read_exact(&mut bytes);
    } else {
        let seed = random_hex(32);
        let seed_bytes = seed.as_bytes();
        for (idx, byte) in bytes.iter_mut().enumerate() {
            *byte = seed_bytes[idx % seed_bytes.len()];
        }
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15],
    )
}

pub(in crate::app::compat_runtime) async fn kobo_library_sync(
    Extension(state): Extension<OperationalState>,
    Path(auth_token): Path<String>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Response {
    let Some(current_user) = resolved_kobo_user(
        auth_token.as_str(),
        &headers,
        state.runtime.database_file.as_path(),
    )
    .await
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let user_id_value = user_id(&current_user).to_string();
    let sync_token_raw = kobo_sync_token_from_request(&headers, &uri);
    let sync_token_payload = sync_token_raw
        .as_deref()
        .and_then(parse_komga_sync_token_payload);

    let ongoing_sync_point_id = if let Some(id) = sync_token_payload
        .as_ref()
        .and_then(|token| token.ongoing_sync_point_id.clone())
    {
        if load_sync_point_state(state.runtime.database_file.as_path(), &id, &user_id_value)
            .await
            .is_some()
        {
            Some(id)
        } else {
            None
        }
    } else {
        None
    };
    let last_successful_sync_point_id = if let Some(id) = sync_token_payload
        .as_ref()
        .and_then(|token| token.last_successful_sync_point_id.clone())
    {
        if load_sync_point_state(state.runtime.database_file.as_path(), &id, &user_id_value)
            .await
            .is_some()
        {
            Some(id)
        } else {
            None
        }
    } else {
        None
    };
    let from_sync_point_id = last_successful_sync_point_id.clone();

    let to_sync_point_id = ongoing_sync_point_id
        .clone()
        .unwrap_or_else(random_uuid_like);
    let mut to_sync_point_state = if let Some(state_entry) = load_sync_point_state(
        state.runtime.database_file.as_path(),
        to_sync_point_id.as_str(),
        &user_id_value,
    )
    .await
    {
        state_entry
    } else {
        KoboSyncPointState {
            user_id: user_id_value.clone(),
            marker: now_sync_marker(),
            cursor: 0,
            from_marker: if let Some(sync_id) = from_sync_point_id.as_ref() {
                load_sync_point_marker(
                    state.runtime.database_file.as_path(),
                    sync_id,
                    &user_id_value,
                )
                .await
            } else {
                None
            }
            .or(sync_token_raw.clone()),
            snapshot: None,
        }
    };

    if to_sync_point_state.from_marker.is_none() {
        let marker = if let Some(sync_id) = from_sync_point_id.as_ref() {
            load_sync_point_marker(
                state.runtime.database_file.as_path(),
                sync_id,
                &user_id_value,
            )
            .await
        } else {
            None
        };
        to_sync_point_state.from_marker = marker.or(sync_token_raw.clone());
    }

    if to_sync_point_state.snapshot.is_none() {
        to_sync_point_state.snapshot =
            match load_kobo_sync_snapshot(state.runtime.database_file.as_path(), &user_id_value)
                .await
            {
                Ok(snapshot) => Some(snapshot),
                Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            };
    }

    let from_sync_snapshot = if let Some(sync_id) = from_sync_point_id.as_ref() {
        load_sync_point_state(
            state.runtime.database_file.as_path(),
            sync_id,
            &user_id_value,
        )
        .await
        .and_then(|sync_state| sync_state.snapshot)
    } else {
        None
    };

    let base_url = request_base_url(&headers);
    let events = build_kobo_sync_events(
        from_sync_snapshot.as_ref(),
        to_sync_point_state
            .snapshot
            .as_ref()
            .expect("snapshot initialized"),
        base_url.as_str(),
        auth_token.as_str(),
    );

    let start_index = to_sync_point_state.cursor.min(events.len());
    let end_index = (start_index + KOBO_SYNC_ITEM_LIMIT).min(events.len());
    let response_events = events[start_index..end_index].to_vec();
    let should_continue = end_index < events.len();

    to_sync_point_state.cursor = if should_continue { end_index } else { 0 };
    if save_sync_point(
        state.runtime.database_file.as_path(),
        to_sync_point_id.as_str(),
        &to_sync_point_state,
    )
    .await
    .is_err()
    {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    let kobo_store_sync_enabled = load_kobo_proxy_enabled(&state).await;
    let mut merged_events = response_events;
    let mut merged_should_continue = should_continue;
    let mut merged_raw_kobo_sync_token = sync_token_payload
        .as_ref()
        .map(|payload| payload.raw_kobo_sync_token.clone())
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            sync_token_raw
                .as_deref()
                .filter(|value| is_kobo_store_sync_token_candidate(value))
                .map(str::to_string)
        });

    if !should_continue
        && kobo_store_sync_enabled
        && let Some(raw_store_sync_token) = merged_raw_kobo_sync_token
            .as_deref()
            .filter(|value| is_kobo_store_sync_token_candidate(value))
        && let Ok(store_response) =
            proxy_kobo_store_library_sync(&headers, &uri, raw_store_sync_token).await
    {
        merged_events.extend(store_response.events);
        merged_should_continue = store_response.should_continue;
        if let Some(raw_store_sync_token) = store_response.raw_sync_token
            && !raw_store_sync_token.trim().is_empty()
        {
            merged_raw_kobo_sync_token = Some(raw_store_sync_token);
        }
    }

    if !merged_should_continue
        && let Some(from_sync_point_id) = from_sync_point_id
        && from_sync_point_id != to_sync_point_id
    {
        if remove_sync_point(
            state.runtime.database_file.as_path(),
            from_sync_point_id.as_str(),
        )
        .await
        .is_err()
        {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }

    let sync_token_payload_sanitized = sync_token_payload.map(|mut payload| {
        payload.ongoing_sync_point_id = ongoing_sync_point_id;
        if let Some(raw) = merged_raw_kobo_sync_token.as_ref() {
            payload.raw_kobo_sync_token = raw.clone();
        }
        payload
    });
    let sync_token_response = build_komga_sync_token_payload(
        sync_token_payload_sanitized,
        merged_raw_kobo_sync_token,
        to_sync_point_id.as_str(),
        merged_should_continue,
    );
    let encoded_sync_token = format!("KOMGA.{}", STANDARD_NO_PAD.encode(sync_token_response));

    let mut response = (
        StatusCode::OK,
        [(
            HeaderName::from_static("x-kobo-synctoken"),
            HeaderValue::from_str(encoded_sync_token.as_str())
                .unwrap_or_else(|_| HeaderValue::from_static("")),
        )],
        Json(Value::Array(merged_events)),
    )
        .into_response();
    if merged_should_continue {
        response.headers_mut().insert(
            HeaderName::from_static("x-kobo-sync"),
            HeaderValue::from_static("continue"),
        );
    }
    response
}

struct KoboStoreSyncMergeResult {
    events: Vec<Value>,
    raw_sync_token: Option<String>,
    should_continue: bool,
}

fn is_kobo_store_sync_token_candidate(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty() && trimmed.contains('.')
}

async fn load_kobo_proxy_enabled(state: &OperationalState) -> bool {
    state
        .settings_store
        .load_map()
        .await
        .ok()
        .and_then(|settings| settings.get("KOBO_PROXY").cloned())
        .and_then(|value| value)
        .map(|value| {
            let normalized = value.trim().to_ascii_lowercase();
            matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false)
}

async fn proxy_kobo_store_library_sync(
    headers: &HeaderMap,
    uri: &axum::http::Uri,
    raw_sync_token: &str,
) -> Result<KoboStoreSyncMergeResult, ()> {
    let mut target = String::from("https://storeapi.kobo.com/v1/library/sync");
    if let Some(query) = uri.query()
        && !query.trim().is_empty()
    {
        target.push('?');
        target.push_str(query);
    }

    let client = Client::builder().build().map_err(|_| ())?;
    let mut request = client.get(target);
    for (name, value) in headers {
        let lower = name.as_str().to_ascii_lowercase();
        if lower == "host" || lower == "content-length" || lower == "x-kobo-synctoken" {
            continue;
        }
        request = request.header(name, value);
    }
    request = request.header("x-kobo-synctoken", raw_sync_token);

    let response = request.send().await.map_err(|_| ())?;
    if !response.status().is_success() {
        return Err(());
    }

    let headers = response.headers().clone();
    let body = response.json::<Value>().await.map_err(|_| ())?;
    let events = body.as_array().cloned().unwrap_or_default();
    let should_continue = headers
        .get("x-kobo-sync")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .is_some_and(|value| value.eq_ignore_ascii_case("continue"));
    let raw_sync_token = headers
        .get("x-kobo-synctoken")
        .and_then(|value| value.to_str().ok())
        .and_then(decode_or_passthrough_sync_token);

    Ok(KoboStoreSyncMergeResult {
        events,
        raw_sync_token,
        should_continue,
    })
}

fn kobo_sync_token_from_request(headers: &HeaderMap, _uri: &axum::http::Uri) -> Option<String> {
    let from_header = headers
        .get("x-kobo-synctoken")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    if from_header.is_some() {
        return from_header.and_then(|value| decode_or_passthrough_sync_token(value.as_str()));
    }
    None
}

fn decode_or_passthrough_sync_token(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let normalized = trimmed.strip_prefix("KOMGA.").unwrap_or(trimmed);
    let decoded = STANDARD
        .decode(normalized)
        .ok()
        .or_else(|| STANDARD_NO_PAD.decode(normalized).ok())
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if let Some(decoded) = decoded.as_ref()
        && let Some(raw_sync_token) = extract_calibre_web_raw_sync_token(decoded)
    {
        return Some(raw_sync_token);
    }
    decoded.or_else(|| Some(trimmed.to_string()))
}

fn extract_calibre_web_raw_sync_token(decoded_token: &str) -> Option<String> {
    serde_json::from_str::<Value>(decoded_token)
        .ok()
        .and_then(|value| value.get("data").cloned())
        .and_then(|value| {
            value
                .get("raw_kobo_store_token")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
}

#[allow(dead_code)]
struct KoboSyncDeltas {
    new_entitlement: Vec<Value>,
    deleted_entitlement: Vec<Value>,
    new_tag: Vec<Value>,
    deleted_tag: Vec<Value>,
    new_book_metadata: Vec<Value>,
    deleted_book_metadata: Vec<Value>,
    new_reading_state: Vec<Value>,
    deleted_reading_state: Vec<Value>,
}

#[derive(Clone, Serialize, Deserialize)]
struct KoboSyncBookSnapshot {
    id: String,
    title: String,
    summary: String,
    release_date: Option<String>,
    language: String,
    file_size: u64,
    page_count: u64,
    created: String,
    last_modified: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct KoboSyncReadProgressSnapshot {
    page: i64,
    completed: bool,
    created: String,
    last_modified: String,
    locator: Option<Vec<u8>>,
}

#[derive(Clone, Serialize, Deserialize)]
struct KoboSyncReadListSnapshot {
    id: String,
    name: String,
    created: String,
    last_modified: String,
    items: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
struct KoboSyncSnapshot {
    books: HashMap<String, KoboSyncBookSnapshot>,
    progress: HashMap<String, KoboSyncReadProgressSnapshot>,
    readlists: HashMap<String, KoboSyncReadListSnapshot>,
}

fn kobo_reading_state_from_snapshot(
    book: &KoboSyncBookSnapshot,
    progress: Option<&KoboSyncReadProgressSnapshot>,
) -> Value {
    if let Some(progress) = progress {
        let locator = parse_locator_payload(progress.locator.as_deref());
        let source_progress = locator
            .get("locations")
            .and_then(|value| value.get("progression"))
            .and_then(Value::as_f64);
        let total_progress = locator
            .get("locations")
            .and_then(|value| value.get("totalProgression"))
            .and_then(Value::as_f64);
        let source = locator
            .get("href")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let value = locator
            .get("koboSpan")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let mut bookmark = serde_json::Map::new();
        bookmark.insert(
            "LastModified".to_string(),
            Value::String(progress.last_modified.clone()),
        );
        if let Some(total_progress) = total_progress {
            bookmark.insert("ProgressPercent".to_string(), json!(total_progress * 100.0));
        }
        if let Some(source_progress) = source_progress {
            bookmark.insert(
                "ContentSourceProgressPercent".to_string(),
                json!(source_progress * 100.0),
            );
        }
        if let Some(source) = source {
            bookmark.insert(
                "Location".to_string(),
                json!({
                    "Source": source,
                    "Type": "KoboSpan",
                    "Value": value,
                }),
            );
        }
        let status = if progress.completed {
            "Finished"
        } else {
            "Reading"
        };
        json!({
            "Created": progress.created,
            "CurrentBookmark": Value::Object(bookmark),
            "EntitlementId": book.id,
            "LastModified": progress.last_modified,
            "PriorityTimestamp": progress.last_modified,
            "Statistics": {
                "LastModified": progress.last_modified,
            },
            "StatusInfo": {
                "LastModified": progress.last_modified,
                "Status": status,
                "TimesStartedReading": 1,
            },
        })
    } else {
        json!({
            "Created": book.created,
            "CurrentBookmark": {
                "LastModified": book.created,
            },
            "EntitlementId": book.id,
            "LastModified": book.created,
            "PriorityTimestamp": book.created,
            "Statistics": {
                "LastModified": book.created,
            },
            "StatusInfo": {
                "LastModified": book.created,
                "Status": "ReadyToRead",
                "TimesStartedReading": 0,
            },
        })
    }
}

fn kobo_book_metadata_from_snapshot(
    book: &KoboSyncBookSnapshot,
    base_url: &str,
    auth_token: &str,
) -> Value {
    let mut metadata = serde_json::Map::new();
    metadata.insert(
        "Categories".to_string(),
        Value::Array(vec![Value::String(
            "00000000-0000-0000-0000-000000000001".to_string(),
        )]),
    );
    metadata.insert("ContributorRoles".to_string(), Value::Array(vec![]));
    metadata.insert("Contributors".to_string(), Value::Array(vec![]));
    metadata.insert("CoverImageId".to_string(), Value::String(book.id.clone()));
    metadata.insert(
        "CrossRevisionId".to_string(),
        Value::String(book.id.clone()),
    );
    metadata.insert(
        "CurrentDisplayPrice".to_string(),
        json!({"CurrencyCode": "USD", "TotalAmount": 0}),
    );
    metadata.insert(
        "CurrentLoveDisplayPrice".to_string(),
        json!({"CurrencyCode": "USD", "TotalAmount": 0}),
    );
    if !book.summary.trim().is_empty() {
        metadata.insert(
            "Description".to_string(),
            Value::String(book.summary.clone()),
        );
    }
    metadata.insert(
        "DownloadUrls".to_string(),
        json!([
            {
                "DrmType": "None",
                "Format": "EPUB",
                "Platform": "Generic",
                "Size": book.file_size,
                "Url": format!("{base_url}/kobo/{auth_token}/v1/books/{}/file/epub", book.id),
            }
        ]),
    );
    metadata.insert("EntitlementId".to_string(), Value::String(book.id.clone()));
    metadata.insert("ExternalIds".to_string(), Value::Array(vec![]));
    metadata.insert(
        "Genre".to_string(),
        Value::String("00000000-0000-0000-0000-000000000001".to_string()),
    );
    metadata.insert("IsEligibleForKoboLove".to_string(), Value::Bool(false));
    metadata.insert("IsInternetArchive".to_string(), Value::Bool(false));
    metadata.insert("IsPreOrder".to_string(), Value::Bool(false));
    metadata.insert("IsSocialEnabled".to_string(), Value::Bool(true));
    metadata.insert(
        "Language".to_string(),
        Value::String(if book.language.trim().is_empty() {
            "en".to_string()
        } else {
            book.language.clone()
        }),
    );
    metadata.insert(
        "PhoneticPronunciations".to_string(),
        Value::Object(serde_json::Map::new()),
    );
    if let Some(release_date) = book.release_date.as_ref() {
        metadata.insert(
            "PublicationDate".to_string(),
            Value::String(release_date.clone()),
        );
    }
    metadata.insert("RevisionId".to_string(), Value::String(book.id.clone()));
    metadata.insert("Title".to_string(), Value::String(book.title.clone()));
    metadata.insert("WorkId".to_string(), Value::String(book.id.clone()));
    Value::Object(metadata)
}

fn kobo_entitlement_from_snapshot(book: &KoboSyncBookSnapshot, is_removed: bool) -> Value {
    json!({
        "Accessibility": "Full",
        "ActivePeriod": {
            "From": now_sync_marker(),
        },
        "Created": book.created,
        "CrossRevisionId": book.id,
        "Id": book.id,
        "IsHiddenFromArchive": false,
        "IsLocked": false,
        "IsRemoved": is_removed,
        "LastModified": book.last_modified,
        "OriginCategory": "Imported",
        "RevisionId": book.id,
        "Status": "Active",
    })
}

fn kobo_tag_from_snapshot(readlist: &KoboSyncReadListSnapshot, include_items: bool) -> Value {
    let mut tag = serde_json::Map::new();
    tag.insert("Id".to_string(), Value::String(readlist.id.clone()));
    tag.insert(
        "Created".to_string(),
        Value::String(readlist.created.clone()),
    );
    tag.insert(
        "LastModified".to_string(),
        Value::String(readlist.last_modified.clone()),
    );
    tag.insert("Name".to_string(), Value::String(readlist.name.clone()));
    tag.insert("Type".to_string(), Value::String("UserTag".to_string()));
    if include_items {
        let items = readlist
            .items
            .iter()
            .map(|book_id| {
                json!({
                    "RevisionId": book_id,
                    "Type": "ProductRevisionTagItem",
                })
            })
            .collect::<Vec<_>>();
        tag.insert("Items".to_string(), Value::Array(items));
    }
    Value::Object(tag)
}

fn kobo_new_entitlement_event(
    book: &KoboSyncBookSnapshot,
    reading_state: Value,
    base_url: &str,
    auth_token: &str,
) -> Value {
    json!({
        "NewEntitlement": {
            "BookEntitlement": kobo_entitlement_from_snapshot(book, false),
            "BookMetadata": kobo_book_metadata_from_snapshot(book, base_url, auth_token),
            "ReadingState": reading_state,
        }
    })
}

fn kobo_changed_entitlement_removed_event(
    book: &KoboSyncBookSnapshot,
    base_url: &str,
    auth_token: &str,
) -> Value {
    json!({
        "ChangedEntitlement": {
            "BookEntitlement": kobo_entitlement_from_snapshot(book, true),
            "BookMetadata": kobo_book_metadata_from_snapshot(book, base_url, auth_token),
        }
    })
}

fn kobo_changed_product_metadata_event(
    book: &KoboSyncBookSnapshot,
    base_url: &str,
    auth_token: &str,
) -> Value {
    json!({
        "ChangedProductMetadata": kobo_book_metadata_from_snapshot(book, base_url, auth_token),
    })
}

fn kobo_changed_reading_state_event(reading_state: Value) -> Value {
    json!({
        "ChangedReadingState": {
            "ReadingState": reading_state,
        }
    })
}

fn build_kobo_sync_events(
    from: Option<&KoboSyncSnapshot>,
    to: &KoboSyncSnapshot,
    base_url: &str,
    auth_token: &str,
) -> Vec<Value> {
    let mut events = Vec::new();

    match from {
        None => {
            let mut books = to.books.values().collect::<Vec<_>>();
            books.sort_by(|a, b| a.id.cmp(&b.id));
            for book in books {
                events.push(kobo_new_entitlement_event(
                    book,
                    kobo_reading_state_from_snapshot(book, to.progress.get(&book.id)),
                    base_url,
                    auth_token,
                ));
            }

            let mut readlists = to.readlists.values().collect::<Vec<_>>();
            readlists.sort_by(|a, b| a.id.cmp(&b.id));
            for readlist in readlists {
                events.push(json!({
                    "NewTag": {
                        "Tag": kobo_tag_from_snapshot(readlist, true),
                    }
                }));
            }
        }
        Some(from) => {
            let mut to_book_ids = to.books.keys().cloned().collect::<Vec<_>>();
            to_book_ids.sort();
            for book_id in to_book_ids {
                let Some(to_book) = to.books.get(&book_id) else {
                    continue;
                };
                match from.books.get(&book_id) {
                    None => {
                        events.push(kobo_new_entitlement_event(
                            to_book,
                            kobo_reading_state_from_snapshot(to_book, to.progress.get(&book_id)),
                            base_url,
                            auth_token,
                        ));
                    }
                    Some(from_book) => {
                        if from_book.last_modified != to_book.last_modified {
                            events.push(kobo_new_entitlement_event(
                                to_book,
                                kobo_reading_state_from_snapshot(
                                    to_book,
                                    to.progress.get(&book_id),
                                ),
                                base_url,
                                auth_token,
                            ));
                            events.push(kobo_changed_product_metadata_event(
                                to_book, base_url, auth_token,
                            ));
                            if let Some(to_progress) = to.progress.get(&book_id) {
                                events.push(kobo_changed_reading_state_event(
                                    kobo_reading_state_from_snapshot(to_book, Some(to_progress)),
                                ));
                            }
                        }
                    }
                }
            }

            let mut removed_book_ids = from.books.keys().cloned().collect::<Vec<_>>();
            removed_book_ids.sort();
            for book_id in removed_book_ids {
                if to.books.contains_key(&book_id) {
                    continue;
                }
                if let Some(from_book) = from.books.get(&book_id) {
                    events.push(kobo_changed_entitlement_removed_event(
                        from_book, base_url, auth_token,
                    ));
                }
            }

            let mut progress_book_ids = to
                .progress
                .keys()
                .chain(from.progress.keys())
                .cloned()
                .collect::<Vec<_>>();
            progress_book_ids.sort();
            progress_book_ids.dedup();
            for book_id in progress_book_ids {
                let from_progress = from.progress.get(&book_id);
                let to_progress = to.progress.get(&book_id);
                if from_progress.map(|value| {
                    (
                        &value.last_modified,
                        value.page,
                        value.completed,
                        value.locator.as_ref(),
                    )
                }) == to_progress.map(|value| {
                    (
                        &value.last_modified,
                        value.page,
                        value.completed,
                        value.locator.as_ref(),
                    )
                }) {
                    continue;
                }
                if let Some(book) = to.books.get(&book_id)
                    && let Some(progress) = to_progress
                {
                    events.push(kobo_changed_reading_state_event(
                        kobo_reading_state_from_snapshot(book, Some(progress)),
                    ));
                }
            }

            let mut to_readlist_ids = to.readlists.keys().cloned().collect::<Vec<_>>();
            to_readlist_ids.sort();
            for readlist_id in to_readlist_ids {
                let Some(to_readlist) = to.readlists.get(&readlist_id) else {
                    continue;
                };
                match from.readlists.get(&readlist_id) {
                    None => events.push(json!({
                        "NewTag": {
                            "Tag": kobo_tag_from_snapshot(to_readlist, true),
                        }
                    })),
                    Some(from_readlist)
                        if from_readlist.last_modified != to_readlist.last_modified
                            || from_readlist.name != to_readlist.name
                            || from_readlist.items != to_readlist.items =>
                    {
                        events.push(json!({
                            "ChangedTag": {
                                "Tag": kobo_tag_from_snapshot(to_readlist, true),
                            }
                        }));
                    }
                    Some(_) => {}
                }
            }

            let mut removed_readlists = from.readlists.keys().cloned().collect::<Vec<_>>();
            removed_readlists.sort();
            for readlist_id in removed_readlists {
                if to.readlists.contains_key(&readlist_id) {
                    continue;
                }
                let Some(previous) = from.readlists.get(&readlist_id) else {
                    continue;
                };
                events.push(json!({
                    "DeletedTag": {
                        "Tag": kobo_tag_from_snapshot(previous, false),
                    }
                }));
            }
        }
    }

    events
}

const KOBO_SYNC_ITEM_LIMIT: usize = 200;

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct KomgaSyncTokenPayload {
    #[serde(default = "default_sync_token_version")]
    version: i32,
    #[serde(default, rename = "rawKoboSyncToken", alias = "raw_kobo_sync_token")]
    raw_kobo_sync_token: String,
    #[serde(
        default,
        rename = "ongoingSyncPointId",
        alias = "ongoing_sync_point_id"
    )]
    ongoing_sync_point_id: Option<String>,
    #[serde(
        default,
        rename = "lastSuccessfulSyncPointId",
        alias = "last_successful_sync_point_id"
    )]
    last_successful_sync_point_id: Option<String>,
}

fn default_sync_token_version() -> i32 {
    1
}

#[derive(Clone, Serialize, Deserialize)]
struct KoboSyncPointState {
    user_id: String,
    marker: String,
    cursor: usize,
    from_marker: Option<String>,
    snapshot: Option<KoboSyncSnapshot>,
}

async fn ensure_kobo_sync_state_table(database_file: &FsPath) -> Result<(), sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS KOBO_SYNC_POINT_STATE ( SYNC_POINT_ID TEXT NOT NULL, USER_ID \
           TEXT NOT NULL, STATE_JSON TEXT NOT NULL, PRIMARY KEY (SYNC_POINT_ID, USER_ID) )",
    )
    .execute(&pool)
    .await?;
    Ok(())
}

async fn load_sync_point_state(
    database_file: &FsPath,
    sync_point_id: &str,
    user_id: &str,
) -> Option<KoboSyncPointState> {
    let _ = ensure_kobo_sync_state_table(database_file).await;
    let pool = connect_pool(database_file, 1).await.ok()?;
    let row = sqlx::query(
        "SELECT STATE_JSON \
         FROM KOBO_SYNC_POINT_STATE \
         WHERE SYNC_POINT_ID = ? \
         AND USER_ID = ? \
         LIMIT 1",
    )
    .bind(sync_point_id)
    .bind(user_id)
    .fetch_optional(&pool)
    .await
    .ok()?;

    row.and_then(|row| {
        serde_json::from_str::<KoboSyncPointState>(row.get::<String, _>("STATE_JSON").as_str()).ok()
    })
}

async fn load_sync_point_marker(
    database_file: &FsPath,
    sync_point_id: &str,
    user_id: &str,
) -> Option<String> {
    load_sync_point_state(database_file, sync_point_id, user_id)
        .await
        .map(|entry| entry.marker)
}

async fn save_sync_point(
    database_file: &FsPath,
    sync_point_id: &str,
    sync_point_state: &KoboSyncPointState,
) -> Result<(), sqlx::Error> {
    ensure_kobo_sync_state_table(database_file).await?;
    let pool = connect_pool(database_file, 1).await?;
    sqlx::query(
        "INSERT INTO KOBO_SYNC_POINT_STATE (SYNC_POINT_ID, USER_ID, STATE_JSON) \
         VALUES (?, ?, ?) \
         ON CONFLICT (SYNC_POINT_ID, USER_ID) DO UPDATE \
         SET STATE_JSON = excluded.STATE_JSON",
    )
    .bind(sync_point_id)
    .bind(sync_point_state.user_id.as_str())
    .bind(serde_json::to_string(sync_point_state).unwrap_or_else(|_| "{}".to_string()))
    .execute(&pool)
    .await?;
    Ok(())
}

async fn remove_sync_point(database_file: &FsPath, sync_point_id: &str) -> Result<(), sqlx::Error> {
    let _ = ensure_kobo_sync_state_table(database_file).await;
    let pool = connect_pool(database_file, 1).await?;
    sqlx::query(
        "DELETE \
                 FROM KOBO_SYNC_POINT_STATE \
                 WHERE SYNC_POINT_ID = ?",
    )
    .bind(sync_point_id)
    .execute(&pool)
    .await?;
    Ok(())
}

fn now_sync_marker() -> String {
    OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "2000-01-01T00:00:00Z".to_string())
}

fn parse_komga_sync_token_payload(value: &str) -> Option<KomgaSyncTokenPayload> {
    serde_json::from_str::<KomgaSyncTokenPayload>(value).ok()
}

fn build_komga_sync_token_payload(
    previous: Option<KomgaSyncTokenPayload>,
    incoming_raw_sync_token: Option<String>,
    sync_point_id: &str,
    should_continue: bool,
) -> String {
    let mut payload = previous.unwrap_or_default();
    if payload.version <= 0 {
        payload.version = default_sync_token_version();
    }
    if payload.raw_kobo_sync_token.is_empty()
        && let Some(raw) = incoming_raw_sync_token
    {
        payload.raw_kobo_sync_token = raw;
    }
    if should_continue {
        payload.ongoing_sync_point_id = Some(sync_point_id.to_string());
    } else {
        let finalized_sync_point = payload
            .ongoing_sync_point_id
            .clone()
            .unwrap_or_else(|| sync_point_id.to_string());
        payload.ongoing_sync_point_id = None;
        payload.last_successful_sync_point_id = Some(finalized_sync_point);
    }
    serde_json::to_string(&payload).unwrap_or_else(|_| {
        json!({
            "version": default_sync_token_version(),
            "rawKoboSyncToken": "",
            "ongoingSyncPointId": if should_continue { Value::String(sync_point_id.to_string()) } else { Value::Null },
            "lastSuccessfulSyncPointId": if should_continue { Value::Null } else { Value::String(sync_point_id.to_string()) },
        })
        .to_string()
    })
}

pub(in crate::app::compat_runtime) async fn kobo_library_book_metadata(
    Extension(state): Extension<OperationalState>,
    Path((auth_token, book_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    if resolved_kobo_user(
        auth_token.as_str(),
        &headers,
        state.runtime.database_file.as_path(),
    )
    .await
    .is_none()
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let database_file = state.runtime.database_file.as_path();
    let metadata = match load_kobo_metadata_record(database_file, &book_id).await {
        Ok(Some(metadata)) => metadata,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let base_url = request_base_url(&headers);
    let media_type = content_type_from_filename(&metadata.file_name, "application/octet-stream");
    let format = if media_type == "application/epub+zip" {
        "EPUB"
    } else {
        "EPUB3"
    };

    Json(json!([
        {
            "Categories": ["00000000-0000-0000-0000-000000000001"],
            "ContributorRoles": [],
            "Contributors": [],
            "CoverImageId": book_id,
            "CrossRevisionId": book_id,
            "CurrentDisplayPrice": {"CurrencyCode": "USD", "TotalAmount": 0},
            "CurrentLoveDisplayPrice": {"CurrencyCode": "USD", "TotalAmount": 0},
            "Description": metadata.summary,
            "DownloadUrls": [
                {
                    "DrmType": "None",
                    "Format": format,
                    "Platform": "Generic",
                    "Size": metadata.file_size,
                    "Url": format!("{base_url}/kobo/{auth_token}/v1/books/{book_id}/file/epub"),
                }
            ],
            "EntitlementId": book_id,
            "ExternalIds": [],
            "Genre": "00000000-0000-0000-0000-000000000001",
            "IsEligibleForKoboLove": false,
            "IsInternetArchive": false,
            "IsPreOrder": false,
            "IsSocialEnabled": true,
            "ISBN": Value::Null,
            "Language": if metadata.language.is_empty() { "en".to_string() } else { metadata.language },
            "PhoneticPronunciations": {},
            "PublicationDate": metadata.release_date,
            "Publisher": Value::Null,
            "RevisionId": book_id,
            "Series": Value::Null,
            "Slug": Value::Null,
            "SubTitle": Value::Null,
            "Title": metadata.title,
            "WorkId": book_id,
        }
    ]))
    .into_response()
}

pub(in crate::app::compat_runtime) async fn kobo_library_book_state(
    Extension(state): Extension<OperationalState>,
    Path((auth_token, book_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let Some(current_user) = resolved_kobo_user(
        auth_token.as_str(),
        &headers,
        state.runtime.database_file.as_path(),
    )
    .await
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let database_file = state.runtime.database_file.as_path();
    if !persisted_book_exists(database_file, &book_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    let user_id_value = user_id(&current_user).to_string();
    let page_count = load_book_page_count(database_file, &book_id)
        .await
        .unwrap_or(1)
        .max(1);
    let created_timestamp = load_book_created_timestamp(database_file, &book_id)
        .await
        .unwrap_or_else(|_| None)
        .unwrap_or_else(now_sync_marker);

    let progress = match load_read_progress(database_file, &book_id, &user_id_value).await {
        Ok(progress) => progress,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let payload = match progress {
        Some(record) => kobo_reading_state_payload(
            &book_id,
            &record,
            page_count,
            parse_locator_payload(record.locator.as_deref()),
        ),
        None => kobo_empty_reading_state_payload(&book_id, created_timestamp.as_str()),
    };

    Json(json!([payload])).into_response()
}

pub(in crate::app::compat_runtime) async fn kobo_library_book_state_update(
    Extension(state): Extension<OperationalState>,
    Path((auth_token, book_id)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Some(current_user) = resolved_kobo_user(
        auth_token.as_str(),
        &headers,
        state.runtime.database_file.as_path(),
    )
    .await
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let database_file = state.runtime.database_file.as_path();
    if !persisted_book_exists(database_file, &book_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    let payload = match serde_json::from_slice::<Value>(&body) {
        Ok(payload) => payload,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "invalid Kobo state payload" })),
            )
                .into_response();
        }
    };

    let Some(state) = payload
        .get("ReadingStates")
        .and_then(Value::as_array)
        .and_then(|values| values.first())
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "ReadingStates must contain one element" })),
        )
            .into_response();
    };
    let Some(_entitlement_id) = state
        .get("EntitlementId")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "EntitlementId is required" })),
        )
            .into_response();
    };

    let Some(current_bookmark) = state.get("CurrentBookmark") else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "CurrentBookmark is required" })),
        )
            .into_response();
    };
    let Some(content_source_progress_percent) = current_bookmark
        .get("ContentSourceProgressPercent")
        .and_then(Value::as_f64)
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "ContentSourceProgressPercent is required" })),
        )
            .into_response();
    };
    let Some(_bookmark_last_modified) = current_bookmark
        .get("LastModified")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "CurrentBookmark.LastModified is required" })),
        )
            .into_response();
    };
    let Some(_statistics_last_modified) = state
        .get("Statistics")
        .and_then(|value| value.get("LastModified"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Statistics.LastModified is required" })),
        )
            .into_response();
    };
    let Some(_status_info_last_modified) = state
        .get("StatusInfo")
        .and_then(|value| value.get("LastModified"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "StatusInfo.LastModified is required" })),
        )
            .into_response();
    };
    let Some(last_modified) = state
        .get("LastModified")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "LastModified is required" })),
        )
            .into_response();
    };
    let Some(href_source) = current_bookmark
        .get("Location")
        .and_then(|value| value.get("Source"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Location.Source is required" })),
        )
            .into_response();
    };

    let content_source_progress = content_source_progress_percent / 100.0;
    let total_progress = current_bookmark
        .get("ProgressPercent")
        .and_then(Value::as_f64)
        .unwrap_or(content_source_progress * 100.0)
        / 100.0;
    let total_progress = total_progress.clamp(0.0, 1.0);
    let content_source_progress = content_source_progress.clamp(0.0, 1.0);
    let Some(status) = state
        .get("StatusInfo")
        .and_then(|value| value.get("Status"))
        .and_then(Value::as_str)
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "StatusInfo.Status is required" })),
        )
            .into_response();
    };
    let completed = status.eq_ignore_ascii_case("Finished");
    let progress_last_modified = last_modified.to_string();
    let page_count = load_book_page_count(database_file, &book_id)
        .await
        .unwrap_or(1)
        .max(1);
    let computed_page = if completed {
        page_count
    } else {
        ((total_progress * page_count as f64).ceil() as u64).clamp(0, page_count)
    };
    let page = computed_page.max(1) as i64;

    let href = href_source.to_string();
    let kobo_span = current_bookmark
        .get("Location")
        .and_then(|value| value.get("Value"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let user_id_value = user_id(&current_user).to_string();
    let request_locator = json!({
        "href": href,
        "type": "application/xhtml+xml",
        "koboSpan": if kobo_span.is_empty() { Value::Null } else { Value::String(kobo_span) },
        "locations": {
            "progression": content_source_progress,
            "totalProgression": total_progress,
            "position": page,
        },
    });

    let locator = if completed {
        match load_book_last_epub_position_locator(database_file, &book_id).await {
            Ok(Some(locator)) => locator,
            _ => {
                return kobo_state_update_failure(book_id.as_str());
            }
        }
    } else {
        request_locator
    };

    let (device_id, device_name) = if configured_api_key().is_some_and(|value| value == auth_token)
    {
        (
            configured_api_key_id().unwrap_or_else(|| "unknown".to_string()),
            configured_api_key_comment().unwrap_or_else(|| "unknown".to_string()),
        )
    } else {
        ("unknown".to_string(), "unknown".to_string())
    };

    let persist_result = persist_read_progress_with_locator(
        database_file,
        &book_id,
        &user_id_value,
        page,
        completed,
        &device_id,
        &device_name,
        progress_last_modified.as_str(),
        Some(locator),
    )
    .await;

    let update_result = if persist_result.is_ok() {
        "Success"
    } else {
        "Failure"
    };

    Json(json!({
        "RequestResult": update_result,
        "UpdateResults": [
            {
                "EntitlementId": book_id,
                "CurrentBookmarkResult": {"Result": update_result},
                "StatisticsResult": {"Result": if persist_result.is_ok() { "Ignored" } else { "Failure" }},
                "StatusInfoResult": {"Result": update_result},
            }
        ],
    }))
    .into_response()
}

fn kobo_state_update_failure(book_id: &str) -> Response {
    Json(json!({
        "RequestResult": "Failure",
        "UpdateResults": [
            {
                "EntitlementId": book_id,
                "CurrentBookmarkResult": {"Result": "Failure"},
                "StatisticsResult": {"Result": "Failure"},
                "StatusInfoResult": {"Result": "Failure"},
            }
        ],
    }))
    .into_response()
}

pub(in crate::app::compat_runtime) async fn kobo_book_file_epub(
    Extension(state): Extension<OperationalState>,
    Path((auth_token, book_id)): Path<(String, String)>,
    headers: HeaderMap,
    Query(query): Query<KoboBookFileQuery>,
) -> Response {
    if resolved_kobo_user(
        auth_token.as_str(),
        &headers,
        state.runtime.database_file.as_path(),
    )
    .await
    .is_none()
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let media = match load_book_media_file(state.runtime.database_file.as_path(), &book_id).await {
        Ok(Some(media)) => media,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let mut body = match std::fs::read(&media.file_path) {
        Ok(body) => body,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    let mut file_name = media.file_name.clone();
    let mut media_type = media.media_type.clone();

    if query.convert_kepub.unwrap_or(false) && media.media_type == "application/epub+zip" {
        let effective_kepubify_path = load_effective_kepubify_path(
            &state,
            state.runtime.database_file.as_path(),
            state.runtime.kepubify_path.as_deref(),
        )
        .await;
        if let Some(kepubify_path) = effective_kepubify_path
            && let Some(converted_body) =
                convert_epub_to_kepub_bytes(&kepubify_path, &media.file_path)
        {
            body = converted_body;
            file_name = kobo_kepub_file_name(media.file_name.as_str());
            media_type = "application/epub+zip".to_string();
        } else {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "error": "Kepub conversion failed" })),
            )
                .into_response();
        }
    }

    (
        StatusCode::OK,
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_str(&media_type)
                    .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
            ),
            (
                header::CONTENT_DISPOSITION,
                HeaderValue::from_str(format!("attachment; filename=\"{}\"", file_name).as_str())
                    .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
            ),
        ],
        body,
    )
        .into_response()
}

pub(in crate::app::compat_runtime) async fn kobo_book_thumbnail(
    Extension(state): Extension<OperationalState>,
    Path((auth_token, thumbnail_id, _width, _height, _is_greyscale)): Path<(
        String,
        String,
        String,
        String,
        String,
    )>,
    headers: HeaderMap,
) -> Response {
    if resolved_kobo_user(
        auth_token.as_str(),
        &headers,
        state.runtime.database_file.as_path(),
    )
    .await
    .is_none()
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    match load_thumbnail_by_id(state.runtime.database_file.as_path(), &thumbnail_id).await {
        Ok(Some((media_type, bytes))) => (
            StatusCode::OK,
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_str(&media_type)
                    .unwrap_or_else(|_| HeaderValue::from_static("image/jpeg")),
            )],
            bytes,
        )
            .into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(in crate::app::compat_runtime) async fn kobo_book_thumbnail_with_quality(
    Extension(state): Extension<OperationalState>,
    Path((auth_token, thumbnail_id, _width, _height, _quality, _is_greyscale)): Path<(
        String,
        String,
        String,
        String,
        String,
        String,
    )>,
    headers: HeaderMap,
) -> Response {
    if resolved_kobo_user(
        auth_token.as_str(),
        &headers,
        state.runtime.database_file.as_path(),
    )
    .await
    .is_none()
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    match load_thumbnail_by_id(state.runtime.database_file.as_path(), &thumbnail_id).await {
        Ok(Some((media_type, bytes))) => (
            StatusCode::OK,
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_str(&media_type)
                    .unwrap_or_else(|_| HeaderValue::from_static("image/jpeg")),
            )],
            bytes,
        )
            .into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(in crate::app::compat_runtime) async fn kobo_catch_all(
    Extension(auth_db): Extension<super::AuthDatabaseState>,
    Path((auth_token, _path)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    if resolved_kobo_user(
        auth_token.as_str(),
        &headers,
        auth_db.database_file.as_path(),
    )
    .await
    .is_none()
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    StatusCode::NOT_FOUND.into_response()
}

async fn load_kobo_sync_snapshot(
    database_file: &FsPath,
    user_id: &str,
) -> Result<KoboSyncSnapshot, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;

    let books_rows = sqlx::query(
        "SELECT b.ID AS BOOK_ID, COALESCE(bm.TITLE, b.NAME) AS TITLE, \
                COALESCE(bm.SUMMARY, '') AS SUMMARY, bm.RELEASE_DATE AS RELEASE_DATE, \
                COALESCE(sm.LANGUAGE, 'en') AS LANGUAGE, COALESCE(b.FILE_SIZE, 0) AS FILE_SIZE, \
                COALESCE(m.PAGE_COUNT, 0) AS PAGE_COUNT, \
                COALESCE(b.CREATED_DATE, '') AS CREATED_DATE, \
                COALESCE(b.LAST_MODIFIED_DATE, b.CREATED_DATE, '') AS LAST_MODIFIED_DATE \
         FROM BOOK b \
         LEFT \
         JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID \
         LEFT \
         JOIN SERIES_METADATA sm ON sm.SERIES_ID = b.SERIES_ID \
         JOIN MEDIA m ON m.BOOK_ID = b.ID \
         WHERE b.DELETED_DATE IS NULL \
         AND m.STATUS = 'READY' \
         AND m.MEDIA_TYPE = 'application/epub+zip' \
         ORDER BY b.ID ASC",
    )
    .fetch_all(&pool)
    .await?;

    let progress_rows = sqlx::query(
        "SELECT rp.BOOK_ID AS BOOK_ID, rp.PAGE AS PAGE, rp.COMPLETED AS COMPLETED, \
                COALESCE(rp.CREATED_DATE, '') AS CREATED_DATE, \
                COALESCE(rp.LAST_MODIFIED_DATE, rp.CREATED_DATE, '') AS LAST_MODIFIED_DATE, \
                rp.LOCATOR AS LOCATOR \
         FROM READ_PROGRESS rp \
         JOIN BOOK b ON b.ID = rp.BOOK_ID \
         JOIN MEDIA m ON m.BOOK_ID = b.ID \
         WHERE rp.USER_ID = ? \
         AND b.DELETED_DATE IS NULL \
         AND m.STATUS = 'READY' \
         AND m.MEDIA_TYPE = 'application/epub+zip' \
         ORDER BY rp.BOOK_ID ASC",
    )
    .bind(user_id)
    .fetch_all(&pool)
    .await?;

    let readlist_rows = sqlx::query(
        "SELECT rl.ID AS READLIST_ID, rl.NAME AS NAME, \
                COALESCE(rl.CREATED_DATE, '') AS CREATED_DATE, \
                COALESCE(rl.LAST_MODIFIED_DATE, rl.CREATED_DATE, '') AS LAST_MODIFIED_DATE, \
                rb.BOOK_ID AS BOOK_ID, rb.NUMBER AS ORDER_INDEX, \
                b.DELETED_DATE AS BOOK_DELETED_DATE \
         FROM READLIST rl \
         LEFT \
         JOIN READLIST_BOOK rb ON rb.READLIST_ID = rl.ID \
         LEFT \
         JOIN BOOK b ON b.ID = rb.BOOK_ID \
         ORDER BY rl.ID ASC, rb.NUMBER ASC, rb.BOOK_ID ASC",
    )
    .fetch_all(&pool)
    .await?;

    let mut books = HashMap::new();
    for row in books_rows {
        let id = row.get::<String, _>("BOOK_ID");
        books.insert(
            id.clone(),
            KoboSyncBookSnapshot {
                id,
                title: row.get::<String, _>("TITLE"),
                summary: row.get::<String, _>("SUMMARY"),
                release_date: row.get::<Option<String>, _>("RELEASE_DATE"),
                language: row.get::<String, _>("LANGUAGE"),
                file_size: row.get::<i64, _>("FILE_SIZE").max(0) as u64,
                page_count: row.get::<i64, _>("PAGE_COUNT").max(1) as u64,
                created: row.get::<String, _>("CREATED_DATE"),
                last_modified: row.get::<String, _>("LAST_MODIFIED_DATE"),
            },
        );
    }

    let mut progress = HashMap::new();
    for row in progress_rows {
        let book_id = row.get::<String, _>("BOOK_ID");
        progress.insert(
            book_id.clone(),
            KoboSyncReadProgressSnapshot {
                page: row.get::<i64, _>("PAGE"),
                completed: row.get::<bool, _>("COMPLETED"),
                created: row.get::<String, _>("CREATED_DATE"),
                last_modified: row.get::<String, _>("LAST_MODIFIED_DATE"),
                locator: row.get::<Option<Vec<u8>>, _>("LOCATOR"),
            },
        );
    }

    let mut readlists = HashMap::<String, KoboSyncReadListSnapshot>::new();
    for row in readlist_rows {
        let readlist_id = row.get::<String, _>("READLIST_ID");
        let entry =
            readlists
                .entry(readlist_id.clone())
                .or_insert_with(|| KoboSyncReadListSnapshot {
                    id: readlist_id,
                    name: row.get::<String, _>("NAME"),
                    created: row.get::<String, _>("CREATED_DATE"),
                    last_modified: row.get::<String, _>("LAST_MODIFIED_DATE"),
                    items: Vec::new(),
                });

        let book_id = row.get::<Option<String>, _>("BOOK_ID");
        let book_deleted_date = row.get::<Option<String>, _>("BOOK_DELETED_DATE");
        if let Some(book_id) = book_id
            && book_deleted_date.is_none()
            && books.contains_key(&book_id)
        {
            entry.items.push(book_id);
        }
    }

    let ondeck_book_ids = load_kobo_ondeck_book_ids(database_file, user_id).await?;
    let ondeck_book_ids = ondeck_book_ids
        .into_iter()
        .filter(|book_id| books.contains_key(book_id))
        .collect::<Vec<_>>();
    if !ondeck_book_ids.is_empty() {
        let ondeck_last_modified = ondeck_book_ids
            .iter()
            .filter_map(|book_id| books.get(book_id).map(|book| book.last_modified.clone()))
            .max()
            .unwrap_or_else(now_sync_marker);
        readlists.insert(
            "komga-on-deck".to_string(),
            KoboSyncReadListSnapshot {
                id: "komga-on-deck".to_string(),
                name: "On Deck".to_string(),
                created: ondeck_last_modified.clone(),
                last_modified: ondeck_last_modified,
                items: ondeck_book_ids,
            },
        );
    }

    Ok(KoboSyncSnapshot {
        books,
        progress,
        readlists,
    })
}

async fn load_kobo_ondeck_book_ids(
    database_file: &FsPath,
    user_id: &str,
) -> Result<Vec<String>, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let rows = sqlx::query(
        "SELECT b.ID, b.SERIES_ID, b.NUMBER \
         FROM BOOK b \
         JOIN MEDIA m ON m.BOOK_ID = b.ID \
         WHERE b.DELETED_DATE IS NULL \
         AND m.STATUS = 'READY' \
         AND m.MEDIA_TYPE = 'application/epub+zip' \
         AND b.SERIES_ID IN ( SELECT DISTINCT b_done.SERIES_ID \
         FROM BOOK b_done \
         JOIN READ_PROGRESS rp_done ON rp_done.BOOK_ID = b_done.ID \
         WHERE rp_done.USER_ID = ? \
         AND rp_done.COMPLETED = 1 ) \
         AND b.SERIES_ID NOT IN ( SELECT DISTINCT b_prog.SERIES_ID \
         FROM BOOK b_prog \
         JOIN READ_PROGRESS rp_prog ON rp_prog.BOOK_ID = b_prog.ID \
         WHERE rp_prog.USER_ID = ? \
         AND rp_prog.COMPLETED = 0 ) \
         AND NOT EXISTS ( SELECT 1 \
         FROM READ_PROGRESS rp_seen \
         WHERE rp_seen.BOOK_ID = b.ID \
         AND rp_seen.USER_ID = ? \
         AND rp_seen.COMPLETED = 1 ) \
         ORDER BY b.SERIES_ID ASC, b.NUMBER ASC",
    )
    .bind(user_id)
    .bind(user_id)
    .bind(user_id)
    .fetch_all(&pool)
    .await?;

    let mut first_per_series = HashMap::<String, String>::new();
    for row in rows {
        let series_id = row.get::<String, _>("SERIES_ID");
        let book_id = row.get::<String, _>("ID");
        first_per_series.entry(series_id).or_insert(book_id);
    }

    let mut ondeck = first_per_series.into_values().collect::<Vec<_>>();
    ondeck.sort();
    Ok(ondeck)
}

#[allow(dead_code)]
async fn load_kobo_sync_deltas(
    database_file: &FsPath,
    user_id: &str,
    since: Option<&str>,
) -> Result<KoboSyncDeltas, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;

    let since_value = since.unwrap_or_default();

    let rows = sqlx::query(
        "SELECT b.ID AS BOOK_ID, COALESCE(bm.TITLE, b.NAME) AS TITLE \
         FROM BOOK b \
         LEFT \
         JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID \
         WHERE b.DELETED_DATE IS NULL \
         AND (? = '' \
         OR COALESCE(b.LAST_MODIFIED_DATE, b.CREATED_DATE, '') > ?) \
         ORDER BY b.ID ASC",
    )
    .bind(since_value)
    .bind(since_value)
    .fetch_all(&pool)
    .await?;

    let deleted_rows = sqlx::query(
        "SELECT b.ID AS BOOK_ID \
         FROM BOOK b \
         WHERE b.DELETED_DATE IS NOT NULL \
         AND (? = '' \
         OR COALESCE(b.DELETED_DATE, '') > ?) \
         ORDER BY b.DELETED_DATE ASC, b.ID ASC",
    )
    .bind(since_value)
    .bind(since_value)
    .fetch_all(&pool)
    .await?;

    let read_progress_rows = sqlx::query(
        "SELECT rp.BOOK_ID AS BOOK_ID, rp.PAGE AS PAGE, rp.COMPLETED AS COMPLETED, \
                COALESCE(rp.LAST_MODIFIED_DATE, rp.CREATED_DATE, '') AS LAST_MODIFIED_DATE \
         FROM READ_PROGRESS rp \
         JOIN BOOK b ON b.ID = rp.BOOK_ID \
         WHERE rp.USER_ID = ? \
         AND b.DELETED_DATE IS NULL \
         AND (? = '' \
         OR COALESCE(rp.LAST_MODIFIED_DATE, rp.CREATED_DATE, '') > ?) \
         ORDER BY LAST_MODIFIED_DATE ASC, rp.BOOK_ID ASC",
    )
    .bind(user_id)
    .bind(since_value)
    .bind(since_value)
    .fetch_all(&pool)
    .await?;

    let tag_rows = sqlx::query(
        "SELECT DISTINCT bt.TAG AS TAG \
         FROM BOOK_METADATA_TAG bt \
         JOIN BOOK b ON b.ID = bt.BOOK_ID \
         WHERE b.DELETED_DATE IS NULL \
         AND (? = '' \
         OR COALESCE(b.LAST_MODIFIED_DATE, b.CREATED_DATE, '') > ?) \
         ORDER BY lower(bt.TAG), bt.TAG",
    )
    .bind(since_value)
    .bind(since_value)
    .fetch_all(&pool)
    .await?;

    let mut entitlement = Vec::with_capacity(rows.len());
    let mut metadata = Vec::with_capacity(rows.len());
    let mut deleted_entitlement = Vec::with_capacity(deleted_rows.len());
    let mut deleted_book_metadata = Vec::with_capacity(deleted_rows.len());
    let mut new_reading_state = Vec::with_capacity(read_progress_rows.len());
    let mut new_tag = Vec::with_capacity(tag_rows.len());

    for row in rows {
        let book_id = row.get::<String, _>("BOOK_ID");
        let title = row.get::<String, _>("TITLE");
        entitlement.push(json!({
            "BookId": book_id,
            "BookMetadataId": book_id,
            "IsRemoved": false,
        }));
        metadata.push(json!({
            "BookId": book_id,
            "Title": title,
        }));
    }

    for row in deleted_rows {
        let book_id = row.get::<String, _>("BOOK_ID");
        deleted_entitlement.push(json!({
            "BookId": book_id,
            "BookMetadataId": book_id,
        }));
        deleted_book_metadata.push(json!({
            "BookId": book_id,
        }));
    }

    for row in read_progress_rows {
        let book_id = row.get::<String, _>("BOOK_ID");
        let page = row.get::<i64, _>("PAGE").max(0) as u64;
        let completed = row.get::<bool, _>("COMPLETED");
        let last_modified = row.get::<String, _>("LAST_MODIFIED_DATE");
        new_reading_state.push(json!({
            "EntitlementId": book_id,
            "LastModified": last_modified,
            "StatusInfo": {
                "Status": if completed { "Finished" } else { "Reading" },
                "TimesStartedReading": if page > 0 { 1 } else { 0 },
            },
        }));
    }

    for row in tag_rows {
        let tag = row.get::<String, _>("TAG");
        new_tag.push(json!({
            "Name": tag,
            "Type": "BookTag",
        }));
    }

    Ok(KoboSyncDeltas {
        new_entitlement: entitlement,
        deleted_entitlement,
        new_tag,
        deleted_tag: vec![],
        new_book_metadata: metadata,
        deleted_book_metadata,
        new_reading_state,
        deleted_reading_state: vec![],
    })
}

pub(in crate::app::compat_runtime) async fn koreader_user_create() -> Response {
    (StatusCode::FORBIDDEN, "User creation is disabled").into_response()
}

pub(in crate::app::compat_runtime) async fn koreader_user_auth(headers: HeaderMap) -> Response {
    if !koreader_authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/vnd.koreader.v1+json"),
        )],
        Json(json!({ "authorized": "OK" })),
    )
        .into_response()
}

pub(in crate::app::compat_runtime) async fn koreader_get_progress(
    Extension(state): Extension<OperationalState>,
    Path(book_hash): Path<String>,
    headers: HeaderMap,
) -> Response {
    if !koreader_authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let Some(user_id_value) = resolved_koreader_user_id(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let target =
        match load_koreader_book_target(state.runtime.database_file.as_path(), &book_hash).await {
            Ok(Some(target)) => target,
            Ok(None) => return StatusCode::NOT_FOUND.into_response(),
            Err(KoreaderBookLookupError::Conflict) => {
                return (
                    StatusCode::CONFLICT,
                    Json(json!({ "error": "More than 1 book found with the same hash" })),
                )
                    .into_response();
            }
            Err(KoreaderBookLookupError::Persistence) => {
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };

    let Some(progress) = (match load_read_progress(
        state.runtime.database_file.as_path(),
        &target.id,
        &user_id_value,
    )
    .await
    {
        Ok(progress) => progress,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let locator = parse_locator_payload(progress.locator.as_deref());
    let percentage = locator
        .get("locations")
        .and_then(|value| value.get("totalProgression"))
        .and_then(Value::as_f64)
        .unwrap_or_else(|| {
            (progress.page.max(0) as f64 / target.page_count.max(1) as f64).clamp(0.0, 1.0)
        });
    let progress_value = locator
        .get("koreaderProgress")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| progress.page.max(0).to_string());

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/vnd.koreader.v1+json"),
        )],
        Json(KoreaderProgressPayload {
            document: book_hash,
            percentage,
            progress: progress_value,
            device: if progress.device_name.is_empty() {
                "KOReader".to_string()
            } else {
                progress.device_name
            },
            device_id: if progress.device_id.is_empty() {
                "koreader-device".to_string()
            } else {
                progress.device_id
            },
        }),
    )
        .into_response()
}

pub(in crate::app::compat_runtime) async fn koreader_put_progress(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !koreader_authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let Ok(payload) = serde_json::from_slice::<KoreaderProgressPayload>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    let Some(user_id_value) = resolved_koreader_user_id(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let target = match load_koreader_book_target(
        state.runtime.database_file.as_path(),
        payload.document.as_str(),
    )
    .await
    {
        Ok(Some(target)) => target,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(KoreaderBookLookupError::Conflict) => {
            return (
                StatusCode::CONFLICT,
                Json(json!({ "error": "More than 1 book found with the same hash" })),
            )
                .into_response();
        }
        Err(KoreaderBookLookupError::Persistence) => {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let percentage = payload.percentage.clamp(0.0, 1.0);
    let page =
        parse_koreader_progress_page(payload.progress.as_str(), target.page_count, percentage)
            as i64;
    let completed = percentage >= 1.0;
    let locator = json!({
        "koreaderProgress": payload.progress,
        "locations": {
            "position": page,
            "totalProgression": percentage,
        },
    });

    if persist_read_progress_with_locator(
        state.runtime.database_file.as_path(),
        &target.id,
        &user_id_value,
        page,
        completed,
        payload.device_id.as_str(),
        payload.device.as_str(),
        now_sync_marker().as_str(),
        Some(locator),
    )
    .await
    .is_err()
    {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    StatusCode::NO_CONTENT.into_response()
}

#[derive(Debug)]
enum KoreaderBookLookupError {
    Persistence,
    Conflict,
}

async fn resolved_kobo_user(
    auth_token: &str,
    headers: &HeaderMap,
    database_file: &FsPath,
) -> Option<AuthUser> {
    let token = auth_token.trim();
    if !token.is_empty()
        && let Some(AuthOutcome::Valid(user)) =
            persisted_api_key_user_by_token(token, database_file).await
    {
        return Some(user);
    }

    auth_token_user(headers)
}

fn resolved_koreader_user_id(headers: &HeaderMap) -> Option<String> {
    if let Some(user) = auth_token_user(headers) {
        return Some(user_id(&user).to_string());
    }

    headers
        .get("X-Auth-User")
        .or_else(|| headers.get("x-auth-user"))
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(|value| {
            configured_api_key().and_then(|api_key| {
                if value == api_key {
                    Some("koreader-api-key".to_string())
                } else {
                    None
                }
            })
        })
}

fn sanitize_identifier(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

fn parse_koreader_progress_page(progress: &str, page_count: u64, fallback_progress: f64) -> u64 {
    let normalized_page_count = page_count.max(1);
    let direct_page = progress
        .trim()
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .map(|value| value.min(normalized_page_count));
    if let Some(page) = direct_page {
        return page;
    }

    let fragment_page = progress
        .split(['[', ']', '#', '_', '.'])
        .filter_map(|part| part.parse::<u64>().ok())
        .find(|value| *value > 0)
        .map(|value| value.min(normalized_page_count));
    if let Some(page) = fragment_page {
        return page;
    }

    ((fallback_progress.clamp(0.0, 1.0) * normalized_page_count as f64).ceil() as u64)
        .clamp(1, normalized_page_count)
}

fn content_type_from_filename(file_name: &str, fallback: &str) -> String {
    let extension = file_name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default();

    match extension.as_str() {
        "cbz" => "application/vnd.comicbook+zip".to_string(),
        "cbr" => "application/vnd.comicbook-rar".to_string(),
        "pdf" => "application/pdf".to_string(),
        "epub" => "application/epub+zip".to_string(),
        "jpg" | "jpeg" => "image/jpeg".to_string(),
        "png" => "image/png".to_string(),
        "gif" => "image/gif".to_string(),
        "webp" => "image/webp".to_string(),
        _ => fallback.to_string(),
    }
}

fn parse_locator_payload(locator: Option<&[u8]>) -> Value {
    locator
        .and_then(|blob| serde_json::from_slice::<Value>(blob).ok())
        .unwrap_or_else(|| json!({}))
}

fn kobo_empty_reading_state_payload(book_id: &str, created_timestamp: &str) -> Value {
    json!({
        "Created": created_timestamp,
        "CurrentBookmark": {
            "LastModified": created_timestamp,
            "ProgressPercent": 0.0,
            "ContentSourceProgressPercent": 0.0,
            "Location": {
                "Source": Value::Null,
                "Type": "koboSpan",
                "Value": Value::Null,
            }
        },
        "EntitlementId": book_id,
        "LastModified": created_timestamp,
        "PriorityTimestamp": created_timestamp,
        "Statistics": {
            "LastModified": created_timestamp,
        },
        "StatusInfo": {
            "LastModified": created_timestamp,
            "Status": "ReadyToRead",
            "TimesStartedReading": 0,
        },
    })
}

fn kobo_reading_state_payload(
    book_id: &str,
    progress: &PersistedReadProgressRecord,
    page_count: u64,
    locator: Value,
) -> Value {
    let total_progression = locator
        .get("locations")
        .and_then(|value| value.get("totalProgression"))
        .and_then(Value::as_f64)
        .unwrap_or_else(|| {
            (progress.page.max(0) as f64 / page_count.max(1) as f64).clamp(0.0, 1.0)
        });
    let source_progression = locator
        .get("locations")
        .and_then(|value| value.get("progression"))
        .and_then(Value::as_f64)
        .unwrap_or(total_progression);
    let source = locator
        .get("href")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let kobo_span = locator
        .get("koboSpan")
        .and_then(Value::as_str)
        .unwrap_or_default();

    json!({
        "Created": progress.created,
        "CurrentBookmark": {
            "LastModified": progress.last_modified,
            "ProgressPercent": total_progression * 100.0,
            "ContentSourceProgressPercent": source_progression * 100.0,
            "Location": {
                "Source": if source.is_empty() { Value::Null } else { Value::String(source.to_string()) },
                "Type": "koboSpan",
                "Value": if kobo_span.is_empty() { Value::Null } else { Value::String(kobo_span.to_string()) },
            }
        },
        "EntitlementId": book_id,
        "LastModified": progress.last_modified,
        "PriorityTimestamp": progress.last_modified,
        "Statistics": {
            "LastModified": progress.last_modified,
        },
        "StatusInfo": {
            "LastModified": progress.last_modified,
            "Status": if progress.completed { "Finished" } else { "Reading" },
            "TimesStartedReading": 1,
        },
    })
}

async fn load_kobo_metadata_record(
    database_file: &FsPath,
    book_id: &str,
) -> Result<Option<KoboMetadataRecord>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1).await?;
    let row = sqlx::query(
        "SELECT COALESCE(bm.TITLE, b.NAME) AS TITLE, COALESCE(bm.SUMMARY, '') AS SUMMARY, \
                bm.RELEASE_DATE AS RELEASE_DATE, COALESCE(sm.LANGUAGE, 'en') AS LANGUAGE, \
                COALESCE(b.FILE_SIZE, 0) AS FILE_SIZE, b.NAME AS FILE_NAME \
         FROM BOOK b \
         LEFT \
         JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID \
         LEFT \
         JOIN SERIES_METADATA sm ON sm.SERIES_ID = b.SERIES_ID \
         WHERE b.ID = ? \
         AND b.DELETED_DATE IS NULL \
         LIMIT 1",
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await?;

    Ok(row.map(|row| KoboMetadataRecord {
        title: row.get::<String, _>("TITLE"),
        summary: row.get::<String, _>("SUMMARY"),
        release_date: row.get::<Option<String>, _>("RELEASE_DATE"),
        language: row.get::<String, _>("LANGUAGE"),
        file_size: row.get::<i64, _>("FILE_SIZE").max(0) as u64,
        file_name: row.get::<String, _>("FILE_NAME"),
    }))
}

async fn load_book_media_file(
    database_file: &FsPath,
    book_id: &str,
) -> Result<Option<PersistedBookMediaFile>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1).await?;
    let row = sqlx::query(
        "SELECT b.NAME AS FILE_NAME, b.URL AS BOOK_URL, l.ROOT AS LIBRARY_ROOT, \
                COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE \
         FROM BOOK b \
         JOIN LIBRARY l ON l.ID = b.LIBRARY_ID \
         LEFT \
         JOIN MEDIA m ON m.BOOK_ID = b.ID \
         WHERE b.ID = ? \
         AND b.DELETED_DATE IS NULL \
         LIMIT 1",
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await?;

    Ok(row.map(|row| {
        let file_name = row.get::<String, _>("FILE_NAME");
        let book_url = row.get::<String, _>("BOOK_URL");
        let library_root = row.get::<String, _>("LIBRARY_ROOT");
        PersistedBookMediaFile {
            file_name: file_name.clone(),
            media_type: content_type_from_filename(
                &file_name,
                row.get::<String, _>("MEDIA_TYPE").as_str(),
            ),
            file_path: PathBuf::from(library_root).join(book_url),
        }
    }))
}

async fn load_thumbnail_by_id(
    database_file: &FsPath,
    thumbnail_id: &str,
) -> Result<Option<(String, Vec<u8>)>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1).await?;
    let direct = sqlx::query(
        "SELECT MEDIA_TYPE, THUMBNAIL \
                     FROM THUMBNAIL_BOOK \
                     WHERE ID = ? \
                     LIMIT 1",
    )
    .bind(thumbnail_id)
    .fetch_optional(&pool)
    .await?;

    let row = if let Some(row) = direct {
        Some(row)
    } else {
        sqlx::query(
            "SELECT MEDIA_TYPE, THUMBNAIL \
             FROM THUMBNAIL_BOOK \
             WHERE BOOK_ID = ? \
             ORDER BY SELECTED DESC, LAST_MODIFIED_DATE DESC, ID ASC \
             LIMIT 1",
        )
        .bind(thumbnail_id)
        .fetch_optional(&pool)
        .await?
    };

    Ok(row.map(|row| {
        (
            row.get::<String, _>("MEDIA_TYPE"),
            row.get::<Vec<u8>, _>("THUMBNAIL"),
        )
    }))
}

async fn persisted_book_exists(database_file: &FsPath, book_id: &str) -> Result<bool, sqlx::Error> {
    if !database_file.exists() {
        return Ok(false);
    }

    let pool = connect_pool(database_file, 1).await?;
    let row = sqlx::query(
        "SELECT 1 AS FOUND \
                     FROM BOOK \
                     WHERE ID = ? \
                     AND DELETED_DATE IS NULL \
                     LIMIT 1",
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await?;
    Ok(row.is_some())
}

async fn load_book_page_count(database_file: &FsPath, book_id: &str) -> Result<u64, sqlx::Error> {
    if !database_file.exists() {
        return Ok(1);
    }

    let pool = connect_pool(database_file, 1).await?;
    let row = sqlx::query(
        "SELECT COALESCE(PAGE_COUNT, 0) AS PAGE_COUNT \
         FROM MEDIA \
         WHERE BOOK_ID = ? \
         LIMIT 1",
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await?;

    Ok(row
        .map(|row| row.get::<i64, _>("PAGE_COUNT").max(0) as u64)
        .unwrap_or(1)
        .max(1))
}

async fn load_book_last_epub_position_locator(
    database_file: &FsPath,
    book_id: &str,
) -> Result<Option<Value>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1).await?;
    let row = sqlx::query(
        "SELECT EXTENSION_CLASS, EXTENSION_VALUE_BLOB \
         FROM MEDIA \
         WHERE BOOK_ID = ? \
         LIMIT 1",
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    let extension_class = row
        .get::<Option<String>, _>("EXTENSION_CLASS")
        .unwrap_or_default();
    if !extension_class.is_empty()
        && !extension_class
            .to_ascii_lowercase()
            .contains("mediaextensionepub")
    {
        return Ok(None);
    }

    let Some(blob) = row.get::<Option<Vec<u8>>, _>("EXTENSION_VALUE_BLOB") else {
        return Ok(None);
    };

    let mut decoder = GzDecoder::new(blob.as_slice());
    let mut decoded = Vec::new();
    if decoder.read_to_end(&mut decoded).is_err() {
        return Ok(None);
    }

    let extension_json = match serde_json::from_slice::<Value>(&decoded) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };

    Ok(extension_json
        .get("positions")
        .and_then(Value::as_array)
        .and_then(|positions| positions.last().cloned()))
}

async fn load_book_created_timestamp(
    database_file: &FsPath,
    book_id: &str,
) -> Result<Option<String>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1).await?;
    let row = sqlx::query(
        "SELECT CREATED_DATE \
                     FROM BOOK \
                     WHERE ID = ? \
                     AND DELETED_DATE IS NULL \
                     LIMIT 1",
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await?;

    Ok(row
        .map(|row| row.get::<Option<String>, _>("CREATED_DATE"))
        .unwrap_or(None)
        .filter(|value| !value.trim().is_empty()))
}

async fn load_read_progress(
    database_file: &FsPath,
    book_id: &str,
    user_id_value: &str,
) -> Result<Option<PersistedReadProgressRecord>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1).await?;
    let row = sqlx::query(
        "SELECT PAGE, COMPLETED, CREATED_DATE, LAST_MODIFIED_DATE, \
                COALESCE(DEVICE_ID, '') AS DEVICE_ID, COALESCE(DEVICE_NAME, '') AS DEVICE_NAME, \
                LOCATOR \
         FROM READ_PROGRESS \
         WHERE BOOK_ID = ? \
         AND USER_ID = ? \
         LIMIT 1",
    )
    .bind(book_id)
    .bind(user_id_value)
    .fetch_optional(&pool)
    .await?;

    Ok(row.map(|row| PersistedReadProgressRecord {
        page: row.get::<i64, _>("PAGE"),
        completed: row.get::<bool, _>("COMPLETED"),
        created: row.get::<String, _>("CREATED_DATE"),
        last_modified: row.get::<String, _>("LAST_MODIFIED_DATE"),
        device_id: row.get::<String, _>("DEVICE_ID"),
        device_name: row.get::<String, _>("DEVICE_NAME"),
        locator: row
            .try_get::<Option<Vec<u8>>, _>("LOCATOR")
            .or_else(|_| row.try_get::<Option<Vec<u8>>, _>("locator"))
            .ok()
            .flatten(),
    }))
}

async fn persist_read_progress_with_locator(
    database_file: &FsPath,
    book_id: &str,
    user_id_value: &str,
    page: i64,
    completed: bool,
    device_id: &str,
    device_name: &str,
    last_modified: &str,
    locator: Option<Value>,
) -> Result<(), String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open read-progress db: {error}"))?;

    let user_exists = sqlx::query(
        "SELECT 1 \
         FROM USER \
         WHERE ID = ? \
         LIMIT 1",
    )
    .bind(user_id_value)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query read-progress user: {error}"))?
    .is_some();

    if !user_exists {
        return Err("read-progress user not found".to_string());
    }

    let locator_blob = locator
        .and_then(|value| serde_json::to_vec(&value).ok())
        .unwrap_or_default();

    sqlx::query(
        "INSERT INTO READ_PROGRESS ( BOOK_ID, USER_ID, PAGE, COMPLETED, DEVICE_ID, DEVICE_NAME, \
           LAST_MODIFIED_DATE, LOCATOR ) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(BOOK_ID, USER_ID) DO UPDATE \
         SET PAGE = excluded.PAGE, COMPLETED = excluded.COMPLETED, DEVICE_ID = \
           excluded.DEVICE_ID, \
             DEVICE_NAME = excluded.DEVICE_NAME, LOCATOR = excluded.LOCATOR, \
             LAST_MODIFIED_DATE = excluded.LAST_MODIFIED_DATE",
    )
    .bind(book_id)
    .bind(user_id_value)
    .bind(page.max(0))
    .bind(completed)
    .bind(device_id)
    .bind(device_name)
    .bind(last_modified)
    .bind(locator_blob)
    .execute(&pool)
    .await
    .map_err(|error| format!("persist read-progress with locator: {error}"))?;

    Ok(())
}

async fn load_koreader_book_target(
    database_file: &FsPath,
    book_hash: &str,
) -> Result<Option<KoreaderBookTarget>, KoreaderBookLookupError> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|_| KoreaderBookLookupError::Persistence)?;
    let rows = sqlx::query(
        "SELECT b.ID AS BOOK_ID, COALESCE(m.PAGE_COUNT, 0) AS PAGE_COUNT \
         FROM BOOK b \
         LEFT \
         JOIN MEDIA m ON m.BOOK_ID = b.ID \
         WHERE b.FILE_HASH_KOREADER = ? \
         AND b.DELETED_DATE IS NULL \
         ORDER BY b.ID ASC",
    )
    .bind(book_hash)
    .fetch_all(&pool)
    .await
    .map_err(|_| KoreaderBookLookupError::Persistence)?;

    if rows.is_empty() {
        return Ok(None);
    }
    if rows.len() > 1 {
        return Err(KoreaderBookLookupError::Conflict);
    }

    let row = rows.first().expect("koreader target row should exist");
    Ok(Some(KoreaderBookTarget {
        id: row.get::<String, _>("BOOK_ID"),
        page_count: row.get::<i64, _>("PAGE_COUNT").max(0) as u64,
    }))
}

fn koreader_authorized(headers: &HeaderMap) -> bool {
    auth_token_user(headers).is_some()
        || headers
            .get("X-Auth-User")
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .is_some_and(|value| configured_api_key().is_some_and(|api_key| value == api_key))
}

fn configured_api_key() -> Option<String> {
    std::env::var("KOMGA_COMPAT_API_KEY")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn configured_api_key_id() -> Option<String> {
    std::env::var("KOMGA_COMPAT_API_KEY_ID")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn configured_api_key_comment() -> Option<String> {
    std::env::var("KOMGA_COMPAT_API_KEY_COMMENT")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::{Extension, Path};
    use axum::http::{HeaderMap, StatusCode};
    use std::fs;
    use std::path::Path as FsPath;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_path(prefix: &str) -> PathBuf {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_millis();
        std::env::temp_dir().join(format!("{prefix}-{millis}.sqlite"))
    }

    async fn create_koreader_lookup_schema(database_file: &FsPath) {
        let pool = connect_pool(database_file, 1)
            .await
            .expect("koreader test db should open");
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS BOOK (ID varchar NOT NULL PRIMARY KEY, FILE_HASH_KOREADER \
               varchar NOT NULL DEFAULT '', DELETED_DATE datetime NULL)",
        )
        .execute(&pool)
        .await
        .expect("book table should be created");
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS MEDIA (BOOK_ID varchar NOT NULL PRIMARY KEY, PAGE_COUNT int \
               NOT NULL DEFAULT 0)",
        )
        .execute(&pool)
        .await
        .expect("media table should be created");
    }

    #[test]
    fn sanitize_identifier_normalizes_and_replaces_non_alnum() {
        assert_eq!(sanitize_identifier("Ab C_1?"), "ab-c-1-");
    }

    #[test]
    fn generated_kobo_api_token_is_non_hardcoded_and_identity_scoped() {
        let token = generated_kobo_api_token("auth-token-a", "user-a");
        assert_ne!(token, "e30=");
        assert!(token.starts_with("KOMGA."));

        let changed_auth_token = generated_kobo_api_token("auth-token-b", "user-a");
        let changed_user_token = generated_kobo_api_token("auth-token-a", "user-b");
        assert_ne!(token, changed_auth_token);
        assert_ne!(token, changed_user_token);
    }

    #[tokio::test]
    async fn resolved_kobo_user_returns_none_when_not_authenticated() {
        let headers = HeaderMap::new();
        assert!(
            resolved_kobo_user(
                "",
                &headers,
                FsPath::new("/tmp/komga-kobo-user-none.sqlite")
            )
            .await
            .is_none()
        );
    }

    #[test]
    fn parse_koreader_progress_page_supports_direct_fragment_and_fallback_modes() {
        assert_eq!(parse_koreader_progress_page("7", 10, 0.0), 7);
        assert_eq!(parse_koreader_progress_page("book[42].epub", 25, 0.0), 25);
        assert_eq!(parse_koreader_progress_page("chapter_3", 10, 0.0), 3);
        assert_eq!(parse_koreader_progress_page("unknown", 20, 0.21), 5);
    }

    #[test]
    fn content_type_from_filename_maps_supported_extensions() {
        assert_eq!(
            content_type_from_filename("volume.cbz", "application/octet-stream"),
            "application/vnd.comicbook+zip"
        );
        assert_eq!(
            content_type_from_filename("volume.cbr", "application/octet-stream"),
            "application/vnd.comicbook-rar"
        );
        assert_eq!(
            content_type_from_filename("book.epub", "application/octet-stream"),
            "application/epub+zip"
        );
        assert_eq!(
            content_type_from_filename("cover.webp", "application/octet-stream"),
            "image/webp"
        );
    }

    #[test]
    fn parse_locator_payload_returns_object_and_handles_invalid_json() {
        let valid = parse_locator_payload(Some(
            br#"{"href":"/chapter-1","locations":{"progression":0.2}}"#,
        ));
        assert_eq!(
            valid.get("href"),
            Some(&Value::String("/chapter-1".to_string()))
        );

        let invalid = parse_locator_payload(Some(br#"{not-json}"#));
        assert_eq!(invalid, json!({}));
    }

    #[test]
    fn decode_or_passthrough_sync_token_extracts_calibre_web_raw_token() {
        let calibre_payload = json!({
            "data": {
                "raw_kobo_store_token": "store.token.segment"
            }
        })
        .to_string();
        let encoded = STANDARD.encode(calibre_payload.as_bytes());

        let decoded = decode_or_passthrough_sync_token(encoded.as_str());
        assert_eq!(decoded, Some("store.token.segment".to_string()));
    }

    #[test]
    fn decode_or_passthrough_sync_token_keeps_komga_payload_json() {
        let payload = json!({
            "version": 1,
            "rawKoboSyncToken": "store.token.segment",
            "ongoingSyncPointId": "sync-1",
            "lastSuccessfulSyncPointId": null,
        })
        .to_string();
        let encoded = format!("KOMGA.{}", STANDARD_NO_PAD.encode(payload.as_bytes()));

        let decoded = decode_or_passthrough_sync_token(encoded.as_str());
        assert_eq!(decoded, Some(payload));
    }

    #[test]
    fn kobo_empty_reading_state_payload_uses_ready_defaults() {
        let payload = kobo_empty_reading_state_payload("book-1", "2026-01-01T00:00:00Z");
        assert_eq!(
            payload.get("EntitlementId"),
            Some(&Value::String("book-1".to_string()))
        );
        assert_eq!(
            payload.get("Created"),
            Some(&Value::String("2026-01-01T00:00:00Z".to_string()))
        );
        assert_eq!(
            payload.get("LastModified"),
            Some(&Value::String("2026-01-01T00:00:00Z".to_string()))
        );
        assert_eq!(
            payload
                .get("StatusInfo")
                .and_then(|value| value.get("Status")),
            Some(&Value::String("ReadyToRead".to_string()))
        );
    }

    #[test]
    fn kobo_reading_state_payload_prefers_locator_progress_values() {
        let progress = PersistedReadProgressRecord {
            page: 3,
            completed: false,
            created: "2026-01-01T00:00:00Z".to_string(),
            last_modified: "2026-01-02T00:00:00Z".to_string(),
            device_id: "device-a".to_string(),
            device_name: "KOReader".to_string(),
            locator: None,
        };
        let locator = json!({
            "href": "/chapter-2.xhtml",
            "koboSpan": "span-2",
            "locations": {
                "progression": 0.25,
                "totalProgression": 0.5,
            }
        });

        let payload = kobo_reading_state_payload("book-1", &progress, 10, locator);
        assert_eq!(
            payload
                .get("CurrentBookmark")
                .and_then(|value| value.get("ProgressPercent")),
            Some(&json!(50.0))
        );
        assert_eq!(
            payload
                .get("CurrentBookmark")
                .and_then(|value| value.get("ContentSourceProgressPercent")),
            Some(&json!(25.0))
        );
        assert_eq!(
            payload
                .get("CurrentBookmark")
                .and_then(|value| value.get("Location"))
                .and_then(|value| value.get("Source")),
            Some(&Value::String("/chapter-2.xhtml".to_string()))
        );
        assert_eq!(
            payload
                .get("CurrentBookmark")
                .and_then(|value| value.get("Location"))
                .and_then(|value| value.get("Value")),
            Some(&Value::String("span-2".to_string()))
        );
        assert_eq!(
            payload
                .get("StatusInfo")
                .and_then(|value| value.get("Status")),
            Some(&Value::String("Reading".to_string()))
        );
    }

    #[test]
    fn build_kobo_sync_events_initial_sync_uses_nested_dto_shape() {
        let mut books = HashMap::new();
        books.insert(
            "book-1".to_string(),
            KoboSyncBookSnapshot {
                id: "book-1".to_string(),
                title: "Book One".to_string(),
                summary: "summary".to_string(),
                release_date: Some("2026-01-01T00:00:00Z".to_string()),
                language: "en".to_string(),
                file_size: 123,
                page_count: 10,
                created: "2026-01-01T00:00:00Z".to_string(),
                last_modified: "2026-01-02T00:00:00Z".to_string(),
            },
        );

        let mut progress = HashMap::new();
        progress.insert(
            "book-1".to_string(),
            KoboSyncReadProgressSnapshot {
                page: 4,
                completed: false,
                created: "2026-01-01T00:00:00Z".to_string(),
                last_modified: "2026-01-03T00:00:00Z".to_string(),
                locator: Some(
                    json!({
                        "href": "/chapter-1.xhtml",
                        "koboSpan": "kobo.1.1",
                        "locations": {
                            "progression": 0.2,
                            "totalProgression": 0.4,
                        }
                    })
                    .to_string()
                    .into_bytes(),
                ),
            },
        );

        let mut readlists = HashMap::new();
        readlists.insert(
            "list-1".to_string(),
            KoboSyncReadListSnapshot {
                id: "list-1".to_string(),
                name: "On Deck".to_string(),
                created: "2026-01-01T00:00:00Z".to_string(),
                last_modified: "2026-01-03T00:00:00Z".to_string(),
                items: vec!["book-1".to_string()],
            },
        );

        let to = KoboSyncSnapshot {
            books,
            progress,
            readlists,
        };

        let events = build_kobo_sync_events(None, &to, "http://localhost:8080", "token-1");
        assert_eq!(events.len(), 2);

        let entitlement = events[0]
            .get("NewEntitlement")
            .expect("new entitlement expected");
        assert_eq!(
            entitlement
                .get("BookEntitlement")
                .and_then(|value| value.get("Id")),
            Some(&Value::String("book-1".to_string()))
        );
        assert_eq!(
            entitlement
                .get("BookMetadata")
                .and_then(|value| value.get("DownloadUrls"))
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .and_then(|item| item.get("Url")),
            Some(&Value::String(
                "http://localhost:8080/kobo/token-1/v1/books/book-1/file/epub".to_string()
            ))
        );
        assert_eq!(
            entitlement
                .get("ReadingState")
                .and_then(|value| value.get("CurrentBookmark"))
                .and_then(|value| value.get("Location"))
                .and_then(|value| value.get("Source")),
            Some(&Value::String("/chapter-1.xhtml".to_string()))
        );

        let tag = events[1].get("NewTag").expect("new tag expected");
        assert_eq!(
            tag.get("Tag")
                .and_then(|value| value.get("Id"))
                .and_then(Value::as_str),
            Some("list-1")
        );
        assert_eq!(
            tag.get("Tag")
                .and_then(|value| value.get("Items"))
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .and_then(|item| item.get("RevisionId"))
                .and_then(Value::as_str),
            Some("book-1")
        );
    }

    #[test]
    fn build_kobo_sync_events_incremental_sync_emits_changed_and_removed_shapes() {
        let from = KoboSyncSnapshot {
            books: HashMap::from([(
                "book-1".to_string(),
                KoboSyncBookSnapshot {
                    id: "book-1".to_string(),
                    title: "Old".to_string(),
                    summary: String::new(),
                    release_date: None,
                    language: "en".to_string(),
                    file_size: 1,
                    page_count: 10,
                    created: "2026-01-01T00:00:00Z".to_string(),
                    last_modified: "2026-01-01T00:00:00Z".to_string(),
                },
            )]),
            progress: HashMap::new(),
            readlists: HashMap::from([(
                "list-1".to_string(),
                KoboSyncReadListSnapshot {
                    id: "list-1".to_string(),
                    name: "List One".to_string(),
                    created: "2026-01-01T00:00:00Z".to_string(),
                    last_modified: "2026-01-01T00:00:00Z".to_string(),
                    items: vec!["book-1".to_string()],
                },
            )]),
        };
        let to = KoboSyncSnapshot {
            books: HashMap::from([(
                "book-2".to_string(),
                KoboSyncBookSnapshot {
                    id: "book-2".to_string(),
                    title: "New".to_string(),
                    summary: String::new(),
                    release_date: None,
                    language: "en".to_string(),
                    file_size: 1,
                    page_count: 10,
                    created: "2026-01-02T00:00:00Z".to_string(),
                    last_modified: "2026-01-02T00:00:00Z".to_string(),
                },
            )]),
            progress: HashMap::from([(
                "book-2".to_string(),
                KoboSyncReadProgressSnapshot {
                    page: 5,
                    completed: false,
                    created: "2026-01-02T00:00:00Z".to_string(),
                    last_modified: "2026-01-03T00:00:00Z".to_string(),
                    locator: None,
                },
            )]),
            readlists: HashMap::new(),
        };

        let events = build_kobo_sync_events(Some(&from), &to, "http://localhost:8080", "token-1");
        assert!(
            events
                .iter()
                .any(|event| event.get("NewEntitlement").is_some())
        );
        assert!(
            events
                .iter()
                .any(|event| event.get("ChangedEntitlement").is_some())
        );
        assert!(
            events
                .iter()
                .any(|event| event.get("ChangedReadingState").is_some())
        );
        assert!(events.iter().any(|event| event.get("DeletedTag").is_some()));

        let removed = events
            .iter()
            .find_map(|event| event.get("ChangedEntitlement"))
            .expect("removed entitlement expected");
        assert_eq!(
            removed
                .get("BookEntitlement")
                .and_then(|value| value.get("IsRemoved")),
            Some(&Value::Bool(true))
        );
    }

    #[tokio::test]
    async fn kobo_ping_rejects_requests_without_valid_auth() {
        let auth_db = super::super::AuthDatabaseState {
            database_file: unique_temp_path("komga-device-auth-ping"),
            remember_me_namespace: "test".to_string(),
        };
        let response = kobo_ping(
            Extension(auth_db),
            Path("invalid-token".to_string()),
            HeaderMap::new(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn koreader_user_auth_rejects_requests_without_auth() {
        let response = koreader_user_auth(HeaderMap::new()).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn koreader_user_create_returns_forbidden() {
        let response = koreader_user_create().await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn load_koreader_book_target_returns_unique_book_and_page_count() {
        let database_file = unique_temp_path("komga-device-auth-koreader-unique");
        create_koreader_lookup_schema(database_file.as_path()).await;

        let pool = connect_pool(database_file.as_path(), 1)
            .await
            .expect("koreader test db should open");
        sqlx::query(
            "INSERT INTO BOOK (ID, FILE_HASH_KOREADER, DELETED_DATE) \
                     VALUES (?, ?, NULL)",
        )
        .bind("book-1")
        .bind("hash-unique")
        .execute(&pool)
        .await
        .expect("book row should be inserted");
        sqlx::query(
            "INSERT INTO MEDIA (BOOK_ID, PAGE_COUNT) \
                     VALUES (?, ?)",
        )
        .bind("book-1")
        .bind(42)
        .execute(&pool)
        .await
        .expect("media row should be inserted");

        let target = load_koreader_book_target(database_file.as_path(), "hash-unique")
            .await
            .expect("unique hash should not fail")
            .expect("unique hash should resolve a book");
        assert_eq!(target.id, "book-1");
        assert_eq!(target.page_count, 42);

        let _ = fs::remove_file(database_file);
    }

    #[tokio::test]
    async fn load_koreader_book_target_reports_conflict_for_duplicate_hash() {
        let database_file = unique_temp_path("komga-device-auth-koreader-conflict");
        create_koreader_lookup_schema(database_file.as_path()).await;

        let pool = connect_pool(database_file.as_path(), 1)
            .await
            .expect("koreader test db should open");
        for book_id in ["book-a", "book-b"] {
            sqlx::query(
                "INSERT INTO BOOK (ID, FILE_HASH_KOREADER, DELETED_DATE) \
                 VALUES (?, ?, NULL)",
            )
            .bind(book_id)
            .bind("hash-dup")
            .execute(&pool)
            .await
            .expect("book row should be inserted");
        }

        let result = load_koreader_book_target(database_file.as_path(), "hash-dup").await;
        assert!(matches!(result, Err(KoreaderBookLookupError::Conflict)));

        let _ = fs::remove_file(database_file);
    }
}
