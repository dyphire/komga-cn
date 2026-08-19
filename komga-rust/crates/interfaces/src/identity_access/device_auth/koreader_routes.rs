use axum::Json;
use axum::body::Bytes;
use axum::extract::{Extension, Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use komga_application::identity_access::{
    DeviceProgressError, KoreaderProgressUpdate, now_sync_marker,
};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::access_log::RequestConnectionInfo;
use crate::contracts::common::SpringErrorDto;
use crate::identity_access::device_auth::auth_resolvers::{
    raw_koreader_header_user, required_koreader_user, required_koreader_user_id,
};
use crate::identity_access::device_auth::device_progress_service;
use crate::state::IdentityAccessState;

const KOREADER_VENDOR_MEDIA_TYPE: &str = "application/vnd.koreader.v1+json";
const KOREADER_PROGRESS_PATH: &str = "/koreader/syncs/progress";
const KOREADER_PROGRESS_PATH_PREFIX: &str = "/koreader/syncs/progress/";

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
struct KoreaderProgressPayload {
    document: String,
    percentage: f64,
    progress: String,
    device: String,
    device_id: String,
}

#[derive(Serialize)]
struct KoreaderAuthorizationDto {
    authorized: &'static str,
}

fn koreader_auth_failure(status: StatusCode, header_user_presented: bool) -> Response {
    if status == StatusCode::UNAUTHORIZED && !header_user_presented {
        StatusCode::FORBIDDEN.into_response()
    } else {
        status.into_response()
    }
}

fn koreader_response_content_type(headers: &HeaderMap) -> &'static str {
    let accept = headers
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("*/*");

    if accept.split(',').any(|value| {
        let value = value.trim().to_ascii_lowercase();
        let media_type = value.split(';').next().map(str::trim).unwrap_or("");
        media_type == "application/vnd.koreader.v1+json"
    }) {
        KOREADER_VENDOR_MEDIA_TYPE
    } else {
        "application/json"
    }
}

fn koreader_progress_error_response(
    headers: &HeaderMap,
    status: StatusCode,
    message: &str,
    path: &str,
) -> Response {
    let reason = status.canonical_reason().unwrap_or("Error");
    (
        status,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_str(koreader_response_content_type(headers))
                .expect("koreader response content type should be valid"),
        )],
        Json(SpringErrorDto {
            error: reason.to_string(),
            message: message.to_string(),
            path: path.to_string(),
            status: status.as_u16(),
            timestamp: now_epoch_millis(),
        }),
    )
        .into_response()
}

pub(crate) async fn koreader_user_create(
    State(app): State<IdentityAccessState>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
) -> Response {
    let header_user_presented = raw_koreader_header_user(&headers).is_some();
    if let Err(status) =
        required_koreader_user(&app.identity, &headers, connection_info.remote_addr()).await
    {
        return koreader_auth_failure(status, header_user_presented);
    }

    (
        StatusCode::FORBIDDEN,
        Json(SpringErrorDto {
            error: "Forbidden".to_string(),
            message: "User creation is disabled".to_string(),
            path: "/koreader/users/create".to_string(),
            status: StatusCode::FORBIDDEN.as_u16(),
            timestamp: now_epoch_millis(),
        }),
    )
        .into_response()
}

pub(crate) async fn koreader_user_auth(
    State(app): State<IdentityAccessState>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
) -> Response {
    let header_user_presented = raw_koreader_header_user(&headers).is_some();
    match required_koreader_user(&app.identity, &headers, connection_info.remote_addr()).await {
        Ok(_) => {}
        Err(status) => return koreader_auth_failure(status, header_user_presented),
    }

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_str(koreader_response_content_type(&headers))
                .expect("koreader response content type should be valid"),
        )],
        Json(KoreaderAuthorizationDto { authorized: "OK" }),
    )
        .into_response()
}

pub(crate) async fn koreader_get_progress(
    State(app): State<IdentityAccessState>,
    Path(book_hash): Path<String>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
) -> Response {
    let user_id_value =
        match required_koreader_user_id(&app.identity, &headers, connection_info.remote_addr())
            .await
        {
            Ok(user_id_value) => user_id_value,
            Err(status) => return status.into_response(),
        };

    let progress_service = device_progress_service(
        app.identity.device_sync(),
        app.device_progress_reader.as_ref(),
        app.epub_navigation_content.as_ref(),
        app.progress.as_ref(),
    );
    let progress = match progress_service
        .koreader_progress(&book_hash, &user_id_value)
        .await
    {
        Ok(progress) => progress,
        Err(DeviceProgressError::NotFound) => {
            return koreader_progress_error_response(
                &headers,
                StatusCode::NOT_FOUND,
                "Book not found",
                &format!("{KOREADER_PROGRESS_PATH_PREFIX}{book_hash}"),
            );
        }
        Err(DeviceProgressError::NoProgress) => {
            return koreader_progress_error_response(
                &headers,
                StatusCode::OK,
                "No progress found for this book",
                &format!("{KOREADER_PROGRESS_PATH_PREFIX}{book_hash}"),
            );
        }
        Err(DeviceProgressError::Conflict) => {
            return koreader_progress_error_response(
                &headers,
                StatusCode::CONFLICT,
                "More than 1 book found with the same hash",
                &format!("{KOREADER_PROGRESS_PATH_PREFIX}{book_hash}"),
            );
        }
        Err(
            DeviceProgressError::BadRequest(_)
            | DeviceProgressError::UnsupportedMediaProfile
            | DeviceProgressError::Persistence,
        ) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_str(koreader_response_content_type(&headers))
                .expect("koreader response content type should be valid"),
        )],
        Json(KoreaderProgressPayload {
            document: book_hash,
            percentage: progress.percentage,
            progress: progress.progress,
            device: progress.device,
            device_id: progress.device_id,
        }),
    )
        .into_response()
}

pub(crate) async fn koreader_put_progress(
    State(app): State<IdentityAccessState>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let header_user_presented = raw_koreader_header_user(&headers).is_some();

    let user_id_value =
        match required_koreader_user_id(&app.identity, &headers, connection_info.remote_addr())
            .await
        {
            Ok(user_id_value) => user_id_value,
            Err(status) => return koreader_auth_failure(status, header_user_presented),
        };

    let Ok(payload) = serde_json::from_slice::<KoreaderProgressPayload>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    let progress_service = device_progress_service(
        app.identity.device_sync(),
        app.device_progress_reader.as_ref(),
        app.epub_navigation_content.as_ref(),
        app.progress.as_ref(),
    );
    if let Err(error) = progress_service
        .update_koreader_progress(
            &user_id_value,
            KoreaderProgressUpdate {
                document: payload.document,
                percentage: payload.percentage,
                progress: payload.progress,
                device: payload.device,
                device_id: payload.device_id,
                modified: now_sync_marker(),
            },
        )
        .await
    {
        return match error {
            DeviceProgressError::NotFound => koreader_progress_error_response(
                &headers,
                StatusCode::NOT_FOUND,
                "Book not found",
                KOREADER_PROGRESS_PATH,
            ),
            DeviceProgressError::Conflict => koreader_progress_error_response(
                &headers,
                StatusCode::CONFLICT,
                "More than 1 book found with the same hash",
                KOREADER_PROGRESS_PATH,
            ),
            DeviceProgressError::BadRequest(message) => koreader_progress_error_response(
                &headers,
                StatusCode::BAD_REQUEST,
                &message,
                KOREADER_PROGRESS_PATH,
            ),
            DeviceProgressError::UnsupportedMediaProfile => koreader_progress_error_response(
                &headers,
                StatusCode::NOT_FOUND,
                "Book has no media profile",
                KOREADER_PROGRESS_PATH,
            ),
            DeviceProgressError::NoProgress => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            DeviceProgressError::Persistence => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };
    }

    StatusCode::OK.into_response()
}

fn now_epoch_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
