use axum::Json;
use axum::body::Bytes;
use axum::extract::{Extension, Path};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use komga_persistence::sqlite::connect_pool;
use oauth2::{
    AuthUrl, ClientId, ClientSecret, CsrfToken, RedirectUrl, TokenUrl, basic::BasicClient,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use std::path::Path as FsPath;

use crate::app::placeholder_auth::auth_token_user;

use super::{KoreaderProgress, OperationalState, ReadProgressState};

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

pub(in crate::app::compat_runtime) async fn oauth2_authorization(
    Extension(state): Extension<OperationalState>,
    Path(registration_id): Path<String>,
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
    let Ok(redirect_url) = RedirectUrl::new(format!(
        "http://127.0.0.1/login/oauth2/code/{}",
        client.registration_id
    )) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let oauth_client = BasicClient::new(ClientId::new(client.client_id.clone()))
        .set_client_secret(ClientSecret::new(client.client_secret.clone()))
        .set_auth_uri(auth_url)
        .set_token_uri(token_url)
        .set_redirect_uri(redirect_url);

    let (url, _) = oauth_client.authorize_url(CsrfToken::new_random).url();

    (
        StatusCode::FOUND,
        [(
            header::LOCATION,
            HeaderValue::from_str(url.as_str()).unwrap_or_else(|_| {
                HeaderValue::from_static("/login?server_redirect=Y&error=oauth2_invalid_redirect")
            }),
        )],
    )
        .into_response()
}

pub(in crate::app::compat_runtime) async fn oauth2_login_code(
    Extension(state): Extension<OperationalState>,
    Path(registration_id): Path<String>,
) -> Response {
    let known_provider = state
        .oauth2_clients
        .iter()
        .any(|client| client.registration_id == registration_id);

    let error = if known_provider {
        "oauth2_not_implemented"
    } else {
        "oauth2_provider_not_found"
    };

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

pub(in crate::app::compat_runtime) async fn kobo_ping(
    Path(auth_token): Path<String>,
    headers: HeaderMap,
) -> Response {
    if !kobo_authorized(&auth_token, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    "pong".into_response()
}

pub(in crate::app::compat_runtime) async fn kobo_initialization(
    Path(auth_token): Path<String>,
    headers: HeaderMap,
) -> Response {
    if !kobo_authorized(&auth_token, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    (
        StatusCode::OK,
        [(
            HeaderName::from_static("x-kobo-apitoken"),
            HeaderValue::from_static("e30="),
        )],
        Json(json!({
            "Resources": {
                "device_auth": format!("/kobo/{auth_token}/v1/auth/device"),
                "library_sync": format!("/kobo/{auth_token}/v1/library/sync"),
            }
        })),
    )
        .into_response()
}

pub(in crate::app::compat_runtime) async fn kobo_auth_device(
    Path(auth_token): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !kobo_authorized(&auth_token, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let payload =
        serde_json::from_slice::<KoboDeviceAuthRequest>(&body).unwrap_or(KoboDeviceAuthRequest {
            user_key: String::new(),
        });
    let (access_token, refresh_token, tracking_id) =
        deterministic_kobo_token_triplet(payload.user_key.as_str());

    Json(KoboDeviceAuthResponse {
        access_token,
        refresh_token,
        token_type: "Bearer",
        tracking_id,
        user_key: payload.user_key,
    })
    .into_response()
}

fn deterministic_kobo_token_triplet(user_key: &str) -> (String, String, String) {
    let key = user_key.trim();
    let source = if key.is_empty() { "anonymous" } else { key };
    let normalized: String = source
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();

    (
        format!("rust-kobo-access-{normalized}"),
        format!("rust-kobo-refresh-{normalized}"),
        format!("rust-kobo-tracking-{normalized}"),
    )
}

pub(in crate::app::compat_runtime) async fn kobo_library_sync(
    Extension(state): Extension<OperationalState>,
    Path(auth_token): Path<String>,
    headers: HeaderMap,
) -> Response {
    if !kobo_authorized(&auth_token, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let has_user_key = headers
        .get("x-kobo-userkey")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| !value.trim().is_empty());
    if !has_user_key {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let (new_entitlement, new_book_metadata) =
        match load_kobo_sync_deltas(state.runtime.database_file.as_path()).await {
            Ok(deltas) => deltas,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };

    Json(json!({
        "NewEntitlement": new_entitlement,
        "DeletedEntitlement": [],
        "NewTag": [],
        "DeletedTag": [],
        "NewBookMetadata": new_book_metadata,
        "DeletedBookMetadata": [],
    }))
    .into_response()
}

async fn load_kobo_sync_deltas(
    database_file: &FsPath,
) -> Result<(Vec<Value>, Vec<Value>), sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let rows = sqlx::query(
        "SELECT b.ID AS BOOK_ID, COALESCE(bm.TITLE, b.NAME) AS TITLE FROM BOOK b LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID WHERE b.DELETED_DATE IS NULL ORDER BY b.ID ASC",
    )
    .fetch_all(&pool)
    .await?;
    pool.close().await;

    let mut entitlement = Vec::with_capacity(rows.len());
    let mut metadata = Vec::with_capacity(rows.len());

    for row in rows {
        let book_id = row.get::<String, _>("BOOK_ID");
        let title = row.get::<String, _>("TITLE");
        entitlement.push(json!({
            "BookId": book_id,
            "BookMetadataId": book_id,
        }));
        metadata.push(json!({
            "BookId": book_id,
            "Title": title,
        }));
    }

    Ok((entitlement, metadata))
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
    Extension(state): Extension<ReadProgressState>,
    Path(book_hash): Path<String>,
    headers: HeaderMap,
) -> Response {
    if !koreader_authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let progress = {
        let guard = state
            .koreader_progress_by_hash
            .lock()
            .expect("koreader progress state lock should not be poisoned");
        guard.get(&book_hash).cloned()
    };

    let Some(progress) = progress else {
        return StatusCode::NOT_FOUND.into_response();
    };

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/vnd.koreader.v1+json"),
        )],
        Json(KoreaderProgressPayload {
            document: progress.document,
            percentage: progress.percentage,
            progress: progress.progress,
            device: progress.device,
            device_id: progress.device_id,
        }),
    )
        .into_response()
}

pub(in crate::app::compat_runtime) async fn koreader_put_progress(
    Extension(state): Extension<ReadProgressState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !koreader_authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let Ok(payload) = serde_json::from_slice::<KoreaderProgressPayload>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    let mut guard = state
        .koreader_progress_by_hash
        .lock()
        .expect("koreader progress state lock should not be poisoned");
    guard.insert(
        payload.document.clone(),
        KoreaderProgress {
            document: payload.document,
            percentage: payload.percentage,
            progress: payload.progress,
            device: payload.device,
            device_id: payload.device_id,
        },
    );

    StatusCode::NO_CONTENT.into_response()
}

fn kobo_authorized(auth_token: &str, headers: &HeaderMap) -> bool {
    configured_api_key().is_some_and(|api_key| auth_token == api_key)
        || auth_token_user(headers).is_some()
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
