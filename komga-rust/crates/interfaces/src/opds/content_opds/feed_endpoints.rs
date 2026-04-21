use axum::Json;
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::identity_access::auth::{require_auth, resolved_auth_user, user_id};
use crate::request_urls::app_absolute_url;
use crate::state::{HttpAppState, OpdsBookFeedEntry, OpdsSeriesEntry};

use super::auth_payload::opds_catalog_unauthorized_response;
use super::feeds::{
    normalize_opds_updated, opds_navigation_response_with_paging, opds_publication_for_feed_entry,
    opds_publications_response_with_paging, opds_subsection_navigation_link, paginate_vec,
    parse_page_size,
};
use super::persisted::{
    allowed_library_ids, content_allowed_by_restrictions, has_visible_collections_for_scope,
    has_visible_readlists_for_scope, library_visible, load_all_readlists, load_collection_series,
    load_collections, load_libraries, load_library, load_readlist_books,
    load_readlists_for_library, opds_restrictions, validate_library_scope,
};

pub(super) async fn opds_v2_keep_reading_feed(
    headers: HeaderMap,
    uri: Uri,
    app: &HttpAppState,
    library_id: Option<&str>,
) -> Response {
    let database_file = app.auth_db.database_file.as_path();
    if require_auth(&headers).is_some() {
        return opds_catalog_unauthorized_response(&headers);
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return opds_catalog_unauthorized_response(&headers);
    };
    if let Some(response) = validate_library_scope(
        app.services.opds_persisted.as_ref(),
        database_file,
        &allowed_library_ids,
        library_id,
    )
    .await
    {
        return response;
    }

    let Some(user) = resolved_auth_user(&headers) else {
        return opds_catalog_unauthorized_response(&headers);
    };
    let user_id = user_id(&user).to_string();
    let selected_library = if let Some(id) = library_id {
        match load_library(app.services.opds_persisted.as_ref(), database_file, id).await {
            Ok(library) => library,
            Err(error) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": format!("load OPDS keep-reading library scope: {error}") })),
                )
                    .into_response();
            }
        }
    } else {
        None
    };
    let restrictions = opds_restrictions(&headers);
    let (page, size) = parse_page_size(uri.query().unwrap_or_default());

    let books = match app
        .services
        .opds_catalog
        .load_keep_reading_books(app.auth_db.database_file.clone(), user_id.clone(), None)
        .await
    {
        Ok(books) => books,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS keep-reading books: {error}") })),
            )
                .into_response();
        }
    };

    let visible_books = books
        .into_iter()
        .filter(|book| {
            library_visible(&allowed_library_ids, &book.library_id)
                && content_allowed_by_restrictions(
                    restrictions.as_ref(),
                    book.age_rating,
                    &book.sharing_labels,
                )
        })
        .collect::<Vec<_>>();
    let total_visible_books = visible_books.len();
    let publications = visible_books
        .into_iter()
        .skip(page.saturating_mul(size))
        .take(size)
        .map(|book| opds_publication_for_feed_entry(&headers, &book))
        .collect::<Vec<_>>();

    let library_segment = library_id.map(|id| format!("/{id}")).unwrap_or_default();
    let self_path = format!("/opds/v2/libraries{library_segment}/keep-reading");
    let title = selected_library
        .as_ref()
        .map(|library| format!("{} - Keep Reading", library.name))
        .unwrap_or_else(|| "All libraries - Keep Reading".to_string());
    opds_publications_response_with_paging(
        &headers,
        title.as_str(),
        self_path.as_str(),
        selected_library
            .as_ref()
            .map(|library| library.last_modified.as_str()),
        publications,
        page,
        size,
        total_visible_books,
    )
}

pub(super) async fn opds_v2_on_deck_feed(
    headers: HeaderMap,
    uri: Uri,
    app: &HttpAppState,
    library_id: Option<&str>,
) -> Response {
    let database_file = app.auth_db.database_file.as_path();
    if require_auth(&headers).is_some() {
        return opds_catalog_unauthorized_response(&headers);
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return opds_catalog_unauthorized_response(&headers);
    };
    if let Some(response) = validate_library_scope(
        app.services.opds_persisted.as_ref(),
        database_file,
        &allowed_library_ids,
        library_id,
    )
    .await
    {
        return response;
    }

    let Some(user) = resolved_auth_user(&headers) else {
        return opds_catalog_unauthorized_response(&headers);
    };
    let user_id = user_id(&user).to_string();
    let restrictions = opds_restrictions(&headers);
    let selected_library = if let Some(id) = library_id {
        match load_library(app.services.opds_persisted.as_ref(), database_file, id).await {
            Ok(library) => library,
            Err(error) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": format!("load OPDS on-deck library scope: {error}") })),
                )
                    .into_response();
            }
        }
    } else {
        None
    };

    let books = match app
        .services
        .opds_catalog
        .load_on_deck_books(
            app.auth_db.database_file.clone(),
            user_id.clone(),
            library_id.map(str::to_string),
        )
        .await
    {
        Ok(books) => books,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS on-deck books: {error}") })),
            )
                .into_response();
        }
    };

    let (page, size) = parse_page_size(uri.query().unwrap_or_default());
    let visible_books = books
        .into_iter()
        .filter(|book| {
            library_visible(&allowed_library_ids, &book.library_id)
                && content_allowed_by_restrictions(
                    restrictions.as_ref(),
                    book.age_rating,
                    &book.sharing_labels,
                )
        })
        .collect::<Vec<_>>();
    let total_visible_books = visible_books.len();
    let publications = visible_books
        .into_iter()
        .skip(page.saturating_mul(size))
        .take(size)
        .map(|book| opds_publication_for_feed_entry(&headers, &book))
        .collect::<Vec<_>>();

    let library_segment = library_id.map(|id| format!("/{id}")).unwrap_or_default();
    let self_path = format!("/opds/v2/libraries{library_segment}/on-deck");
    let title = selected_library
        .as_ref()
        .map(|library| format!("{} - On Deck", library.name))
        .unwrap_or_else(|| "All libraries - On Deck".to_string());
    opds_publications_response_with_paging(
        &headers,
        title.as_str(),
        self_path.as_str(),
        selected_library
            .as_ref()
            .map(|library| library.last_modified.as_str()),
        publications,
        page,
        size,
        total_visible_books,
    )
}

pub(super) async fn opds_v2_latest_books_feed(
    headers: HeaderMap,
    uri: Uri,
    app: &HttpAppState,
    library_id: Option<&str>,
) -> Response {
    let database_file = app.auth_db.database_file.as_path();
    if require_auth(&headers).is_some() {
        return opds_catalog_unauthorized_response(&headers);
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return opds_catalog_unauthorized_response(&headers);
    };
    if let Some(response) = validate_library_scope(
        app.services.opds_persisted.as_ref(),
        database_file,
        &allowed_library_ids,
        library_id,
    )
    .await
    {
        return response;
    }

    let selected_library = if let Some(id) = library_id {
        match load_library(app.services.opds_persisted.as_ref(), database_file, id).await {
            Ok(library) => library,
            Err(error) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": format!("load OPDS latest-books library scope: {error}") })),
                )
                    .into_response();
            }
        }
    } else {
        None
    };
    let restrictions = opds_restrictions(&headers);
    let (page, size) = parse_page_size(uri.query().unwrap_or_default());

    let (visible_books, total_visible_books) = match load_visible_latest_books_page(
        app,
        &allowed_library_ids,
        restrictions.as_ref(),
        page,
        size,
    )
    .await
    {
        Ok(result) => result,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS latest books: {error}") })),
            )
                .into_response();
        }
    };

    let publications = visible_books
        .into_iter()
        .map(|book| opds_publication_for_feed_entry(&headers, &book))
        .collect::<Vec<_>>();

    let library_segment = library_id.map(|id| format!("/{id}")).unwrap_or_default();
    let self_path = format!("/opds/v2/libraries{library_segment}/books/latest");
    let title = selected_library
        .as_ref()
        .map(|library| format!("{} - Latest Books", library.name))
        .unwrap_or_else(|| "All libraries - Latest Books".to_string());
    opds_publications_response_with_paging(
        &headers,
        title.as_str(),
        self_path.as_str(),
        selected_library
            .as_ref()
            .map(|library| library.last_modified.as_str()),
        publications,
        page,
        size,
        total_visible_books,
    )
}

async fn load_visible_latest_books_page(
    app: &HttpAppState,
    allowed_library_ids: &Option<std::collections::HashSet<String>>,
    restrictions: Option<&super::types::OpdsRestrictions>,
    page: usize,
    size: usize,
) -> Result<(Vec<OpdsBookFeedEntry>, usize), String> {
    let mut offset = 0;
    let mut visible = Vec::new();
    let mut total = 0;
    let scan_limit = std::cmp::max(size, 100) as i64;
    let page_start = page.saturating_mul(size);
    let page_end = page_start.saturating_add(size);

    loop {
        let batch = app
            .services
            .opds_catalog
            .load_latest_books_paged(
                app.auth_db.database_file.clone(),
                allowed_library_ids.clone(),
                None,
                None,
                offset,
                scan_limit,
            )
            .await?;
        if batch.is_empty() {
            break;
        }

        let batch_len = batch.len();
        for book in batch {
            if library_visible(allowed_library_ids, &book.library_id)
                && content_allowed_by_restrictions(
                    restrictions,
                    book.age_rating,
                    &book.sharing_labels,
                )
            {
                if total >= page_start && total < page_end {
                    visible.push(book);
                }
                total += 1;
            }
        }

        if batch_len < scan_limit as usize {
            break;
        }
        offset += batch_len as i64;
    }

    Ok((visible, total))
}

pub(super) async fn opds_v2_latest_series_feed(
    headers: HeaderMap,
    uri: Uri,
    app: &HttpAppState,
    library_id: Option<&str>,
) -> Response {
    let database_file = app.auth_db.database_file.as_path();
    if require_auth(&headers).is_some() {
        return opds_catalog_unauthorized_response(&headers);
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if let Some(response) = validate_library_scope(
        app.services.opds_persisted.as_ref(),
        database_file,
        &allowed_library_ids,
        library_id,
    )
    .await
    {
        return response;
    }

    let libraries = match load_libraries(app.services.opds_persisted.as_ref(), database_file).await
    {
        Ok(libraries) => libraries,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS libraries: {error}") })),
            )
                .into_response();
        }
    };
    let selected_library =
        library_id.and_then(|id| libraries.iter().find(|library| library.id == id));

    let restrictions = opds_restrictions(&headers);
    let (page, size) = parse_page_size(uri.query().unwrap_or_default());
    let (visible_series, total_series) = match load_visible_latest_series_page(
        app,
        &allowed_library_ids,
        restrictions.as_ref(),
        library_id,
        page,
        size,
    )
    .await
    {
        Ok(result) => result,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS latest series: {error}") })),
            )
                .into_response();
        }
    };

    let navigation = visible_series
        .into_iter()
        .map(|series| {
            json!({
                "title": series.title,
                "href": app_absolute_url(&headers, format!("/opds/v2/series/{}", series.id).as_str()),
                "type": "application/opds+json",
            })
        })
        .collect::<Vec<_>>();

    let library_segment = library_id.map(|id| format!("/{id}")).unwrap_or_default();
    let self_path = format!("/opds/v2/libraries{library_segment}/series/latest");
    let modified = selected_library
        .map(|library| library.last_modified.as_str())
        .filter(|value| !value.is_empty());
    let title = selected_library
        .map(|library| format!("{} - Latest Series", library.name))
        .unwrap_or_else(|| "All libraries - Latest Series".to_string());

    opds_navigation_response_with_paging(
        &headers,
        title.as_str(),
        self_path.as_str(),
        modified,
        navigation,
        page,
        size,
        total_series,
    )
}

async fn load_visible_latest_series_page(
    app: &HttpAppState,
    allowed_library_ids: &Option<std::collections::HashSet<String>>,
    restrictions: Option<&super::types::OpdsRestrictions>,
    library_id: Option<&str>,
    page: usize,
    size: usize,
) -> Result<(Vec<OpdsSeriesEntry>, usize), String> {
    let batch_size = 100_i64.max(size as i64);
    let start = page.saturating_mul(size);
    let end = start.saturating_add(size);
    let mut offset = 0_i64;
    let mut total = 0_usize;
    let mut visible = Vec::new();

    loop {
        let batch = app
            .services
            .opds_catalog
            .load_latest_series_paged(
                app.auth_db.database_file.clone(),
                allowed_library_ids.clone(),
                library_id.map(str::to_string),
                offset,
                batch_size,
            )
            .await?;
        if batch.is_empty() {
            break;
        }

        let batch_len = batch.len();
        for series in batch {
            if !series.one_shot
                && library_visible(allowed_library_ids, &series.library_id)
                && content_allowed_by_restrictions(
                    restrictions,
                    series.age_rating,
                    &series.sharing_labels,
                )
            {
                if total >= start && total < end {
                    visible.push(series);
                }
                total += 1;
            }
        }

        if batch_len < batch_size as usize {
            break;
        }
        offset += batch_len as i64;
    }

    Ok((visible, total))
}

pub(super) async fn opds_v2_collections_feed(
    headers: HeaderMap,
    uri: Uri,
    app: &HttpAppState,
    library_id: Option<&str>,
) -> Response {
    let database_file = app.auth_db.database_file.as_path();
    if require_auth(&headers).is_some() {
        return opds_catalog_unauthorized_response(&headers);
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if let Some(response) = validate_library_scope(
        app.services.opds_persisted.as_ref(),
        database_file,
        &allowed_library_ids,
        library_id,
    )
    .await
    {
        return response;
    }

    let libraries = match load_libraries(app.services.opds_persisted.as_ref(), database_file).await
    {
        Ok(libraries) => libraries,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS libraries: {error}") })),
            )
                .into_response();
        }
    };
    let selected_library =
        library_id.and_then(|id| libraries.iter().find(|library| library.id == id));
    let restrictions = opds_restrictions(&headers);
    let (page, size) = parse_page_size(uri.query().unwrap_or_default());

    let collections = match load_collections(
        app.services.opds_persisted.as_ref(),
        database_file,
        library_id,
    )
    .await
    {
        Ok(collections) => collections,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS collections: {error}") })),
            )
                .into_response();
        }
    };

    let mut visible_collections = Vec::new();
    for collection in collections {
        let series = match load_collection_series(
            app.services.opds_persisted.as_ref(),
            database_file,
            &collection.id,
            collection.ordered,
        )
        .await
        {
            Ok(series) => series,
            Err(_) => continue,
        };
        if series
            .iter()
            .any(|series| library_visible(&allowed_library_ids, &series.library_id))
        {
            visible_collections.push(collection);
        }
    }
    let total_visible_collections = visible_collections.len();
    let (paged_collections, has_next) = paginate_vec(visible_collections, page, size);
    let collection_navigation = paged_collections
        .into_iter()
        .map(|collection| {
            json!({
                "title": collection.name,
                "href": app_absolute_url(&headers, format!("/opds/v2/collections/{}", collection.id).as_str()),
                "type": "application/opds+json",
            })
        })
        .collect::<Vec<_>>();

    let library_segment = library_id.map(|id| format!("/{id}")).unwrap_or_default();
    let self_path = format!("/opds/v2/libraries{library_segment}/collections");
    let has_visible_collections = has_visible_collections_for_scope(
        app.services.opds_persisted.as_ref(),
        database_file,
        &allowed_library_ids,
        restrictions.as_ref(),
        library_id,
    )
    .await;
    let has_visible_readlists = has_visible_readlists_for_scope(
        app.services.opds_catalog.as_ref(),
        app.services.opds_persisted.as_ref(),
        database_file,
        &allowed_library_ids,
        restrictions.as_ref(),
        library_id,
    )
    .await;

    let mut navigation = vec![
        json!({
            "title": "Recommended",
            "rel": "subsection",
            "href": app_absolute_url(&headers, format!("/opds/v2/libraries{library_segment}").as_str()),
            "type": "application/opds+json",
        }),
        json!({
            "title": "Browse",
            "rel": "subsection",
            "href": app_absolute_url(&headers, format!("/opds/v2/libraries{library_segment}/browse").as_str()),
            "type": "application/opds+json",
        }),
    ];
    if has_visible_collections {
        navigation.push(json!({
            "title": "Collections",
            "rel": "subsection",
            "href": app_absolute_url(&headers, format!("/opds/v2/libraries{library_segment}/collections").as_str()),
            "type": "application/opds+json",
        }));
    }
    if has_visible_readlists {
        navigation.push(json!({
            "title": "Read lists",
            "rel": "subsection",
            "href": app_absolute_url(&headers, format!("/opds/v2/libraries{library_segment}/readlists").as_str()),
            "type": "application/opds+json",
        }));
    }

    let modified = selected_library
        .map(|library| library.last_modified.as_str())
        .filter(|value| !value.is_empty())
        .map(normalize_opds_updated)
        .unwrap_or_else(super::feeds::opds_now_timestamp);

    let mut links = vec![
        json!({
            "rel": "self",
            "href": app_absolute_url(&headers, self_path.as_str()),
        }),
        json!({
            "title": "Home",
            "rel": "start",
            "href": app_absolute_url(&headers, "/opds/v2/catalog"),
            "type": "application/opds+json",
        }),
        json!({
            "title": "Search",
            "rel": "search",
            "href": app_absolute_url(&headers, "/opds/v2/search{?query}"),
            "type": "application/opds+json",
            "templated": true,
        }),
    ];
    if page > 0 {
        links.push(json!({
            "rel": "previous",
            "href": app_absolute_url(&headers, format!("{self_path}?page={}", page.saturating_sub(1)).as_str()),
        }));
    }
    if has_next {
        links.push(json!({
            "rel": "next",
            "href": app_absolute_url(&headers, format!("{self_path}?page={}", page + 1).as_str()),
        }));
    }

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/opds+json"),
        )],
        Json(json!({
            "metadata": {
                "title": selected_library
                    .as_ref()
                    .map(|library| format!("{} - Collections", library.name))
                    .unwrap_or_else(|| "All libraries - Collections".to_string()),
                "modified": modified,
                "itemsPerPage": size,
                "currentPage": page + 1,
                "numberOfItems": total_visible_collections,
            },
            "links": links,
            "navigation": navigation,
            "groups": [
                {
                    "metadata": {
                        "title": "Collections"
                    },
                    "navigation": collection_navigation,
                }
            ],
        })),
    )
        .into_response()
}

pub(super) async fn opds_v2_readlists_feed(
    headers: HeaderMap,
    uri: Uri,
    app: &HttpAppState,
    library_id: Option<&str>,
) -> Response {
    let database_file = app.auth_db.database_file.as_path();
    if require_auth(&headers).is_some() {
        return opds_catalog_unauthorized_response(&headers);
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if let Some(response) = validate_library_scope(
        app.services.opds_persisted.as_ref(),
        database_file,
        &allowed_library_ids,
        library_id,
    )
    .await
    {
        return response;
    }

    let selected_library = if let Some(id) = library_id {
        match load_library(app.services.opds_persisted.as_ref(), database_file, id).await {
            Ok(library) => library,
            Err(error) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": format!("load OPDS library readlists scope: {error}") })),
                )
                    .into_response();
            }
        }
    } else {
        None
    };

    let restrictions = opds_restrictions(&headers);
    let (page, size) = parse_page_size(uri.query().unwrap_or_default());
    let readlists = match match library_id {
        Some(id) => {
            load_readlists_for_library(app.services.opds_persisted.as_ref(), database_file, id)
                .await
        }
        None => load_all_readlists(app.services.opds_catalog.as_ref(), database_file).await,
    } {
        Ok(readlists) => readlists,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS readlists: {error}") })),
            )
                .into_response();
        }
    };

    let visible_readlists = if library_id.is_some() {
        readlists
    } else {
        let mut visible = Vec::new();
        for readlist in readlists {
            let readlist_books = match load_readlist_books(
                app.services.opds_persisted.as_ref(),
                database_file,
                &readlist.id,
            )
            .await
            {
                Ok(books) => books,
                Err(_) => continue,
            };
            if readlist_books
                .iter()
                .any(|book| library_visible(&allowed_library_ids, &book.library_id))
            {
                visible.push(readlist);
            }
        }
        visible
    };

    let total_readlists = visible_readlists.len();
    let readlist_navigation = visible_readlists
        .into_iter()
        .skip(page.saturating_mul(size))
        .take(size)
        .map(|readlist| {
            json!({
                "title": readlist.name,
                "href": app_absolute_url(&headers, format!("/opds/v2/readlists/{}", readlist.id).as_str()),
                "type": "application/opds+json",
            })
        })
        .collect::<Vec<_>>();

    let library_segment = library_id.map(|id| format!("/{id}")).unwrap_or_default();
    let has_visible_collections = has_visible_collections_for_scope(
        app.services.opds_persisted.as_ref(),
        database_file,
        &allowed_library_ids,
        restrictions.as_ref(),
        library_id,
    )
    .await;
    let has_visible_readlists = has_visible_readlists_for_scope(
        app.services.opds_catalog.as_ref(),
        app.services.opds_persisted.as_ref(),
        database_file,
        &allowed_library_ids,
        restrictions.as_ref(),
        library_id,
    )
    .await;
    let mut navigation = vec![
        opds_subsection_navigation_link(
            &headers,
            "Recommended",
            format!("/opds/v2/libraries{library_segment}").as_str(),
        ),
        opds_subsection_navigation_link(
            &headers,
            "Browse",
            format!("/opds/v2/libraries{library_segment}/browse").as_str(),
        ),
    ];
    if has_visible_collections {
        navigation.push(opds_subsection_navigation_link(
            &headers,
            "Collections",
            format!("/opds/v2/libraries{library_segment}/collections").as_str(),
        ));
    }
    if has_visible_readlists {
        navigation.push(opds_subsection_navigation_link(
            &headers,
            "Read lists",
            format!("/opds/v2/libraries{library_segment}/readlists").as_str(),
        ));
    }

    let modified = selected_library
        .as_ref()
        .map(|library| library.last_modified.as_str())
        .filter(|value| !value.is_empty())
        .map(normalize_opds_updated)
        .unwrap_or_else(super::feeds::opds_now_timestamp);
    let self_path = format!("/opds/v2/libraries{library_segment}/readlists");
    let mut links = vec![
        json!({
            "rel": "self",
            "href": app_absolute_url(&headers, self_path.as_str()),
        }),
        json!({
            "title": "Home",
            "rel": "start",
            "href": app_absolute_url(&headers, "/opds/v2/catalog"),
            "type": "application/opds+json",
        }),
        json!({
            "title": "Search",
            "rel": "search",
            "href": app_absolute_url(&headers, "/opds/v2/search{?query}"),
            "type": "application/opds+json",
            "templated": true,
        }),
    ];
    if page > 0 {
        links.push(json!({
            "rel": "previous",
            "href": app_absolute_url(&headers, format!("{self_path}?page={}", page.saturating_sub(1)).as_str()),
        }));
    }
    if page.saturating_add(1).saturating_mul(size) < total_readlists {
        links.push(json!({
            "rel": "next",
            "href": app_absolute_url(&headers, format!("{self_path}?page={}", page + 1).as_str()),
        }));
    }

    let readlists_group = json!({
        "metadata": {
            "title": "Read Lists",
        },
        "navigation": readlist_navigation,
    });

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/opds+json"),
        )],
        Json(json!({
            "metadata": {
                "title": selected_library
                    .as_ref()
                    .map(|library| format!("{} - Read Lists", library.name))
                    .unwrap_or_else(|| "All libraries - Read Lists".to_string()),
                "modified": modified,
                "itemsPerPage": size,
                "currentPage": page + 1,
                "numberOfItems": total_readlists,
            },
            "links": links,
            "navigation": navigation,
            "groups": [readlists_group],
        })),
    )
        .into_response()
}
