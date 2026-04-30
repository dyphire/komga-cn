use super::readlists_support::{
    PersistedReadlistBooksQuery, PersistedReadlistWriteInput, merge_readlist_write_input,
};
use super::*;
use crate::helpers::validation_error_response;
use crate::state::HttpAppState;
use axum::extract::State;
use axum_extra::extract::{Multipart, multipart::MultipartRejection};
use icu::collator::{
    Collator,
    options::{CollatorOptions, Strength},
};
use icu::locale::locale;
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

const READLIST_SEARCH_CANDIDATE_LIMIT: usize = 1000;

pub async fn readlists(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Some(response) = require_request_auth(&*app.services.runtime_identity, &headers).await {
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
    let unpaged = query_bool(query_string, "unpaged");
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

    let requested_context = match app
        .discovery_auth
        .resolve_query_context_with_persistence(
            &*app.services.runtime_identity,
            &headers,
            library_ids.as_deref(),
        )
        .await
    {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };
    let visibility_context = match app
        .discovery_auth
        .resolve_query_context_with_persistence(&*app.services.runtime_identity, &headers, None)
        .await
    {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    let mut content =
        match load_persisted_readlists(&app, requested_context.authorized_library_ids.as_deref())
            .await
        {
            Ok(readlists) => readlists,
            Err(error) => return internal_error_response(error),
        };
    let search_ranks = match search.as_deref() {
        Some(search_term) => {
            let search_groups = search_term
                .split(',')
                .map(str::trim)
                .filter(|group| !group.is_empty())
                .collect::<Vec<_>>();
            if search_groups.is_empty() {
                None
            } else {
                let mut next_rank = 0_usize;
                let mut search_ranks = HashMap::new();
                for search_group in search_groups {
                    // Kotlin takes a bounded Lucene hit window first and filters for
                    // visibility afterward. Keeping the same fixed candidate window avoids
                    // hidden higher-ranked readlists crowding visible matches out of Rust's
                    // pre-filtered result set.
                    let ranked_hits = match app
                        .services
                        .discovery_persisted
                        .search_readlist_scored_ids(
                            search_group.to_string(),
                            READLIST_SEARCH_CANDIDATE_LIMIT,
                        )
                        .await
                    {
                        Ok(hits) => hits,
                        Err(error) => return internal_error_response(error),
                    };
                    for (_score, id) in ranked_hits {
                        if let std::collections::hash_map::Entry::Vacant(entry) =
                            search_ranks.entry(id)
                        {
                            entry.insert(next_rank);
                            next_rank += 1;
                        }
                    }
                }
                Some(search_ranks)
            }
        }
        None => None,
    };

    if let Some(search_ranks) = search_ranks.as_ref() {
        content.retain(|readlist| search_ranks.contains_key(readlist.id.as_str()));
    }

    let list_query = PersistedReadlistBooksQuery {
        page: 0,
        size: 20,
        unpaged: false,
        library_ids: None,
        deleted: None,
        tags: Vec::new(),
        read_statuses: Vec::new(),
        media_statuses: Vec::new(),
        authors: Vec::new(),
    };
    let requested_library_query =
        library_ids
            .clone()
            .map(|library_ids| PersistedReadlistBooksQuery {
                page: 0,
                size: 20,
                unpaged: false,
                library_ids: Some(library_ids),
                deleted: None,
                tags: Vec::new(),
                read_statuses: Vec::new(),
                media_statuses: Vec::new(),
                authors: Vec::new(),
            });

    let mut visible_content = Vec::with_capacity(content.len());
    for readlist in content {
        let Some(mut visible_readlist) = (match load_persisted_readlist_detail(
            &app,
            &readlist.id,
            visibility_context.authorized_library_ids.as_deref(),
        )
        .await
        {
            Ok(readlist) => readlist,
            Err(error) => return internal_error_response(error),
        }) else {
            continue;
        };

        if let Some(requested_library_query) = requested_library_query.as_ref() {
            let Some(requested_library_books) = (match load_visible_persisted_readlist_books(
                &app,
                &headers,
                &readlist.id,
                requested_library_query,
            )
            .await
            {
                Ok(books) => books,
                Err(error) => return internal_error_response(error),
            }) else {
                continue;
            };

            if requested_library_books.is_empty() {
                continue;
            }
        }

        let Some(visible_books) =
            (match load_visible_persisted_readlist_books(&app, &headers, &readlist.id, &list_query)
                .await
            {
                Ok(books) => books,
                Err(error) => return internal_error_response(error),
            })
        else {
            continue;
        };

        let visible_book_ids = visible_books
            .into_iter()
            .map(|book| book.id)
            .collect::<Vec<_>>();
        if visible_book_ids.is_empty() {
            if visible_readlist.book_ids.is_empty() && !visible_readlist.filtered {
                visible_content.push(visible_readlist);
            }
            continue;
        }

        visible_readlist.filtered =
            visible_readlist.filtered || visible_readlist.book_ids != visible_book_ids;
        visible_readlist.book_ids = visible_book_ids;
        visible_content.push(visible_readlist);
    }
    content = visible_content;

    match requested_sort
        .as_deref()
        .map(parse_readlists_sort)
        .unwrap_or(ReadListsSort::SearchOrName)
    {
        ReadListsSort::NameAsc => {
            sort_readlists_by_name(&mut content, false);
        }
        ReadListsSort::NameDesc => {
            sort_readlists_by_name(&mut content, true);
        }
        ReadListsSort::CreatedDateAsc => {
            content.sort_by(|left, right| left.created_date.cmp(&right.created_date));
        }
        ReadListsSort::CreatedDateDesc => {
            content.sort_by(|left, right| right.created_date.cmp(&left.created_date));
        }
        ReadListsSort::LastModifiedDateAsc => {
            content.sort_by(|left, right| left.last_modified_date.cmp(&right.last_modified_date));
        }
        ReadListsSort::LastModifiedDateDesc => {
            content.sort_by(|left, right| right.last_modified_date.cmp(&left.last_modified_date));
        }
        ReadListsSort::SearchOrName => {
            if let Some(search_ranks) = search_ranks.as_ref() {
                content.sort_by_key(|readlist| {
                    search_ranks
                        .get(readlist.id.as_str())
                        .copied()
                        .unwrap_or(usize::MAX)
                });
            } else {
                sort_readlists_by_name(&mut content, false);
            }
        }
    }

    let total_elements = content.len();
    let page_size = if unpaged {
        total_elements.max(20)
    } else {
        size.max(1)
    };
    let page_number = if unpaged { 0 } else { page };
    let page_content = if unpaged {
        content
    } else {
        let offset = page.saturating_mul(page_size);
        if offset >= total_elements {
            vec![]
        } else {
            content
                .into_iter()
                .skip(offset)
                .take(page_size)
                .collect::<Vec<_>>()
        }
    };
    let page = PageEnvelope::from_slice(page_content, page_number, page_size, total_elements);

    let mut response = Json(readlists_page_payload(page)).into_response();
    mark_runtime_owned(&mut response);
    response
}

fn sort_readlists_by_name(content: &mut [ReadListReadModel], descending: bool) {
    let collator = readlists_unicode_collator();
    content.sort_by(|left, right| {
        if descending {
            collator.compare(right.name.as_str(), left.name.as_str())
        } else {
            collator.compare(left.name.as_str(), right.name.as_str())
        }
    });
}

fn readlists_unicode_collator() -> icu::collator::CollatorBorrowed<'static> {
    let mut options = CollatorOptions::default();
    options.strength = Some(Strength::Tertiary);
    Collator::try_new(locale!("und").into(), options)
        .expect("unicode collator for readlists sorting should construct")
}

pub async fn readlist_create(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Some(response) = require_request_admin(&*app.services.runtime_identity, &headers).await {
        return response;
    }

    let payload = serde_json::from_slice::<Value>(&body).unwrap_or_else(|_| json!({}));
    let input = match parse_readlist_create_input(&payload) {
        Ok(input) => input,
        Err(response) => return response,
    };

    match load_persisted_readlists(&app, None).await {
        Ok(readlists)
            if readlists
                .iter()
                .any(|readlist| readlist.name.eq_ignore_ascii_case(&input.name)) =>
        {
            return readlist_create_bad_request("Read list name already exists");
        }
        Ok(_) => {}
        Err(error) => return internal_error_response(error),
    }

    let created_id = match persist_readlist_create(&app, &input).await {
        Ok(id) => id,
        Err(error) => return internal_error_response(error),
    };

    if let Err(error) = upsert_readlist_search_document(&app, &created_id).await {
        return internal_error_response(error);
    }

    match load_persisted_readlist_detail(&app, &created_id, None).await {
        Ok(Some(readlist)) => Json(readlist_payload(&readlist)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

#[allow(clippy::result_large_err)]
fn parse_readlist_create_input(payload: &Value) -> Result<PersistedReadlistWriteInput, Response> {
    let Some(payload) = payload.as_object() else {
        return Err(readlist_create_bad_request(
            "Request body must be a JSON object",
        ));
    };

    let name = match payload.get("name") {
        Some(value) => match value.as_str() {
            Some(value) => value,
            None => return Err(readlist_create_bad_request("name must be a string")),
        },
        None => {
            return Err(readlist_create_bad_request(
                "Required field 'name' is not present",
            ));
        }
    };
    let summary = match payload.get("summary") {
        Some(value) => match value.as_str() {
            Some(value) => value,
            None => return Err(readlist_create_bad_request("summary must be a string")),
        },
        None => "",
    };
    let ordered = match payload.get("ordered") {
        Some(value) => match value.as_bool() {
            Some(value) => value,
            None => return Err(readlist_create_bad_request("ordered must be a boolean")),
        },
        None => true,
    };
    let book_values = match payload.get("bookIds") {
        Some(value) => match value.as_array() {
            Some(value) => value,
            None => return Err(readlist_create_bad_request("bookIds must be an array")),
        },
        None => {
            return Err(readlist_create_bad_request(
                "Required field 'bookIds' is not present",
            ));
        }
    };

    let mut violations = Vec::new();
    if name.trim().is_empty() {
        violations.push(json!({
            "fieldName": "name",
            "message": "must not be blank",
        }));
    }
    if book_values.is_empty() {
        violations.push(json!({
            "fieldName": "bookIds",
            "message": "must not be empty",
        }));
    }

    let mut seen_book_ids = BTreeSet::new();
    let mut book_ids = Vec::with_capacity(book_values.len());
    let mut saw_duplicate_book_id = false;
    for value in book_values {
        let Some(book_id) = value.as_str() else {
            return Err(readlist_create_bad_request(
                "bookIds must be an array of strings",
            ));
        };
        let book_id = book_id.to_string();
        if !seen_book_ids.insert(book_id.clone()) {
            saw_duplicate_book_id = true;
            continue;
        }
        book_ids.push(book_id);
    }

    if saw_duplicate_book_id {
        violations.push(json!({
            "fieldName": "bookIds",
            "message": "must only contain unique elements",
        }));
    }

    if !violations.is_empty() {
        return Err(validation_error_response(violations));
    }

    Ok(PersistedReadlistWriteInput {
        name: name.to_string(),
        summary: summary.to_string(),
        ordered,
        book_ids,
    })
}

fn readlist_create_bad_request(message: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({
            "error": "Bad Request",
            "message": message,
            "path": "/api/v1/readlists",
            "status": 400,
            "timestamp": SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
        })),
    )
        .into_response()
}

pub async fn readlist_match_comicrack(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    multipart: Result<Multipart, MultipartRejection>,
) -> Response {
    if let Some(response) = require_request_admin(&*app.services.runtime_identity, &headers).await {
        return response;
    }

    let xml = match extract_comicrack_upload_xml(multipart).await {
        Ok(xml) => xml,
        Err(error) => return comicrack_bad_request_response(error.as_str()),
    };

    let request = match parse_comicrack_readlist(&xml) {
        Ok(request) => request,
        Err(error_code) => return comicrack_bad_request_response(error_code),
    };

    match match_comicrack_readlist(&app, &request).await {
        Ok(payload) => Json(payload).into_response(),
        Err(error) => internal_error_response(error),
    }
}

fn comicrack_bad_request_response(error_code: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({
            "error": "Bad Request",
            "message": error_code,
            "path": "/api/v1/readlists/match/comicrack",
            "status": 400,
            "timestamp": SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
        })),
    )
        .into_response()
}

pub async fn readlist_update(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
    body: Bytes,
) -> Response {
    if let Some(response) = require_request_admin(&*app.services.runtime_identity, &headers).await {
        return response;
    }

    let payload = serde_json::from_slice::<Value>(&body).unwrap_or_else(|_| json!({}));
    let Some(existing) = (match load_persisted_readlist_detail(&app, &readlist_id, None).await {
        Ok(readlist) => readlist,
        Err(error) => return internal_error_response(error),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let input = merge_readlist_write_input(&existing, &payload);

    match persist_readlist_update(&app, &readlist_id, &input).await {
        Ok(true) => {
            if let Err(error) = upsert_readlist_search_document(&app, &readlist_id).await {
                return internal_error_response(error);
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn readlist_delete(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
) -> Response {
    if let Some(response) = require_request_admin(&*app.services.runtime_identity, &headers).await {
        return response;
    }

    match delete_persisted_readlist(&app, &readlist_id).await {
        Ok(true) => {
            if let Err(error) = delete_readlist_search_document(&app, &readlist_id).await {
                return internal_error_response(error);
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn readlist_books(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
    uri: Uri,
) -> Response {
    if let Some(response) = require_request_auth(&*app.services.runtime_identity, &headers).await {
        return response;
    }

    let query = parse_persisted_readlist_books_query(uri.query().unwrap_or_default());
    let Some(mut visible_books) =
        (match load_visible_persisted_readlist_books(&app, &headers, &readlist_id, &query).await {
            Ok(books) => books,
            Err(error) => return internal_error_response(error),
        })
    else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let Some(context) = app
        .discovery_auth
        .resolve_query_context_with_persistence(
            &*app.services.runtime_identity,
            &headers,
            query.library_ids.as_deref(),
        )
        .await
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Some(readlist) = (match load_persisted_readlist_detail(
        &app,
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
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
) -> Response {
    if let Some(response) = require_request_auth(&*app.services.runtime_identity, &headers).await {
        return response;
    }

    let context = match app
        .discovery_auth
        .resolve_query_context_with_persistence(&*app.services.runtime_identity, &headers, None)
        .await
    {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };
    let detail_query = PersistedReadlistBooksQuery {
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

    match load_persisted_readlist_detail(
        &app,
        &readlist_id,
        context.authorized_library_ids.as_deref(),
    )
    .await
    {
        Ok(Some(mut readlist)) => {
            let Some(visible_books) = (match load_visible_persisted_readlist_books(
                &app,
                &headers,
                &readlist_id,
                &detail_query,
            )
            .await
            {
                Ok(books) => books,
                Err(error) => return internal_error_response(error),
            }) else {
                return StatusCode::NOT_FOUND.into_response();
            };

            let visible_book_ids = visible_books
                .into_iter()
                .map(|book| book.id)
                .collect::<Vec<_>>();
            if visible_book_ids.is_empty() {
                return StatusCode::NOT_FOUND.into_response();
            }

            readlist.filtered = readlist.book_ids != visible_book_ids;
            readlist.book_ids = visible_book_ids;
            return Json(readlist_payload(&readlist)).into_response();
        }
        Ok(None) => {}
        Err(error) => return internal_error_response(error),
    }

    StatusCode::NOT_FOUND.into_response()
}

pub async fn readlist_book_sibling_previous(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path((readlist_id, book_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_request_auth(&*app.services.runtime_identity, &headers).await {
        return response;
    }

    sibling_response(&app, &headers, &readlist_id, &book_id, false).await
}

pub async fn readlist_book_sibling_next(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path((readlist_id, book_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_request_auth(&*app.services.runtime_identity, &headers).await {
        return response;
    }

    sibling_response(&app, &headers, &readlist_id, &book_id, true).await
}

async fn sibling_response(
    app: &HttpAppState,
    headers: &HeaderMap,
    readlist_id: &str,
    book_id: &str,
    next: bool,
) -> Response {
    let auth_state = &app.discovery_auth;
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

    let Some(mut visible_books) =
        (match load_visible_persisted_readlist_books(app, headers, readlist_id, &query).await {
            Ok(books) => books,
            Err(error) => return internal_error_response(error),
        })
    else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let Some(context) = auth_state
        .resolve_query_context_with_persistence(&*app.services.runtime_identity, headers, None)
        .await
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Some(readlist) = (match load_persisted_readlist_detail(
        app,
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

async fn extract_comicrack_upload_xml(
    multipart: Result<Multipart, MultipartRejection>,
) -> Result<Vec<u8>, String> {
    let mut multipart = multipart.map_err(|rejection| rejection.body_text())?;

    loop {
        let field = multipart
            .next_field()
            .await
            .map_err(|error| error.body_text())?;
        let Some(field) = field else {
            return Err("Required request part 'file' is not present".to_string());
        };

        if field.name() != Some("file") {
            continue;
        }

        let bytes = field.bytes().await.map_err(|error| error.body_text())?;
        if bytes.is_empty() {
            return Err("ERR_1015".to_string());
        }

        return Ok(bytes.to_vec());
    }
}
