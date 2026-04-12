use super::*;
use komga_domain::discovery::PageEnvelope;

fn optional_query_bool(query: &str, key: &str) -> Result<Option<bool>, ()> {
    match query_value(query, key) {
        Some(value) if value.eq_ignore_ascii_case("true") => Ok(Some(true)),
        Some(value) if value.eq_ignore_ascii_case("false") => Ok(Some(false)),
        Some(_) => Err(()),
        None => Ok(None),
    }
}

fn normalize_kotlin_unpaged_page_shape<T>(mut page: PageEnvelope<T>) -> PageEnvelope<T> {
    let normalized_size = page.total_elements.max(20);
    page.page = 0;
    page.size = normalized_size;
    page.total_pages = if page.total_elements == 0 {
        0
    } else {
        ((page.total_elements - 1) / normalized_size) + 1
    };
    page
}

fn deprecated_boolean_series_condition(kind: &str, value: bool) -> Value {
    json!({
        "type": kind,
        "operator": if value { "isTrue" } else { "isFalse" },
    })
}

fn deprecated_any_of_series_conditions(kind: &str, values: Vec<String>) -> Value {
    if values.len() == 1 {
        return json!({
            "type": kind,
            "operator": "is",
            "value": values.into_iter().next().unwrap_or_default(),
        });
    }

    json!({
        "type": "AnyOfSeries",
        "conditions": values
            .into_iter()
            .map(|value| json!({
                "type": kind,
                "operator": "is",
                "value": value,
            }))
            .collect::<Vec<_>>(),
    })
}

fn deprecated_series_query_condition(
    library_ids: Option<Vec<String>>,
    genres: Option<Vec<String>>,
    tags: Option<Vec<String>>,
    deleted: Option<bool>,
    oneshot: Option<bool>,
) -> Option<Value> {
    let mut conditions = Vec::new();

    if let Some(library_ids) = library_ids.filter(|values| !values.is_empty()) {
        conditions.push(deprecated_any_of_series_conditions(
            "LibraryId",
            library_ids,
        ));
    }
    if let Some(genres) = genres.filter(|values| !values.is_empty()) {
        conditions.push(deprecated_any_of_series_conditions("Genre", genres));
    }
    if let Some(tags) = tags.filter(|values| !values.is_empty()) {
        conditions.push(deprecated_any_of_series_conditions("Tag", tags));
    }
    if let Some(deleted) = deleted {
        conditions.push(deprecated_boolean_series_condition("Deleted", deleted));
    }
    if let Some(oneshot) = oneshot {
        conditions.push(deprecated_boolean_series_condition("OneShot", oneshot));
    }

    match conditions.len() {
        0 => None,
        1 => conditions.into_iter().next(),
        _ => Some(json!({
            "type": "AllOfSeries",
            "conditions": conditions,
        })),
    }
}

fn empty_series_page_response(page: usize, size: usize, unpaged: bool, sorted: bool) -> Response {
    Json(series_page_payload(
        PageEnvelope {
            content: vec![],
            page,
            size,
            total_elements: 0,
            total_pages: 0,
        },
        !unpaged,
        sorted,
    ))
    .into_response()
}

async fn series_feed(
    headers: HeaderMap,
    uri: Uri,
    auth_state: DiscoveryAuthState,
    database_file: &FsPath,
    sort_mode: PersistedSeriesSortMode,
    exclude_newly_added: bool,
    kotlin_unpaged_page_shape: bool,
) -> Response {
    if let Some(response) = require_request_auth(&headers, database_file).await {
        return response;
    }

    if !database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let query = uri.query().unwrap_or_default();
    let library_ids = requested_query_values(query, "library_id");
    let context = match auth_state
        .resolve_query_context_with_persistence(&headers, library_ids.as_deref(), database_file)
        .await
    {
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
                exclude_newly_added,
                ..SeriesFilterCriteria::default()
            },
            None,
            page,
            size,
            unpaged,
            vec![sort_mode],
        ),
    )
    .await
    {
        Ok(page) => {
            let (page, paged) = if unpaged && kotlin_unpaged_page_shape {
                (normalize_kotlin_unpaged_page_shape(page), true)
            } else {
                (page, !unpaged)
            };
            Json(series_page_payload(page, paged, true)).into_response()
        }
        Err(error) => internal_error_response(error),
    }
}

fn should_use_strict_runtime_shape(payload: Option<&Value>) -> bool {
    payload
        .and_then(|value| value.get("condition"))
        .and_then(|condition| condition.get("type"))
        .is_some()
}

pub async fn series_latest(
    headers: HeaderMap,
    uri: Uri,
    auth_state: DiscoveryAuthState,
    database_file: &FsPath,
) -> Response {
    series_feed(
        headers,
        uri,
        auth_state,
        database_file,
        PersistedSeriesSortMode::LastModifiedDesc,
        false,
        false,
    )
    .await
}

pub async fn series_deprecated_get(
    headers: HeaderMap,
    uri: Uri,
    auth_state: DiscoveryAuthState,
    database_file: &FsPath,
) -> Response {
    if let Some(response) = require_request_auth(&headers, database_file).await {
        return response;
    }

    if !database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let query = uri.query().unwrap_or_default();
    let requested_library_ids = requested_query_values(query, "library_id");
    let library_ids =
        remap_requested_library_ids_for_persisted(database_file, requested_library_ids.as_ref())
            .await;
    let genres = requested_query_values(query, "genre");
    let tags = requested_query_values(query, "tag");
    let context = match auth_state
        .resolve_query_context_with_persistence(&headers, library_ids.as_deref(), database_file)
        .await
    {
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
    let search = requested_query_values(query, "search")
        .and_then(|values| values.into_iter().next())
        .filter(|value| !value.trim().is_empty());
    let sorts = query_values(query, "sort")
        .into_iter()
        .map(decode_query_component)
        .collect::<Vec<_>>();
    let sorted = !sorts.is_empty() || search.is_some();
    let requested_non_empty_library_ids = requested_library_ids
        .as_ref()
        .is_some_and(|values| !values.is_empty());
    if requested_non_empty_library_ids && library_ids.is_none() {
        return empty_series_page_response(page, size, unpaged, sorted);
    }
    let payload = deprecated_series_query_condition(
        library_ids.clone(),
        genres.clone(),
        tags.clone(),
        deleted,
        oneshot,
    )
    .map(|condition| json!({ "condition": condition }));

    if let Some(runtime_response) = runtime_owned_series_list_response(
        &headers,
        &uri,
        payload.as_ref(),
        search.clone(),
        &auth_state,
        database_file,
        true,
    )
    .await
    {
        return runtime_response;
    }

    let sort_modes = if search.is_some() {
        vec![PersistedSeriesSortMode::RelevanceAsc]
    } else {
        vec![PersistedSeriesSortMode::TitleAsc]
    };

    match load_persisted_series_page(
        database_file,
        &context,
        PersistedSeriesBrowseQuery::from_filters(
            SeriesFilterCriteria {
                library_ids,
                deleted,
                oneshot,
                genres,
                tags,
                ..SeriesFilterCriteria::default()
            },
            search.clone(),
            page,
            size,
            unpaged,
            sort_modes,
        ),
    )
    .await
    {
        Ok(page) => Json(series_page_payload(page, !unpaged, sorted)).into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn series_alphabetical_groups(
    headers: HeaderMap,
    body: Value,
    auth_state: DiscoveryAuthState,
    database_file: &FsPath,
) -> Response {
    if !body.is_object() {
        return StatusCode::BAD_REQUEST.into_response();
    }

    if let Some(response) = require_request_auth(&headers, database_file).await {
        return response;
    }

    if !database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let filters = match parse_runtime_series_filters_with_mode(
        body.get("condition"),
        OperatorValidationMode::Strict,
    ) {
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

    let context = match auth_state
        .resolve_query_context_with_persistence(&headers, None, database_file)
        .await
    {
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
    if let Some(response) = require_request_auth(&headers, auth_db.database_file.as_path()).await {
        return response;
    }

    let payload = if body.is_empty() {
        return StatusCode::BAD_REQUEST.into_response();
    } else {
        match serde_json::from_slice::<Value>(&body) {
            Ok(Value::Object(object)) => Value::Object(object),
            Ok(_) | Err(_) => return StatusCode::BAD_REQUEST.into_response(),
        }
    };
    let full_text_search = extract_full_text_search(&payload);
    let strict_runtime_shape = should_use_strict_runtime_shape(Some(&payload));

    if auth_db.database_file.exists()
        && let Some(runtime_response) = runtime_owned_series_list_response(
            &headers,
            &uri,
            Some(&payload),
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
    series_feed(
        headers,
        uri,
        auth_state,
        database_file,
        PersistedSeriesSortMode::CreatedDesc,
        false,
        false,
    )
    .await
}

pub async fn series_updated(
    headers: HeaderMap,
    uri: Uri,
    auth_state: DiscoveryAuthState,
    database_file: &FsPath,
) -> Response {
    series_feed(
        headers,
        uri,
        auth_state,
        database_file,
        PersistedSeriesSortMode::LastModifiedDesc,
        true,
        true,
    )
    .await
}
