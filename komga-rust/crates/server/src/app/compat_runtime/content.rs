use axum::Json;
use axum::extract::{Extension, Path};
use axum::http::{HeaderMap, Uri};
use axum::response::Response;
use serde_json::Value;

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
mod media;

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
    book_analyze, book_file, book_file_delete, book_manifest, book_manifest_divina,
    book_manifest_epub, book_manifest_pdf, book_metadata_batch_update, book_metadata_refresh,
    book_metadata_update, book_page, book_page_thumbnail, book_pages, book_progression,
    book_progression_get, book_read_progress, book_read_progress_delete, book_read_progress_get,
    book_thumbnail, book_thumbnail_delete, book_thumbnail_select, book_thumbnail_upload,
    book_thumbnails, books_import, books_thumbnails_regenerate, collection_thumbnail,
    collection_thumbnail_by_id, collection_thumbnail_delete, collection_thumbnail_select,
    collection_thumbnail_upload, collection_thumbnails, readlist_file,
    readlist_tachiyomi_read_progress_get, readlist_tachiyomi_read_progress_put, readlist_thumbnail,
    readlist_thumbnail_by_id, readlist_thumbnail_delete, readlist_thumbnail_select,
    readlist_thumbnail_upload, readlist_thumbnails, series_thumbnail,
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
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    content_libraries::library_create(headers, auth_db.database_file.as_path(), body).await
}

pub(super) async fn library_update(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    path: axum::extract::Path<String>,
    Json(body): Json<Value>,
) -> Response {
    content_libraries::library_update(headers, auth_db.database_file.as_path(), path, body).await
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
    path: axum::extract::Path<String>,
) -> Response {
    content_libraries::library_scan(headers, auth_db.database_file.as_path(), state, path).await
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

pub(super) async fn opds_catalog(headers: HeaderMap) -> Response {
    content_opds::opds_catalog(headers).await
}

pub(super) async fn opds_v1_series(
    Extension(profile): Extension<CompatProfile>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
) -> Response {
    content_opds::opds_v1_series(profile, headers, auth_db.database_file.as_path()).await
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
