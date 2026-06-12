use axum::extract::State;

use axum::extract::{Path as AxumPath, Query};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};

use crate::book_page_query::BookPageQuery;
use crate::media_responses::OpdsBookMediaResponses;
use crate::opds_auth::{OpdsV1Authenticated, OpdsV2Authenticated, opds_auth};
use crate::state::OpdsState;
use komga_application::identity_access::{AuthUserRole, user_has_role, user_is_admin};

mod feed_endpoints;
mod feeds;
mod manifest;
mod persisted;
mod types;
mod v1;
mod v2;
mod xml_renderer;

use self::feeds::percent_decode;

pub(crate) use self::manifest::{opds_manifest, opds_manifest_with_profile};
pub(crate) use self::v2::{opds_catalog, opds_v2_libraries};

fn opds_book_media_responses(app: &OpdsState) -> OpdsBookMediaResponses<'_> {
    OpdsBookMediaResponses::new(
        app.book_media_reader.as_ref(),
        app.book_media_content.as_ref(),
        app.book_id_resolver.as_ref(),
        app.server_settings.as_ref(),
    )
}

pub(crate) async fn opds_manifest_route(
    State(app): State<OpdsState>,
    OpdsV2Authenticated(user): OpdsV2Authenticated,
    headers: HeaderMap,
    AxumPath(book_id): AxumPath<String>,
) -> Response {
    opds_manifest(headers, &app, &book_id, &user).await
}

pub(crate) async fn opds_manifest_profile_route(
    State(app): State<OpdsState>,
    OpdsV2Authenticated(user): OpdsV2Authenticated,
    headers: HeaderMap,
    AxumPath((book_id, manifest_profile)): AxumPath<(String, String)>,
) -> Response {
    opds_manifest_with_profile(headers, &app, &book_id, &manifest_profile, &user).await
}

pub(crate) async fn opds_auth_route(headers: HeaderMap) -> Response {
    opds_auth(headers).await
}

pub(crate) async fn opds_v1_series_route(
    State(app): State<OpdsState>,
    OpdsV1Authenticated(user): OpdsV1Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    v1::opds_v1_series(headers, uri, &app, &user).await
}

pub(crate) async fn opds_v1_catalog_route(
    State(app): State<OpdsState>,
    _: OpdsV1Authenticated,
    headers: HeaderMap,
) -> Response {
    v1::opds_v1_catalog(&app, headers).await
}

pub(crate) async fn opds_v1_search_route(
    State(app): State<OpdsState>,
    _: OpdsV1Authenticated,
    headers: HeaderMap,
) -> Response {
    v1::opds_v1_search(&app, headers).await
}

pub(crate) async fn opds_v1_on_deck_route(
    State(app): State<OpdsState>,
    OpdsV1Authenticated(user): OpdsV1Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    v1::opds_v1_on_deck(headers, uri, &app, &user).await
}

pub(crate) async fn opds_v1_keep_reading_route(
    State(app): State<OpdsState>,
    OpdsV1Authenticated(user): OpdsV1Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    v1::opds_v1_keep_reading(headers, uri, &app, &user).await
}

pub(crate) async fn opds_v1_series_latest_route(
    State(app): State<OpdsState>,
    OpdsV1Authenticated(user): OpdsV1Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    v1::opds_v1_series_latest(headers, uri, &app, &user).await
}

pub(crate) async fn opds_v1_books_latest_route(
    State(app): State<OpdsState>,
    OpdsV1Authenticated(user): OpdsV1Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    v1::opds_v1_books_latest(headers, uri, &app, &user).await
}

pub(crate) async fn opds_v1_libraries_route(
    State(app): State<OpdsState>,
    OpdsV1Authenticated(user): OpdsV1Authenticated,
    headers: HeaderMap,
) -> Response {
    v1::opds_v1_libraries(headers, &app, &user).await
}

pub(crate) async fn opds_v1_collections_route(
    State(app): State<OpdsState>,
    OpdsV1Authenticated(user): OpdsV1Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    v1::opds_v1_collections(headers, uri, &app, &user).await
}

pub(crate) async fn opds_v1_readlists_route(
    State(app): State<OpdsState>,
    OpdsV1Authenticated(user): OpdsV1Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    v1::opds_v1_readlists(headers, uri, &app, &user).await
}

pub(crate) async fn opds_v1_publishers_route(
    State(app): State<OpdsState>,
    OpdsV1Authenticated(user): OpdsV1Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    v1::opds_v1_publishers(headers, uri, &app, &user).await
}

pub(crate) async fn opds_v1_series_detail_route(
    State(app): State<OpdsState>,
    OpdsV1Authenticated(user): OpdsV1Authenticated,
    headers: HeaderMap,
    uri: Uri,
    AxumPath(series_id): AxumPath<String>,
) -> Response {
    v1::opds_v1_series_detail(headers, uri, &app, &series_id, &user).await
}

pub(crate) async fn opds_v1_library_detail_route(
    State(app): State<OpdsState>,
    OpdsV1Authenticated(user): OpdsV1Authenticated,
    headers: HeaderMap,
    uri: Uri,
    AxumPath(library_id): AxumPath<String>,
) -> Response {
    v1::opds_v1_library_detail(headers, uri, &app, &library_id, &user).await
}

pub(crate) async fn opds_v1_collection_detail_route(
    State(app): State<OpdsState>,
    OpdsV1Authenticated(user): OpdsV1Authenticated,
    headers: HeaderMap,
    uri: Uri,
    AxumPath(collection_id): AxumPath<String>,
) -> Response {
    v1::opds_v1_collection_detail(headers, uri, &app, &collection_id, &user).await
}

pub(crate) async fn opds_v1_readlist_detail_route(
    State(app): State<OpdsState>,
    OpdsV1Authenticated(user): OpdsV1Authenticated,
    headers: HeaderMap,
    uri: Uri,
    AxumPath(readlist_id): AxumPath<String>,
) -> Response {
    v1::opds_v1_readlist_detail(headers, uri, &app, &readlist_id, &user).await
}

pub(crate) async fn opds_v1_book_file_route(
    State(app): State<OpdsState>,
    OpdsV1Authenticated(user): OpdsV1Authenticated,
    AxumPath((book_id, _file_name)): AxumPath<(String, String)>,
) -> Response {
    if !user_is_admin(&user) && !user_has_role(&user, AuthUserRole::FileDownload) {
        return StatusCode::FORBIDDEN.into_response();
    }

    opds_book_media_responses(&app)
        .book_file(&user, &book_id)
        .await
}

pub(crate) async fn opds_v1_book_thumbnail_route(
    State(app): State<OpdsState>,
    OpdsV1Authenticated(user): OpdsV1Authenticated,
    headers: HeaderMap,
    AxumPath(book_id): AxumPath<String>,
) -> Response {
    opds_book_media_responses(&app)
        .book_thumbnail_opds(&headers, &book_id, &user)
        .await
}

pub(crate) async fn opds_v1_book_thumbnail_small_route(
    State(app): State<OpdsState>,
    OpdsV1Authenticated(user): OpdsV1Authenticated,
    headers: HeaderMap,
    AxumPath(book_id): AxumPath<String>,
) -> Response {
    opds_book_media_responses(&app)
        .book_thumbnail_opds_small_default(&headers, &book_id, &user)
        .await
}

pub(crate) async fn opds_v2_book_file_route(
    State(app): State<OpdsState>,
    OpdsV2Authenticated(user): OpdsV2Authenticated,
    AxumPath(book_id): AxumPath<String>,
) -> Response {
    if !user_is_admin(&user) && !user_has_role(&user, AuthUserRole::FileDownload) {
        return StatusCode::FORBIDDEN.into_response();
    }

    opds_book_media_responses(&app)
        .book_file(&user, &book_id)
        .await
}

pub(crate) async fn opds_v2_book_file_with_suffix_route(
    State(app): State<OpdsState>,
    OpdsV2Authenticated(user): OpdsV2Authenticated,
    AxumPath((book_id, _file_name)): AxumPath<(String, String)>,
) -> Response {
    if !user_is_admin(&user) && !user_has_role(&user, AuthUserRole::FileDownload) {
        return StatusCode::FORBIDDEN.into_response();
    }

    opds_book_media_responses(&app)
        .book_file(&user, &book_id)
        .await
}

pub(crate) async fn opds_v2_book_page_route(
    State(app): State<OpdsState>,
    OpdsV2Authenticated(user): OpdsV2Authenticated,
    headers: HeaderMap,
    Query(query): Query<BookPageQuery>,
    AxumPath((book_id, page_number)): AxumPath<(String, u32)>,
) -> Response {
    opds_book_media_responses(&app)
        .book_page(
            &user,
            &headers,
            &book_id,
            page_number,
            query.into_opds_v2_response_options(),
        )
        .await
}

pub(crate) async fn opds_v2_book_page_raw_route(
    State(app): State<OpdsState>,
    OpdsV2Authenticated(user): OpdsV2Authenticated,
    headers: HeaderMap,
    AxumPath((book_id, page_number)): AxumPath<(String, i32)>,
) -> Response {
    opds_book_media_responses(&app)
        .book_page_raw(&user, &headers, &book_id, page_number)
        .await
}

pub(crate) async fn opds_v2_book_thumbnail_route(
    State(app): State<OpdsState>,
    OpdsV2Authenticated(user): OpdsV2Authenticated,
    headers: HeaderMap,
    AxumPath(book_id): AxumPath<String>,
) -> Response {
    opds_book_media_responses(&app)
        .book_thumbnail_opds(&headers, &book_id, &user)
        .await
}

pub(crate) async fn opds_v2_library_route(
    State(app): State<OpdsState>,
    OpdsV2Authenticated(user): OpdsV2Authenticated,
    headers: HeaderMap,
    AxumPath(library_id): AxumPath<String>,
) -> Response {
    v2::opds_v2_library(headers, &app, &library_id, &user).await
}

pub(crate) async fn opds_v2_library_readlists_route(
    State(app): State<OpdsState>,
    OpdsV2Authenticated(user): OpdsV2Authenticated,
    headers: HeaderMap,
    uri: Uri,
    AxumPath(library_id): AxumPath<String>,
) -> Response {
    v2::opds_v2_library_readlists(headers, uri, &app, &library_id, &user).await
}

pub(crate) async fn opds_v2_libraries_readlists_route(
    State(app): State<OpdsState>,
    OpdsV2Authenticated(user): OpdsV2Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    v2::opds_v2_libraries_readlists(headers, uri, &app, &user).await
}

pub(crate) async fn opds_v2_libraries_keep_reading_route(
    State(app): State<OpdsState>,
    OpdsV2Authenticated(user): OpdsV2Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    v2::opds_v2_libraries_keep_reading(headers, uri, &app, &user).await
}

pub(crate) async fn opds_v2_library_keep_reading_route(
    State(app): State<OpdsState>,
    OpdsV2Authenticated(user): OpdsV2Authenticated,
    headers: HeaderMap,
    uri: Uri,
    AxumPath(library_id): AxumPath<String>,
) -> Response {
    v2::opds_v2_library_keep_reading(headers, uri, &app, &library_id, &user).await
}

pub(crate) async fn opds_v2_libraries_on_deck_route(
    State(app): State<OpdsState>,
    OpdsV2Authenticated(user): OpdsV2Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    v2::opds_v2_libraries_on_deck(headers, uri, &app, &user).await
}

pub(crate) async fn opds_v2_library_on_deck_route(
    State(app): State<OpdsState>,
    OpdsV2Authenticated(user): OpdsV2Authenticated,
    headers: HeaderMap,
    uri: Uri,
    AxumPath(library_id): AxumPath<String>,
) -> Response {
    v2::opds_v2_library_on_deck(headers, uri, &app, &library_id, &user).await
}

pub(crate) async fn opds_v2_libraries_latest_books_route(
    State(app): State<OpdsState>,
    OpdsV2Authenticated(user): OpdsV2Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    v2::opds_v2_libraries_latest_books(headers, uri, &app, &user).await
}

pub(crate) async fn opds_v2_library_latest_books_route(
    State(app): State<OpdsState>,
    OpdsV2Authenticated(user): OpdsV2Authenticated,
    headers: HeaderMap,
    uri: Uri,
    AxumPath(library_id): AxumPath<String>,
) -> Response {
    v2::opds_v2_library_latest_books(headers, uri, &app, &library_id, &user).await
}

pub(crate) async fn opds_v2_libraries_latest_series_route(
    State(app): State<OpdsState>,
    OpdsV2Authenticated(user): OpdsV2Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    v2::opds_v2_libraries_latest_series(headers, uri, &app, &user).await
}

pub(crate) async fn opds_v2_library_latest_series_route(
    State(app): State<OpdsState>,
    OpdsV2Authenticated(user): OpdsV2Authenticated,
    headers: HeaderMap,
    uri: Uri,
    AxumPath(library_id): AxumPath<String>,
) -> Response {
    v2::opds_v2_library_latest_series(headers, uri, &app, &library_id, &user).await
}

pub(crate) async fn opds_v2_libraries_browse_route(
    State(app): State<OpdsState>,
    OpdsV2Authenticated(user): OpdsV2Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    v2::opds_v2_libraries_browse(headers, uri, &app, &user).await
}

pub(crate) async fn opds_v2_library_browse_route(
    State(app): State<OpdsState>,
    OpdsV2Authenticated(user): OpdsV2Authenticated,
    headers: HeaderMap,
    uri: Uri,
    AxumPath(library_id): AxumPath<String>,
) -> Response {
    v2::opds_v2_library_browse(headers, uri, &app, Some(&library_id), &user).await
}

pub(crate) async fn opds_v2_libraries_collections_route(
    State(app): State<OpdsState>,
    OpdsV2Authenticated(user): OpdsV2Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    v2::opds_v2_libraries_collections(headers, uri, &app, &user).await
}

pub(crate) async fn opds_v2_library_collections_route(
    State(app): State<OpdsState>,
    OpdsV2Authenticated(user): OpdsV2Authenticated,
    headers: HeaderMap,
    uri: Uri,
    AxumPath(library_id): AxumPath<String>,
) -> Response {
    v2::opds_v2_library_collections(headers, uri, &app, &library_id, &user).await
}

pub(crate) async fn opds_v2_collection_route(
    State(app): State<OpdsState>,
    OpdsV2Authenticated(user): OpdsV2Authenticated,
    headers: HeaderMap,
    uri: Uri,
    AxumPath(collection_id): AxumPath<String>,
) -> Response {
    v2::opds_v2_collection(headers, uri, &app, &collection_id, &user).await
}

pub(crate) async fn opds_v2_series_route(
    State(app): State<OpdsState>,
    OpdsV2Authenticated(user): OpdsV2Authenticated,
    headers: HeaderMap,
    uri: Uri,
    AxumPath(series_id): AxumPath<String>,
) -> Response {
    v2::opds_v2_series(headers, uri, &app, &series_id, &user).await
}

pub(crate) async fn opds_v2_readlist_route(
    State(app): State<OpdsState>,
    OpdsV2Authenticated(user): OpdsV2Authenticated,
    headers: HeaderMap,
    uri: Uri,
    AxumPath(readlist_id): AxumPath<String>,
) -> Response {
    v2::opds_v2_readlist(headers, uri, &app, &readlist_id, &user).await
}

pub(crate) async fn opds_v2_search_route(
    State(app): State<OpdsState>,
    OpdsV2Authenticated(user): OpdsV2Authenticated,
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
        .map(|value| percent_decode(&value.replace('+', " ")));

    v2::opds_v2_search(headers, &app, query.as_deref(), &user).await
}
