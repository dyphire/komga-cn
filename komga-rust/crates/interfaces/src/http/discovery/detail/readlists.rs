use super::*;

pub async fn readlists(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let query_string = uri.query().unwrap_or_default();
    let page = query_value(query_string, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query_string, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(20);
    let library_ids = {
        let values = query_values(query_string, "library_id")
            .into_iter()
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        (!values.is_empty()).then_some(values)
    };
    let search_values = query_values(query_string, "search")
        .into_iter()
        .map(decode_query_component)
        .collect::<Vec<_>>();
    let search = normalize_readlists_search(match search_values.as_slice() {
        [] => None,
        [single] => Some(single.clone()),
        _ => Some(search_values.join(",")),
    });
    let sort = query_values(query_string, "sort")
        .into_iter()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    let unpaged = query_bool(query_string, "unpaged");
    let requested_sort = sort.first().cloned();

    let context = match auth_state.resolve_query_context(&headers, library_ids.as_deref()) {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    if auth_db.database_file.exists() {
        let persisted_rows_exist =
            match persisted_readlists_exist(auth_db.database_file.as_path()).await {
                Ok(exists) => exists,
                Err(error) => return internal_error_response(error),
            };

        if persisted_rows_exist {
            let mut content = match load_persisted_readlists(
                auth_db.database_file.as_path(),
                context.authorized_library_ids.as_deref(),
            )
            .await
            {
                Ok(readlists) => readlists,
                Err(error) => return internal_error_response(error),
            };

            if let Some(search_term) = search.as_deref() {
                let tokens = search_term
                    .split(',')
                    .map(str::trim)
                    .filter(|token| !token.is_empty())
                    .map(str::to_ascii_lowercase)
                    .collect::<Vec<_>>();

                if !tokens.is_empty() {
                    content.retain(|readlist| {
                        let haystack = format!(
                            "{} {}",
                            readlist.name.to_ascii_lowercase(),
                            readlist.summary.to_ascii_lowercase(),
                        );

                        tokens.iter().any(|token| haystack.contains(token))
                    });
                }
            }

            match requested_sort
                .as_deref()
                .map(parse_readlists_sort)
                .unwrap_or(ReadListsSort::SearchOrName)
            {
                ReadListsSort::NameAsc => {
                    content.sort_by(|left, right| {
                        left.name
                            .to_ascii_lowercase()
                            .cmp(&right.name.to_ascii_lowercase())
                    });
                }
                ReadListsSort::NameDesc => {
                    content.sort_by(|left, right| {
                        right
                            .name
                            .to_ascii_lowercase()
                            .cmp(&left.name.to_ascii_lowercase())
                    });
                }
                ReadListsSort::SearchOrName => {
                    if let Some(search_term) = search.as_deref() {
                        let tokens = search_term
                            .split(',')
                            .map(str::trim)
                            .filter(|token| !token.is_empty())
                            .map(str::to_ascii_lowercase)
                            .collect::<Vec<_>>();

                        if !tokens.is_empty() {
                            content.sort_by(|left, right| {
                                let left_score = readlist_search_score(left, &tokens);
                                let right_score = readlist_search_score(right, &tokens);

                                right_score.cmp(&left_score).then_with(|| {
                                    left.name
                                        .to_ascii_lowercase()
                                        .cmp(&right.name.to_ascii_lowercase())
                                })
                            });
                        } else {
                            content.sort_by(|left, right| {
                                left.name
                                    .to_ascii_lowercase()
                                    .cmp(&right.name.to_ascii_lowercase())
                            });
                        }
                    } else {
                        content.sort_by(|left, right| {
                            left.name
                                .to_ascii_lowercase()
                                .cmp(&right.name.to_ascii_lowercase())
                        });
                    }
                }
            }

            let page_size = if size == 0 { 20 } else { size };
            let total_elements = content.len();
            let offset = page.saturating_mul(page_size);
            let page_content = if offset >= total_elements {
                vec![]
            } else {
                content
                    .into_iter()
                    .skip(offset)
                    .take(page_size)
                    .collect::<Vec<_>>()
            };
            let page = PageEnvelope::from_slice(page_content, page, page_size, total_elements);

            let mut response = Json(readlists_page_payload(page)).into_response();
            let _ = unpaged;
            mark_runtime_owned(&mut response);
            return response;
        }
    }

    let _ = (
        context,
        page,
        size,
        requested_sort,
        search,
        sort,
        unpaged,
        query_string,
    );
    StatusCode::NOT_FOUND.into_response()
}

pub async fn readlist_create(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let payload = serde_json::from_slice::<Value>(&body).unwrap_or_else(|_| json!({}));
    let input = readlist_write_input(&payload);

    let created_id = match persist_readlist_create(auth_db.database_file.as_path(), &input).await {
        Ok(id) => id,
        Err(error) => return internal_error_response(error),
    };

    match load_persisted_readlist_detail(auth_db.database_file.as_path(), &created_id, None).await {
        Ok(Some(readlist)) => Json(readlist_payload(&readlist)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn readlist_match_comicrack(headers: HeaderMap) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    Json(json!({
        "name": "ComicRack",
        "readLists": [],
        "unmatchedBooks": [],
    }))
    .into_response()
}

pub async fn readlist_update(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
    body: Bytes,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let payload = serde_json::from_slice::<Value>(&body).unwrap_or_else(|_| json!({}));
    let input = readlist_write_input(&payload);

    match persist_readlist_update(auth_db.database_file.as_path(), &readlist_id, &input).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn readlist_delete(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match delete_persisted_readlist(auth_db.database_file.as_path(), &readlist_id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn readlist_books(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
    uri: Uri,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let query_string = uri.query().unwrap_or_default();
    let page = query_value(query_string, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query_string, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);
    let unpaged = query_bool(query_string, "unpaged");
    let library_ids = {
        let values = query_values(query_string, "library_id")
            .into_iter()
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        (!values.is_empty()).then_some(values)
    };
    let read_statuses = readlist_query_values(query_string, "read_status");
    let media_statuses = readlist_query_values(query_string, "media_status");
    let tags = readlist_query_values(query_string, "tag");
    let authors = match readlist_author_query_values(query_string) {
        Ok(authors) => authors,
        Err(error) => {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": error }))).into_response();
        }
    };
    let deleted = query_value(query_string, "deleted").and_then(parse_optional_query_bool);

    let context = match auth_state.resolve_query_context(&headers, library_ids.as_deref()) {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    let _ = (
        context,
        readlist_id,
        page,
        size,
        unpaged,
        library_ids,
        read_statuses,
        media_statuses,
        tags,
        authors,
        deleted,
    );
    StatusCode::NOT_FOUND.into_response()
}

pub async fn readlist_detail(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let context = match auth_state.resolve_query_context(&headers, None) {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    if auth_db.database_file.exists() {
        match load_persisted_readlist_detail(
            auth_db.database_file.as_path(),
            &readlist_id,
            context.authorized_library_ids.as_deref(),
        )
        .await
        {
            Ok(Some(readlist)) => return Json(readlist_payload(&readlist)).into_response(),
            Ok(None) => {}
            Err(error) => return internal_error_response(error),
        }
    }

    let _ = context;
    StatusCode::NOT_FOUND.into_response()
}

pub async fn readlist_book_sibling_previous(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    Path((readlist_id, book_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let _ = (auth_state, readlist_id, book_id);
    StatusCode::NOT_FOUND.into_response()
}

pub async fn readlist_book_sibling_next(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    Path((readlist_id, book_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let _ = (auth_state, readlist_id, book_id);
    StatusCode::NOT_FOUND.into_response()
}
