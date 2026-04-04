use super::*;

fn optional_query_bool(query: &str, key: &str) -> Result<Option<bool>, ()> {
    match query_value(query, key) {
        Some(value) if value.eq_ignore_ascii_case("true") => Ok(Some(true)),
        Some(value) if value.eq_ignore_ascii_case("false") => Ok(Some(false)),
        Some(_) => Err(()),
        None => Ok(None),
    }
}

fn should_use_strict_runtime_shape(payload: Option<&Value>) -> bool {
    payload
        .and_then(|value| value.get("condition"))
        .and_then(|condition| condition.get("type"))
        .is_some()
}

pub async fn series(
    headers: HeaderMap,
    uri: Uri,
    auth_state: DiscoveryAuthState,
    database_file: &FsPath,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let query = uri.query().unwrap_or_default();
    if contains_legacy_search_query(query) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let requested_library_ids = requested_query_values(query, "library_id");
    let library_ids =
        remap_requested_library_ids_for_persisted(database_file, requested_library_ids.as_ref())
            .await
            .or(requested_library_ids);
    let collection_ids = requested_query_values(query, "collection_id");
    let search = query_value(query, "search").map(decode_query_component);
    let page = query_value(query, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);
    let unpaged = query_bool(query, "unpaged");

    let context = match auth_state.resolve_query_context(&headers, library_ids.as_deref()) {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    let series_page = match load_persisted_series_page(
        database_file,
        &context,
        PersistedSeriesBrowseQuery::from_filters(
            SeriesFilterCriteria {
                library_ids,
                collection_ids,
                ..SeriesFilterCriteria::default()
            },
            search,
            page,
            size,
            unpaged,
            vec![PersistedSeriesSortMode::TitleAsc],
        ),
    )
    .await
    {
        Ok(page) => page,
        Err(error) => return internal_error_response(error),
    };

    let mut response = Json(series_page_payload(series_page, !unpaged)).into_response();
    if wants_persisted_marker(&headers, None) {
        mark_persisted_owned(&mut response);
    }

    response
}

pub async fn series_latest(
    headers: HeaderMap,
    uri: Uri,
    auth_state: DiscoveryAuthState,
    database_file: &FsPath,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let query = uri.query().unwrap_or_default();
    let library_ids = requested_query_values(query, "library_id");
    let context = match auth_state.resolve_query_context(&headers, library_ids.as_deref()) {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    let page = query_value(query, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);
    let unpaged = query_bool(query, "unpaged");
    let deleted = match optional_query_bool(query, "deleted") {
        Ok(value) => value,
        Err(()) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let oneshot = match optional_query_bool(query, "oneshot") {
        Ok(value) => value,
        Err(()) => return StatusCode::BAD_REQUEST.into_response(),
    };

    match load_persisted_series_page(
        database_file,
        &context,
        PersistedSeriesBrowseQuery::from_filters(
            SeriesFilterCriteria {
                library_ids,
                deleted,
                oneshot,
                ..SeriesFilterCriteria::default()
            },
            None,
            page,
            size,
            unpaged,
            vec![PersistedSeriesSortMode::Latest],
        ),
    )
    .await
    {
        Ok(page) => Json(series_page_payload(page, !unpaged)).into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn series_alphabetical_groups(
    headers: HeaderMap,
    body: Value,
    auth_state: DiscoveryAuthState,
    database_file: &FsPath,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let filters = match parse_runtime_series_filters(body.get("condition")) {
        Ok(filters) => filters,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("invalid series alphabetical-groups request: {error:?}") })),
            )
                .into_response();
        }
    };

    let mut filters = filters;
    filters.criteria.library_ids = remap_requested_library_ids_for_persisted(
        database_file,
        filters.criteria.library_ids.as_ref(),
    )
    .await;

    let full_text_search = extract_full_text_search(&body);

    let context = match auth_state.resolve_query_context(&headers, None) {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    match load_persisted_alphabetical_groups(database_file, &context, filters, full_text_search)
        .await
    {
        Ok(groups) => Json(Value::Array(groups)).into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn series_list(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let payload = serde_json::from_slice::<Value>(&body).ok();
    let full_text_search = payload.as_ref().and_then(extract_full_text_search);
    let strict_runtime_shape = should_use_strict_runtime_shape(payload.as_ref());

    if auth_db.database_file.exists()
        && let Some(runtime_response) = runtime_owned_series_list_response(
            &headers,
            &uri,
            payload.as_ref(),
            full_text_search.clone(),
            &auth_state,
            auth_db.database_file.as_path(),
            strict_runtime_shape,
        )
        .await
    {
        return runtime_response;
    }

    if !auth_db.database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    invalid_runtime_series_list_response(DiscoveryError::InvalidSemantics(
        "unsupported runtime series filter combination".to_string(),
    ))
}

pub async fn series_new(
    headers: HeaderMap,
    uri: Uri,
    auth_state: DiscoveryAuthState,
    database_file: &FsPath,
) -> Response {
    series_latest(headers, uri, auth_state, database_file).await
}

pub async fn series_updated(
    headers: HeaderMap,
    uri: Uri,
    auth_state: DiscoveryAuthState,
    database_file: &FsPath,
) -> Response {
    series_latest(headers, uri, auth_state, database_file).await
}
