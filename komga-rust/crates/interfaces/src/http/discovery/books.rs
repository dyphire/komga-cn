use super::*;

fn should_use_strict_runtime_shape(payload: Option<&Value>, has_oneshot_bootstrap: bool) -> bool {
    has_oneshot_bootstrap
        || payload
            .and_then(|value| value.get("condition"))
            .and_then(|condition| condition.get("type"))
            .is_some()
}

pub async fn books(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !auth_db.database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    if auth_db.database_file.exists() {
        let query = uri.query().unwrap_or_default();
        let requested_library_ids = requested_query_values(query, "library_id");
        let library_ids = remap_requested_library_ids_for_persisted(
            auth_db.database_file.as_path(),
            requested_library_ids.as_ref(),
        )
        .await
        .or(requested_library_ids);

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
        let sort_values = query_values(query, "sort")
            .into_iter()
            .map(decode_query_component)
            .collect::<Vec<_>>();
        let search = query_value(query, "search").map(decode_query_component);

        let sort_modes = parse_persisted_books_sort_modes(&sort_values);
        match load_persisted_books_page(
            auth_db.database_file.as_path(),
            &context,
            PersistedBooksBrowseQuery::from_filters(
                BooksFilterCriteria {
                    library_ids,
                    ..BooksFilterCriteria::default()
                },
                search,
                page,
                size,
                unpaged,
                sort_modes,
            ),
        )
        .await
        {
            Ok(page) => {
                return Json(books_page_payload(page, context.is_admin, !unpaged)).into_response();
            }
            Err(error) => return internal_error_response(error),
        }
    }

    empty_books_page_response(&uri, false)
}

pub async fn books_list(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !auth_db.database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let payload = serde_json::from_slice::<Value>(&body).ok();
    if payload.as_ref().is_some_and(contains_legacy_search_input) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let full_text_search = payload.as_ref().and_then(extract_full_text_search);
    let is_exact_oneshot_bootstrap = exact_oneshot_bootstrap_series_id(payload.as_ref()).is_some();
    let strict_runtime_shape =
        should_use_strict_runtime_shape(payload.as_ref(), is_exact_oneshot_bootstrap);

    if (auth_db.database_file.exists() || is_exact_oneshot_bootstrap)
        && let Some(runtime_response) = runtime_owned_books_list_response(
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

    invalid_runtime_books_list_response(DiscoveryError::InvalidSemantics(
        "unsupported runtime books filter combination".to_string(),
    ))
}

pub async fn books_latest(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let query = uri.query().unwrap_or_default();
    let sorts = query_values(query, "sort");

    if auth_db.database_file.exists() && sorts.is_empty() {
        let requested_library_ids = requested_query_values(query, "library_id");
        let library_ids = remap_requested_library_ids_for_persisted(
            auth_db.database_file.as_path(),
            requested_library_ids.as_ref(),
        )
        .await
        .or(requested_library_ids);

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

        match load_persisted_books_page(
            auth_db.database_file.as_path(),
            &context,
            PersistedBooksBrowseQuery::from_filters(
                BooksFilterCriteria {
                    library_ids,
                    ..BooksFilterCriteria::default()
                },
                None,
                page,
                size,
                unpaged,
                vec![PersistedBooksSortMode::LastModifiedDateDesc],
            ),
        )
        .await
        {
            Ok(page) => {
                return Json(books_page_payload(page, context.is_admin, !unpaged)).into_response();
            }
            Err(error) => return internal_error_response(error),
        }
    }

    if auth_db.database_file.exists()
        && let Some(runtime_response) = runtime_owned_books_latest_response(
            &headers,
            &uri,
            &auth_state,
            auth_db.database_file.as_path(),
        )
        .await
    {
        return runtime_response;
    }

    if !auth_db.database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    invalid_runtime_books_list_response(DiscoveryError::InvalidSemantics(
        "unsupported runtime books latest filter combination".to_string(),
    ))
}

pub async fn books_ondeck(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !auth_db.database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let user_id = user_id(&user).to_string();

    match load_persisted_ondeck_books(auth_db.database_file.as_path(), &user_id).await {
        Ok(entries) => {
            let mut response = Json(books_page_for_entries(entries, &uri)).into_response();
            mark_runtime_owned(&mut response);
            response
        }
        Err(error) => internal_error_response(error),
    }
}

pub async fn books_duplicates(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !auth_db.database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    match load_persisted_duplicate_books(auth_db.database_file.as_path()).await {
        Ok(entries) => {
            let mut response = Json(books_page_for_entries(entries, &uri)).into_response();
            mark_runtime_owned(&mut response);
            response
        }
        Err(error) => internal_error_response(error),
    }
}

pub async fn book_tags(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !auth_db.database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let query = uri.query().unwrap_or_default();
    let library_ids = query_values(query, "library_id")
        .into_iter()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(decode_query_component)
        .collect::<Vec<_>>();
    let scope = query_value(query, "series_id")
        .filter(|value| !value.is_empty())
        .map(|value| PersistedBookTagsScope::Series(decode_query_component(value)))
        .or_else(|| {
            query_value(query, "readlist_id")
                .filter(|value| !value.is_empty())
                .map(|value| PersistedBookTagsScope::ReadList(decode_query_component(value)))
        })
        .or_else(|| {
            (!library_ids.is_empty()).then_some(PersistedBookTagsScope::Libraries(library_ids))
        });

    match load_persisted_book_tags(auth_db.database_file.as_path(), scope.as_ref()).await {
        Ok(tags) => Json(json!(tags)).into_response(),
        Err(error) => internal_error_response(error),
    }
}
