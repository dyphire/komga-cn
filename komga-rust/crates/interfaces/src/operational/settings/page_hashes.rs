use axum::Json;
use axum::body::Bytes;
use axum::extract::Path as AxumPath;
use axum::extract::State;
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use komga_application::operational::{
    PageHashAction, PageHashDeleteError, PageHashDeleteMatch, PageHashKnownEntry,
    PageHashKnownQuery, PageHashMatchEntry, PageHashMatchesQuery, PageHashPage, PageHashSort,
    PageHashSortDirection, PageHashUnknownEntry, PageHashUnknownQuery, PageHashUpsertCommand,
};
use komga_application::operational::{
    PageHashKnownSortProperty, PageHashMatchSortProperty, PageHashUnknownSortProperty,
};
use serde::{Deserialize, Serialize};

use crate::identity_access::auth::Admin;

use super::{query_value, query_values};
use crate::state::OperationalApiState;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeletePageHashMatchRequest {
    book_id: String,
    url: String,
    page_number: i64,
    file_name: String,
    file_size: i64,
    media_type: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PutPageHashRequest {
    hash: String,
    size: Option<i64>,
    action: String,
}

pub(crate) async fn get_page_hashes(
    State(app): State<OperationalApiState>,
    _admin: Admin,
    uri: Uri,
) -> Response {
    let query = uri.query().unwrap_or_default();
    let actions = match parse_page_hash_actions(query_values(query, "action")) {
        Ok(actions) => actions,
        Err(status) => return status.into_response(),
    };

    let page_data = match app
        .page_hash_control
        .load_page_hashes(PageHashKnownQuery {
            page: page_query(query),
            size: size_query(query),
            actions,
            sorts: page_hash_known_sorts(query),
        })
        .await
    {
        Ok(page_data) => page_data,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    Json(known_page_hash_page_payload(&page_data)).into_response()
}

fn parse_page_hash_actions(raw_values: Vec<String>) -> Result<Vec<PageHashAction>, StatusCode> {
    let mut actions = Vec::new();

    for raw_value in raw_values {
        for action in raw_value.split(',') {
            let Some(action) = page_hash_action(action) else {
                return Err(StatusCode::BAD_REQUEST);
            };
            actions.push(action);
        }
    }

    Ok(actions)
}

fn page_query(query: &str) -> u64 {
    query_value(query, "page")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0)
}

fn size_query(query: &str) -> u64 {
    query_value(query, "size")
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(20)
}

pub(crate) async fn get_page_hashes_unknown(
    State(app): State<OperationalApiState>,
    _admin: Admin,
    uri: Uri,
) -> Response {
    let query = uri.query().unwrap_or_default();

    let page_data = match app
        .page_hash_control
        .load_unknown_page_hashes(PageHashUnknownQuery {
            page: page_query(query),
            size: size_query(query),
            sorts: page_hash_unknown_sorts(query),
        })
        .await
    {
        Ok(page_data) => page_data,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    Json(unknown_page_hash_page_payload(&page_data)).into_response()
}

pub(crate) async fn get_page_hash_matches(
    State(app): State<OperationalApiState>,
    _admin: Admin,
    AxumPath(page_hash): AxumPath<String>,
    uri: Uri,
) -> Response {
    let query = uri.query().unwrap_or_default();

    let page_data = match app
        .page_hash_control
        .load_page_hash_matches(PageHashMatchesQuery {
            hash: page_hash,
            page: page_query(query),
            size: size_query(query),
            sorts: page_hash_match_sorts(query),
        })
        .await
    {
        Ok(page_data) => page_data,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    Json(page_hash_matches_page_payload(&page_data)).into_response()
}

pub(crate) async fn get_page_hash_thumbnail(
    State(app): State<OperationalApiState>,
    _admin: Admin,
    AxumPath(page_hash): AxumPath<String>,
) -> Response {
    let thumbnail = match app
        .page_hash_control
        .load_page_hash_thumbnail(&page_hash)
        .await
    {
        Ok(Some(thumbnail)) => thumbnail,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    (
        [(header::CONTENT_TYPE, thumbnail.media_type.as_str())],
        thumbnail.bytes,
    )
        .into_response()
}

pub(crate) async fn get_page_hash_unknown_thumbnail(
    State(app): State<OperationalApiState>,
    _admin: Admin,
    AxumPath(page_hash): AxumPath<String>,
    uri: Uri,
) -> Response {
    let query = uri.query().unwrap_or_default();
    let resize_to = match query_value(query, "resize") {
        None => None,
        Some(value) => match value.parse::<u32>() {
            Ok(parsed) if parsed > 0 => Some(parsed),
            _ => return StatusCode::BAD_REQUEST.into_response(),
        },
    };

    let thumbnail = match app
        .page_hash_control
        .load_unknown_page_hash_thumbnail(&page_hash, resize_to)
        .await
    {
        Ok(Some(thumbnail)) => thumbnail,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    (
        [(header::CONTENT_TYPE, thumbnail.media_type.as_str())],
        thumbnail.bytes,
    )
        .into_response()
}

pub(crate) async fn put_page_hash(
    State(app): State<OperationalApiState>,
    _admin: Admin,
    body: Bytes,
) -> Response {
    let Ok(payload) = serde_json::from_slice::<PutPageHashRequest>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(action) = page_hash_action(&payload.action) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Ok(command) = PageHashUpsertCommand::new(payload.hash, payload.size, action) else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    match app.page_hash_control.upsert_page_hash(command).await {
        Ok(()) => StatusCode::ACCEPTED.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) async fn post_page_hash_delete_all(
    State(app): State<OperationalApiState>,
    _admin: Admin,
    AxumPath(page_hash): AxumPath<String>,
) -> Response {
    match app.page_hash_control.enqueue_delete_all(&page_hash).await {
        Ok(()) => StatusCode::ACCEPTED.into_response(),
        Err(error) => page_hash_delete_error_response(error),
    }
}

pub(crate) async fn post_page_hash_delete_match(
    State(app): State<OperationalApiState>,
    _admin: Admin,
    AxumPath(page_hash): AxumPath<String>,
    body: Bytes,
) -> Response {
    let Ok(DeletePageHashMatchRequest {
        book_id,
        url: _url,
        page_number,
        file_name,
        file_size,
        media_type,
    }) = serde_json::from_slice::<DeletePageHashMatchRequest>(&body)
    else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    match app
        .page_hash_control
        .enqueue_delete_match(PageHashDeleteMatch {
            book_id,
            page_hash,
            page_number,
            file_name,
            file_size,
            media_type,
        })
        .await
    {
        Ok(()) => StatusCode::ACCEPTED.into_response(),
        Err(error) => page_hash_delete_error_response(error),
    }
}

fn page_hash_delete_error_response(error: PageHashDeleteError) -> Response {
    match error {
        PageHashDeleteError::LoadTargets(_) | PageHashDeleteError::Enqueue(_) => {
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PageHashPagePayload<C> {
    content: Vec<C>,
    pageable: PageHashPageablePayload,
    last: bool,
    total_elements: u64,
    total_pages: u64,
    first: bool,
    size: u64,
    number: u64,
    sort: PageHashSortStatePayload,
    number_of_elements: u64,
    empty: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PageHashPageablePayload {
    page_number: u64,
    page_size: u64,
    sort: PageHashSortStatePayload,
    offset: u64,
    paged: bool,
    unpaged: bool,
}

#[derive(Serialize)]
struct PageHashSortStatePayload {
    empty: bool,
    sorted: bool,
    unsorted: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct KnownPageHashPayload<'a> {
    hash: &'a str,
    size: Option<i64>,
    action: &'static str,
    delete_count: i64,
    match_count: i64,
    created: &'a str,
    last_modified: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UnknownPageHashPayload<'a> {
    hash: &'a str,
    size: Option<i64>,
    match_count: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PageHashMatchPayload<'a> {
    book_id: &'a str,
    url: &'a str,
    page_number: i64,
    file_name: &'a str,
    file_size: i64,
    media_type: &'a str,
}

fn known_page_hash_page_payload(
    page: &PageHashPage<PageHashKnownEntry>,
) -> PageHashPagePayload<KnownPageHashPayload<'_>> {
    page_hash_page_payload(
        page,
        page.content.iter().map(known_page_hash_payload).collect(),
    )
}

fn unknown_page_hash_page_payload(
    page: &PageHashPage<PageHashUnknownEntry>,
) -> PageHashPagePayload<UnknownPageHashPayload<'_>> {
    page_hash_page_payload(
        page,
        page.content.iter().map(unknown_page_hash_payload).collect(),
    )
}

fn page_hash_matches_page_payload(
    page: &PageHashPage<PageHashMatchEntry>,
) -> PageHashPagePayload<PageHashMatchPayload<'_>> {
    page_hash_page_payload(
        page,
        page.content.iter().map(page_hash_match_payload).collect(),
    )
}

fn page_hash_page_payload<C>(
    page: &PageHashPage<impl Sized>,
    content: Vec<C>,
) -> PageHashPagePayload<C> {
    let sort = page_hash_sort_state_payload(page.sorted);
    let number_of_elements = page.number_of_elements();

    PageHashPagePayload {
        content,
        pageable: page_hash_pageable_payload(page),
        last: page.total_pages == 0 || page.page + 1 >= page.total_pages,
        total_elements: page.total_elements,
        total_pages: page.total_pages,
        first: page.page == 0,
        size: page.size,
        number: page.page,
        sort,
        number_of_elements,
        empty: number_of_elements == 0,
    }
}

fn page_hash_pageable_payload(page: &PageHashPage<impl Sized>) -> PageHashPageablePayload {
    PageHashPageablePayload {
        page_number: page.page,
        page_size: page.size,
        sort: page_hash_sort_state_payload(page.sorted),
        offset: page.offset(),
        paged: true,
        unpaged: false,
    }
}

fn page_hash_sort_state_payload(sorted: bool) -> PageHashSortStatePayload {
    PageHashSortStatePayload {
        empty: !sorted,
        sorted,
        unsorted: !sorted,
    }
}

fn known_page_hash_payload(entry: &PageHashKnownEntry) -> KnownPageHashPayload<'_> {
    KnownPageHashPayload {
        hash: &entry.hash,
        size: entry.size,
        action: page_hash_action_name(entry.action),
        delete_count: entry.delete_count,
        match_count: entry.match_count,
        created: &entry.created,
        last_modified: &entry.last_modified,
    }
}

fn unknown_page_hash_payload(entry: &PageHashUnknownEntry) -> UnknownPageHashPayload<'_> {
    UnknownPageHashPayload {
        hash: &entry.hash,
        size: entry.size,
        match_count: entry.match_count,
    }
}

fn page_hash_match_payload(entry: &PageHashMatchEntry) -> PageHashMatchPayload<'_> {
    PageHashMatchPayload {
        book_id: &entry.book_id,
        url: &entry.url,
        page_number: entry.page_number,
        file_name: &entry.file_name,
        file_size: entry.file_size,
        media_type: &entry.media_type,
    }
}

fn page_hash_action(value: &str) -> Option<PageHashAction> {
    match value {
        "DELETE_MANUAL" => Some(PageHashAction::DeleteManual),
        "DELETE_AUTO" => Some(PageHashAction::DeleteAuto),
        "IGNORE" => Some(PageHashAction::Ignore),
        _ => None,
    }
}

fn page_hash_action_name(action: PageHashAction) -> &'static str {
    match action {
        PageHashAction::DeleteManual => "DELETE_MANUAL",
        PageHashAction::DeleteAuto => "DELETE_AUTO",
        PageHashAction::Ignore => "IGNORE",
    }
}

fn page_hash_known_sorts(query: &str) -> Vec<PageHashSort<PageHashKnownSortProperty>> {
    page_hash_sorts(query, page_hash_known_sort_property)
}

fn page_hash_unknown_sorts(query: &str) -> Vec<PageHashSort<PageHashUnknownSortProperty>> {
    page_hash_sorts(query, page_hash_unknown_sort_property)
}

fn page_hash_match_sorts(query: &str) -> Vec<PageHashSort<PageHashMatchSortProperty>> {
    page_hash_sorts(query, page_hash_match_sort_property)
}

fn page_hash_sorts<P>(
    query: &str,
    property_for_key: fn(&str) -> Option<P>,
) -> Vec<PageHashSort<P>> {
    query_values(query, "sort")
        .into_iter()
        .filter_map(|value| page_hash_sort(value.as_str(), property_for_key))
        .collect()
}

fn page_hash_sort<P>(
    value: &str,
    property_for_key: fn(&str) -> Option<P>,
) -> Option<PageHashSort<P>> {
    let mut parts = value.split(',');
    let property_key = parts.next()?.trim();
    if property_key.is_empty() {
        return None;
    }
    let direction = match parts.next().unwrap_or("asc").trim() {
        value if value.eq_ignore_ascii_case("desc") => PageHashSortDirection::Desc,
        _ => PageHashSortDirection::Asc,
    };

    Some(PageHashSort {
        property: property_for_key(property_key)?,
        direction,
    })
}

fn page_hash_known_sort_property(value: &str) -> Option<PageHashKnownSortProperty> {
    match value {
        "hash" => Some(PageHashKnownSortProperty::Hash),
        "matchCount" => Some(PageHashKnownSortProperty::MatchCount),
        "deleteCount" => Some(PageHashKnownSortProperty::DeleteCount),
        "deleteSize" => Some(PageHashKnownSortProperty::DeleteSize),
        "fileSize" | "size" => Some(PageHashKnownSortProperty::FileSize),
        "createdDate" | "created" => Some(PageHashKnownSortProperty::CreatedDate),
        "lastModifiedDate" | "lastModified" => Some(PageHashKnownSortProperty::LastModifiedDate),
        _ => None,
    }
}

fn page_hash_unknown_sort_property(value: &str) -> Option<PageHashUnknownSortProperty> {
    match value {
        "hash" => Some(PageHashUnknownSortProperty::Hash),
        "fileSize" | "size" => Some(PageHashUnknownSortProperty::FileSize),
        "matchCount" => Some(PageHashUnknownSortProperty::MatchCount),
        "totalSize" => Some(PageHashUnknownSortProperty::TotalSize),
        "url" => Some(PageHashUnknownSortProperty::Url),
        "bookId" => Some(PageHashUnknownSortProperty::BookId),
        "pageNumber" => Some(PageHashUnknownSortProperty::PageNumber),
        _ => None,
    }
}

fn page_hash_match_sort_property(value: &str) -> Option<PageHashMatchSortProperty> {
    match value {
        "hash" => Some(PageHashMatchSortProperty::Hash),
        "fileSize" => Some(PageHashMatchSortProperty::FileSize),
        "url" => Some(PageHashMatchSortProperty::Url),
        "bookId" => Some(PageHashMatchSortProperty::BookId),
        "pageNumber" => Some(PageHashMatchSortProperty::PageNumber),
        "matchCount" => Some(PageHashMatchSortProperty::MatchCount),
        "totalSize" => Some(PageHashMatchSortProperty::TotalSize),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_hash_known_sorts_parse_supported_keys_and_ignore_unknown_keys() {
        let sorts = page_hash_known_sorts("sort=matchCount,desc&sort=unknown,asc");

        assert_eq!(sorts.len(), 1);
        assert_eq!(sorts[0].property, PageHashKnownSortProperty::MatchCount);
        assert_eq!(sorts[0].direction, PageHashSortDirection::Desc);
    }

    #[test]
    fn page_hash_sorts_accept_current_size_and_timestamp_aliases() {
        let known = page_hash_known_sorts("sort=size,desc&sort=created,asc&sort=lastModified,desc");
        let unknown = page_hash_unknown_sorts("sort=size,asc");

        assert_eq!(
            known.iter().map(|sort| sort.property).collect::<Vec<_>>(),
            vec![
                PageHashKnownSortProperty::FileSize,
                PageHashKnownSortProperty::CreatedDate,
                PageHashKnownSortProperty::LastModifiedDate,
            ]
        );
        assert_eq!(known[0].direction, PageHashSortDirection::Desc);
        assert_eq!(known[1].direction, PageHashSortDirection::Asc);
        assert_eq!(known[2].direction, PageHashSortDirection::Desc);
        assert_eq!(
            unknown.iter().map(|sort| sort.property).collect::<Vec<_>>(),
            vec![PageHashUnknownSortProperty::FileSize]
        );
    }

    #[test]
    fn page_hash_unknown_sorts_parse_legacy_keys() {
        let sorts = page_hash_unknown_sorts("sort=url,desc&sort=pageNumber,asc");

        assert_eq!(
            sorts.iter().map(|sort| sort.property).collect::<Vec<_>>(),
            vec![
                PageHashUnknownSortProperty::Url,
                PageHashUnknownSortProperty::PageNumber,
            ]
        );
    }

    #[test]
    fn page_hash_match_sorts_keep_unsupported_aggregate_keys_typed() {
        let sorts = page_hash_match_sorts("sort=matchCount,desc&sort=totalSize,asc");

        assert_eq!(
            sorts.iter().map(|sort| sort.property).collect::<Vec<_>>(),
            vec![
                PageHashMatchSortProperty::MatchCount,
                PageHashMatchSortProperty::TotalSize,
            ]
        );
    }

    #[test]
    fn page_hash_actions_parse_wire_names_exactly() {
        assert_eq!(
            page_hash_action("DELETE_MANUAL"),
            Some(PageHashAction::DeleteManual)
        );
        assert_eq!(
            page_hash_action("DELETE_AUTO"),
            Some(PageHashAction::DeleteAuto)
        );
        assert_eq!(page_hash_action("IGNORE"), Some(PageHashAction::Ignore));
        assert_eq!(page_hash_action(" DELETE_MANUAL "), None);
    }
}
