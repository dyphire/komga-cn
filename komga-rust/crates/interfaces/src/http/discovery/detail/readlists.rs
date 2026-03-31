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
            mark_runtime_owned(&mut response);
            return response;
        }
    }

    StatusCode::NOT_FOUND.into_response()
}

pub async fn readlist_create(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(operational): Extension<crate::http::state::OperationalState>,
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

    if let Err(error) = upsert_readlist_search_document(
        auth_db.database_file.as_path(),
        operational.runtime.lucene_data_directory.as_path(),
        &created_id,
    )
    .await
    {
        return internal_error_response(error);
    }

    match load_persisted_readlist_detail(auth_db.database_file.as_path(), &created_id, None).await {
        Ok(Some(readlist)) => Json(readlist_payload(&readlist)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn readlist_match_comicrack(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let xml = match extract_comicrack_upload_xml(&headers, &body) {
        Ok(xml) => xml,
        Err(error_code) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(comicrack_error_payload(error_code)),
            )
                .into_response();
        }
    };

    let request = match parse_comicrack_readlist(&xml) {
        Ok(request) => request,
        Err(error_code) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(comicrack_error_payload(error_code)),
            )
                .into_response();
        }
    };

    match match_comicrack_readlist(auth_db.database_file.as_path(), &request).await {
        Ok(payload) => Json(payload).into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn readlist_update(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(operational): Extension<crate::http::state::OperationalState>,
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
        Ok(true) => {
            if let Err(error) = upsert_readlist_search_document(
                auth_db.database_file.as_path(),
                operational.runtime.lucene_data_directory.as_path(),
                &readlist_id,
            )
            .await
            {
                return internal_error_response(error);
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn readlist_delete(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(operational): Extension<crate::http::state::OperationalState>,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match delete_persisted_readlist(auth_db.database_file.as_path(), &readlist_id).await {
        Ok(true) => {
            if let Err(error) = delete_readlist_search_document(
                operational.runtime.lucene_data_directory.as_path(),
                &readlist_id,
            )
            .await
            {
                return internal_error_response(error);
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn readlist_books(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
    uri: Uri,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let query = parse_persisted_readlist_books_query(uri.query().unwrap_or_default());
    let Some(mut visible_books) = (match load_visible_persisted_readlist_books(
        auth_db.database_file.as_path(),
        &auth_state,
        &headers,
        &readlist_id,
        &query,
    )
    .await
    {
        Ok(books) => books,
        Err(error) => return internal_error_response(error),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let Some(context) = auth_state.resolve_query_context(&headers, query.library_ids.as_deref())
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Some(readlist) = (match load_persisted_readlist_detail(
        auth_db.database_file.as_path(),
        &readlist_id,
        context.authorized_library_ids.as_deref(),
    )
    .await
    {
        Ok(readlist) => readlist,
        Err(error) => return internal_error_response(error),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    sort_visible_persisted_readlist_books(&mut visible_books, readlist.ordered);

    Json(book_details_page_payload(
        paginate_persisted_readlist_books(visible_books, &query),
        context.is_admin,
        !query.unpaged,
    ))
    .into_response()
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

    StatusCode::NOT_FOUND.into_response()
}

pub async fn readlist_book_sibling_previous(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    Path((readlist_id, book_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    sibling_response(
        auth_db.database_file.as_path(),
        &auth_state,
        &headers,
        &readlist_id,
        &book_id,
        false,
    )
    .await
}

pub async fn readlist_book_sibling_next(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    Path((readlist_id, book_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    sibling_response(
        auth_db.database_file.as_path(),
        &auth_state,
        &headers,
        &readlist_id,
        &book_id,
        true,
    )
    .await
}

async fn sibling_response(
    database_file: &FsPath,
    auth_state: &DiscoveryAuthState,
    headers: &HeaderMap,
    readlist_id: &str,
    book_id: &str,
    next: bool,
) -> Response {
    let query = PersistedReadlistBooksQuery {
        page: 0,
        size: 20,
        unpaged: true,
        library_ids: None,
        deleted: None,
        tags: Vec::new(),
        read_statuses: Vec::new(),
        media_statuses: Vec::new(),
        authors: Vec::new(),
    };

    let Some(mut visible_books) = (match load_visible_persisted_readlist_books(
        database_file,
        auth_state,
        headers,
        readlist_id,
        &query,
    )
    .await
    {
        Ok(books) => books,
        Err(error) => return internal_error_response(error),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let Some(context) = auth_state.resolve_query_context(headers, None) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Some(readlist) = (match load_persisted_readlist_detail(
        database_file,
        readlist_id,
        context.authorized_library_ids.as_deref(),
    )
    .await
    {
        Ok(readlist) => readlist,
        Err(error) => return internal_error_response(error),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    sort_visible_persisted_readlist_books(&mut visible_books, readlist.ordered);

    let visible_book_ids = visible_books
        .iter()
        .map(|book| book.id.as_str())
        .collect::<Vec<_>>();
    let Some(current_index) = visible_book_ids
        .iter()
        .position(|candidate| *candidate == book_id)
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let sibling_index = if next {
        current_index + 1
    } else if current_index == 0 {
        return StatusCode::NOT_FOUND.into_response();
    } else {
        current_index - 1
    };

    let Some(sibling) = visible_books.get(sibling_index) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    Json(book_detail_payload(sibling, context.is_admin)).into_response()
}

fn book_details_page_payload(
    page: PageEnvelope<BookDetailReadModel>,
    is_admin: bool,
    paged: bool,
) -> Value {
    let content = page
        .content
        .iter()
        .map(|book| book_detail_payload(book, is_admin))
        .collect::<Vec<_>>();
    let number_of_elements = content.len();
    let first = page.page == 0;
    let last = page.total_pages == 0 || page.page + 1 >= page.total_pages;
    let offset = if paged {
        page.page.saturating_mul(page.size)
    } else {
        0
    };

    json!({
        "content": content,
        "pageable": {
            "pageNumber": page.page,
            "pageSize": page.size,
            "sort": {
                "empty": false,
                "sorted": true,
                "unsorted": false
            },
            "offset": offset,
            "paged": paged,
            "unpaged": !paged
        },
        "last": last,
        "totalElements": page.total_elements,
        "totalPages": page.total_pages,
        "first": first,
        "size": page.size,
        "number": page.page,
        "sort": {
            "empty": false,
            "sorted": true,
            "unsorted": false
        },
        "numberOfElements": number_of_elements,
        "empty": number_of_elements == 0
    })
}

fn extract_comicrack_upload_xml(headers: &HeaderMap, body: &[u8]) -> Result<Vec<u8>, &'static str> {
    let Some(content_type) = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
    else {
        return Err("ERR_1015");
    };
    let Some(boundary) = content_type
        .split(';')
        .map(str::trim)
        .find_map(|part| part.strip_prefix("boundary="))
        .map(|boundary| boundary.trim_matches('"'))
    else {
        return Err("ERR_1015");
    };

    let body = String::from_utf8_lossy(body);
    let marker = format!("--{boundary}");
    for part in body.split(&marker) {
        if !part.contains("name=\"file\"") {
            continue;
        }
        let Some((_, content)) = part.split_once("\r\n\r\n") else {
            continue;
        };
        let content = content
            .trim_matches('\r')
            .trim_matches('\n')
            .trim_end_matches("--");
        if !content.trim().is_empty() {
            return Ok(content.as_bytes().to_vec());
        }
    }

    Err("ERR_1015")
}
