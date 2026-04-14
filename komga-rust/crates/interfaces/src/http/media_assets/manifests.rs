use super::*;

pub async fn book_manifest(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_request_auth(&headers, auth_db.database_file.as_path()).await {
        return response;
    }

    match build_persisted_book_manifest(
        auth_db.database_file.as_path(),
        &headers,
        &book_id,
        ManifestVariant::Default,
    )
    .await
    {
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
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_request_auth(&headers, auth_db.database_file.as_path()).await {
        return response;
    }

    match build_persisted_book_manifest(
        auth_db.database_file.as_path(),
        &headers,
        &book_id,
        ManifestVariant::Epub,
    )
    .await
    {
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
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_request_auth(&headers, auth_db.database_file.as_path()).await {
        return response;
    }

    match build_persisted_book_manifest(
        auth_db.database_file.as_path(),
        &headers,
        &book_id,
        ManifestVariant::Pdf,
    )
    .await
    {
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
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_request_auth(&headers, auth_db.database_file.as_path()).await {
        return response;
    }

    match build_persisted_book_manifest(
        auth_db.database_file.as_path(),
        &headers,
        &book_id,
        ManifestVariant::Divina,
    )
    .await
    {
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
