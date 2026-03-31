use super::*;

pub async fn series(
    profile: RuntimeProfile,
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
    let search_regex = query_value(query, "search_regex").and_then(parse_search_regex);

    let context = match auth_state.resolve_query_context(&headers, library_ids.as_deref()) {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    let series_page = match load_persisted_series_page(
        database_file,
        &context,
        PersistedSeriesBrowseQuery {
            library_ids,
            collection_ids,
            titles: None,
            titles_excluded: None,
            titles_contains: None,
            titles_contains_excluded: None,
            titles_begins_with: None,
            titles_begins_with_excluded: None,
            titles_ends_with: None,
            titles_ends_with_excluded: None,
            title_sorts: None,
            title_sorts_excluded: None,
            title_sorts_contains: None,
            title_sorts_contains_excluded: None,
            title_sorts_begins_with: None,
            title_sorts_begins_with_excluded: None,
            title_sorts_ends_with: None,
            title_sorts_ends_with_excluded: None,
            deleted: None,
            oneshot: None,
            read_statuses: None,
            read_statuses_excluded: None,
            complete: None,
            genres: None,
            genres_excluded: None,
            genres_null: None,
            tags: None,
            tags_excluded: None,
            tags_null: None,
            languages: None,
            languages_excluded: None,
            publishers: None,
            publishers_excluded: None,
            age_ratings: None,
            age_ratings_excluded: None,
            age_ratings_null: None,
            age_rating_gt: None,
            age_rating_lt: None,
            sharing_labels: None,
            sharing_labels_excluded: None,
            sharing_labels_null: None,
            authors: None,
            authors_excluded: None,
            release_dates: None,
            release_dates_excluded: None,
            release_dates_null: None,
            release_date_gt: None,
            release_date_lt: None,
            release_date_begins_with: None,
            release_date_ends_with: None,
            release_date_contains_excluded: None,
            release_date_begins_with_excluded: None,
            release_date_ends_with_excluded: None,
            release_date_in_last_days: None,
            release_date_not_in_last_days: None,
            series_statuses: None,
            series_statuses_excluded: None,
            search,
            search_regex,
            page,
            size,
            unpaged,
            sort_modes: vec![PersistedSeriesSortMode::TitleAsc],
        },
    )
    .await
    {
        Ok(page) => page,
        Err(error) => return internal_error_response(error),
    };

    let mut response = Json(series_page_payload(series_page, !unpaged)).into_response();
    if wants_persisted_marker(&headers, None) {
        mark_persisted_owned(&mut response);
    } else if discovery_ownership_route(profile, &headers, DiscoveryShape::SeriesList)
        == DiscoveryOwnershipRoute::RuntimeOwned
    {
        mark_runtime_owned(&mut response);
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

    match load_persisted_series_page(
        database_file,
        &context,
        PersistedSeriesBrowseQuery {
            library_ids,
            collection_ids: None,
            titles: None,
            titles_excluded: None,
            titles_contains: None,
            titles_contains_excluded: None,
            titles_begins_with: None,
            titles_begins_with_excluded: None,
            titles_ends_with: None,
            titles_ends_with_excluded: None,
            title_sorts: None,
            title_sorts_excluded: None,
            title_sorts_contains: None,
            title_sorts_contains_excluded: None,
            title_sorts_begins_with: None,
            title_sorts_begins_with_excluded: None,
            title_sorts_ends_with: None,
            title_sorts_ends_with_excluded: None,
            deleted: None,
            oneshot: None,
            read_statuses: None,
            read_statuses_excluded: None,
            complete: None,
            genres: None,
            genres_excluded: None,
            genres_null: None,
            tags: None,
            tags_excluded: None,
            tags_null: None,
            languages: None,
            languages_excluded: None,
            publishers: None,
            publishers_excluded: None,
            age_ratings: None,
            age_ratings_excluded: None,
            age_ratings_null: None,
            age_rating_gt: None,
            age_rating_lt: None,
            sharing_labels: None,
            sharing_labels_excluded: None,
            sharing_labels_null: None,
            authors: None,
            authors_excluded: None,
            release_dates: None,
            release_dates_excluded: None,
            release_dates_null: None,
            release_date_gt: None,
            release_date_lt: None,
            release_date_begins_with: None,
            release_date_ends_with: None,
            release_date_contains_excluded: None,
            release_date_begins_with_excluded: None,
            release_date_ends_with_excluded: None,
            release_date_in_last_days: None,
            release_date_not_in_last_days: None,
            series_statuses: None,
            series_statuses_excluded: None,
            search: None,
            search_regex: None,
            page,
            size,
            unpaged,
            sort_modes: vec![PersistedSeriesSortMode::Latest],
        },
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

    let filters = RuntimeSeriesFilters {
        library_ids: remap_requested_library_ids_for_persisted(
            database_file,
            filters.library_ids.as_ref(),
        )
        .await,
        ..filters
    };

    let full_text_search = extract_full_text_search(&body);
    let search_regex = extract_regex_search(&body);

    let context = match auth_state.resolve_query_context(&headers, None) {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    match load_persisted_alphabetical_groups(
        database_file,
        &context,
        filters,
        full_text_search,
        search_regex,
    )
    .await
    {
        Ok(groups) => Json(Value::Array(groups)).into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn series_list(
    Extension(profile): Extension<RuntimeProfile>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let ownership_route = discovery_ownership_route(profile, &headers, DiscoveryShape::SeriesList);
    let payload = serde_json::from_slice::<Value>(&body).ok();
    let full_text_search = payload.as_ref().and_then(extract_full_text_search);

    if auth_db.database_file.exists()
        && let Some(mut runtime_response) = runtime_owned_series_list_response(
            &headers,
            &uri,
            payload.as_ref(),
            full_text_search.clone(),
            &auth_state,
            auth_db.database_file.as_path(),
            ownership_route == DiscoveryOwnershipRoute::RuntimeOwned,
        )
        .await
    {
        if ownership_route != DiscoveryOwnershipRoute::RuntimeOwned {
            runtime_response
                .headers_mut()
                .remove("x-komga-runtime-search-ownership");
        }
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
