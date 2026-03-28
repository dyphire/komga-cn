use axum::Json;
use axum::http::Uri;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use komga_persistence::sqlite::connect_pool;
use serde_json::Value;
use serde_json::json;
use sqlx::Row;
use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::app::CompatProfile;
use crate::app::discovery_auth::AgeRestrictionKind;
use crate::app::runtime_auth::{
    require_auth, resolved_auth_user, user_id, user_payload_json, user_shared_all_libraries,
    user_shared_library_ids,
};
use crate::app::snapshots::{app_absolute_url, opds_auth_json, request_base_url};

pub(super) async fn opds_manifest(
    _profile: CompatProfile,
    headers: HeaderMap,
    database_file: &Path,
    book_id: &str,
) -> Response {
    opds_manifest_variant(_profile, headers, database_file, book_id, None).await
}

pub(super) async fn opds_manifest_with_profile(
    _profile: CompatProfile,
    headers: HeaderMap,
    database_file: &Path,
    book_id: &str,
    profile: &str,
) -> Response {
    opds_manifest_variant(_profile, headers, database_file, book_id, Some(profile)).await
}

async fn opds_manifest_variant(
    _profile: CompatProfile,
    headers: HeaderMap,
    database_file: &Path,
    book_id: &str,
    profile: Option<&str>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    match persisted_opds_manifest(database_file, &headers, book_id, profile).await {
        Ok(Some(manifest)) => {
            return (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/opds-publication+json")],
                Json(manifest),
            )
                .into_response();
        }
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "error": format!(
                        "OPDS manifest requires persisted/domain-backed data; snapshot fallback removed for book {book_id}",
                    ),
                })),
            )
                .into_response();
        }
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load persisted OPDS manifest: {error}") })),
            )
                .into_response();
        }
    }
}

async fn persisted_opds_manifest(
    database_file: &Path,
    headers: &HeaderMap,
    book_id: &str,
    profile: Option<&str>,
) -> Result<Option<Value>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1).await?;
    let row = sqlx::query(
        "SELECT COALESCE(bm.TITLE, b.NAME) AS TITLE, b.NAME AS NAME, m.MEDIA_TYPE AS MEDIA_TYPE, \
                COALESCE(m.PAGE_COUNT, 1) AS PAGE_COUNT \
         FROM BOOK b \
         LEFT \
         JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID \
         LEFT \
         JOIN MEDIA m ON m.BOOK_ID = b.ID \
         WHERE b.ID = ? \
         LIMIT 1",
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    let title = row.get::<String, _>("TITLE");
    let file_name = row.get::<String, _>("NAME");
    let media_type = row
        .try_get::<String, _>("MEDIA_TYPE")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| content_type_from_filename(&file_name, "application/octet-stream"));
    if !manifest_profile_matches_media(profile, &media_type) {
        return Ok(None);
    }
    let page_count = row.try_get::<i64, _>("PAGE_COUNT").unwrap_or(1).max(1) as usize;
    let manifest =
        persisted_manifest_payload(headers, book_id, &title, &media_type, page_count, profile);

    Ok(Some(manifest))
}

fn manifest_profile_matches_media(profile: Option<&str>, media_type: &str) -> bool {
    let Some(profile) = profile else {
        return true;
    };

    match profile {
        "epub" => media_type == "application/epub+zip",
        "pdf" => media_type == "application/pdf",
        "divina" => {
            media_type.starts_with("image/")
                || media_type == "application/vnd.comicbook+zip"
                || media_type == "application/vnd.comicbook-rar"
        }
        _ => false,
    }
}

fn persisted_manifest_payload(
    headers: &HeaderMap,
    book_id: &str,
    title: &str,
    media_type: &str,
    page_count: usize,
    profile: Option<&str>,
) -> Value {
    let self_href = if let Some(profile) = profile {
        app_absolute_url(
            headers,
            format!("/opds/v2/books/{book_id}/manifest/{profile}").as_str(),
        )
    } else {
        app_absolute_url(
            headers,
            format!("/opds/v2/books/{book_id}/manifest").as_str(),
        )
    };
    let auth_href = app_absolute_url(headers, "/opds/v2/auth");
    let file_href = app_absolute_url(headers, format!("/opds/v2/books/{book_id}/file").as_str());
    let progression_href = app_absolute_url(
        headers,
        format!("/opds/v2/books/{book_id}/progression").as_str(),
    );
    let thumbnail_href = app_absolute_url(
        headers,
        format!("/opds/v2/books/{book_id}/thumbnail").as_str(),
    );
    let profile_tag = profile.unwrap_or_else(|| manifest_profile_name(media_type));

    let mut reading_order = Vec::new();
    let mut page_list = Vec::new();
    match profile_tag {
        "pdf" | "divina" => {
            for page in 1..=page_count {
                let href = app_absolute_url(
                    headers,
                    format!("/opds/v2/books/{book_id}/pages/{page}/raw").as_str(),
                );
                reading_order.push(json!({
                    "href": href,
                    "type": "image/jpeg",
                }));
                page_list.push(json!({
                    "href": href,
                    "title": format!("Page {page}"),
                }));
            }
        }
        _ => {
            reading_order.push(json!({
                "href": file_href,
                "type": media_type,
            }));
        }
    }

    let mut links = vec![
        json!({
          "rel": "self",
          "href": self_href,
          "type": "application/opds-publication+json",
          "properties": {
            "authenticate": {
              "href": auth_href,
              "type": "application/opds-authentication+json",
            },
          },
        }),
        json!({
          "rel": "http://opds-spec.org/acquisition",
          "href": app_absolute_url(headers, format!("/opds/v2/books/{book_id}/file").as_str()),
          "type": media_type,
          "properties": {
            "authenticate": {
              "href": app_absolute_url(headers, "/opds/v2/auth"),
              "type": "application/opds-authentication+json",
            },
          },
        }),
        json!({
          "rel": "http://www.cantook.com/api/progression",
          "href": progression_href,
          "type": "application/vnd.readium.progression+json",
          "properties": {
            "authenticate": {
              "href": app_absolute_url(headers, "/opds/v2/auth"),
              "type": "application/opds-authentication+json",
            },
          },
        }),
    ];

    for variant in ["epub", "pdf", "divina"] {
        links.push(json!({
            "rel": "alternate",
            "href": app_absolute_url(headers, format!("/opds/v2/books/{book_id}/manifest/{variant}").as_str()),
            "type": "application/opds-publication+json",
            "title": variant,
        }));
    }

    json!({
      "context": "https://readium.org/webpub-manifest/context.jsonld",
      "metadata": {
        "title": title,
      },
      "links": links,
      "images": [],
      "readingOrder": reading_order,
      "resources": [
        {
          "href": thumbnail_href,
          "type": "image/jpeg",
          "properties": {
            "authenticate": {
              "href": app_absolute_url(headers, "/opds/v2/auth"),
              "type": "application/opds-authentication+json",
            },
          },
        }
      ],
      "toc": [],
      "landmarks": [],
      "pageList": page_list,
    })
}

fn manifest_profile_name(media_type: &str) -> &'static str {
    match media_type {
        "application/pdf" => "pdf",
        "application/vnd.comicbook+zip" | "application/vnd.comicbook-rar" => "divina",
        _ => "epub",
    }
}

fn content_type_from_filename(file_name: &str, fallback: &str) -> String {
    let extension = file_name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default();

    match extension.as_str() {
        "cbz" => "application/vnd.comicbook+zip".to_string(),
        "cbr" => "application/vnd.comicbook-rar".to_string(),
        "pdf" => "application/pdf".to_string(),
        "epub" => "application/epub+zip".to_string(),
        "jpg" | "jpeg" => "image/jpeg".to_string(),
        "png" => "image/png".to_string(),
        _ => fallback.to_string(),
    }
}

pub(super) async fn opds_auth(headers: HeaderMap) -> Response {
    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/opds-authentication+json"),
        )],
        Json(opds_auth_json(&headers)),
    )
        .into_response()
}

pub(super) async fn opds_catalog(headers: HeaderMap, database_file: &Path) -> Response {
    if require_auth(&headers).is_none() {
        return opds_v2_libraries(headers, database_file).await;
    }

    let base_url = request_base_url(&headers);
    let auth_href = format!("{base_url}/opds/v2/auth");
    let link = format!(
        "<{}>; rel=\"http://opds-spec.org/auth/document\"; type=\"application/opds-authentication+json\"",
        auth_href
    );

    (
        StatusCode::UNAUTHORIZED,
        [
            (
                header::WWW_AUTHENTICATE,
                HeaderValue::from_static("Basic realm=\"Realm\""),
            ),
            (header::LINK, HeaderValue::from_str(&link).unwrap()),
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/opds-authentication+json;charset=UTF-8"),
            ),
        ],
        Json(opds_auth_json(&headers)),
    )
        .into_response()
}

pub(super) async fn opds_v1_catalog(headers: HeaderMap) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }
    opds_v1_navigation_feed_response(
        &headers,
        "root",
        "Komga OPDS catalog",
        "/opds/v1.2/catalog",
        vec![
            (
                "keepReading".to_string(),
                "Keep Reading".to_string(),
                "/opds/v1.2/keep-reading".to_string(),
            ),
            (
                "ondeck".to_string(),
                "On Deck".to_string(),
                "/opds/v1.2/ondeck".to_string(),
            ),
            (
                "allSeries".to_string(),
                "All series".to_string(),
                "/opds/v1.2/series".to_string(),
            ),
            (
                "latestSeries".to_string(),
                "Latest series".to_string(),
                "/opds/v1.2/series/latest".to_string(),
            ),
            (
                "latestBooks".to_string(),
                "Latest books".to_string(),
                "/opds/v1.2/books/latest".to_string(),
            ),
            (
                "allLibraries".to_string(),
                "All libraries".to_string(),
                "/opds/v1.2/libraries".to_string(),
            ),
            (
                "allCollections".to_string(),
                "All collections".to_string(),
                "/opds/v1.2/collections".to_string(),
            ),
            (
                "allReadLists".to_string(),
                "All read lists".to_string(),
                "/opds/v1.2/readlists".to_string(),
            ),
            (
                "allPublishers".to_string(),
                "All publishers".to_string(),
                "/opds/v1.2/publishers".to_string(),
            ),
        ],
        None,
    )
}

pub(super) async fn opds_v1_search(headers: HeaderMap) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let template_href = app_absolute_url(&headers, "/opds/v1.2/series?search={searchTerms}");
    let payload = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?><OpenSearchDescription xmlns=\"http://a9.com/-/spec/opensearch/1.1/\"><ShortName>Search</ShortName><Description>Search for series</Description><Url type=\"application/atom+xml;profile=opds-catalog;kind=navigation\" template=\"{}\"/></OpenSearchDescription>",
        xml_escape(&template_href)
    );

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/xml"),
        )],
        payload,
    )
        .into_response()
}

pub(super) async fn opds_v1_on_deck(
    headers: HeaderMap,
    uri: Uri,
    database_file: &Path,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let user_id = user_id(&user).to_string();
    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let (page, size) = parse_page_size(uri.query().unwrap_or_default());
    let books = load_on_deck_books(database_file, &user_id, None)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|book| library_visible(&allowed_library_ids, &book.library_id))
        .collect::<Vec<_>>();
    let (books, has_next) = paginate_vec(books, page, size);

    opds_v1_acquisition_feed_response(
        &headers,
        "ondeck",
        "On Deck",
        "/opds/v1.2/ondeck",
        books,
        None,
        Some((page, has_next)),
    )
}

pub(super) async fn opds_v1_keep_reading(
    headers: HeaderMap,
    uri: Uri,
    database_file: &Path,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let user_id = user_id(&user).to_string();
    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let (page, size) = parse_page_size(uri.query().unwrap_or_default());
    let books = load_keep_reading_books(database_file, &user_id, None)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|book| library_visible(&allowed_library_ids, &book.library_id))
        .collect::<Vec<_>>();
    let (books, has_next) = paginate_vec(books, page, size);

    opds_v1_acquisition_feed_response(
        &headers,
        "keepReading",
        "Keep Reading",
        "/opds/v1.2/keep-reading",
        books,
        None,
        Some((page, has_next)),
    )
}

pub(super) async fn opds_v1_series_latest(
    headers: HeaderMap,
    uri: Uri,
    database_file: &Path,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let (page, size) = parse_page_size(uri.query().unwrap_or_default());
    let rows = load_latest_series_paged(
        database_file,
        &allowed_library_ids,
        None,
        page.saturating_mul(size) as i64,
        (size + 1) as i64,
    )
    .await
    .unwrap_or_default();
    let (rows, has_next) = paginate_vec(rows, 0, size);

    opds_v1_navigation_feed_response(
        &headers,
        "latestSeries",
        "Latest series",
        "/opds/v1.2/series/latest",
        rows.into_iter()
            .map(|series| {
                let series_id = series.id.clone();
                (
                    series_id.clone(),
                    series.title,
                    format!("/opds/v1.2/series/{series_id}"),
                )
            })
            .collect(),
        Some((page, has_next)),
    )
}

pub(super) async fn opds_v1_books_latest(
    headers: HeaderMap,
    uri: Uri,
    database_file: &Path,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let (page, size) = parse_page_size(uri.query().unwrap_or_default());
    let books = load_latest_books_paged(
        database_file,
        &allowed_library_ids,
        None,
        page.saturating_mul(size) as i64,
        (size + 1) as i64,
    )
    .await
    .unwrap_or_default();
    let (books, has_next) = paginate_vec(books, 0, size);

    opds_v1_acquisition_feed_response(
        &headers,
        "latestBooks",
        "Latest books",
        "/opds/v1.2/books/latest",
        books,
        None,
        Some((page, has_next)),
    )
}

pub(super) async fn opds_v1_libraries(headers: HeaderMap, database_file: &Path) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let rows = load_libraries(database_file)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|library| library_visible(&allowed_library_ids, &library.id))
        .map(|library| {
            (
                library.id.clone(),
                library.name,
                format!("/opds/v1.2/libraries/{}", library.id),
            )
        })
        .collect::<Vec<_>>();

    opds_v1_navigation_feed_response(
        &headers,
        "allLibraries",
        "All libraries",
        "/opds/v1.2/libraries",
        rows,
        None,
    )
}

pub(super) async fn opds_v1_collections(
    headers: HeaderMap,
    uri: Uri,
    database_file: &Path,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let mut rows = Vec::new();
    for collection in load_collections(database_file, None)
        .await
        .unwrap_or_default()
    {
        let books = load_collection_books(database_file, &collection.id)
            .await
            .unwrap_or_default();
        if books
            .iter()
            .any(|book| library_visible(&allowed_library_ids, &book.library_id))
        {
            rows.push((
                collection.id.clone(),
                collection.name,
                format!("/opds/v1.2/collections/{}", collection.id),
            ));
        }
    }

    let (page, size) = parse_page_size(uri.query().unwrap_or_default());
    let (rows, has_next) = paginate_vec(rows, page, size);

    opds_v1_navigation_feed_response(
        &headers,
        "allCollections",
        "All collections",
        "/opds/v1.2/collections",
        rows,
        Some((page, has_next)),
    )
}

pub(super) async fn opds_v1_readlists(
    headers: HeaderMap,
    uri: Uri,
    database_file: &Path,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let mut rows = Vec::new();
    for readlist in load_all_readlists(database_file).await.unwrap_or_default() {
        let books = load_readlist_books(database_file, &readlist.id)
            .await
            .unwrap_or_default();
        if books
            .iter()
            .any(|book| library_visible(&allowed_library_ids, &book.library_id))
        {
            rows.push((
                readlist.id.clone(),
                readlist.name,
                format!("/opds/v1.2/readlists/{}", readlist.id),
            ));
        }
    }

    let (page, size) = parse_page_size(uri.query().unwrap_or_default());
    let (rows, has_next) = paginate_vec(rows, page, size);

    opds_v1_navigation_feed_response(
        &headers,
        "allReadLists",
        "All read lists",
        "/opds/v1.2/readlists",
        rows,
        Some((page, has_next)),
    )
}

pub(super) async fn opds_v1_publishers(
    headers: HeaderMap,
    uri: Uri,
    database_file: &Path,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let publishers = load_publishers(database_file, &allowed_library_ids)
        .await
        .unwrap_or_default();
    let (page, size) = parse_page_size(uri.query().unwrap_or_default());
    let rows = publishers
        .into_iter()
        .map(|publisher| {
            (
                publisher.clone(),
                publisher.clone(),
                format!("/opds/v1.2/series?publisher={}", query_escape(&publisher)),
            )
        })
        .collect::<Vec<_>>();
    let (rows, has_next) = paginate_vec(rows, page, size);
    opds_v1_navigation_feed_response(
        &headers,
        "allPublishers",
        "All publishers",
        "/opds/v1.2/publishers",
        rows,
        Some((page, has_next)),
    )
}

pub(super) async fn opds_v1_series_detail(
    headers: HeaderMap,
    uri: Uri,
    database_file: &Path,
    series_id: &str,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let Some(series) = load_series(database_file, series_id).await.unwrap_or(None) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !library_visible(&allowed_library_ids, &series.library_id) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let (page, size) = parse_page_size(uri.query().unwrap_or_default());
    let books = load_series_books_paged(
        database_file,
        &series.id,
        page.saturating_mul(size) as i64,
        (size + 1) as i64,
    )
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|book| PersistedBookFeedItem {
        id: book.id,
        title: book.title,
        file_name: book.file_name,
        media_type: book.media_type,
        library_id: series.library_id.clone(),
        age_rating: None,
        sharing_labels: vec![],
        last_modified: book.last_modified,
    })
    .collect::<Vec<_>>();
    let (books, has_next) = paginate_vec(books, 0, size);
    opds_v1_acquisition_feed_response(
        &headers,
        series.id.as_str(),
        series.title.as_str(),
        format!("/opds/v1.2/series/{series_id}").as_str(),
        books,
        Some(series.last_modified.as_str()),
        Some((page, has_next)),
    )
}

pub(super) async fn opds_v1_library_detail(
    headers: HeaderMap,
    uri: Uri,
    database_file: &Path,
    library_id: &str,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Some(library) = load_library(database_file, library_id)
        .await
        .unwrap_or(None)
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !library_visible(&allowed_library_ids, library_id) {
        return StatusCode::FORBIDDEN.into_response();
    }

    let query = uri.query().unwrap_or_default();
    let page = query_value(query, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .clamp(1, 100);
    let offset = page.saturating_mul(size);

    let series = load_library_series(database_file, library_id, offset as i64, (size + 1) as i64)
        .await
        .unwrap_or_default();
    let mut entries = series
        .into_iter()
        .filter(|item| library_visible(&allowed_library_ids, &item.library_id))
        .collect::<Vec<_>>();
    let has_next = entries.len() > size;
    if has_next {
        entries.truncate(size);
    }

    opds_v1_library_series_feed_response(
        &headers,
        library.id.as_str(),
        library.name.as_str(),
        format!("/opds/v1.2/libraries/{library_id}").as_str(),
        entries,
        None,
        page,
        has_next,
    )
}

pub(super) async fn opds_v1_collection_detail(
    headers: HeaderMap,
    uri: Uri,
    database_file: &Path,
    collection_id: &str,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Some(collection) = load_collection(database_file, collection_id)
        .await
        .unwrap_or(None)
    else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let series = load_collection_series(database_file, collection_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|series| library_visible(&allowed_library_ids, &series.library_id))
        .collect::<Vec<_>>();
    let (page, size) = parse_page_size(uri.query().unwrap_or_default());
    let (series, has_next) = paginate_vec(series, page, size);
    if series.is_empty() {
        return StatusCode::FORBIDDEN.into_response();
    }

    let entries = series
        .into_iter()
        .map(|series| {
            let id = series.id;
            (id.clone(), series.title, format!("/opds/v1.2/series/{id}"))
        })
        .collect::<Vec<_>>();

    opds_v1_navigation_feed_response(
        &headers,
        collection.id.as_str(),
        collection.name.as_str(),
        format!("/opds/v1.2/collections/{collection_id}").as_str(),
        entries,
        Some((page, has_next)),
    )
}

pub(super) async fn opds_v1_readlist_detail(
    headers: HeaderMap,
    uri: Uri,
    database_file: &Path,
    readlist_id: &str,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Some(readlist) = load_readlist(database_file, readlist_id)
        .await
        .unwrap_or(None)
    else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let books = load_readlist_books(database_file, readlist_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|book| library_visible(&allowed_library_ids, &book.library_id))
        .map(|book| PersistedBookFeedItem {
            id: book.id,
            title: book.title,
            file_name: book.file_name,
            media_type: book.media_type,
            library_id: book.library_id,
            age_rating: book.age_rating,
            sharing_labels: book.sharing_labels,
            last_modified: book.last_modified,
        })
        .collect::<Vec<_>>();
    let (page, size) = parse_page_size(uri.query().unwrap_or_default());
    let (books, has_next) = paginate_vec(books, page, size);
    if books.is_empty() {
        return StatusCode::FORBIDDEN.into_response();
    }
    opds_v1_acquisition_feed_response(
        &headers,
        readlist.id.as_str(),
        readlist.name.as_str(),
        format!("/opds/v1.2/readlists/{readlist_id}").as_str(),
        books,
        Some(readlist.last_modified.as_str()),
        Some((page, has_next)),
    )
}

pub(super) async fn opds_v1_series(
    profile: CompatProfile,
    headers: HeaderMap,
    uri: Uri,
    database_file: &Path,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let _ = profile;

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let query = uri.query().unwrap_or_default();
    let (page, size) = parse_page_size(query);
    let search = query_value(query, "search").map(percent_decode);
    let publishers = query_values(query, "publisher");

    let rows = load_series_page(
        database_file,
        &allowed_library_ids,
        search.as_deref(),
        publishers.as_slice(),
        page.saturating_mul(size) as i64,
        (size + 1) as i64,
    )
    .await
    .unwrap_or_default();
    let (rows, has_next) = paginate_vec(rows, 0, size);

    let entries = rows
        .into_iter()
        .map(|series| {
            let series_id = series.id;
            (
                series_id.clone(),
                series.title,
                format!("/opds/v1.2/series/{series_id}"),
            )
        })
        .collect::<Vec<_>>();

    opds_v1_navigation_feed_response(
        &headers,
        "allSeries",
        "All series",
        "/opds/v1.2/series",
        entries,
        Some((page, has_next)),
    )
}

pub(super) async fn opds_v2_libraries(headers: HeaderMap, database_file: &Path) -> Response {
    opds_v2_recommended(headers, database_file, None).await
}

pub(super) async fn opds_v2_library(
    headers: HeaderMap,
    database_file: &Path,
    library_id: &str,
) -> Response {
    opds_v2_recommended(headers, database_file, Some(library_id)).await
}

async fn opds_v2_recommended(
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

    let libraries = match load_libraries(database_file).await {
        Ok(libraries) => libraries,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS libraries: {error}") })),
            )
                .into_response();
        }
    };

    let selected_library = if let Some(id) = library_id {
        let Some(library) = libraries.iter().find(|library| library.id == id).cloned() else {
            return StatusCode::NOT_FOUND.into_response();
        };
        if !library_visible(&allowed_library_ids, id) {
            return StatusCode::FORBIDDEN.into_response();
        }
        Some(library)
    } else {
        None
    };

    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let user_id_value = user_id(&user).to_string();

    let library_segment = selected_library
        .as_ref()
        .map(|library| format!("/{}", library.id))
        .unwrap_or_default();
    let self_path = format!("/opds/v2/libraries{library_segment}");

    let mut keep_reading = load_keep_reading_books(database_file, &user_id_value, library_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|book| library_visible(&allowed_library_ids, &book.library_id))
        .collect::<Vec<_>>();
    keep_reading.truncate(5);
    let keep_reading_publications = keep_reading
        .into_iter()
        .map(|book| opds_publication_for_book(&headers, &book.id, &book.title, &book.media_type))
        .collect::<Vec<_>>();

    let mut on_deck = load_on_deck_books(database_file, &user_id_value, library_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|book| library_visible(&allowed_library_ids, &book.library_id))
        .collect::<Vec<_>>();
    on_deck.truncate(5);
    let on_deck_publications = on_deck
        .into_iter()
        .map(|book| opds_publication_for_book(&headers, &book.id, &book.title, &book.media_type))
        .collect::<Vec<_>>();

    let mut latest_books = load_latest_books(database_file, library_id, 5)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|book| library_visible(&allowed_library_ids, &book.library_id))
        .collect::<Vec<_>>();
    latest_books.truncate(5);
    let latest_books_publications = latest_books
        .into_iter()
        .map(|book| opds_publication_for_book(&headers, &book.id, &book.title, &book.media_type))
        .collect::<Vec<_>>();

    let mut latest_series = load_latest_series(database_file, library_id, 5)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|series| library_visible(&allowed_library_ids, &series.library_id))
        .collect::<Vec<_>>();
    latest_series.truncate(5);
    let latest_series_navigation = latest_series
        .into_iter()
        .map(|series| {
            json!({
                "title": series.title,
                "href": app_absolute_url(&headers, format!("/opds/v2/series/{}", series.id).as_str()),
                "type": "application/opds+json",
            })
        })
        .collect::<Vec<_>>();

    let restrictions = opds_restrictions(&headers);

    let has_visible_collections = has_visible_collections_for_scope(
        database_file,
        &allowed_library_ids,
        restrictions.as_ref(),
        library_id,
    )
    .await;
    let has_visible_readlists = has_visible_readlists_for_scope(
        database_file,
        &allowed_library_ids,
        restrictions.as_ref(),
        library_id,
    )
    .await;

    let mut navigation = vec![
        opds_subsection_navigation_link(&headers, "Recommended", self_path.as_str()),
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

    let mut groups = Vec::new();
    if selected_library.is_none() {
        let libraries_navigation = libraries
            .into_iter()
            .filter(|library| library_visible(&allowed_library_ids, &library.id))
            .map(|library| {
                opds_navigation_link(
                    &headers,
                    library.name.as_str(),
                    format!("/opds/v2/libraries/{}", library.id).as_str(),
                )
            })
            .collect::<Vec<_>>();
        if !libraries_navigation.is_empty() {
            groups.push(json!({
                "metadata": { "title": "Libraries" },
                "links": [{
                    "rel": "self",
                    "href": app_absolute_url(&headers, "/opds/v2/libraries"),
                    "type": "application/opds+json",
                }],
                "navigation": libraries_navigation,
            }));
        }
    }
    if !keep_reading_publications.is_empty() {
        groups.push(json!({
            "metadata": { "title": "Keep Reading" },
            "links": [{
                "rel": "self",
                "href": app_absolute_url(&headers, format!("/opds/v2/libraries{library_segment}/keep-reading").as_str()),
                "type": "application/opds+json",
            }],
            "publications": keep_reading_publications,
        }));
    }
    if !on_deck_publications.is_empty() {
        groups.push(json!({
            "metadata": { "title": "On Deck" },
            "links": [{
                "rel": "self",
                "href": app_absolute_url(&headers, format!("/opds/v2/libraries{library_segment}/on-deck").as_str()),
                "type": "application/opds+json",
            }],
            "publications": on_deck_publications,
        }));
    }
    if !latest_books_publications.is_empty() {
        groups.push(json!({
            "metadata": { "title": "Latest Books" },
            "links": [{
                "rel": "self",
                "href": app_absolute_url(&headers, format!("/opds/v2/libraries{library_segment}/books/latest").as_str()),
                "type": "application/opds+json",
            }],
            "publications": latest_books_publications,
        }));
    }
    if !latest_series_navigation.is_empty() {
        groups.push(json!({
            "metadata": { "title": "Latest Series" },
            "links": [{
                "rel": "self",
                "href": app_absolute_url(&headers, format!("/opds/v2/libraries{library_segment}/series/latest").as_str()),
                "type": "application/opds+json",
            }],
            "navigation": latest_series_navigation,
        }));
    }

    let modified = selected_library
        .as_ref()
        .map(|library| library.last_modified.as_str())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(opds_now_timestamp);

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
                    .map(|library| format!("{} - Recommended", library.name))
                    .unwrap_or_else(|| "All libraries - Recommended".to_string()),
                "modified": modified,
            },
            "links": [
                {
                    "rel": "self",
                    "href": app_absolute_url(&headers, self_path.as_str()),
                    "type": "application/opds+json",
                },
                {
                    "rel": "start",
                    "href": app_absolute_url(&headers, "/opds/v2/catalog"),
                    "type": "application/opds+json",
                },
                {
                    "rel": "search",
                    "href": app_absolute_url(&headers, "/opds/v2/search{?query}"),
                    "type": "application/opds+json",
                    "templated": true,
                }
            ],
            "navigation": navigation,
            "groups": groups,
        })),
    )
        .into_response()
}

pub(super) async fn opds_v2_libraries_keep_reading(
    headers: HeaderMap,
    database_file: &Path,
) -> Response {
    opds_v2_keep_reading_feed(headers, database_file, None).await
}

pub(super) async fn opds_v2_library_keep_reading(
    headers: HeaderMap,
    database_file: &Path,
    library_id: &str,
) -> Response {
    opds_v2_keep_reading_feed(headers, database_file, Some(library_id)).await
}

pub(super) async fn opds_v2_libraries_on_deck(
    headers: HeaderMap,
    database_file: &Path,
) -> Response {
    opds_v2_on_deck_feed(headers, database_file, None).await
}

pub(super) async fn opds_v2_library_on_deck(
    headers: HeaderMap,
    database_file: &Path,
    library_id: &str,
) -> Response {
    opds_v2_on_deck_feed(headers, database_file, Some(library_id)).await
}

pub(super) async fn opds_v2_libraries_latest_books(
    headers: HeaderMap,
    database_file: &Path,
) -> Response {
    opds_v2_latest_books_feed(headers, database_file, None).await
}

pub(super) async fn opds_v2_library_latest_books(
    headers: HeaderMap,
    database_file: &Path,
    library_id: &str,
) -> Response {
    opds_v2_latest_books_feed(headers, database_file, Some(library_id)).await
}

pub(super) async fn opds_v2_libraries_latest_series(
    headers: HeaderMap,
    database_file: &Path,
) -> Response {
    opds_v2_latest_series_feed(headers, database_file, None).await
}

pub(super) async fn opds_v2_library_latest_series(
    headers: HeaderMap,
    database_file: &Path,
    library_id: &str,
) -> Response {
    opds_v2_latest_series_feed(headers, database_file, Some(library_id)).await
}

pub(super) async fn opds_v2_libraries_browse(
    headers: HeaderMap,
    uri: Uri,
    database_file: &Path,
) -> Response {
    opds_v2_library_browse(headers, uri, database_file, None).await
}

pub(super) async fn opds_v2_library_browse(
    headers: HeaderMap,
    uri: Uri,
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

    let libraries = match load_libraries(database_file).await {
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

    let library_segment = library_id.map(|id| format!("/{id}")).unwrap_or_default();
    let browse_base_path = format!("/opds/v2/libraries{library_segment}/browse");
    let self_href = app_absolute_url(&headers, browse_base_path.as_str());
    let query = uri.query().unwrap_or_default();
    let page = query_value(query, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .clamp(1, 100);
    let publishers = query
        .split('&')
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let key = parts.next().unwrap_or_default();
            let value = parts.next().unwrap_or_default();
            if key == "publisher" && !value.is_empty() {
                Some(percent_decode(value))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

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
    let has_collections = has_visible_collections_for_scope(
        database_file,
        &allowed_library_ids,
        restrictions.as_ref(),
        library_id,
    )
    .await;
    if has_collections {
        navigation.push(opds_subsection_navigation_link(
            &headers,
            "Collections",
            format!("/opds/v2/libraries{library_segment}/collections").as_str(),
        ));
    }
    let has_readlists = has_visible_readlists_for_scope(
        database_file,
        &allowed_library_ids,
        restrictions.as_ref(),
        library_id,
    )
    .await;
    if has_readlists {
        navigation.push(opds_subsection_navigation_link(
            &headers,
            "Read lists",
            format!("/opds/v2/libraries{library_segment}/readlists").as_str(),
        ));
    }

    let (series_navigation, total_series) = load_browse_series_navigation(
        &headers,
        database_file,
        &allowed_library_ids,
        library_id,
        publishers.as_slice(),
        page,
        size,
    )
    .await
    .unwrap_or_default();
    let publisher_navigation =
        load_browse_publisher_navigation(&headers, database_file, &allowed_library_ids, library_id)
            .await
            .unwrap_or_default();
    let mut groups = vec![json!({
        "metadata": { "title": "Series" },
        "navigation": series_navigation,
    })];
    if !publisher_navigation.is_empty() {
        groups.push(json!({
            "metadata": { "title": "Publisher" },
            "navigation": publisher_navigation,
        }));
    }
    let mut links = vec![
        json!({
            "rel": "self",
            "href": self_href,
            "type": "application/opds+json",
        }),
        json!({
            "rel": "start",
            "href": app_absolute_url(&headers, "/opds/v2/catalog"),
            "type": "application/opds+json",
        }),
        json!({
            "rel": "search",
            "href": app_absolute_url(&headers, "/opds/v2/search{?query}"),
            "type": "application/opds+json",
            "templated": true,
        }),
    ];
    if page > 0 {
        links.push(json!({
            "rel": "previous",
            "href": app_absolute_url(&headers, format!("{browse_base_path}?page={}", page.saturating_sub(1)).as_str()),
            "type": "application/opds+json",
        }));
    }
    if (page + 1) * size < total_series {
        links.push(json!({
            "rel": "next",
            "href": app_absolute_url(&headers, format!("{browse_base_path}?page={}", page + 1).as_str()),
            "type": "application/opds+json",
        }));
    }

    let modified = selected_library
        .as_ref()
        .map(|library| library.last_modified.as_str())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(opds_now_timestamp);

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
                    .map(|library| library.name.clone())
                    .unwrap_or_else(|| "All libraries".to_string()),
                "modified": modified,
                "itemsPerPage": size,
                "currentPage": page + 1,
                "numberOfItems": total_series,
            },
            "links": links,
            "navigation": navigation,
            "groups": groups,
        })),
    )
        .into_response()
}

pub(super) async fn opds_v2_libraries_collections(
    headers: HeaderMap,
    database_file: &Path,
) -> Response {
    opds_v2_collections_feed(headers, database_file, None).await
}

pub(super) async fn opds_v2_library_collections(
    headers: HeaderMap,
    database_file: &Path,
    library_id: &str,
) -> Response {
    opds_v2_collections_feed(headers, database_file, Some(library_id)).await
}

pub(super) async fn opds_v2_collection(
    headers: HeaderMap,
    database_file: &Path,
    collection_id: &str,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let Some(collection) = (match load_collection(database_file, collection_id).await {
        Ok(collection) => collection,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS collection: {error}") })),
            )
                .into_response();
        }
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let books = match load_collection_books(database_file, collection_id).await {
        Ok(books) => books,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS collection books: {error}") })),
            )
                .into_response();
        }
    };

    let visible_books = books
        .into_iter()
        .filter(|book| library_visible(&allowed_library_ids, &book.library_id))
        .collect::<Vec<_>>();

    if visible_books.is_empty()
        && !collection_empty_for_authorized_user(database_file, collection_id, &allowed_library_ids)
            .await
    {
        return StatusCode::FORBIDDEN.into_response();
    }

    let publications = visible_books
        .into_iter()
        .map(|book| opds_publication_for_book(&headers, &book.id, &book.title, &book.media_type))
        .collect::<Vec<_>>();

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/opds+json"),
        )],
        Json(json!({
            "metadata": {
                "title": collection.name,
            },
            "links": [
                {
                    "rel": "self",
                    "href": app_absolute_url(&headers, format!("/opds/v2/collections/{collection_id}").as_str()),
                    "type": "application/opds+json",
                },
                {
                    "rel": "start",
                    "href": app_absolute_url(&headers, "/opds/v2/catalog"),
                    "type": "application/opds+json",
                }
            ],
            "publications": publications,
        })),
    )
        .into_response()
}

pub(super) async fn opds_v2_libraries_readlists(
    headers: HeaderMap,
    database_file: &Path,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let readlists = match load_all_readlists(database_file).await {
        Ok(readlists) => readlists,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS readlists: {error}") })),
            )
                .into_response();
        }
    };

    let mut navigation = Vec::new();
    for readlist in readlists {
        let readlist_books = match load_readlist_books(database_file, &readlist.id).await {
            Ok(books) => books,
            Err(_) => continue,
        };
        if readlist_books
            .iter()
            .any(|book| library_visible(&allowed_library_ids, &book.library_id))
        {
            navigation.push(json!({
                "title": readlist.name,
                "href": app_absolute_url(&headers, format!("/opds/v2/readlists/{}", readlist.id).as_str()),
                "type": "application/opds+json",
            }));
        }
    }

    opds_navigation_response(
        &headers,
        "Read lists",
        app_absolute_url(&headers, "/opds/v2/libraries/readlists").as_str(),
        navigation,
        None,
    )
}

pub(super) async fn opds_v2_book_thumbnail_small(headers: HeaderMap, book_id: &str) -> Response {
    redirect_to_opds_v2(headers, &format!("/opds/v2/books/{book_id}/thumbnail"))
}

pub(super) async fn opds_v2_library_readlists(
    headers: HeaderMap,
    database_file: &Path,
    library_id: &str,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    if !library_visible(&allowed_library_ids, library_id) {
        return StatusCode::FORBIDDEN.into_response();
    }

    let readlists = match load_readlists_for_library(database_file, library_id).await {
        Ok(readlists) => readlists,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS library readlists: {error}") })),
            )
                .into_response();
        }
    };

    let navigation = readlists
        .into_iter()
        .map(|readlist| {
            json!({
                "title": readlist.name,
                "href": app_absolute_url(&headers, format!("/opds/v2/readlists/{}", readlist.id).as_str()),
                "type": "application/opds+json",
            })
        })
        .collect::<Vec<_>>();

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/opds+json"),
        )],
        Json(json!({
            "metadata": {
                "title": "Read lists",
            },
            "links": [
                {
                    "rel": "self",
                    "href": app_absolute_url(&headers, format!("/opds/v2/libraries/{library_id}/readlists").as_str()),
                    "type": "application/opds+json",
                },
                {
                    "rel": "start",
                    "href": app_absolute_url(&headers, "/opds/v2/catalog"),
                    "type": "application/opds+json",
                }
            ],
            "navigation": navigation,
        })),
    )
        .into_response()
}

pub(super) async fn opds_v2_series(
    headers: HeaderMap,
    database_file: &Path,
    series_id: &str,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let Some(series) = (match load_series(database_file, series_id).await {
        Ok(series) => series,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS series: {error}") })),
            )
                .into_response();
        }
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    if !library_visible(&allowed_library_ids, &series.library_id) {
        return StatusCode::FORBIDDEN.into_response();
    }

    let books = match load_series_books(database_file, &series.id).await {
        Ok(books) => books,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS series books: {error}") })),
            )
                .into_response();
        }
    };

    let publications = books
        .into_iter()
        .map(|book| {
            json!({
                "metadata": {
                    "title": book.title,
                },
                "links": [
                    {
                        "rel": "self",
                        "href": app_absolute_url(&headers, format!("/opds/v2/books/{}/manifest", book.id).as_str()),
                        "type": "application/opds-publication+json",
                    },
                    {
                        "rel": "http://opds-spec.org/acquisition",
                        "href": app_absolute_url(&headers, format!("/opds/v2/books/{}/file", book.id).as_str()),
                        "type": book.media_type,
                    }
                ],
            })
        })
        .collect::<Vec<_>>();

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/opds+json"),
        )],
        Json(json!({
            "metadata": {
                "title": series.title,
            },
            "links": [
                {
                    "rel": "self",
                    "href": app_absolute_url(&headers, format!("/opds/v2/series/{}", series.id).as_str()),
                    "type": "application/opds+json",
                },
                {
                    "rel": "start",
                    "href": app_absolute_url(&headers, "/opds/v2/catalog"),
                    "type": "application/opds+json",
                }
            ],
            "publications": publications,
        })),
    )
        .into_response()
}

pub(super) async fn opds_v2_readlist(
    headers: HeaderMap,
    database_file: &Path,
    readlist_id: &str,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let Some(readlist) = (match load_readlist(database_file, readlist_id).await {
        Ok(readlist) => readlist,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS readlist: {error}") })),
            )
                .into_response();
        }
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let books = match load_readlist_books(database_file, &readlist.id).await {
        Ok(books) => books,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS readlist books: {error}") })),
            )
                .into_response();
        }
    };

    let visible_books = books
        .into_iter()
        .filter(|book| library_visible(&allowed_library_ids, &book.library_id))
        .collect::<Vec<_>>();
    if visible_books.is_empty() {
        return StatusCode::FORBIDDEN.into_response();
    }

    let publications = visible_books
        .into_iter()
        .map(|book| {
            json!({
                "metadata": {
                    "title": book.title,
                },
                "links": [
                    {
                        "rel": "self",
                        "href": app_absolute_url(&headers, format!("/opds/v2/books/{}/manifest", book.id).as_str()),
                        "type": "application/opds-publication+json",
                    },
                    {
                        "rel": "http://opds-spec.org/acquisition",
                        "href": app_absolute_url(&headers, format!("/opds/v2/books/{}/file", book.id).as_str()),
                        "type": book.media_type,
                    }
                ],
            })
        })
        .collect::<Vec<_>>();

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/opds+json"),
        )],
        Json(json!({
            "metadata": {
                "title": readlist.name,
            },
            "links": [
                {
                    "rel": "self",
                    "href": app_absolute_url(&headers, format!("/opds/v2/readlists/{}", readlist.id).as_str()),
                    "type": "application/opds+json",
                },
                {
                    "rel": "start",
                    "href": app_absolute_url(&headers, "/opds/v2/catalog"),
                    "type": "application/opds+json",
                }
            ],
            "publications": publications,
        })),
    )
        .into_response()
}

pub(super) async fn opds_v2_search(
    headers: HeaderMap,
    database_file: &Path,
    query: Option<&str>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let search_query = query.unwrap_or_default().trim();

    let (series, books, collections, readlists) =
        match load_search_results(database_file, search_query).await {
            Ok(results) => results,
            Err(error) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": format!("load OPDS search results: {error}") })),
                )
                    .into_response();
            }
        };

    let series_navigation = series
        .into_iter()
        .filter(|item| library_visible(&allowed_library_ids, &item.library_id))
        .map(|item| {
            json!({
                "title": item.title,
                "href": app_absolute_url(&headers, format!("/opds/v2/series/{}", item.id).as_str()),
                "type": "application/opds+json",
            })
        })
        .collect::<Vec<_>>();

    let book_publications = books
        .into_iter()
        .filter(|item| library_visible(&allowed_library_ids, &item.library_id))
        .map(|item| {
            json!({
                "metadata": {
                    "title": item.title,
                },
                "links": [
                    {
                        "rel": "self",
                        "href": app_absolute_url(&headers, format!("/opds/v2/books/{}/manifest", item.id).as_str()),
                        "type": "application/opds-publication+json",
                    }
                ],
            })
        })
        .collect::<Vec<_>>();

    let mut collections_navigation = Vec::new();
    for item in collections {
        let books = match load_collection_books(database_file, &item.id).await {
            Ok(books) => books,
            Err(_) => continue,
        };
        if books
            .iter()
            .any(|book| library_visible(&allowed_library_ids, &book.library_id))
        {
            collections_navigation.push(json!({
                "title": item.name,
                "href": app_absolute_url(&headers, format!("/opds/v2/collections/{}", item.id).as_str()),
                "type": "application/opds+json",
            }));
        }
    }

    let mut readlist_navigation = Vec::new();
    for item in readlists {
        let books = match load_readlist_books(database_file, &item.id).await {
            Ok(books) => books,
            Err(_) => continue,
        };
        if books
            .iter()
            .any(|book| library_visible(&allowed_library_ids, &book.library_id))
        {
            readlist_navigation.push(json!({
                "title": item.name,
                "href": app_absolute_url(&headers, format!("/opds/v2/readlists/{}", item.id).as_str()),
                "type": "application/opds+json",
            }));
        }
    }

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/opds+json"),
        )],
        Json(json!({
            "metadata": {
                "title": "Search results",
            },
            "links": [
                {
                    "rel": "start",
                    "href": app_absolute_url(&headers, "/opds/v2/catalog"),
                    "type": "application/opds+json",
                },
                {
                    "rel": "search",
                    "href": app_absolute_url(&headers, "/opds/v2/search{?query}"),
                    "type": "application/opds+json",
                    "templated": true,
                }
            ],
            "groups": [
                {
                    "metadata": {
                        "title": "Series",
                    },
                    "navigation": series_navigation,
                },
                {
                    "metadata": {
                        "title": "Books",
                    },
                    "publications": book_publications,
                },
                {
                    "metadata": {
                        "title": "Collections",
                    },
                    "navigation": collections_navigation,
                },
                {
                    "metadata": {
                        "title": "Read Lists",
                    },
                    "navigation": readlist_navigation,
                }
            ],
        })),
    )
        .into_response()
}

#[derive(Clone)]
struct PersistedLibrary {
    id: String,
    name: String,
    last_modified: String,
}

struct PersistedSeries {
    id: String,
    library_id: String,
    title: String,
    last_modified: String,
}

struct PersistedSeriesBook {
    id: String,
    title: String,
    file_name: String,
    media_type: String,
    last_modified: String,
}

struct PersistedReadlist {
    id: String,
    name: String,
    last_modified: String,
}

struct PersistedReadlistBook {
    id: String,
    title: String,
    file_name: String,
    media_type: String,
    library_id: String,
    age_rating: Option<u16>,
    sharing_labels: Vec<String>,
    last_modified: String,
}

struct PersistedSeriesSearchResult {
    id: String,
    title: String,
    library_id: String,
}

struct PersistedBookSearchResult {
    id: String,
    title: String,
    library_id: String,
}

struct PersistedReadlistSearchResult {
    id: String,
    name: String,
}

struct PersistedCollectionSearchResult {
    id: String,
    name: String,
}

struct PersistedBookFeedItem {
    id: String,
    title: String,
    file_name: String,
    media_type: String,
    library_id: String,
    age_rating: Option<u16>,
    sharing_labels: Vec<String>,
    last_modified: String,
}

struct PersistedCollection {
    id: String,
    name: String,
}

#[derive(Clone, Debug, Default)]
struct OpdsRestrictions {
    age: Option<u16>,
    age_restriction: Option<AgeRestrictionKind>,
    labels_allow: Vec<String>,
    labels_exclude: Vec<String>,
}

fn allowed_library_ids(headers: &HeaderMap) -> Option<Option<HashSet<String>>> {
    let user = resolved_auth_user(headers)?;
    if user_shared_all_libraries(&user) {
        return Some(None);
    }

    let ids = user_shared_library_ids(&user)
        .iter()
        .cloned()
        .collect::<HashSet<_>>();
    Some(Some(ids))
}

fn library_visible(allowed: &Option<HashSet<String>>, library_id: &str) -> bool {
    match allowed {
        None => true,
        Some(ids) => ids.contains(library_id),
    }
}

fn opds_restrictions(headers: &HeaderMap) -> Option<OpdsRestrictions> {
    let user = resolved_auth_user(headers)?;
    let payload = user_payload_json(&user);

    let age = payload
        .get("ageRestriction")
        .and_then(|value| value.get("age"))
        .and_then(Value::as_u64)
        .and_then(|value| u16::try_from(value).ok());
    let age_restriction = payload
        .get("ageRestriction")
        .and_then(|value| value.get("restriction"))
        .and_then(Value::as_str)
        .and_then(|value| match value.trim().to_ascii_uppercase().as_str() {
            "ALLOW_ONLY" => Some(AgeRestrictionKind::AllowOnly),
            "EXCLUDE" => Some(AgeRestrictionKind::Exclude),
            _ => None,
        });
    let labels_allow = payload
        .get("labelsAllow")
        .and_then(Value::as_array)
        .map(|labels| {
            labels
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| value.to_ascii_lowercase())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let labels_exclude = payload
        .get("labelsExclude")
        .and_then(Value::as_array)
        .map(|labels| {
            labels
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| value.to_ascii_lowercase())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if age.is_none()
        && age_restriction.is_none()
        && labels_allow.is_empty()
        && labels_exclude.is_empty()
    {
        None
    } else {
        Some(OpdsRestrictions {
            age,
            age_restriction,
            labels_allow,
            labels_exclude,
        })
    }
}

fn normalized_sharing_labels(labels: &[String]) -> Vec<String> {
    labels
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect()
}

fn content_allowed_by_restrictions(
    restrictions: Option<&OpdsRestrictions>,
    age_rating: Option<u16>,
    sharing_labels: &[String],
) -> bool {
    let Some(restrictions) = restrictions else {
        return true;
    };

    let labels = normalized_sharing_labels(sharing_labels);

    let age_allowed = if restrictions.age_restriction == Some(AgeRestrictionKind::AllowOnly) {
        restrictions
            .age
            .map(|age_limit| age_rating.is_some_and(|age| age <= age_limit))
    } else {
        None
    };
    let label_allowed = if restrictions.labels_allow.is_empty() {
        None
    } else {
        Some(
            restrictions
                .labels_allow
                .iter()
                .any(|candidate| labels.contains(candidate)),
        )
    };

    let allowed = match (age_allowed, label_allowed) {
        (None, label_allowed) => label_allowed != Some(false),
        (age_allowed, None) => age_allowed != Some(false),
        (age_allowed, label_allowed) => age_allowed != Some(false) || label_allowed != Some(false),
    };
    if !allowed {
        return false;
    }

    let age_denied = if restrictions.age_restriction == Some(AgeRestrictionKind::Exclude) {
        restrictions
            .age
            .is_some_and(|age_limit| age_rating.is_some_and(|age| age >= age_limit))
    } else {
        false
    };
    let label_denied = if restrictions.labels_exclude.is_empty() {
        false
    } else {
        restrictions
            .labels_exclude
            .iter()
            .any(|candidate| labels.contains(candidate))
    };

    !age_denied && !label_denied
}

async fn load_libraries(database_file: &Path) -> Result<Vec<PersistedLibrary>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1).await?;
    let rows = sqlx::query(
        "SELECT ID, NAME, COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED \
         FROM LIBRARY \
         ORDER BY NAME COLLATE NOCASE ASC, ID ASC",
    )
    .fetch_all(&pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| PersistedLibrary {
            id: row.get::<String, _>("ID"),
            name: row.get::<String, _>("NAME"),
            last_modified: row.get::<String, _>("LAST_MODIFIED"),
        })
        .collect())
}

async fn load_library(
    database_file: &Path,
    library_id: &str,
) -> Result<Option<PersistedLibrary>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1).await?;
    let row = sqlx::query(
        "SELECT ID, NAME, COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED \
         FROM LIBRARY \
         WHERE ID = ? \
         LIMIT 1",
    )
    .bind(library_id)
    .fetch_optional(&pool)
    .await?;

    Ok(row.map(|row| PersistedLibrary {
        id: row.get::<String, _>("ID"),
        name: row.get::<String, _>("NAME"),
        last_modified: row.get::<String, _>("LAST_MODIFIED"),
    }))
}

async fn load_readlists_for_library(
    database_file: &Path,
    library_id: &str,
) -> Result<Vec<PersistedReadlist>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1).await?;
    let rows = sqlx::query(
        "SELECT DISTINCT rl.ID, rl.NAME, \
                COALESCE(rl.LAST_MODIFIED_DATE, rl.CREATED_DATE, '') AS LAST_MODIFIED \
         FROM READLIST rl \
         JOIN READLIST_BOOK rb ON rb.READLIST_ID = rl.ID \
         JOIN BOOK b ON b.ID = rb.BOOK_ID \
         WHERE b.LIBRARY_ID = ? \
         ORDER BY rl.NAME COLLATE NOCASE ASC, rl.ID ASC",
    )
    .bind(library_id)
    .fetch_all(&pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| PersistedReadlist {
            id: row.get::<String, _>("ID"),
            name: row.get::<String, _>("NAME"),
            last_modified: row.get::<String, _>("LAST_MODIFIED"),
        })
        .collect())
}

async fn load_series(
    database_file: &Path,
    series_id: &str,
) -> Result<Option<PersistedSeries>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1).await?;
    let row = sqlx::query(
        "SELECT s.ID, s.LIBRARY_ID, COALESCE(sm.TITLE, s.NAME) AS TITLE, \
                COALESCE(s.LAST_MODIFIED_DATE, s.CREATED_DATE, '') AS LAST_MODIFIED \
         FROM SERIES s \
         LEFT \
         JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID \
         WHERE s.ID = ? \
         AND s.DELETED_DATE IS NULL \
         LIMIT 1",
    )
    .bind(series_id)
    .fetch_optional(&pool)
    .await?;

    Ok(row.map(|row| PersistedSeries {
        id: row.get::<String, _>("ID"),
        library_id: row.get::<String, _>("LIBRARY_ID"),
        title: row.get::<String, _>("TITLE"),
        last_modified: row.get::<String, _>("LAST_MODIFIED"),
    }))
}

async fn load_series_books(
    database_file: &Path,
    series_id: &str,
) -> Result<Vec<PersistedSeriesBook>, sqlx::Error> {
    load_series_books_paged(database_file, series_id, 0, i64::MAX).await
}

async fn load_series_books_paged(
    database_file: &Path,
    series_id: &str,
    offset: i64,
    limit: i64,
) -> Result<Vec<PersistedSeriesBook>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1).await?;
    let rows = sqlx::query(
        "SELECT b.ID, COALESCE(bm.TITLE, b.NAME) AS TITLE, b.NAME AS FILE_NAME, \
                COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE, \
                COALESCE(b.LAST_MODIFIED_DATE, b.CREATED_DATE, '') AS LAST_MODIFIED \
         FROM BOOK b \
         LEFT \
         JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID \
         LEFT \
         JOIN MEDIA m ON m.BOOK_ID = b.ID \
         WHERE b.SERIES_ID = ? \
         AND b.DELETED_DATE IS NULL \
         ORDER BY COALESCE(bm.NUMBER_SORT, CAST(b.NUMBER AS REAL), 0) ASC, b.ID ASC \
         LIMIT ? \
         OFFSET ?",
    )
    .bind(series_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| PersistedSeriesBook {
            id: row.get::<String, _>("ID"),
            title: row.get::<String, _>("TITLE"),
            file_name: row.get::<String, _>("FILE_NAME"),
            media_type: row.get::<String, _>("MEDIA_TYPE"),
            last_modified: row.get::<String, _>("LAST_MODIFIED"),
        })
        .collect())
}

async fn load_readlist(
    database_file: &Path,
    readlist_id: &str,
) -> Result<Option<PersistedReadlist>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1).await?;
    let row = sqlx::query(
        "SELECT ID, NAME, COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED \
         FROM READLIST \
         WHERE ID = ? \
         LIMIT 1",
    )
    .bind(readlist_id)
    .fetch_optional(&pool)
    .await?;

    Ok(row.map(|row| PersistedReadlist {
        id: row.get::<String, _>("ID"),
        name: row.get::<String, _>("NAME"),
        last_modified: row.get::<String, _>("LAST_MODIFIED"),
    }))
}

async fn load_readlist_books(
    database_file: &Path,
    readlist_id: &str,
) -> Result<Vec<PersistedReadlistBook>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1).await?;
    let rows = sqlx::query(
        "SELECT b.ID, b.LIBRARY_ID, COALESCE(bm.TITLE, b.NAME) AS TITLE, b.NAME AS FILE_NAME, \
                COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE, \
                COALESCE(sm.AGE_RATING, NULL) AS AGE_RATING, \
                COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS SHARING_LABELS, \
                COALESCE(b.LAST_MODIFIED_DATE, b.CREATED_DATE, '') AS LAST_MODIFIED \
         FROM READLIST_BOOK rb \
         JOIN BOOK b ON b.ID = rb.BOOK_ID \
         LEFT \
         JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID \
         LEFT \
         JOIN MEDIA m ON m.BOOK_ID = b.ID \
         LEFT \
         JOIN SERIES s ON s.ID = b.SERIES_ID \
         LEFT \
         JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID \
         LEFT \
         JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID \
         WHERE rb.READLIST_ID = ? \
         AND b.DELETED_DATE IS NULL \
         GROUP BY b.ID, b.LIBRARY_ID, TITLE, FILE_NAME, MEDIA_TYPE, AGE_RATING, LAST_MODIFIED \
         ORDER BY rb.NUMBER ASC",
    )
    .bind(readlist_id)
    .fetch_all(&pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| PersistedReadlistBook {
            id: row.get::<String, _>("ID"),
            title: row.get::<String, _>("TITLE"),
            file_name: row.get::<String, _>("FILE_NAME"),
            media_type: row.get::<String, _>("MEDIA_TYPE"),
            library_id: row.get::<String, _>("LIBRARY_ID"),
            age_rating: row
                .try_get::<i64, _>("AGE_RATING")
                .ok()
                .and_then(|value| u16::try_from(value).ok()),
            sharing_labels: row
                .get::<String, _>("SHARING_LABELS")
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect(),
            last_modified: row.get::<String, _>("LAST_MODIFIED"),
        })
        .collect())
}

async fn load_search_results(
    database_file: &Path,
    query: &str,
) -> Result<
    (
        Vec<PersistedSeriesSearchResult>,
        Vec<PersistedBookSearchResult>,
        Vec<PersistedCollectionSearchResult>,
        Vec<PersistedReadlistSearchResult>,
    ),
    sqlx::Error,
> {
    if !database_file.exists() {
        return Ok((vec![], vec![], vec![], vec![]));
    }

    let pool = connect_pool(database_file, 1).await?;
    let pattern = if query.is_empty() {
        "%".to_string()
    } else {
        format!("%{query}%")
    };

    let series_rows = sqlx::query(
        "SELECT s.ID, s.LIBRARY_ID, COALESCE(sm.TITLE, s.NAME) AS TITLE \
         FROM SERIES s \
         LEFT \
         JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID \
         WHERE s.DELETED_DATE IS NULL \
         AND LOWER(COALESCE(sm.TITLE, s.NAME)) LIKE LOWER(?) \
         ORDER BY TITLE COLLATE NOCASE ASC, s.ID ASC \
         LIMIT 20",
    )
    .bind(&pattern)
    .fetch_all(&pool)
    .await?;

    let book_rows = sqlx::query(
        "SELECT b.ID, b.LIBRARY_ID, COALESCE(bm.TITLE, b.NAME) AS TITLE \
         FROM BOOK b \
         LEFT \
         JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID \
         WHERE b.DELETED_DATE IS NULL \
         AND LOWER(COALESCE(bm.TITLE, b.NAME)) LIKE LOWER(?) \
         ORDER BY TITLE COLLATE NOCASE ASC, b.ID ASC \
         LIMIT 20",
    )
    .bind(&pattern)
    .fetch_all(&pool)
    .await?;

    let readlist_rows = sqlx::query(
        "SELECT ID, NAME \
         FROM READLIST \
         WHERE LOWER(NAME) LIKE LOWER(?) \
         ORDER BY NAME COLLATE NOCASE ASC, ID ASC \
         LIMIT 20",
    )
    .bind(&pattern)
    .fetch_all(&pool)
    .await?;

    let collection_rows = sqlx::query(
        "SELECT ID, NAME \
         FROM COLLECTION \
         WHERE LOWER(NAME) LIKE LOWER(?) \
         ORDER BY NAME COLLATE NOCASE ASC, ID ASC \
         LIMIT 20",
    )
    .bind(&pattern)
    .fetch_all(&pool)
    .await?;

    Ok((
        series_rows
            .into_iter()
            .map(|row| PersistedSeriesSearchResult {
                id: row.get::<String, _>("ID"),
                title: row.get::<String, _>("TITLE"),
                library_id: row.get::<String, _>("LIBRARY_ID"),
            })
            .collect(),
        book_rows
            .into_iter()
            .map(|row| PersistedBookSearchResult {
                id: row.get::<String, _>("ID"),
                title: row.get::<String, _>("TITLE"),
                library_id: row.get::<String, _>("LIBRARY_ID"),
            })
            .collect(),
        collection_rows
            .into_iter()
            .map(|row| PersistedCollectionSearchResult {
                id: row.get::<String, _>("ID"),
                name: row.get::<String, _>("NAME"),
            })
            .collect(),
        readlist_rows
            .into_iter()
            .map(|row| PersistedReadlistSearchResult {
                id: row.get::<String, _>("ID"),
                name: row.get::<String, _>("NAME"),
            })
            .collect(),
    ))
}

fn opds_v1_navigation_feed_response(
    headers: &HeaderMap,
    feed_id: &str,
    title: &str,
    self_path: &str,
    entries: Vec<(String, String, String)>,
    pagination: Option<(usize, bool)>,
) -> Response {
    let self_href = app_absolute_url(headers, self_path);
    let start_href = app_absolute_url(headers, "/opds/v1.2/catalog");
    let now = opds_now_timestamp();

    let mut body = String::new();
    body.push_str("<feed xmlns=\"http://www.w3.org/2005/Atom\">");
    body.push_str(format!("<id>{}</id>", xml_escape(feed_id)).as_str());
    body.push_str(format!("<title>{}</title>", xml_escape(title)).as_str());
    body.push_str(format!("<updated>{}</updated><author><name>Komga</name><uri>https://github.com/gotson/komga</uri></author>", xml_escape(&now)).as_str());
    body.push_str(format!("<link type=\"application/atom+xml;profile=opds-catalog;kind=navigation\" rel=\"self\" href=\"{}\"/>", xml_escape(&self_href)).as_str());
    body.push_str(format!("<link type=\"application/atom+xml;profile=opds-catalog;kind=navigation\" rel=\"start\" href=\"{}\"/>", xml_escape(&start_href)).as_str());
    if let Some((page, has_next)) = pagination {
        if page > 0 {
            let previous_href = app_absolute_url(
                headers,
                format!("{self_path}?page={}", page.saturating_sub(1)).as_str(),
            );
            body.push_str(format!("<link type=\"application/atom+xml;profile=opds-catalog;kind=navigation\" rel=\"previous\" href=\"{}\"/>", xml_escape(previous_href.as_str())).as_str());
        }
        if has_next {
            let next_href =
                app_absolute_url(headers, format!("{self_path}?page={}", page + 1).as_str());
            body.push_str(format!("<link type=\"application/atom+xml;profile=opds-catalog;kind=navigation\" rel=\"next\" href=\"{}\"/>", xml_escape(next_href.as_str())).as_str());
        }
    }
    for (entry_id, entry_title, href_path) in entries {
        let href = app_absolute_url(headers, href_path.as_str());
        body.push_str(
            format!(
                "<entry><title>{}</title><updated>{}</updated><id>{}</id><content></content><link type=\"application/atom+xml;profile=opds-catalog;kind=navigation\" rel=\"subsection\" href=\"{}\"/></entry>",
                xml_escape(&entry_title),
                xml_escape(&now),
                xml_escape(&entry_id),
                xml_escape(&href),
            )
            .as_str(),
        );
    }
    body.push_str("</feed>");

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/atom+xml"),
        )],
        body,
    )
        .into_response()
}

fn opds_v1_library_series_feed_response(
    headers: &HeaderMap,
    feed_id: &str,
    title: &str,
    self_path: &str,
    series_entries: Vec<PersistedSeries>,
    feed_updated: Option<&str>,
    page: usize,
    has_next: bool,
) -> Response {
    let self_href = app_absolute_url(headers, self_path);
    let start_href = app_absolute_url(headers, "/opds/v1.2/catalog");
    let now = opds_now_timestamp();
    let feed_updated = feed_updated
        .filter(|value| !value.is_empty())
        .unwrap_or(now.as_str())
        .to_string();

    let mut body = String::new();
    body.push_str("<feed xmlns=\"http://www.w3.org/2005/Atom\">");
    body.push_str(format!("<id>{}</id>", xml_escape(feed_id)).as_str());
    body.push_str(format!("<title>{}</title>", xml_escape(title)).as_str());
    body.push_str(format!("<updated>{}</updated><author><name>Komga</name><uri>https://github.com/gotson/komga</uri></author>", xml_escape(feed_updated.as_str())).as_str());
    body.push_str(format!("<link type=\"application/atom+xml;profile=opds-catalog;kind=navigation\" rel=\"self\" href=\"{}\"/>", xml_escape(&self_href)).as_str());
    body.push_str(format!("<link type=\"application/atom+xml;profile=opds-catalog;kind=navigation\" rel=\"start\" href=\"{}\"/>", xml_escape(&start_href)).as_str());
    if page > 0 {
        let previous_href = app_absolute_url(
            headers,
            format!(
                "/opds/v1.2/libraries/{feed_id}?page={}",
                page.saturating_sub(1)
            )
            .as_str(),
        );
        body.push_str(format!("<link type=\"application/atom+xml;profile=opds-catalog;kind=navigation\" rel=\"previous\" href=\"{}\"/>", xml_escape(previous_href.as_str())).as_str());
    }
    if has_next {
        let next_href = app_absolute_url(
            headers,
            format!("/opds/v1.2/libraries/{feed_id}?page={}", page + 1).as_str(),
        );
        body.push_str(format!("<link type=\"application/atom+xml;profile=opds-catalog;kind=navigation\" rel=\"next\" href=\"{}\"/>", xml_escape(next_href.as_str())).as_str());
    }
    for entry in series_entries {
        let entry_updated = if entry.last_modified.is_empty() {
            now.clone()
        } else {
            entry.last_modified.clone()
        };
        let href = app_absolute_url(headers, format!("/opds/v1.2/series/{}", entry.id).as_str());
        body.push_str(
            format!(
                "<entry><title>{}</title><updated>{}</updated><id>{}</id><content></content><link type=\"application/atom+xml;profile=opds-catalog;kind=navigation\" rel=\"subsection\" href=\"{}\"/></entry>",
                xml_escape(&entry.title),
                xml_escape(&entry_updated),
                xml_escape(&entry.id),
                xml_escape(&href),
            )
            .as_str(),
        );
    }
    body.push_str("</feed>");

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/atom+xml"),
        )],
        body,
    )
        .into_response()
}

fn opds_v1_acquisition_feed_response(
    headers: &HeaderMap,
    feed_id: &str,
    title: &str,
    self_path: &str,
    books: Vec<PersistedBookFeedItem>,
    feed_updated: Option<&str>,
    pagination: Option<(usize, bool)>,
) -> Response {
    let self_href = app_absolute_url(headers, self_path);
    let start_href = app_absolute_url(headers, "/opds/v1.2/catalog");
    let now = opds_now_timestamp();
    let feed_updated = feed_updated
        .filter(|value| !value.is_empty())
        .unwrap_or(now.as_str())
        .to_string();

    let mut body = String::new();
    body.push_str("<feed xmlns=\"http://www.w3.org/2005/Atom\">");
    body.push_str(format!("<id>{}</id>", xml_escape(feed_id)).as_str());
    body.push_str(format!("<title>{}</title>", xml_escape(title)).as_str());
    body.push_str(format!("<updated>{}</updated><author><name>Komga</name><uri>https://github.com/gotson/komga</uri></author>", xml_escape(feed_updated.as_str())).as_str());
    body.push_str(format!("<link type=\"application/atom+xml;profile=opds-catalog;kind=acquisition\" rel=\"self\" href=\"{}\"/>", xml_escape(&self_href)).as_str());
    body.push_str(format!("<link type=\"application/atom+xml;profile=opds-catalog;kind=navigation\" rel=\"start\" href=\"{}\"/>", xml_escape(&start_href)).as_str());
    if let Some((page, has_next)) = pagination {
        if page > 0 {
            let previous_href = app_absolute_url(
                headers,
                format!("{self_path}?page={}", page.saturating_sub(1)).as_str(),
            );
            body.push_str(format!("<link type=\"application/atom+xml;profile=opds-catalog;kind=acquisition\" rel=\"previous\" href=\"{}\"/>", xml_escape(previous_href.as_str())).as_str());
        }
        if has_next {
            let next_href =
                app_absolute_url(headers, format!("{self_path}?page={}", page + 1).as_str());
            body.push_str(format!("<link type=\"application/atom+xml;profile=opds-catalog;kind=acquisition\" rel=\"next\" href=\"{}\"/>", xml_escape(next_href.as_str())).as_str());
        }
    }

    for book in books {
        let entry_updated = if book.last_modified.is_empty() {
            now.clone()
        } else {
            book.last_modified.clone()
        };
        let book_href = app_absolute_url(
            headers,
            format!(
                "/opds/v1.2/books/{}/file/{}",
                book.id,
                query_escape(book.file_name.as_str()),
            )
            .as_str(),
        );
        let thumb_href = app_absolute_url(
            headers,
            format!("/opds/v1.2/books/{}/thumbnail/small", book.id).as_str(),
        );
        let image_href = app_absolute_url(
            headers,
            format!("/opds/v1.2/books/{}/thumbnail", book.id).as_str(),
        );
        body.push_str(
            format!(
                "<entry><title>{}</title><updated>{}</updated><id>{}</id><content></content><link type=\"{}\" rel=\"http://opds-spec.org/acquisition\" href=\"{}\"/><link type=\"image/jpeg\" rel=\"http://opds-spec.org/image/thumbnail\" href=\"{}\"/><link type=\"image/jpeg\" rel=\"http://opds-spec.org/image\" href=\"{}\"/></entry>",
                xml_escape(&book.title),
                xml_escape(&entry_updated),
                xml_escape(&book.id),
                xml_escape(&book.media_type),
                xml_escape(&book_href),
                xml_escape(&thumb_href),
                xml_escape(&image_href),
            )
            .as_str(),
        );
    }

    body.push_str("</feed>");

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/atom+xml"),
        )],
        body,
    )
        .into_response()
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn query_escape(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            b' ' => "%20".to_string(),
            _ => format!("%{:02X}", byte),
        })
        .collect::<String>()
}

fn query_value<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split('&').find_map(|pair| {
        let mut parts = pair.splitn(2, '=');
        let name = parts.next().unwrap_or_default();
        if name != key {
            return None;
        }
        Some(parts.next().unwrap_or_default())
    })
}

fn query_values(query: &str, key: &str) -> Vec<String> {
    query
        .split('&')
        .filter_map(|segment| {
            let mut parts = segment.splitn(2, '=');
            let name = parts.next().unwrap_or_default();
            if name != key {
                return None;
            }
            let value = parts.next().unwrap_or_default();
            if value.is_empty() {
                None
            } else {
                Some(percent_decode(value))
            }
        })
        .collect()
}

fn parse_page_size(query: &str) -> (usize, usize) {
    let page = query_value(query, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .clamp(1, 100);
    (page, size)
}

fn paginate_vec<T>(items: Vec<T>, page: usize, size: usize) -> (Vec<T>, bool) {
    let start = page.saturating_mul(size);
    let end = start.saturating_add(size);
    if start >= items.len() {
        return (Vec::new(), false);
    }
    let has_next = end < items.len();
    let page_items = items.into_iter().skip(start).take(size).collect::<Vec<_>>();
    (page_items, has_next)
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hi = bytes[index + 1] as char;
            let lo = bytes[index + 2] as char;
            let parsed = hi
                .to_digit(16)
                .and_then(|hi| lo.to_digit(16).map(|lo| ((hi << 4) | lo) as u8));
            if let Some(byte) = parsed {
                decoded.push(byte);
                index += 3;
                continue;
            }
        }
        if bytes[index] == b'+' {
            decoded.push(b' ');
        } else {
            decoded.push(bytes[index]);
        }
        index += 1;
    }
    String::from_utf8(decoded).unwrap_or_else(|_| value.to_string())
}

fn opds_now_timestamp() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "2000-01-01T00:00:00Z".to_string())
}

async fn load_publishers(
    database_file: &Path,
    allowed_library_ids: &Option<HashSet<String>>,
) -> Result<Vec<String>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1).await?;
    let rows = sqlx::query(
        "SELECT DISTINCT sm.PUBLISHER AS PUBLISHER, s.LIBRARY_ID AS LIBRARY_ID \
         FROM SERIES_METADATA sm \
         JOIN SERIES s ON s.ID = sm.SERIES_ID \
         WHERE sm.PUBLISHER IS NOT NULL \
         AND trim(sm.PUBLISHER) != '' \
         ORDER BY lower(sm.PUBLISHER), sm.PUBLISHER",
    )
    .fetch_all(&pool)
    .await?;

    let mut values = Vec::new();
    let mut seen = HashSet::new();
    for row in rows {
        let library_id = row.get::<String, _>("LIBRARY_ID");
        if !library_visible(allowed_library_ids, &library_id) {
            continue;
        }
        let publisher = row.get::<String, _>("PUBLISHER");
        if seen.insert(publisher.clone()) {
            values.push(publisher);
        }
    }

    Ok(values)
}

async fn has_visible_collections_for_scope(
    database_file: &Path,
    allowed_library_ids: &Option<HashSet<String>>,
    restrictions: Option<&OpdsRestrictions>,
    library_id: Option<&str>,
) -> bool {
    let collections = match load_collections(database_file, library_id).await {
        Ok(collections) => collections,
        Err(_) => return false,
    };
    for collection in collections {
        let books = match load_collection_books(database_file, &collection.id).await {
            Ok(books) => books,
            Err(_) => continue,
        };
        if books.iter().any(|book| {
            library_visible(allowed_library_ids, &book.library_id)
                && content_allowed_by_restrictions(
                    restrictions,
                    book.age_rating,
                    &book.sharing_labels,
                )
        }) {
            return true;
        }
    }
    false
}

async fn has_visible_readlists_for_scope(
    database_file: &Path,
    allowed_library_ids: &Option<HashSet<String>>,
    restrictions: Option<&OpdsRestrictions>,
    library_id: Option<&str>,
) -> bool {
    if let Some(id) = library_id {
        for readlist in load_readlists_for_library(database_file, id)
            .await
            .unwrap_or_default()
        {
            let books = match load_readlist_books(database_file, &readlist.id).await {
                Ok(books) => books,
                Err(_) => continue,
            };
            if books.iter().any(|book| {
                library_visible(allowed_library_ids, &book.library_id)
                    && content_allowed_by_restrictions(
                        restrictions,
                        book.age_rating,
                        &book.sharing_labels,
                    )
            }) {
                return true;
            }
        }
        return false;
    }

    for readlist in load_all_readlists(database_file).await.unwrap_or_default() {
        let books = match load_readlist_books(database_file, &readlist.id).await {
            Ok(books) => books,
            Err(_) => continue,
        };
        if books.iter().any(|book| {
            library_visible(allowed_library_ids, &book.library_id)
                && content_allowed_by_restrictions(
                    restrictions,
                    book.age_rating,
                    &book.sharing_labels,
                )
        }) {
            return true;
        }
    }
    false
}

async fn load_browse_series_navigation(
    headers: &HeaderMap,
    database_file: &Path,
    allowed_library_ids: &Option<HashSet<String>>,
    library_id: Option<&str>,
    publishers: &[String],
    page: usize,
    size: usize,
) -> Result<(Vec<Value>, usize), sqlx::Error> {
    if !database_file.exists() {
        return Ok((vec![], 0));
    }

    let pool = connect_pool(database_file, 1).await?;
    let mut authorized_library_ids = allowed_library_ids
        .as_ref()
        .map(|ids| ids.iter().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    authorized_library_ids.sort();

    if allowed_library_ids.is_some() && authorized_library_ids.is_empty() {
        return Ok((vec![], 0));
    }

    let mut clauses = vec!["s.DELETED_DATE IS NULL".to_string()];
    if library_id.is_some() {
        clauses.push("s.LIBRARY_ID = ?".to_string());
    }
    if !authorized_library_ids.is_empty() {
        let placeholders = (0..authorized_library_ids.len())
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        clauses.push(format!("s.LIBRARY_ID IN ({placeholders})"));
    }
    if !publishers.is_empty() {
        for _ in publishers {
            clauses.push("sm.PUBLISHER = ?".to_string());
        }
    }
    let where_clause = clauses.join(" AND ");

    let count_sql = format!(
        "SELECT COUNT(*) AS TOTAL \
         FROM SERIES s \
         LEFT \
         JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID \
         WHERE {where_clause}",
    );
    let mut count_query = sqlx::query(count_sql.as_str());
    if let Some(id) = library_id {
        count_query = count_query.bind(id);
    }
    for library in &authorized_library_ids {
        count_query = count_query.bind(library);
    }
    for publisher in publishers {
        count_query = count_query.bind(publisher);
    }
    let total = count_query
        .fetch_one(&pool)
        .await?
        .get::<i64, _>("TOTAL")
        .max(0) as usize;

    let rows_sql = format!(
        "SELECT s.ID, COALESCE(sm.TITLE, s.NAME) AS TITLE, s.LIBRARY_ID \
         FROM SERIES s \
         LEFT \
         JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID \
         WHERE {where_clause} \
         ORDER BY COALESCE(sm.TITLE_SORT, sm.TITLE, s.NAME) COLLATE NOCASE ASC, s.ID ASC \
         LIMIT ? \
         OFFSET ?",
    );
    let mut rows_query = sqlx::query(rows_sql.as_str());
    if let Some(id) = library_id {
        rows_query = rows_query.bind(id);
    }
    for library in &authorized_library_ids {
        rows_query = rows_query.bind(library);
    }
    for publisher in publishers {
        rows_query = rows_query.bind(publisher);
    }
    let rows = rows_query
        .bind(size as i64)
        .bind((page.saturating_mul(size)) as i64)
        .fetch_all(&pool)
        .await?;

    let navigation = rows
        .into_iter()
        .map(|row| {
            let id = row.get::<String, _>("ID");
            let title = row.get::<String, _>("TITLE");
            opds_navigation_link(
                headers,
                title.as_str(),
                format!("/opds/v2/series/{id}").as_str(),
            )
        })
        .collect::<Vec<_>>();

    Ok((navigation, total))
}

async fn load_browse_publisher_navigation(
    headers: &HeaderMap,
    database_file: &Path,
    allowed_library_ids: &Option<HashSet<String>>,
    library_id: Option<&str>,
) -> Result<Vec<Value>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1).await?;
    let rows = sqlx::query(
        "SELECT DISTINCT sm.PUBLISHER AS PUBLISHER, s.LIBRARY_ID AS LIBRARY_ID \
         FROM SERIES_METADATA sm \
         JOIN SERIES s ON s.ID = sm.SERIES_ID \
         WHERE sm.PUBLISHER IS NOT NULL \
         AND trim(sm.PUBLISHER) != '' \
         AND s.DELETED_DATE IS NULL \
         AND (? IS NULL \
         OR s.LIBRARY_ID = ?) \
         ORDER BY lower(sm.PUBLISHER), sm.PUBLISHER",
    )
    .bind(library_id)
    .bind(library_id)
    .fetch_all(&pool)
    .await?;

    let mut seen = HashSet::new();
    let mut navigation = Vec::new();
    let library_segment = library_id.map(|id| format!("/{id}")).unwrap_or_default();
    for row in rows {
        let library = row.get::<String, _>("LIBRARY_ID");
        if !library_visible(allowed_library_ids, &library) {
            continue;
        }
        let publisher = row.get::<String, _>("PUBLISHER");
        if !seen.insert(publisher.clone()) {
            continue;
        }
        let href = format!(
            "/opds/v2/libraries{library_segment}/browse?publisher={}",
            query_escape(publisher.as_str())
        );
        navigation.push(opds_navigation_link(
            headers,
            publisher.as_str(),
            href.as_str(),
        ));
    }
    Ok(navigation)
}

fn redirect_to_opds_v2(headers: HeaderMap, target_path: &str) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let location = app_absolute_url(&headers, target_path);
    (
        StatusCode::FOUND,
        [(
            header::LOCATION,
            HeaderValue::from_str(&location)
                .unwrap_or_else(|_| HeaderValue::from_static("/opds/v2/catalog")),
        )],
    )
        .into_response()
}

fn opds_subsection_navigation_link(headers: &HeaderMap, title: &str, path: &str) -> Value {
    json!({
        "title": title,
        "rel": "subsection",
        "href": app_absolute_url(headers, path),
        "type": "application/opds+json",
    })
}

fn opds_navigation_link(headers: &HeaderMap, title: &str, path: &str) -> Value {
    json!({
        "title": title,
        "href": app_absolute_url(headers, path),
        "type": "application/opds+json",
    })
}

fn opds_navigation_response(
    headers: &HeaderMap,
    title: &str,
    self_href: &str,
    navigation: Vec<Value>,
    _modified: Option<&str>,
) -> Response {
    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/opds+json"),
        )],
        Json(json!({
            "metadata": {
                "title": title,
            },
            "links": [
                {
                    "rel": "self",
                    "href": self_href,
                    "type": "application/opds+json",
                },
                {
                    "rel": "start",
                    "href": app_absolute_url(headers, "/opds/v2/catalog"),
                    "type": "application/opds+json",
                },
                {
                    "rel": "search",
                    "href": app_absolute_url(headers, "/opds/v2/search{?query}"),
                    "type": "application/opds+json",
                    "templated": true,
                }
            ],
            "navigation": navigation,
        })),
    )
        .into_response()
}

fn opds_publications_response(
    headers: &HeaderMap,
    title: &str,
    self_href: &str,
    publications: Vec<Value>,
) -> Response {
    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/opds+json"),
        )],
        Json(json!({
            "metadata": {
                "title": title,
            },
            "links": [
                {
                    "rel": "self",
                    "href": self_href,
                    "type": "application/opds+json",
                },
                {
                    "rel": "start",
                    "href": app_absolute_url(headers, "/opds/v2/catalog"),
                    "type": "application/opds+json",
                },
                {
                    "rel": "search",
                    "href": app_absolute_url(headers, "/opds/v2/search{?query}"),
                    "type": "application/opds+json",
                    "templated": true,
                }
            ],
            "publications": publications,
        })),
    )
        .into_response()
}

fn opds_publication_for_book(
    headers: &HeaderMap,
    book_id: &str,
    title: &str,
    media_type: &str,
) -> Value {
    json!({
        "metadata": {
            "title": title,
        },
        "links": [
            {
                "rel": "self",
                "href": app_absolute_url(headers, format!("/opds/v2/books/{book_id}/manifest").as_str()),
                "type": "application/opds-publication+json",
            },
            {
                "rel": "http://opds-spec.org/acquisition",
                "href": app_absolute_url(headers, format!("/opds/v2/books/{book_id}/file").as_str()),
                "type": media_type,
            },
            {
                "rel": "http://opds-spec.org/image/thumbnail",
                "href": app_absolute_url(headers, format!("/opds/v2/books/{book_id}/thumbnail/small").as_str()),
                "type": "image/jpeg",
            }
        ],
    })
}

async fn opds_v2_keep_reading_feed(
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

    let books = match load_keep_reading_books(database_file, &user_id, None).await {
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
        .filter(|book| library_visible(&allowed_library_ids, &book.library_id))
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

async fn opds_v2_on_deck_feed(
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
        .filter(|book| library_visible(&allowed_library_ids, &book.library_id))
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

async fn opds_v2_latest_books_feed(
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

    let books = match load_latest_books(database_file, None, 100).await {
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
        .filter(|book| library_visible(&allowed_library_ids, &book.library_id))
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

async fn opds_v2_latest_series_feed(
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
        None,
    )
}

async fn opds_v2_collections_feed(
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
        None,
    )
}

async fn load_keep_reading_books(
    database_file: &Path,
    user_id: &str,
    library_id: Option<&str>,
) -> Result<Vec<PersistedBookFeedItem>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1).await?;
    let rows = sqlx::query(
        "SELECT b.ID, b.LIBRARY_ID, COALESCE(bm.TITLE, b.NAME) AS TITLE, b.NAME AS FILE_NAME, \
                COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE, \
                COALESCE(b.LAST_MODIFIED_DATE, b.CREATED_DATE, '') AS LAST_MODIFIED \
         FROM READ_PROGRESS rp \
         JOIN BOOK b ON b.ID = rp.BOOK_ID \
         LEFT \
         JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID \
         LEFT \
         JOIN MEDIA m ON m.BOOK_ID = b.ID \
         WHERE rp.USER_ID = ? \
         AND rp.COMPLETED = 0 \
         AND b.DELETED_DATE IS NULL \
         AND (? IS NULL \
         OR b.LIBRARY_ID = ?) \
         ORDER BY rp.LAST_MODIFIED_DATE DESC, b.ID ASC",
    )
    .bind(user_id)
    .bind(library_id)
    .bind(library_id)
    .fetch_all(&pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| PersistedBookFeedItem {
            id: row.get::<String, _>("ID"),
            title: row.get::<String, _>("TITLE"),
            file_name: row.get::<String, _>("FILE_NAME"),
            media_type: row.get::<String, _>("MEDIA_TYPE"),
            library_id: row.get::<String, _>("LIBRARY_ID"),
            age_rating: None,
            sharing_labels: vec![],
            last_modified: row.get::<String, _>("LAST_MODIFIED"),
        })
        .collect())
}

async fn load_on_deck_books(
    database_file: &Path,
    user_id: &str,
    library_id: Option<&str>,
) -> Result<Vec<PersistedBookFeedItem>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1).await?;
    let rows = sqlx::query(
        "SELECT b.ID, b.LIBRARY_ID, b.SERIES_ID, COALESCE(bm.TITLE, b.NAME) AS TITLE, \
                b.NAME AS FILE_NAME, \
                COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE, \
                COALESCE(bm.NUMBER_SORT, CAST(b.NUMBER AS REAL), 0) AS ORDER_INDEX, \
                COALESCE(b.LAST_MODIFIED_DATE, b.CREATED_DATE, '') AS LAST_MODIFIED \
         FROM BOOK b \
         LEFT \
         JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID \
         LEFT \
         JOIN MEDIA m ON m.BOOK_ID = b.ID \
         WHERE b.DELETED_DATE IS NULL \
         AND (? IS NULL \
         OR b.LIBRARY_ID = ?) \
         AND b.SERIES_ID IN (SELECT DISTINCT b_done.SERIES_ID \
         FROM BOOK b_done \
         JOIN READ_PROGRESS rp_done ON rp_done.BOOK_ID = b_done.ID \
         WHERE rp_done.USER_ID = ? \
         AND rp_done.COMPLETED = 1) \
         AND b.SERIES_ID NOT IN (SELECT DISTINCT b_prog.SERIES_ID \
         FROM BOOK b_prog \
         JOIN READ_PROGRESS rp_prog ON rp_prog.BOOK_ID = b_prog.ID \
         WHERE rp_prog.USER_ID = ? \
         AND rp_prog.COMPLETED = 0) \
         AND NOT EXISTS (SELECT 1 \
         FROM READ_PROGRESS rp_seen \
         WHERE rp_seen.BOOK_ID = b.ID \
         AND rp_seen.USER_ID = ? \
         AND rp_seen.COMPLETED = 1) \
         ORDER BY b.SERIES_ID ASC, ORDER_INDEX ASC, b.ID ASC",
    )
    .bind(library_id)
    .bind(library_id)
    .bind(user_id)
    .bind(user_id)
    .bind(user_id)
    .fetch_all(&pool)
    .await?;

    let mut first_per_series = BTreeMap::<String, PersistedBookFeedItem>::new();
    for row in rows {
        let series_id = row.get::<String, _>("SERIES_ID");
        first_per_series
            .entry(series_id)
            .or_insert_with(|| PersistedBookFeedItem {
                id: row.get::<String, _>("ID"),
                title: row.get::<String, _>("TITLE"),
                file_name: row.get::<String, _>("FILE_NAME"),
                media_type: row.get::<String, _>("MEDIA_TYPE"),
                library_id: row.get::<String, _>("LIBRARY_ID"),
                age_rating: None,
                sharing_labels: vec![],
                last_modified: row.get::<String, _>("LAST_MODIFIED"),
            });
    }

    Ok(first_per_series.into_values().collect())
}

async fn load_latest_books(
    database_file: &Path,
    library_id: Option<&str>,
    limit: i64,
) -> Result<Vec<PersistedBookFeedItem>, sqlx::Error> {
    load_latest_books_paged(database_file, &None, library_id, 0, limit).await
}

async fn load_latest_books_paged(
    database_file: &Path,
    allowed_library_ids: &Option<HashSet<String>>,
    library_id: Option<&str>,
    offset: i64,
    limit: i64,
) -> Result<Vec<PersistedBookFeedItem>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1).await?;
    let mut authorized_library_ids = allowed_library_ids
        .as_ref()
        .map(|ids| ids.iter().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    authorized_library_ids.sort();
    if allowed_library_ids.is_some() && authorized_library_ids.is_empty() {
        return Ok(vec![]);
    }

    let mut clauses = vec!["b.DELETED_DATE IS NULL".to_string()];
    if library_id.is_some() {
        clauses.push("b.LIBRARY_ID = ?".to_string());
    }
    if !authorized_library_ids.is_empty() {
        let placeholders = (0..authorized_library_ids.len())
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        clauses.push(format!("b.LIBRARY_ID IN ({placeholders})"));
    }
    let where_clause = clauses.join(" AND ");
    let sql = format!(
        "SELECT b.ID, b.LIBRARY_ID, COALESCE(bm.TITLE, b.NAME) AS TITLE, b.NAME AS FILE_NAME, \
                COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE, \
                COALESCE(b.LAST_MODIFIED_DATE, b.CREATED_DATE, '') AS LAST_MODIFIED \
         FROM BOOK b \
         LEFT \
         JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID \
         LEFT \
         JOIN MEDIA m ON m.BOOK_ID = b.ID \
         WHERE {where_clause} \
         ORDER BY b.CREATED_DATE DESC, b.ID DESC \
         LIMIT ? \
         OFFSET ?",
    );
    let mut query = sqlx::query(sql.as_str());
    if let Some(id) = library_id {
        query = query.bind(id);
    }
    for id in &authorized_library_ids {
        query = query.bind(id);
    }
    let rows = query.bind(limit).bind(offset).fetch_all(&pool).await?;

    Ok(rows
        .into_iter()
        .map(|row| PersistedBookFeedItem {
            id: row.get::<String, _>("ID"),
            title: row.get::<String, _>("TITLE"),
            file_name: row.get::<String, _>("FILE_NAME"),
            media_type: row.get::<String, _>("MEDIA_TYPE"),
            library_id: row.get::<String, _>("LIBRARY_ID"),
            age_rating: None,
            sharing_labels: vec![],
            last_modified: row.get::<String, _>("LAST_MODIFIED"),
        })
        .collect())
}

async fn load_latest_series(
    database_file: &Path,
    library_id: Option<&str>,
    limit: i64,
) -> Result<Vec<PersistedSeries>, sqlx::Error> {
    load_latest_series_paged(database_file, &None, library_id, 0, limit).await
}

async fn load_latest_series_paged(
    database_file: &Path,
    allowed_library_ids: &Option<HashSet<String>>,
    library_id: Option<&str>,
    offset: i64,
    limit: i64,
) -> Result<Vec<PersistedSeries>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1).await?;
    let mut authorized_library_ids = allowed_library_ids
        .as_ref()
        .map(|ids| ids.iter().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    authorized_library_ids.sort();
    if allowed_library_ids.is_some() && authorized_library_ids.is_empty() {
        return Ok(vec![]);
    }

    let mut clauses = vec!["s.DELETED_DATE IS NULL".to_string()];
    if library_id.is_some() {
        clauses.push("s.LIBRARY_ID = ?".to_string());
    }
    if !authorized_library_ids.is_empty() {
        let placeholders = (0..authorized_library_ids.len())
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        clauses.push(format!("s.LIBRARY_ID IN ({placeholders})"));
    }
    let where_clause = clauses.join(" AND ");
    let sql = format!(
        "SELECT s.ID, s.LIBRARY_ID, COALESCE(sm.TITLE, s.NAME) AS TITLE, \
                COALESCE(s.LAST_MODIFIED_DATE, s.CREATED_DATE, '') AS LAST_MODIFIED \
         FROM SERIES s \
         LEFT \
         JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID \
         WHERE {where_clause} \
         ORDER BY s.LAST_MODIFIED_DATE DESC, s.ID DESC \
         LIMIT ? \
         OFFSET ?",
    );
    let mut query = sqlx::query(sql.as_str());
    if let Some(id) = library_id {
        query = query.bind(id);
    }
    for id in &authorized_library_ids {
        query = query.bind(id);
    }
    let rows = query.bind(limit).bind(offset).fetch_all(&pool).await?;

    Ok(rows
        .into_iter()
        .map(|row| PersistedSeries {
            id: row.get::<String, _>("ID"),
            library_id: row.get::<String, _>("LIBRARY_ID"),
            title: row.get::<String, _>("TITLE"),
            last_modified: row.get::<String, _>("LAST_MODIFIED"),
        })
        .collect())
}

async fn load_library_series(
    database_file: &Path,
    library_id: &str,
    offset: i64,
    limit: i64,
) -> Result<Vec<PersistedSeries>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1).await?;
    let rows = sqlx::query(
        "SELECT s.ID, s.LIBRARY_ID, COALESCE(sm.TITLE, s.NAME) AS TITLE, \
                COALESCE(s.LAST_MODIFIED_DATE, s.CREATED_DATE, '') AS LAST_MODIFIED \
         FROM SERIES s \
         LEFT \
         JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID \
         WHERE s.DELETED_DATE IS NULL \
         AND s.LIBRARY_ID = ? \
         ORDER BY TITLE COLLATE NOCASE ASC, s.ID ASC \
         LIMIT ? \
         OFFSET ?",
    )
    .bind(library_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| PersistedSeries {
            id: row.get::<String, _>("ID"),
            library_id: row.get::<String, _>("LIBRARY_ID"),
            title: row.get::<String, _>("TITLE"),
            last_modified: row.get::<String, _>("LAST_MODIFIED"),
        })
        .collect())
}

async fn load_collections(
    database_file: &Path,
    library_id: Option<&str>,
) -> Result<Vec<PersistedCollection>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1).await?;
    let rows = if let Some(library_id) = library_id {
        sqlx::query(
            "SELECT DISTINCT c.ID, c.NAME, \
                    COALESCE(c.LAST_MODIFIED_DATE, c.CREATED_DATE, '') AS LAST_MODIFIED \
             FROM COLLECTION c \
             JOIN COLLECTION_SERIES cs ON cs.COLLECTION_ID = c.ID \
             JOIN SERIES s ON s.ID = cs.SERIES_ID \
             WHERE s.LIBRARY_ID = ? \
             ORDER BY c.NAME COLLATE NOCASE ASC, c.ID ASC",
        )
        .bind(library_id)
        .fetch_all(&pool)
        .await?
    } else {
        sqlx::query(
            "SELECT ID, NAME, COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED \
             FROM COLLECTION \
             ORDER BY NAME COLLATE NOCASE ASC, ID ASC",
        )
        .fetch_all(&pool)
        .await?
    };

    Ok(rows
        .into_iter()
        .map(|row| PersistedCollection {
            id: row.get::<String, _>("ID"),
            name: row.get::<String, _>("NAME"),
        })
        .collect())
}

async fn load_collection(
    database_file: &Path,
    collection_id: &str,
) -> Result<Option<PersistedCollection>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1).await?;
    let row = sqlx::query(
        "SELECT ID, NAME, COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED \
         FROM COLLECTION \
         WHERE ID = ? \
         LIMIT 1",
    )
    .bind(collection_id)
    .fetch_optional(&pool)
    .await?;

    Ok(row.map(|row| PersistedCollection {
        id: row.get::<String, _>("ID"),
        name: row.get::<String, _>("NAME"),
    }))
}

async fn load_collection_books(
    database_file: &Path,
    collection_id: &str,
) -> Result<Vec<PersistedBookFeedItem>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1).await?;
    let rows = sqlx::query(
        "SELECT b.ID, b.LIBRARY_ID, COALESCE(bm.TITLE, b.NAME) AS TITLE, b.NAME AS FILE_NAME, \
                COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE, \
                COALESCE(sm.AGE_RATING, NULL) AS AGE_RATING, \
                COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS SHARING_LABELS, \
                COALESCE(b.LAST_MODIFIED_DATE, b.CREATED_DATE, '') AS LAST_MODIFIED \
         FROM COLLECTION_SERIES cs \
         JOIN BOOK b ON b.SERIES_ID = cs.SERIES_ID \
         LEFT \
         JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID \
         LEFT \
         JOIN MEDIA m ON m.BOOK_ID = b.ID \
         LEFT \
         JOIN SERIES s ON s.ID = b.SERIES_ID \
         LEFT \
         JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID \
         LEFT \
         JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID \
         WHERE cs.COLLECTION_ID = ? \
         AND b.DELETED_DATE IS NULL \
         GROUP BY b.ID, b.LIBRARY_ID, TITLE, FILE_NAME, MEDIA_TYPE, AGE_RATING, LAST_MODIFIED \
         ORDER BY cs.NUMBER ASC, COALESCE(bm.NUMBER_SORT, CAST(b.NUMBER AS REAL), 0) ASC, \
                  b.ID ASC",
    )
    .bind(collection_id)
    .fetch_all(&pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| PersistedBookFeedItem {
            id: row.get::<String, _>("ID"),
            title: row.get::<String, _>("TITLE"),
            file_name: row.get::<String, _>("FILE_NAME"),
            media_type: row.get::<String, _>("MEDIA_TYPE"),
            library_id: row.get::<String, _>("LIBRARY_ID"),
            age_rating: row
                .try_get::<i64, _>("AGE_RATING")
                .ok()
                .and_then(|value| u16::try_from(value).ok()),
            sharing_labels: row
                .get::<String, _>("SHARING_LABELS")
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect(),
            last_modified: row.get::<String, _>("LAST_MODIFIED"),
        })
        .collect())
}

async fn load_collection_series(
    database_file: &Path,
    collection_id: &str,
) -> Result<Vec<PersistedSeries>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1).await?;
    let rows = sqlx::query(
        "SELECT s.ID, s.LIBRARY_ID, COALESCE(sm.TITLE, s.NAME) AS TITLE, \
                COALESCE(s.LAST_MODIFIED_DATE, s.CREATED_DATE, '') AS LAST_MODIFIED \
         FROM COLLECTION_SERIES cs \
         JOIN SERIES s ON s.ID = cs.SERIES_ID \
         LEFT \
         JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID \
         WHERE cs.COLLECTION_ID = ? \
         AND s.DELETED_DATE IS NULL \
         ORDER BY cs.NUMBER ASC, COALESCE(sm.TITLE_SORT, sm.TITLE, s.NAME) COLLATE NOCASE ASC, \
                  s.ID ASC",
    )
    .bind(collection_id)
    .fetch_all(&pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| PersistedSeries {
            id: row.get::<String, _>("ID"),
            library_id: row.get::<String, _>("LIBRARY_ID"),
            title: row.get::<String, _>("TITLE"),
            last_modified: row.get::<String, _>("LAST_MODIFIED"),
        })
        .collect())
}

async fn load_series_page(
    database_file: &Path,
    allowed_library_ids: &Option<HashSet<String>>,
    search: Option<&str>,
    publishers: &[String],
    offset: i64,
    limit: i64,
) -> Result<Vec<PersistedSeries>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1).await?;
    let mut authorized_library_ids = allowed_library_ids
        .as_ref()
        .map(|ids| ids.iter().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    authorized_library_ids.sort();
    if allowed_library_ids.is_some() && authorized_library_ids.is_empty() {
        return Ok(vec![]);
    }

    let mut clauses = vec!["s.DELETED_DATE IS NULL".to_string()];
    if !authorized_library_ids.is_empty() {
        let placeholders = (0..authorized_library_ids.len())
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        clauses.push(format!("s.LIBRARY_ID IN ({placeholders})"));
    }
    if search.is_some() {
        clauses.push("lower(COALESCE(sm.TITLE, s.NAME)) LIKE ?".to_string());
    }
    if !publishers.is_empty() {
        let placeholders = (0..publishers.len())
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        clauses.push(format!("sm.PUBLISHER IN ({placeholders})"));
    }
    let where_clause = clauses.join(" AND ");
    let sql = format!(
        "SELECT s.ID, s.LIBRARY_ID, COALESCE(sm.TITLE, s.NAME) AS TITLE, \
                COALESCE(s.LAST_MODIFIED_DATE, s.CREATED_DATE, '') AS LAST_MODIFIED \
         FROM SERIES s \
         LEFT \
         JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID \
         WHERE {where_clause} \
         ORDER BY COALESCE(sm.TITLE_SORT, sm.TITLE, s.NAME) COLLATE NOCASE ASC, s.ID ASC \
         LIMIT ? \
         OFFSET ?",
    );
    let mut query = sqlx::query(sql.as_str());
    for id in &authorized_library_ids {
        query = query.bind(id);
    }
    if let Some(value) = search {
        query = query.bind(format!("%{}%", value.to_lowercase()));
    }
    for publisher in publishers {
        query = query.bind(publisher);
    }

    let rows = query.bind(limit).bind(offset).fetch_all(&pool).await?;
    Ok(rows
        .into_iter()
        .map(|row| PersistedSeries {
            id: row.get::<String, _>("ID"),
            library_id: row.get::<String, _>("LIBRARY_ID"),
            title: row.get::<String, _>("TITLE"),
            last_modified: row.get::<String, _>("LAST_MODIFIED"),
        })
        .collect())
}

async fn collection_empty_for_authorized_user(
    database_file: &Path,
    collection_id: &str,
    allowed_library_ids: &Option<HashSet<String>>,
) -> bool {
    let Ok(books) = load_collection_books(database_file, collection_id).await else {
        return false;
    };
    if books.is_empty() {
        return true;
    }
    books
        .into_iter()
        .any(|book| library_visible(allowed_library_ids, &book.library_id))
}

async fn load_all_readlists(database_file: &Path) -> Result<Vec<PersistedReadlist>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1).await?;
    let rows = sqlx::query(
        "SELECT ID, NAME, COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED \
         FROM READLIST \
         ORDER BY NAME COLLATE NOCASE ASC, ID ASC",
    )
    .fetch_all(&pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| PersistedReadlist {
            id: row.get::<String, _>("ID"),
            name: row.get::<String, _>("NAME"),
            last_modified: row.get::<String, _>("LAST_MODIFIED"),
        })
        .collect())
}

async fn validate_library_scope(
    database_file: &Path,
    allowed_library_ids: &Option<HashSet<String>>,
    library_id: Option<&str>,
) -> Option<Response> {
    let Some(library_id) = library_id else {
        return None;
    };

    let library = match load_library(database_file, library_id).await {
        Ok(library) => library,
        Err(error) => {
            return Some(
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": format!("load OPDS library scope: {error}") })),
                )
                    .into_response(),
            );
        }
    };

    if library.is_none() {
        return Some(StatusCode::NOT_FOUND.into_response());
    }
    if !library_visible(allowed_library_ids, library_id) {
        return Some(StatusCode::FORBIDDEN.into_response());
    }

    None
}
