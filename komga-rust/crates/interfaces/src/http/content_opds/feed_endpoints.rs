use std::path::Path;

use axum::Json;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::http::identity_access::auth::{require_auth, resolved_auth_user, user_id};
use crate::http::request_urls::app_absolute_url;
use crate::opds_catalog_access::{
    load_keep_reading_books, load_latest_books, load_latest_series, load_on_deck_books,
};

use super::feeds::{
    opds_navigation_response, opds_publication_for_book, opds_publications_response,
};
use super::persisted::{
    allowed_library_ids, content_allowed_by_restrictions, library_visible, load_collection_books,
    load_collections, opds_restrictions, validate_library_scope,
};

pub(super) async fn opds_v2_keep_reading_feed(
    headers: HeaderMap,
    database_file: &Path,
    library_id: Option<&str>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if let Some(response) =
        validate_library_scope(database_file, &allowed_library_ids, library_id).await
    {
        return response;
    }

    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let user_id = user_id(&user).to_string();
    let restrictions = opds_restrictions(&headers);

    let books = match load_keep_reading_books(database_file, &user_id, library_id).await {
        Ok(books) => books,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS keep-reading books: {error}") })),
            )
                .into_response();
        }
    };

    let publications = books
        .into_iter()
        .filter(|book| {
            library_visible(&allowed_library_ids, &book.library_id)
                && content_allowed_by_restrictions(
                    restrictions.as_ref(),
                    book.age_rating,
                    &book.sharing_labels,
                )
        })
        .map(|book| opds_publication_for_book(&headers, &book.id, &book.title, &book.media_type))
        .collect::<Vec<_>>();

    let library_segment = library_id.map(|id| format!("/{id}")).unwrap_or_default();
    opds_publications_response(
        &headers,
        "Keep Reading",
        app_absolute_url(
            &headers,
            format!("/opds/v2/libraries{library_segment}/keep-reading").as_str(),
        )
        .as_str(),
        publications,
    )
}

pub(super) async fn opds_v2_on_deck_feed(
    headers: HeaderMap,
    database_file: &Path,
    library_id: Option<&str>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if let Some(response) =
        validate_library_scope(database_file, &allowed_library_ids, library_id).await
    {
        return response;
    }

    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let user_id = user_id(&user).to_string();
    let restrictions = opds_restrictions(&headers);

    let books = match load_on_deck_books(database_file, &user_id, library_id).await {
        Ok(books) => books,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS on-deck books: {error}") })),
            )
                .into_response();
        }
    };

    let publications = books
        .into_iter()
        .filter(|book| {
            library_visible(&allowed_library_ids, &book.library_id)
                && content_allowed_by_restrictions(
                    restrictions.as_ref(),
                    book.age_rating,
                    &book.sharing_labels,
                )
        })
        .map(|book| opds_publication_for_book(&headers, &book.id, &book.title, &book.media_type))
        .collect::<Vec<_>>();

    let library_segment = library_id.map(|id| format!("/{id}")).unwrap_or_default();
    opds_publications_response(
        &headers,
        "On Deck",
        app_absolute_url(
            &headers,
            format!("/opds/v2/libraries{library_segment}/on-deck").as_str(),
        )
        .as_str(),
        publications,
    )
}

pub(super) async fn opds_v2_latest_books_feed(
    headers: HeaderMap,
    database_file: &Path,
    library_id: Option<&str>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if let Some(response) =
        validate_library_scope(database_file, &allowed_library_ids, library_id).await
    {
        return response;
    }
    let restrictions = opds_restrictions(&headers);

    let books = match load_latest_books(database_file, library_id, 100).await {
        Ok(books) => books,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS latest books: {error}") })),
            )
                .into_response();
        }
    };

    let publications = books
        .into_iter()
        .filter(|book| {
            library_visible(&allowed_library_ids, &book.library_id)
                && content_allowed_by_restrictions(
                    restrictions.as_ref(),
                    book.age_rating,
                    &book.sharing_labels,
                )
        })
        .map(|book| opds_publication_for_book(&headers, &book.id, &book.title, &book.media_type))
        .collect::<Vec<_>>();

    let library_segment = library_id.map(|id| format!("/{id}")).unwrap_or_default();
    opds_publications_response(
        &headers,
        "Latest Books",
        app_absolute_url(
            &headers,
            format!("/opds/v2/libraries{library_segment}/books/latest").as_str(),
        )
        .as_str(),
        publications,
    )
}

pub(super) async fn opds_v2_latest_series_feed(
    headers: HeaderMap,
    database_file: &Path,
    library_id: Option<&str>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if let Some(response) =
        validate_library_scope(database_file, &allowed_library_ids, library_id).await
    {
        return response;
    }

    let rows = match load_latest_series(database_file, library_id, 100).await {
        Ok(rows) => rows,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS latest series: {error}") })),
            )
                .into_response();
        }
    };

    let navigation = rows
        .into_iter()
        .filter(|series| library_visible(&allowed_library_ids, &series.library_id))
        .map(|series| {
            json!({
                "title": series.title,
                "href": app_absolute_url(&headers, format!("/opds/v2/series/{}", series.id).as_str()),
                "type": "application/opds+json",
            })
        })
        .collect::<Vec<_>>();

    let library_segment = library_id.map(|id| format!("/{id}")).unwrap_or_default();
    opds_navigation_response(
        &headers,
        "Latest Series",
        app_absolute_url(
            &headers,
            format!("/opds/v2/libraries{library_segment}/books/latest").as_str(),
        )
        .as_str(),
        navigation,
    )
}

pub(super) async fn opds_v2_collections_feed(
    headers: HeaderMap,
    database_file: &Path,
    library_id: Option<&str>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if let Some(response) =
        validate_library_scope(database_file, &allowed_library_ids, library_id).await
    {
        return response;
    }

    let collections = match load_collections(database_file, library_id).await {
        Ok(collections) => collections,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS collections: {error}") })),
            )
                .into_response();
        }
    };

    let mut navigation = Vec::new();
    for collection in collections {
        let books = match load_collection_books(database_file, &collection.id).await {
            Ok(books) => books,
            Err(_) => continue,
        };
        if books
            .iter()
            .any(|book| library_visible(&allowed_library_ids, &book.library_id))
        {
            navigation.push(json!({
                "title": collection.name,
                "href": app_absolute_url(&headers, format!("/opds/v2/collections/{}", collection.id).as_str()),
                "type": "application/opds+json",
            }));
        }
    }

    let library_segment = library_id.map(|id| format!("/{id}")).unwrap_or_default();
    opds_navigation_response(
        &headers,
        "Collections",
        app_absolute_url(
            &headers,
            format!("/opds/v2/libraries{library_segment}/collections").as_str(),
        )
        .as_str(),
        navigation,
    )
}
