use super::*;
use axum::extract::State;
use std::sync::Arc;

pub async fn book_manifest(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_request_auth(&*app.services.runtime_identity, &headers).await {
        return response;
    }

    match build_persisted_book_manifest(&app, &headers, &book_id, ManifestVariant::Default).await {
        Ok(ManifestBuildOutcome::Found(content_type, payload)) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, content_type)],
            Json(payload),
        )
            .into_response(),
        Ok(ManifestBuildOutcome::BadRequest(message)) => {
            (StatusCode::BAD_REQUEST, Json(json!({ "error": message }))).into_response()
        }
        Ok(ManifestBuildOutcome::NotFound) => StatusCode::NOT_FOUND.into_response(),
        Ok(ManifestBuildOutcome::Forbidden) => StatusCode::FORBIDDEN.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn book_manifest_epub(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_request_auth(&*app.services.runtime_identity, &headers).await {
        return response;
    }

    match build_persisted_book_manifest(&app, &headers, &book_id, ManifestVariant::Epub).await {
        Ok(ManifestBuildOutcome::Found(content_type, payload)) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, content_type)],
            Json(payload),
        )
            .into_response(),
        Ok(ManifestBuildOutcome::BadRequest(message)) => {
            (StatusCode::BAD_REQUEST, Json(json!({ "error": message }))).into_response()
        }
        Ok(ManifestBuildOutcome::NotFound) => StatusCode::NOT_FOUND.into_response(),
        Ok(ManifestBuildOutcome::Forbidden) => StatusCode::FORBIDDEN.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn book_manifest_pdf(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_request_auth(&*app.services.runtime_identity, &headers).await {
        return response;
    }

    match build_persisted_book_manifest(&app, &headers, &book_id, ManifestVariant::Pdf).await {
        Ok(ManifestBuildOutcome::Found(content_type, payload)) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, content_type)],
            Json(payload),
        )
            .into_response(),
        Ok(ManifestBuildOutcome::BadRequest(message)) => {
            (StatusCode::BAD_REQUEST, Json(json!({ "error": message }))).into_response()
        }
        Ok(ManifestBuildOutcome::NotFound) => StatusCode::NOT_FOUND.into_response(),
        Ok(ManifestBuildOutcome::Forbidden) => StatusCode::FORBIDDEN.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn book_manifest_divina(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_request_auth(&*app.services.runtime_identity, &headers).await {
        return response;
    }

    match build_persisted_book_manifest(&app, &headers, &book_id, ManifestVariant::Divina).await {
        Ok(ManifestBuildOutcome::Found(content_type, payload)) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, content_type)],
            Json(payload),
        )
            .into_response(),
        Ok(ManifestBuildOutcome::BadRequest(message)) => {
            (StatusCode::BAD_REQUEST, Json(json!({ "error": message }))).into_response()
        }
        Ok(ManifestBuildOutcome::NotFound) => StatusCode::NOT_FOUND.into_response(),
        Ok(ManifestBuildOutcome::Forbidden) => StatusCode::FORBIDDEN.into_response(),
        Err(error) => internal_error_response(error),
    }
}
