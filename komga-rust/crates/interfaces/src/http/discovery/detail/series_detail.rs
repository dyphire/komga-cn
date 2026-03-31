use super::*;

pub async fn series_detail(
    headers: HeaderMap,
    Path(series_id): Path<String>,
    auth_state: DiscoveryAuthState,
    database_file: &FsPath,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let requested_series_id = series_id.clone();
    let series_id = resolve_series_id_for_persisted(database_file, &series_id).await;

    let Some(resource) = (match load_persisted_series_resource(database_file, &series_id).await {
        Ok(resource) => resource,
        Err(error) => return internal_error_response(error),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let detail_context = DetailResourceContext {
        library_id: Some(resource.library_id.clone()),
        content: Some(DetailContentContext {
            age_rating: resource.age_rating,
            sharing_labels: resource.sharing_labels.clone(),
        }),
    };

    let detail_query_context =
        match auth_state.resolve_detail_query_context(&headers, &detail_context) {
            Ok(context) => context,
            Err(denial) => return detail_access_denial_response(denial),
        };
    let is_admin = detail_query_context.is_admin;
    let Some(series) = (match load_persisted_series_detail(
        database_file,
        &series_id,
        detail_query_context.user_id.as_deref(),
    )
    .await
    {
        Ok(series) => series,
        Err(error) => return internal_error_response(error),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let mut payload = series_detail_payload(&series, is_admin);
    if uses_id_bridge(&requested_series_id, &series_id) {
        coerce_library_id_for_id_bridge(&mut payload);
    }

    Json(payload).into_response()
}

pub async fn series_collections(
    headers: HeaderMap,
    Path(series_id): Path<String>,
    auth_state: DiscoveryAuthState,
    database_file: &FsPath,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let series_id = resolve_series_id_for_persisted(database_file, &series_id).await;

    let Some(resource) = (match load_persisted_series_resource(database_file, &series_id).await {
        Ok(resource) => resource,
        Err(error) => return internal_error_response(error),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let detail_context = DetailResourceContext {
        library_id: Some(resource.library_id),
        content: Some(DetailContentContext {
            age_rating: resource.age_rating,
            sharing_labels: resource.sharing_labels,
        }),
    };

    match auth_state.resolve_detail_query_context(&headers, &detail_context) {
        Ok(_) => match load_persisted_series_collections(database_file, &series_id).await {
            Ok(collections) => Json(series_collections_payload(&collections)).into_response(),
            Err(error) => internal_error_response(error),
        },
        Err(denial) => detail_access_denial_response(denial),
    }
}

pub async fn series_metadata_update(
    headers: HeaderMap,
    database_file: &FsPath,
    Path(series_id): Path<String>,
    body: Value,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    if !database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let body = match body.as_object() {
        Some(body) => body,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "series metadata update payload must be a JSON object" })),
            )
                .into_response();
        }
    };

    let existing = match load_existing_series_metadata(database_file, &series_id).await {
        Ok(Some(existing)) => existing,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return internal_error_response(error),
    };

    let title = body
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or(existing.title.as_str());
    let title_sort = body
        .get("titleSort")
        .and_then(Value::as_str)
        .unwrap_or(existing.title_sort.as_str());
    let summary = body
        .get("summary")
        .and_then(Value::as_str)
        .unwrap_or(existing.summary.as_str());

    match persist_series_metadata_update(database_file, &series_id, title, title_sort, summary)
        .await
    {
        Ok(true) => {
            if let Err(error) = refresh_series_search_document(database_file, &series_id).await {
                return internal_error_response(error);
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}
