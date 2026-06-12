use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum_extra::extract::{Multipart, multipart::MultipartRejection};
use komga_application::discovery::{
    ReadlistMutationError, ReadlistMutationInput, parse_comicrack_readlist,
};
use komga_domain::discovery::PageEnvelope;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::time::{SystemTime, UNIX_EPOCH};

use super::detail_utils::internal_error_response;
use super::readlists_support::{
    comicrack_match_payload, merge_readlist_write_input, readlist_payload, readlists_page_payload,
};
use super::{BookDetailReadModel, book_detail_payload};
use crate::discovery::query::{resolve_readlist_books_query, resolve_readlists_query};
use crate::helpers::{to_domain_query_context, validation_error_response};
use crate::identity_access::auth::{Admin, Authenticated};
use crate::state::DiscoveryState;
pub(crate) async fn readlists(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let query = resolve_readlists_query(&uri);

    let requested_context = match app
        .discovery_auth
        .resolve_query_context_with_persistence(
            &app.identity,
            &headers,
            query.library_ids.as_deref(),
        )
        .await
    {
        Ok(Some(context)) => context,
        Ok(None) => return StatusCode::UNAUTHORIZED.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let visibility_context = match app
        .discovery_auth
        .resolve_query_context_with_persistence(&app.identity, &headers, None)
        .await
    {
        Ok(Some(context)) => context,
        Ok(None) => return StatusCode::UNAUTHORIZED.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let page = match app
        .persisted_sets
        .list_readlists(
            &to_domain_query_context(requested_context),
            &to_domain_query_context(visibility_context),
            query,
        )
        .await
    {
        Ok(page) => page,
        Err(error) => return internal_error_response(error),
    };

    Json(readlists_page_payload(page)).into_response()
}

pub(crate) async fn readlist_create(
    State(app): State<DiscoveryState>,
    _: Admin,
    body: Bytes,
) -> Response {
    let payload = match serde_json::from_slice::<Value>(&body) {
        Ok(value) => value,
        Err(_) => {
            return readlist_create_bad_request("Request body must be a JSON object");
        }
    };
    let input = match parse_readlist_create_input(&payload) {
        Ok(input) => input,
        Err(response) => return response,
    };

    let created = match app.persisted_sets.create_readlist(input).await {
        Ok(created) => created,
        Err(error) => return readlist_mutation_error_response(error, "/api/v1/readlists"),
    };

    match app
        .persisted_sets
        .readlist_for_mutation(&created.readlist_id)
        .await
    {
        Ok(Some(readlist)) => Json(readlist_payload(&readlist)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

#[allow(clippy::result_large_err)]
fn parse_readlist_create_input(payload: &Value) -> Result<ReadlistMutationInput, Response> {
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

    Ok(ReadlistMutationInput {
        name: name.to_string(),
        summary: summary.to_string(),
        ordered,
        book_ids,
    })
}

fn readlist_create_bad_request(message: &str) -> Response {
    readlist_bad_request(message, "/api/v1/readlists")
}

fn readlist_bad_request(message: &str, path: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({
            "error": "Bad Request",
            "message": message,
            "path": path,
            "status": 400,
            "timestamp": SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
        })),
    )
        .into_response()
}

fn readlist_mutation_error_response(error: ReadlistMutationError, path: &str) -> Response {
    match error {
        ReadlistMutationError::DuplicateName => {
            readlist_bad_request("Read list name already exists", path)
        }
        ReadlistMutationError::Persistence(error) => internal_error_response(error),
    }
}

pub(crate) async fn readlist_match_comicrack(
    State(app): State<DiscoveryState>,
    _: Admin,
    multipart: Result<Multipart, MultipartRejection>,
) -> Response {
    let xml = match extract_comicrack_upload_xml(multipart).await {
        Ok(xml) => xml,
        Err(error) => return comicrack_bad_request_response(error.as_str()),
    };

    let request = match parse_comicrack_readlist(&xml) {
        Ok(request) => request,
        Err(error) => return comicrack_bad_request_response(error.error_code()),
    };

    match app.persisted_sets.match_comicrack_readlist(&request).await {
        Ok(result) => Json(comicrack_match_payload(&result)).into_response(),
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

pub(crate) async fn readlist_update(
    State(app): State<DiscoveryState>,
    _: Admin,
    Path(readlist_id): Path<String>,
    body: Bytes,
) -> Response {
    let path = format!("/api/v1/readlists/{readlist_id}");
    let payload = match serde_json::from_slice::<Value>(&body) {
        Ok(value) => value,
        Err(_) => {
            return readlist_bad_request("Request body must be a JSON object", path.as_str());
        }
    };
    let Some(existing) = (match app.persisted_sets.readlist_for_mutation(&readlist_id).await {
        Ok(readlist) => readlist,
        Err(error) => return internal_error_response(error),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let input = merge_readlist_write_input(&existing, &payload);

    match app
        .persisted_sets
        .update_readlist(&readlist_id, input)
        .await
    {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => readlist_mutation_error_response(error, path.as_str()),
    }
}

pub(crate) async fn readlist_delete(
    State(app): State<DiscoveryState>,
    _: Admin,
    Path(readlist_id): Path<String>,
) -> Response {
    match app.persisted_sets.delete_readlist(&readlist_id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error.to_string()),
    }
}

pub(crate) async fn readlist_books(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
    uri: Uri,
) -> Response {
    let query = match resolve_readlist_books_query(readlist_id, &uri) {
        Ok(query) => query,
        Err(error) => return error.into_response(),
    };
    let response_context = match app
        .discovery_auth
        .resolve_query_context_with_persistence(
            &app.identity,
            &headers,
            query.library_ids.as_deref(),
        )
        .await
    {
        Ok(Some(context)) => context,
        Ok(None) => return StatusCode::UNAUTHORIZED.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let visibility_context = match app
        .discovery_auth
        .resolve_query_context_with_persistence(&app.identity, &headers, None)
        .await
    {
        Ok(Some(context)) => context,
        Ok(None) => return StatusCode::UNAUTHORIZED.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let paged = !query.unpaged;
    let page = match app
        .persisted_sets
        .list_readlist_books(&to_domain_query_context(visibility_context), query)
        .await
    {
        Ok(Some(page)) => page,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return internal_error_response(error),
    };

    Json(book_details_page_payload(
        page,
        response_context.is_admin,
        paged,
    ))
    .into_response()
}

pub(crate) async fn readlist_detail(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
) -> Response {
    let context = match app
        .discovery_auth
        .resolve_query_context_with_persistence(&app.identity, &headers, None)
        .await
    {
        Ok(Some(context)) => context,
        Ok(None) => return StatusCode::UNAUTHORIZED.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    match app
        .persisted_sets
        .readlist_detail(&to_domain_query_context(context), &readlist_id)
        .await
    {
        Ok(Some(readlist)) => Json(readlist_payload(&readlist)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(crate) async fn readlist_book_sibling_previous(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    Path((readlist_id, book_id)): Path<(String, String)>,
) -> Response {
    sibling_response(&app, &headers, &readlist_id, &book_id, false).await
}

pub(crate) async fn readlist_book_sibling_next(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    Path((readlist_id, book_id)): Path<(String, String)>,
) -> Response {
    sibling_response(&app, &headers, &readlist_id, &book_id, true).await
}

async fn sibling_response(
    app: &DiscoveryState,
    headers: &HeaderMap,
    readlist_id: &str,
    book_id: &str,
    next: bool,
) -> Response {
    let context = match app
        .discovery_auth
        .resolve_query_context_with_persistence(&app.identity, headers, None)
        .await
    {
        Ok(Some(context)) => context,
        Ok(None) => return StatusCode::UNAUTHORIZED.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let is_admin = context.is_admin;
    let sibling = match app
        .persisted_sets
        .readlist_book_sibling(
            &to_domain_query_context(context),
            readlist_id,
            book_id,
            next,
        )
        .await
    {
        Ok(Some(sibling)) => sibling,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return internal_error_response(error),
    };

    Json(book_detail_payload(&sibling, is_admin)).into_response()
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
