use axum::Json;
use axum::body::Bytes;
use axum::extract::{Extension, Path};
use axum::http::{HeaderMap, Uri};
use axum::response::Response;
use serde_json::{Value, json};

use super::AuthDatabaseState;
use crate::app::CompatProfile;
use crate::app::discovery_auth::DiscoveryAuthState;

#[path = "content_auth.rs"]
mod content_auth;
#[path = "content_libraries.rs"]
mod content_libraries;
#[path = "content_opds.rs"]
mod content_opds;
#[path = "content/detail.rs"]
mod detail;
#[path = "content/discovery.rs"]
mod discovery;
#[path = "content/helpers.rs"]
mod helpers;
#[path = "content/media.rs"]
pub(crate) mod media;

pub(super) use detail::{
    book_detail, book_readlists, book_sibling_next, book_sibling_previous, collection_create,
    collection_delete, collection_detail, collection_series, collection_update, collections,
    readlist_book_sibling_next, readlist_book_sibling_previous, readlist_books, readlist_create,
    readlist_delete, readlist_detail, readlist_match_comicrack, readlist_update, readlists,
};
pub(super) use discovery::{
    book_tags, books, books_duplicates, books_latest, books_list, books_ondeck, series_list,
};
pub(super) use helpers::mark_native;
pub(super) use media::{
    book_analyze, book_file, book_file_delete, book_file_with_suffix, book_manifest,
    book_manifest_divina, book_manifest_epub, book_manifest_pdf, book_metadata_batch_update,
    book_metadata_refresh, book_metadata_update, book_page, book_page_raw, book_page_thumbnail,
    book_pages, book_positions, book_progression, book_progression_get, book_read_progress,
    book_read_progress_delete, book_read_progress_get, book_resource, book_thumbnail,
    book_thumbnail_by_id, book_thumbnail_delete, book_thumbnail_select, book_thumbnail_upload,
    book_thumbnails, books_import, books_thumbnails_regenerate, collection_thumbnail,
    collection_thumbnail_by_id, collection_thumbnail_delete, collection_thumbnail_select,
    collection_thumbnail_upload, collection_thumbnails, readlist_file,
    readlist_tachiyomi_read_progress_get, readlist_tachiyomi_read_progress_put, readlist_thumbnail,
    readlist_thumbnail_by_id, readlist_thumbnail_delete, readlist_thumbnail_select,
    readlist_thumbnail_upload, readlist_thumbnails, series_analyze, series_file,
    series_file_delete, series_metadata_refresh, series_read_progress_delete,
    series_read_progress_post, series_tachiyomi_read_progress_get,
    series_tachiyomi_read_progress_put, series_thumbnail, series_thumbnail_by_id,
    series_thumbnail_delete, series_thumbnail_select, series_thumbnail_upload, series_thumbnails,
};

pub(super) async fn libraries(
    Extension(profile): Extension<CompatProfile>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
) -> Response {
    content_libraries::response(
        profile,
        headers,
        auth_state,
        auth_db.database_file.as_path(),
    )
    .await
}

pub(super) async fn series(
    Extension(profile): Extension<CompatProfile>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    discovery::series(
        profile,
        headers,
        uri,
        auth_state,
        auth_db.database_file.as_path(),
    )
    .await
}

pub(super) async fn authors(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    discovery::authors(headers, uri, auth_db.database_file.as_path()).await
}

pub(super) async fn authors_names(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    discovery::authors_names(headers, uri, auth_db.database_file.as_path()).await
}

pub(super) async fn authors_roles(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
) -> Response {
    discovery::authors_roles(headers, auth_db.database_file.as_path()).await
}

pub(super) async fn authors_v2(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    discovery::authors_v2(headers, uri, auth_db.database_file.as_path()).await
}

pub(super) async fn genres(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    discovery::genres(headers, uri, auth_db.database_file.as_path()).await
}

pub(super) async fn tags(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    discovery::tags(headers, uri, auth_db.database_file.as_path()).await
}

pub(super) async fn series_tags(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    discovery::series_tags(headers, uri, auth_db.database_file.as_path()).await
}

pub(super) async fn languages(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    discovery::languages(headers, uri, auth_db.database_file.as_path()).await
}

pub(super) async fn publishers(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    discovery::publishers(headers, uri, auth_db.database_file.as_path()).await
}

pub(super) async fn age_ratings(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    discovery::age_ratings(headers, uri, auth_db.database_file.as_path()).await
}

pub(super) async fn sharing_labels(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    discovery::sharing_labels(headers, uri, auth_db.database_file.as_path()).await
}

pub(super) async fn series_latest(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    discovery::series_latest(headers, uri, auth_state, auth_db.database_file.as_path()).await
}

pub(super) async fn series_new(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    discovery::series_new(headers, uri, auth_state, auth_db.database_file.as_path()).await
}

pub(super) async fn series_updated(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    discovery::series_updated(headers, uri, auth_state, auth_db.database_file.as_path()).await
}

pub(super) async fn series_release_dates(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    discovery::series_release_dates(headers, uri, auth_db.database_file.as_path()).await
}

pub(super) async fn series_alphabetical_groups(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    discovery::series_alphabetical_groups(
        headers,
        body,
        auth_state,
        auth_db.database_file.as_path(),
    )
    .await
}

pub(super) async fn series_alphabetical_groups_deprecated(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let query = uri.query().unwrap_or_default();
    let mut conditions: Vec<Value> = vec![];

    push_series_any_of_string_conditions(&mut conditions, query, "library_id", "LibraryId");
    push_series_any_of_string_conditions(&mut conditions, query, "collection_id", "CollectionId");
    push_series_any_of_string_conditions(&mut conditions, query, "status", "SeriesStatus");
    push_series_any_of_string_conditions(&mut conditions, query, "read_status", "ReadStatus");
    push_series_any_of_string_conditions(&mut conditions, query, "publisher", "Publisher");
    push_series_any_of_string_conditions(&mut conditions, query, "language", "Language");
    push_series_any_of_string_conditions(&mut conditions, query, "genre", "Genre");
    push_series_any_of_string_conditions(&mut conditions, query, "tag", "Tag");
    push_series_any_of_string_conditions(&mut conditions, query, "sharing_label", "SharingLabel");

    let age_ratings = query_values(query, "age_rating");
    if !age_ratings.is_empty() {
        conditions.push(json!({
            "type": "AnyOfSeries",
            "conditions": age_ratings
                .into_iter()
                .map(|value| {
                    let parsed = value.parse::<u16>().ok();
                    if let Some(parsed) = parsed {
                        json!({
                            "type": "AgeRating",
                            "operator": "is",
                            "value": parsed,
                        })
                    } else {
                        json!({
                            "type": "AgeRating",
                            "operator": "isNull",
                        })
                    }
                })
                .collect::<Vec<_>>(),
        }));
    }

    let release_years = query_values(query, "release_year");
    if !release_years.is_empty() {
        conditions.push(json!({
            "type": "AnyOfSeries",
            "conditions": release_years
                .into_iter()
                .filter_map(|value| value.parse::<i32>().ok())
                .map(|year| {
                    let after = format!("{}-12-31T12:00:00Z", year - 1);
                    let before = format!("{}-01-01T12:00:00Z", year + 1);
                    json!({
                        "type": "AllOfSeries",
                        "conditions": [
                            {
                                "type": "ReleaseDate",
                                "operator": "after",
                                "value": after,
                            },
                            {
                                "type": "ReleaseDate",
                                "operator": "before",
                                "value": before,
                            }
                        ],
                    })
                })
                .collect::<Vec<_>>(),
        }));
    }

    let authors = query_values(query, "author");
    if !authors.is_empty() {
        conditions.push(json!({
            "type": "AnyOfSeries",
            "conditions": authors
                .into_iter()
                .map(author_query_to_series_condition)
                .collect::<Vec<_>>(),
        }));
    }

    if let Some(deleted) = query_bool(query, "deleted") {
        conditions.push(json!({
            "type": "Deleted",
            "operator": if deleted { "isTrue" } else { "isFalse" },
        }));
    }
    if let Some(complete) = query_bool(query, "complete") {
        conditions.push(json!({
            "type": "Complete",
            "operator": if complete { "isTrue" } else { "isFalse" },
        }));
    }
    if let Some(oneshot) = query_bool(query, "oneshot") {
        conditions.push(json!({
            "type": "OneShot",
            "operator": if oneshot { "isTrue" } else { "isFalse" },
        }));
    }

    let mut body = json!({
        "condition": {
            "type": "AllOfSeries",
            "conditions": conditions,
        }
    });

    if let Some(search) = query_value(query, "search").map(decode_query_component)
        && !search.trim().is_empty()
    {
        body["fullTextSearch"] = Value::String(search);
    }

    if let Some((pattern, field)) = query_search_regex(query) {
        body["regexSearch"] = json!({
            "regex": pattern,
            "field": field,
        });
    }

    discovery::series_alphabetical_groups(
        headers,
        body,
        auth_state,
        auth_db.database_file.as_path(),
    )
    .await
}

fn query_value<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split('&').find_map(|pair| {
        let (candidate_key, value) = pair.split_once('=')?;
        (candidate_key == key).then_some(value)
    })
}

fn query_values(query: &str, key: &str) -> Vec<String> {
    query
        .split('&')
        .filter_map(|pair| {
            let (candidate_key, value) = pair.split_once('=')?;
            (candidate_key == key).then_some(value.replace('+', " "))
        })
        .filter(|value| !value.trim().is_empty())
        .collect()
}

fn query_bool(query: &str, key: &str) -> Option<bool> {
    query_value(query, key).and_then(|value| {
        if value.eq_ignore_ascii_case("true") {
            Some(true)
        } else if value.eq_ignore_ascii_case("false") {
            Some(false)
        } else {
            None
        }
    })
}

pub(super) async fn series_detail(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    path: axum::extract::Path<String>,
    uri: Uri,
) -> Response {
    detail::series_detail(
        headers,
        path,
        uri,
        auth_state,
        auth_db.database_file.as_path(),
    )
    .await
}

pub(super) async fn series_books(
    Extension(profile): Extension<CompatProfile>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    Path(series_id): Path<String>,
    uri: Uri,
) -> Response {
    let query = uri.query().unwrap_or_default();
    let mut all_of = vec![json!({
        "type": "SeriesId",
        "operator": "is",
        "value": series_id,
    })];

    let media_statuses = query_values(query, "media_status");
    if !media_statuses.is_empty() {
        all_of.push(json!({
            "type": "AnyOfBook",
            "conditions": media_statuses
                .into_iter()
                .map(|value| {
                    json!({
                        "type": "MediaStatus",
                        "operator": "is",
                        "value": value,
                    })
                })
                .collect::<Vec<_>>(),
        }));
    }

    let read_statuses = query_values(query, "read_status");
    if !read_statuses.is_empty() {
        all_of.push(json!({
            "type": "AnyOfBook",
            "conditions": read_statuses
                .into_iter()
                .map(|value| {
                    json!({
                        "type": "ReadStatus",
                        "operator": "is",
                        "value": value,
                    })
                })
                .collect::<Vec<_>>(),
        }));
    }

    let tags = query_values(query, "tag");
    if !tags.is_empty() {
        all_of.push(json!({
            "type": "AnyOfBook",
            "conditions": tags
                .into_iter()
                .map(|value| {
                    json!({
                        "type": "Tag",
                        "operator": "is",
                        "value": value,
                    })
                })
                .collect::<Vec<_>>(),
        }));
    }

    let authors = query_values(query, "author");
    if !authors.is_empty() {
        all_of.push(json!({
            "type": "AnyOfBook",
            "conditions": authors
                .into_iter()
                .map(author_query_to_book_condition)
                .collect::<Vec<_>>(),
        }));
    }

    if let Some(deleted) = query_bool(query, "deleted") {
        all_of.push(json!({
            "type": "Deleted",
            "operator": if deleted { "isTrue" } else { "isFalse" },
        }));
    }

    let body = json!({
        "condition": {
            "type": "AllOfBook",
            "conditions": all_of,
        }
    });

    let request_uri = ensure_sort_query(uri, "series,metadata.numberSort,asc");

    discovery::books_list(
        Extension(profile),
        Extension(auth_db),
        Extension(auth_state),
        headers,
        request_uri,
        Bytes::from(body.to_string()),
    )
    .await
}

fn push_series_any_of_string_conditions(
    conditions: &mut Vec<Value>,
    query: &str,
    key: &str,
    condition_type: &str,
) {
    let values = query_values(query, key);
    if values.is_empty() {
        return;
    }

    conditions.push(json!({
        "type": "AnyOfSeries",
        "conditions": values
            .into_iter()
            .map(|value| {
                json!({
                    "type": condition_type,
                    "operator": "is",
                    "value": value,
                })
            })
            .collect::<Vec<_>>(),
    }));
}

fn author_query_to_author_match(value: String) -> Value {
    let normalized = decode_query_component(&value);
    let (name, role) = normalized
        .split_once(',')
        .map(|(name, role)| (name.trim(), role.trim()))
        .unwrap_or((normalized.trim(), ""));

    let mut payload = serde_json::Map::new();
    if !name.is_empty() {
        payload.insert("name".to_string(), Value::String(name.to_string()));
    }
    if !role.is_empty() {
        payload.insert("role".to_string(), Value::String(role.to_string()));
    }

    Value::Object(payload)
}

fn author_query_to_series_condition(value: String) -> Value {
    json!({
        "type": "Author",
        "operator": "is",
        "value": author_query_to_author_match(value),
    })
}

fn author_query_to_book_condition(value: String) -> Value {
    json!({
        "type": "Author",
        "operator": "is",
        "value": author_query_to_author_match(value),
    })
}

fn query_search_regex(query: &str) -> Option<(String, String)> {
    let regex = query_value(query, "search_regex").map(decode_query_component)?;
    let (pattern, field) = regex.split_once(',')?;
    let normalized_field = match field.trim().to_ascii_lowercase().as_str() {
        "title" => "TITLE",
        "title_sort" => "TITLE_SORT",
        _ => return None,
    };
    if pattern.trim().is_empty() {
        return None;
    }
    Some((pattern.trim().to_string(), normalized_field.to_string()))
}

fn ensure_sort_query(uri: Uri, default_sort: &str) -> Uri {
    if uri
        .query()
        .is_some_and(|query| query.split('&').any(|pair| pair.starts_with("sort=")))
    {
        return uri;
    }

    let path = uri.path();
    let next_query = match uri.query() {
        Some(query) if !query.is_empty() => format!("{query}&sort={default_sort}"),
        _ => format!("sort={default_sort}"),
    };

    Uri::builder()
        .path_and_query(format!("{path}?{next_query}"))
        .build()
        .unwrap_or(uri)
}

fn decode_query_component(value: &str) -> String {
    let mut decoded = String::with_capacity(value.len());
    let mut bytes = value.as_bytes().iter().copied();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let high = bytes.next();
            let low = bytes.next();
            if let (Some(high), Some(low)) = (high, low) {
                let hex = [high, low];
                if let Ok(hex) = std::str::from_utf8(&hex)
                    && let Ok(parsed) = u8::from_str_radix(hex, 16)
                {
                    decoded.push(parsed as char);
                    continue;
                }
            }
            decoded.push('%');
            if let Some(high) = high {
                decoded.push(high as char);
            }
            if let Some(low) = low {
                decoded.push(low as char);
            }
            continue;
        }
        if byte == b'+' {
            decoded.push(' ');
        } else {
            decoded.push(byte as char);
        }
    }
    decoded
}

pub(super) async fn series_collections(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    path: axum::extract::Path<String>,
) -> Response {
    detail::series_collections(headers, path, auth_state, auth_db.database_file.as_path()).await
}

pub(super) async fn series_metadata_update(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(state): Extension<super::OperationalState>,
    headers: HeaderMap,
    path: axum::extract::Path<String>,
    Json(body): Json<Value>,
) -> Response {
    detail::series_metadata_update(
        headers,
        auth_db.database_file.as_path(),
        state.runtime.lucene_data_directory.as_path(),
        path,
        body,
    )
    .await
}

pub(super) async fn library_detail(
    Extension(profile): Extension<CompatProfile>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    path: axum::extract::Path<String>,
) -> Response {
    content_libraries::library_detail(
        profile,
        headers,
        auth_state,
        auth_db.database_file.as_path(),
        path,
    )
    .await
}

pub(super) async fn library_create(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(state): Extension<super::OperationalState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    content_libraries::library_create(headers, auth_db.database_file.as_path(), state, body).await
}

pub(super) async fn library_update(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(state): Extension<super::OperationalState>,
    headers: HeaderMap,
    path: axum::extract::Path<String>,
    Json(body): Json<Value>,
) -> Response {
    content_libraries::library_update(headers, auth_db.database_file.as_path(), state, path, body)
        .await
}

pub(super) async fn library_delete(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    path: axum::extract::Path<String>,
) -> Response {
    content_libraries::library_delete(headers, auth_db.database_file.as_path(), path).await
}

pub(super) async fn library_scan(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(state): Extension<super::OperationalState>,
    headers: HeaderMap,
    uri: Uri,
    path: axum::extract::Path<String>,
) -> Response {
    content_libraries::library_scan(headers, uri, auth_db.database_file.as_path(), state, path)
        .await
}

pub(super) async fn library_analyze(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(state): Extension<super::OperationalState>,
    headers: HeaderMap,
    path: axum::extract::Path<String>,
) -> Response {
    content_libraries::library_analyze(headers, auth_db.database_file.as_path(), state, path).await
}

pub(super) async fn library_metadata_refresh(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(state): Extension<super::OperationalState>,
    headers: HeaderMap,
    path: axum::extract::Path<String>,
) -> Response {
    content_libraries::library_metadata_refresh(
        headers,
        auth_db.database_file.as_path(),
        state,
        path,
    )
    .await
}

pub(super) async fn library_empty_trash(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(state): Extension<super::OperationalState>,
    headers: HeaderMap,
    path: axum::extract::Path<String>,
) -> Response {
    content_libraries::library_empty_trash(headers, auth_db.database_file.as_path(), state, path)
        .await
}

pub(super) async fn users_me(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    content_auth::users_me(headers, uri, auth_state, auth_db).await
}

pub(super) async fn users_list(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
) -> Response {
    content_auth::users_list(headers, auth_db).await
}

pub(super) async fn users_create(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    content_auth::users_create(headers, body, auth_db).await
}

pub(super) async fn users_update(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    path: axum::extract::Path<String>,
    Json(body): Json<Value>,
) -> Response {
    content_auth::users_update(headers, path, body, auth_db).await
}

pub(super) async fn users_delete(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    path: axum::extract::Path<String>,
) -> Response {
    content_auth::users_delete(headers, path, auth_db).await
}

pub(super) async fn users_me_password(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    content_auth::users_me_password(headers, body, auth_db).await
}

pub(super) async fn users_by_id_password(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    path: axum::extract::Path<String>,
    Json(body): Json<Value>,
) -> Response {
    content_auth::users_by_id_password(headers, path, body, auth_db).await
}

pub(super) async fn users_me_api_keys_create(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    content_auth::users_me_api_keys_create(headers, body, auth_db).await
}

pub(super) async fn users_me_api_keys_list(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
) -> Response {
    content_auth::users_me_api_keys_list(headers, auth_db).await
}

pub(super) async fn users_me_api_keys_delete(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    path: axum::extract::Path<String>,
) -> Response {
    content_auth::users_me_api_keys_delete(headers, path, auth_db).await
}

pub(super) async fn users_me_authentication_activity(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    content_auth::users_me_authentication_activity(headers, uri, auth_db).await
}

pub(super) async fn users_authentication_activity(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    content_auth::users_authentication_activity(headers, uri, auth_db).await
}

pub(super) async fn users_by_id_authentication_activity_latest(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    path: axum::extract::Path<String>,
    uri: Uri,
) -> Response {
    content_auth::users_by_id_authentication_activity_latest(headers, path, uri, auth_db).await
}

pub(super) async fn login_set_cookie(headers: HeaderMap) -> Response {
    content_auth::login_set_cookie(headers).await
}

pub(super) async fn logout(headers: HeaderMap) -> Response {
    content_auth::logout(headers).await
}

pub(super) async fn opds_manifest(
    Extension(profile): Extension<CompatProfile>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    content_opds::opds_manifest(profile, headers, auth_db.database_file.as_path(), &book_id).await
}

pub(super) async fn opds_manifest_profile(
    Extension(profile): Extension<CompatProfile>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((book_id, manifest_profile)): Path<(String, String)>,
) -> Response {
    content_opds::opds_manifest_with_profile(
        profile,
        headers,
        auth_db.database_file.as_path(),
        &book_id,
        &manifest_profile,
    )
    .await
}

pub(super) async fn opds_auth(headers: HeaderMap) -> Response {
    content_opds::opds_auth(headers).await
}

pub(super) async fn opds_catalog(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
) -> Response {
    content_opds::opds_catalog(headers, auth_db.database_file.as_path()).await
}

pub(super) async fn opds_v1_series(
    Extension(profile): Extension<CompatProfile>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    content_opds::opds_v1_series(profile, headers, uri, auth_db.database_file.as_path()).await
}

pub(super) async fn opds_v1_catalog(headers: HeaderMap) -> Response {
    content_opds::opds_v1_catalog(headers).await
}

pub(super) async fn opds_v1_search(headers: HeaderMap) -> Response {
    content_opds::opds_v1_search(headers).await
}

pub(super) async fn opds_v1_on_deck(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Response {
    content_opds::opds_v1_on_deck(headers, uri, auth_db.database_file.as_path()).await
}

pub(super) async fn opds_v1_keep_reading(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Response {
    content_opds::opds_v1_keep_reading(headers, uri, auth_db.database_file.as_path()).await
}

pub(super) async fn opds_v1_series_latest(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Response {
    content_opds::opds_v1_series_latest(headers, uri, auth_db.database_file.as_path()).await
}

pub(super) async fn opds_v1_books_latest(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Response {
    content_opds::opds_v1_books_latest(headers, uri, auth_db.database_file.as_path()).await
}

pub(super) async fn opds_v1_libraries(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
) -> Response {
    content_opds::opds_v1_libraries(headers, auth_db.database_file.as_path()).await
}

pub(super) async fn opds_v1_collections(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Response {
    content_opds::opds_v1_collections(headers, uri, auth_db.database_file.as_path()).await
}

pub(super) async fn opds_v1_readlists(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Response {
    content_opds::opds_v1_readlists(headers, uri, auth_db.database_file.as_path()).await
}

pub(super) async fn opds_v1_publishers(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Response {
    content_opds::opds_v1_publishers(headers, uri, auth_db.database_file.as_path()).await
}

pub(super) async fn opds_v1_series_detail(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: axum::http::Uri,
    Path(series_id): Path<String>,
) -> Response {
    content_opds::opds_v1_series_detail(headers, uri, auth_db.database_file.as_path(), &series_id)
        .await
}

pub(super) async fn opds_v1_library_detail(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
    Path(library_id): Path<String>,
) -> Response {
    content_opds::opds_v1_library_detail(headers, uri, auth_db.database_file.as_path(), &library_id)
        .await
}

pub(super) async fn opds_v1_collection_detail(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: axum::http::Uri,
    Path(collection_id): Path<String>,
) -> Response {
    content_opds::opds_v1_collection_detail(
        headers,
        uri,
        auth_db.database_file.as_path(),
        &collection_id,
    )
    .await
}

pub(super) async fn opds_v1_readlist_detail(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: axum::http::Uri,
    Path(readlist_id): Path<String>,
) -> Response {
    content_opds::opds_v1_readlist_detail(
        headers,
        uri,
        auth_db.database_file.as_path(),
        &readlist_id,
    )
    .await
}

pub(super) async fn opds_v1_book_file(
    Extension(profile): Extension<CompatProfile>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((book_id, _file_name)): Path<(String, String)>,
) -> Response {
    media::book_file(
        Extension(profile),
        Extension(auth_db),
        headers,
        Path(book_id),
    )
    .await
}

pub(super) async fn opds_v2_libraries(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
) -> Response {
    content_opds::opds_v2_libraries(headers, auth_db.database_file.as_path()).await
}

pub(super) async fn opds_v2_library(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(library_id): Path<String>,
) -> Response {
    content_opds::opds_v2_library(headers, auth_db.database_file.as_path(), &library_id).await
}

pub(super) async fn opds_v2_library_readlists(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(library_id): Path<String>,
) -> Response {
    content_opds::opds_v2_library_readlists(headers, auth_db.database_file.as_path(), &library_id)
        .await
}

pub(super) async fn opds_v2_libraries_readlists(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
) -> Response {
    content_opds::opds_v2_libraries_readlists(headers, auth_db.database_file.as_path()).await
}

pub(super) async fn opds_v2_libraries_keep_reading(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
) -> Response {
    content_opds::opds_v2_libraries_keep_reading(headers, auth_db.database_file.as_path()).await
}

pub(super) async fn opds_v2_library_keep_reading(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(library_id): Path<String>,
) -> Response {
    content_opds::opds_v2_library_keep_reading(
        headers,
        auth_db.database_file.as_path(),
        &library_id,
    )
    .await
}

pub(super) async fn opds_v2_libraries_on_deck(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
) -> Response {
    content_opds::opds_v2_libraries_on_deck(headers, auth_db.database_file.as_path()).await
}

pub(super) async fn opds_v2_library_on_deck(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(library_id): Path<String>,
) -> Response {
    content_opds::opds_v2_library_on_deck(headers, auth_db.database_file.as_path(), &library_id)
        .await
}

pub(super) async fn opds_v2_libraries_latest_books(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
) -> Response {
    content_opds::opds_v2_libraries_latest_books(headers, auth_db.database_file.as_path()).await
}

pub(super) async fn opds_v2_library_latest_books(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(library_id): Path<String>,
) -> Response {
    content_opds::opds_v2_library_latest_books(
        headers,
        auth_db.database_file.as_path(),
        &library_id,
    )
    .await
}

pub(super) async fn opds_v2_libraries_latest_series(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
) -> Response {
    content_opds::opds_v2_libraries_latest_series(headers, auth_db.database_file.as_path()).await
}

pub(super) async fn opds_v2_library_latest_series(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(library_id): Path<String>,
) -> Response {
    content_opds::opds_v2_library_latest_series(
        headers,
        auth_db.database_file.as_path(),
        &library_id,
    )
    .await
}

pub(super) async fn opds_v2_libraries_browse(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Response {
    content_opds::opds_v2_libraries_browse(headers, uri, auth_db.database_file.as_path()).await
}

pub(super) async fn opds_v2_library_browse(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: axum::http::Uri,
    Path(library_id): Path<String>,
) -> Response {
    content_opds::opds_v2_library_browse(
        headers,
        uri,
        auth_db.database_file.as_path(),
        Some(&library_id),
    )
    .await
}

pub(super) async fn opds_v2_libraries_collections(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
) -> Response {
    content_opds::opds_v2_libraries_collections(headers, auth_db.database_file.as_path()).await
}

pub(super) async fn opds_v2_library_collections(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(library_id): Path<String>,
) -> Response {
    content_opds::opds_v2_library_collections(headers, auth_db.database_file.as_path(), &library_id)
        .await
}

pub(super) async fn opds_v2_collection(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(collection_id): Path<String>,
) -> Response {
    content_opds::opds_v2_collection(headers, auth_db.database_file.as_path(), &collection_id).await
}

pub(super) async fn opds_v2_book_thumbnail_small(
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    content_opds::opds_v2_book_thumbnail_small(headers, &book_id).await
}

pub(super) async fn opds_v2_series(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(series_id): Path<String>,
) -> Response {
    content_opds::opds_v2_series(headers, auth_db.database_file.as_path(), &series_id).await
}

pub(super) async fn opds_v2_readlist(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
) -> Response {
    content_opds::opds_v2_readlist(headers, auth_db.database_file.as_path(), &readlist_id).await
}

#[cfg(test)]
mod tests {
    use super::{
        author_query_to_author_match, decode_query_component, ensure_sort_query, query_search_regex,
    };
    use axum::http::Uri;

    #[test]
    fn decode_query_component_decodes_plus_and_percent_encoding() {
        let decoded = decode_query_component("John+Doe%2Cwriter%20team");
        assert_eq!(decoded, "John Doe,writer team");
    }

    #[test]
    fn query_search_regex_parses_supported_fields() {
        let parsed = query_search_regex("search_regex=%5Eabc%24,title_sort")
            .expect("search_regex with title_sort should parse");
        assert_eq!(parsed.0, "^abc$");
        assert_eq!(parsed.1, "TITLE_SORT");
    }

    #[test]
    fn query_search_regex_rejects_unsupported_field() {
        let parsed = query_search_regex("search_regex=abc,unknown");
        assert!(parsed.is_none());
    }

    #[test]
    fn ensure_sort_query_appends_default_sort_when_missing() {
        let uri: Uri = "/api/v1/series/series-1/books?page=2"
            .parse()
            .expect("uri should parse");
        let updated = ensure_sort_query(uri, "series,metadata.numberSort,asc");
        assert_eq!(
            updated.path_and_query().map(|value| value.as_str()),
            Some("/api/v1/series/series-1/books?page=2&sort=series,metadata.numberSort,asc"),
        );
    }

    #[test]
    fn ensure_sort_query_keeps_existing_sort() {
        let uri: Uri = "/api/v1/series/series-1/books?sort=createdDate,desc"
            .parse()
            .expect("uri should parse");
        let updated = ensure_sort_query(uri.clone(), "series,metadata.numberSort,asc");
        assert_eq!(updated, uri);
    }

    #[test]
    fn author_query_to_author_match_splits_name_and_role() {
        let parsed = author_query_to_author_match("Jane+Doe,writer".to_string());
        assert_eq!(parsed["name"], "Jane Doe");
        assert_eq!(parsed["role"], "writer");
    }
}

pub(super) async fn opds_v2_search(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let query = uri
        .query()
        .and_then(|raw| {
            raw.split('&').find_map(|pair| {
                let (key, value) = pair.split_once('=')?;
                (key == "query").then_some(value)
            })
        })
        .map(|value| value.replace('+', " "))
        .unwrap_or_default();
    content_opds::opds_v2_search(
        headers,
        auth_db.database_file.as_path(),
        Some(query.as_str()),
    )
    .await
}
