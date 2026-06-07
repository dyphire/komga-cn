use axum::Json;
use axum::body::Bytes;
use axum::extract::Path as AxumPath;
use axum::extract::State;
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use komga_application::operational::{
    PageHashAction, PageHashDeleteError, PageHashDeleteMatch, PageHashKnownQuery,
    PageHashMatchesQuery, PageHashSort, PageHashUnknownQuery, PageHashUpsertCommand,
};
use serde::Deserialize;

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
    action: PageHashAction,
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
            sorts: page_hash_sorts(query),
        })
        .await
    {
        Ok(page_data) => page_data,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    Json(page_data).into_response()
}

fn parse_page_hash_actions(raw_values: Vec<String>) -> Result<Vec<PageHashAction>, StatusCode> {
    let mut actions = Vec::new();

    for raw_value in raw_values {
        for action in raw_value.split(',') {
            let Some(action) = PageHashAction::parse(action) else {
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
            sorts: page_hash_sorts(query),
        })
        .await
    {
        Ok(page_data) => page_data,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    Json(page_data).into_response()
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
            sorts: page_hash_sorts(query),
        })
        .await
    {
        Ok(page_data) => page_data,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    Json(page_data).into_response()
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

    ([(header::CONTENT_TYPE, "image/jpeg")], thumbnail.bytes).into_response()
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
    let Ok(command) = PageHashUpsertCommand::new(payload.hash, payload.size, payload.action) else {
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

fn page_hash_sorts(query: &str) -> Vec<PageHashSort> {
    query_values(query, "sort")
        .into_iter()
        .filter_map(|value| PageHashSort::parse(&value))
        .collect()
}
