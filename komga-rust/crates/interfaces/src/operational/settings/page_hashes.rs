use axum::Json;
use axum::body::Bytes;
use axum::extract::Path as AxumPath;
use axum::extract::State;
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use komga_application::operational::{
    PageHashAction, PageHashDeleteError, PageHashDeleteMatch, PageHashKnownQuery,
    PageHashMatchesQuery, PageHashSort, PageHashSortDirection, PageHashUnknownQuery,
    PageHashUpsertCommand,
};
use komga_application::operational::{
    PageHashKnownSortProperty, PageHashMatchSortProperty, PageHashUnknownSortProperty,
};
use serde::Deserialize;

use crate::contracts::page_hashes::{
    known_page_hash_page, page_hash_matches_page, unknown_page_hash_page,
};
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

    let payload = match known_page_hash_page(&page_data) {
        Ok(payload) => payload,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    Json(payload).into_response()
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

    let payload = match unknown_page_hash_page(&page_data) {
        Ok(payload) => payload,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    Json(payload).into_response()
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

    let payload = match page_hash_matches_page(&page_data) {
        Ok(payload) => payload,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    Json(payload).into_response()
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

fn page_hash_action(value: &str) -> Option<PageHashAction> {
    match value {
        "DELETE_MANUAL" => Some(PageHashAction::DeleteManual),
        "DELETE_AUTO" => Some(PageHashAction::DeleteAuto),
        "IGNORE" => Some(PageHashAction::Ignore),
        _ => None,
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
