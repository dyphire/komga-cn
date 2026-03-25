use axum::Json;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use komga_persistence::sqlite::connect_pool;
use serde_json::Value;
use serde_json::json;
use sqlx::Row;
use std::collections::HashSet;
use std::path::Path;

use crate::app::CompatProfile;
use crate::app::placeholder_auth::{
    require_auth, resolved_auth_user, user_shared_all_libraries, user_shared_library_ids,
};
use crate::app::snapshots::{opds_auth_json, request_host};

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
        "SELECT COALESCE(bm.TITLE, b.NAME) AS TITLE, b.NAME AS NAME, m.MEDIA_TYPE AS MEDIA_TYPE FROM BOOK b LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID WHERE b.ID = ? LIMIT 1",
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await?;
    pool.close().await;

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
    let host = request_host(headers);
    let manifest = persisted_manifest_payload(&host, book_id, &title, &media_type, profile);

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
    host: &str,
    book_id: &str,
    title: &str,
    media_type: &str,
    profile: Option<&str>,
) -> Value {
    let self_href = if let Some(profile) = profile {
        format!("http://{host}/opds/v2/books/{book_id}/manifest/{profile}")
    } else {
        format!("http://{host}/opds/v2/books/{book_id}/manifest")
    };

    json!({
      "context": "https://readium.org/webpub-manifest/context.jsonld",
      "metadata": {
        "title": title,
      },
      "links": [
        {
          "rel": "self",
          "href": self_href,
          "type": "application/webpub+json",
          "properties": {
            "authenticate": {
              "href": format!("http://{host}/opds/v2/auth"),
              "type": "application/opds-authentication+json",
            },
          },
        },
        {
          "rel": "http://opds-spec.org/acquisition",
          "href": format!("http://{host}/opds/v2/books/{book_id}/file"),
          "type": media_type,
          "properties": {
            "authenticate": {
              "href": format!("http://{host}/opds/v2/auth"),
              "type": "application/opds-authentication+json",
            },
          },
        },
        {
          "rel": "http://www.cantook.com/api/progression",
          "href": format!("http://{host}/opds/v2/books/{book_id}/progression"),
          "type": "application/vnd.readium.progression+json",
          "properties": {
            "authenticate": {
              "href": format!("http://{host}/opds/v2/auth"),
              "type": "application/opds-authentication+json",
            },
          },
        }
      ],
      "images": [],
      "readingOrder": [
        {
          "href": format!("http://{host}/opds/v2/books/{book_id}/pages/1?contentNegotiation=false"),
          "type": media_type,
        }
      ],
      "resources": [
        {
          "href": format!("http://{host}/opds/v2/books/{book_id}/thumbnail"),
          "type": "image/jpeg",
          "properties": {
            "authenticate": {
              "href": format!("http://{host}/opds/v2/auth"),
              "type": "application/opds-authentication+json",
            },
          },
        }
      ],
      "toc": [],
      "landmarks": [],
      "pageList": [],
    })
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

pub(super) async fn opds_catalog(headers: HeaderMap) -> Response {
    let host = request_host(&headers);
    let auth_href = format!("http://{host}/opds/v2/auth");
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

pub(super) async fn opds_v1_series(
    profile: CompatProfile,
    headers: HeaderMap,
    database_file: &Path,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let _ = profile;

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/atom+xml"),
        )],
        opds_v1_series_xml(&headers, database_file).await,
    )
        .into_response()
}

pub(super) async fn opds_v2_libraries(headers: HeaderMap, database_file: &Path) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let host = request_host(&headers);
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

    let navigation = libraries
        .into_iter()
        .filter(|library| library_visible(&allowed_library_ids, &library.id))
        .map(|library| {
            json!({
                "title": library.name,
                "href": format!("http://{host}/opds/v2/libraries/{}", library.id),
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
                "title": "Libraries",
            },
            "links": [
                {
                    "rel": "self",
                    "href": format!("http://{host}/opds/v2/libraries"),
                    "type": "application/opds+json",
                },
                {
                    "rel": "start",
                    "href": format!("http://{host}/opds/v2/catalog"),
                    "type": "application/opds+json",
                },
                {
                    "rel": "search",
                    "href": format!("http://{host}/opds/v2/search{{?query}}"),
                    "type": "application/opds+json",
                    "templated": true,
                }
            ],
            "navigation": navigation,
        })),
    )
        .into_response()
}

pub(super) async fn opds_v2_library(
    headers: HeaderMap,
    database_file: &Path,
    library_id: &str,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let host = request_host(&headers);
    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let Some(library) = (match load_library(database_file, library_id).await {
        Ok(library) => library,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS library: {error}") })),
            )
                .into_response();
        }
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    if !library_visible(&allowed_library_ids, &library.id) {
        return StatusCode::FORBIDDEN.into_response();
    }

    let navigation = vec![
        json!({
            "title": "Browse",
            "href": format!("http://{host}/opds/v2/libraries/{}/browse", library.id),
            "type": "application/opds+json",
        }),
        json!({
            "title": "Read lists",
            "href": format!("http://{host}/opds/v2/libraries/{}/readlists", library.id),
            "type": "application/opds+json",
        }),
    ];

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/opds+json"),
        )],
        Json(json!({
            "metadata": {
                "title": library.name,
            },
            "links": [
                {
                    "rel": "self",
                    "href": format!("http://{host}/opds/v2/libraries/{}", library.id),
                    "type": "application/opds+json",
                },
                {
                    "rel": "start",
                    "href": format!("http://{host}/opds/v2/catalog"),
                    "type": "application/opds+json",
                }
            ],
            "navigation": navigation,
        })),
    )
        .into_response()
}

pub(super) async fn opds_v2_library_readlists(
    headers: HeaderMap,
    database_file: &Path,
    library_id: &str,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let host = request_host(&headers);
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
                "href": format!("http://{host}/opds/v2/readlists/{}", readlist.id),
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
                    "href": format!("http://{host}/opds/v2/libraries/{library_id}/readlists"),
                    "type": "application/opds+json",
                },
                {
                    "rel": "start",
                    "href": format!("http://{host}/opds/v2/catalog"),
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

    let host = request_host(&headers);
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
                        "href": format!("http://{host}/opds/v2/books/{}/manifest", book.id),
                        "type": "application/opds-publication+json",
                    },
                    {
                        "rel": "http://opds-spec.org/acquisition",
                        "href": format!("http://{host}/opds/v2/books/{}/file", book.id),
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
                    "href": format!("http://{host}/opds/v2/series/{}", series.id),
                    "type": "application/opds+json",
                },
                {
                    "rel": "start",
                    "href": format!("http://{host}/opds/v2/catalog"),
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

    let host = request_host(&headers);
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

    if books
        .iter()
        .any(|book| !library_visible(&allowed_library_ids, &book.library_id))
    {
        return StatusCode::FORBIDDEN.into_response();
    }

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
                        "href": format!("http://{host}/opds/v2/books/{}/manifest", book.id),
                        "type": "application/opds-publication+json",
                    },
                    {
                        "rel": "http://opds-spec.org/acquisition",
                        "href": format!("http://{host}/opds/v2/books/{}/file", book.id),
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
                    "href": format!("http://{host}/opds/v2/readlists/{}", readlist.id),
                    "type": "application/opds+json",
                },
                {
                    "rel": "start",
                    "href": format!("http://{host}/opds/v2/catalog"),
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

    let host = request_host(&headers);
    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let search_query = query.unwrap_or_default().trim();

    let (series, books, readlists) = match load_search_results(database_file, search_query).await {
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
                "href": format!("http://{host}/opds/v2/series/{}", item.id),
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
                        "href": format!("http://{host}/opds/v2/books/{}/manifest", item.id),
                        "type": "application/opds-publication+json",
                    }
                ],
            })
        })
        .collect::<Vec<_>>();

    let readlist_navigation = readlists
        .into_iter()
        .map(|item| {
            json!({
                "title": item.name,
                "href": format!("http://{host}/opds/v2/readlists/{}", item.id),
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
                "title": "Search results",
            },
            "links": [
                {
                    "rel": "start",
                    "href": format!("http://{host}/opds/v2/catalog"),
                    "type": "application/opds+json",
                },
                {
                    "rel": "search",
                    "href": format!("http://{host}/opds/v2/search{{?query}}"),
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
                        "title": "Read Lists",
                    },
                    "navigation": readlist_navigation,
                }
            ],
        })),
    )
        .into_response()
}

struct PersistedLibrary {
    id: String,
    name: String,
}

struct PersistedSeries {
    id: String,
    library_id: String,
    title: String,
}

struct PersistedSeriesBook {
    id: String,
    title: String,
    media_type: String,
}

struct PersistedReadlist {
    id: String,
    name: String,
}

struct PersistedReadlistBook {
    id: String,
    title: String,
    media_type: String,
    library_id: String,
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

async fn load_libraries(database_file: &Path) -> Result<Vec<PersistedLibrary>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1).await?;
    let rows = sqlx::query("SELECT ID, NAME FROM LIBRARY ORDER BY NAME COLLATE NOCASE ASC, ID ASC")
        .fetch_all(&pool)
        .await?;
    pool.close().await;

    Ok(rows
        .into_iter()
        .map(|row| PersistedLibrary {
            id: row.get::<String, _>("ID"),
            name: row.get::<String, _>("NAME"),
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
    let row = sqlx::query("SELECT ID, NAME FROM LIBRARY WHERE ID = ? LIMIT 1")
        .bind(library_id)
        .fetch_optional(&pool)
        .await?;
    pool.close().await;

    Ok(row.map(|row| PersistedLibrary {
        id: row.get::<String, _>("ID"),
        name: row.get::<String, _>("NAME"),
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
        "SELECT DISTINCT rl.ID, rl.NAME FROM READLIST rl JOIN READLIST_BOOK rb ON rb.READLIST_ID = rl.ID JOIN BOOK b ON b.ID = rb.BOOK_ID WHERE b.LIBRARY_ID = ? ORDER BY rl.NAME COLLATE NOCASE ASC, rl.ID ASC",
    )
    .bind(library_id)
    .fetch_all(&pool)
    .await?;
    pool.close().await;

    Ok(rows
        .into_iter()
        .map(|row| PersistedReadlist {
            id: row.get::<String, _>("ID"),
            name: row.get::<String, _>("NAME"),
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
        "SELECT s.ID, s.LIBRARY_ID, COALESCE(sm.TITLE, s.NAME) AS TITLE FROM SERIES s LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID WHERE s.ID = ? AND s.DELETED_DATE IS NULL LIMIT 1",
    )
    .bind(series_id)
    .fetch_optional(&pool)
    .await?;
    pool.close().await;

    Ok(row.map(|row| PersistedSeries {
        id: row.get::<String, _>("ID"),
        library_id: row.get::<String, _>("LIBRARY_ID"),
        title: row.get::<String, _>("TITLE"),
    }))
}

async fn load_series_books(
    database_file: &Path,
    series_id: &str,
) -> Result<Vec<PersistedSeriesBook>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1).await?;
    let rows = sqlx::query(
        "SELECT b.ID, COALESCE(bm.TITLE, b.NAME) AS TITLE, COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE FROM BOOK b LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID WHERE b.SERIES_ID = ? AND b.DELETED_DATE IS NULL ORDER BY COALESCE(bm.NUMBER_SORT, CAST(b.NUMBER AS REAL), 0) ASC, b.ID ASC",
    )
    .bind(series_id)
    .fetch_all(&pool)
    .await?;
    pool.close().await;

    Ok(rows
        .into_iter()
        .map(|row| PersistedSeriesBook {
            id: row.get::<String, _>("ID"),
            title: row.get::<String, _>("TITLE"),
            media_type: row.get::<String, _>("MEDIA_TYPE"),
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
    let row = sqlx::query("SELECT ID, NAME FROM READLIST WHERE ID = ? LIMIT 1")
        .bind(readlist_id)
        .fetch_optional(&pool)
        .await?;
    pool.close().await;

    Ok(row.map(|row| PersistedReadlist {
        id: row.get::<String, _>("ID"),
        name: row.get::<String, _>("NAME"),
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
        "SELECT b.ID, b.LIBRARY_ID, COALESCE(bm.TITLE, b.NAME) AS TITLE, COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE FROM READLIST_BOOK rb JOIN BOOK b ON b.ID = rb.BOOK_ID LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID WHERE rb.READLIST_ID = ? AND b.DELETED_DATE IS NULL ORDER BY rb.NUMBER ASC",
    )
    .bind(readlist_id)
    .fetch_all(&pool)
    .await?;
    pool.close().await;

    Ok(rows
        .into_iter()
        .map(|row| PersistedReadlistBook {
            id: row.get::<String, _>("ID"),
            title: row.get::<String, _>("TITLE"),
            media_type: row.get::<String, _>("MEDIA_TYPE"),
            library_id: row.get::<String, _>("LIBRARY_ID"),
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
        Vec<PersistedReadlistSearchResult>,
    ),
    sqlx::Error,
> {
    if !database_file.exists() {
        return Ok((vec![], vec![], vec![]));
    }

    let pool = connect_pool(database_file, 1).await?;
    let pattern = if query.is_empty() {
        "%".to_string()
    } else {
        format!("%{query}%")
    };

    let series_rows = sqlx::query(
        "SELECT s.ID, s.LIBRARY_ID, COALESCE(sm.TITLE, s.NAME) AS TITLE FROM SERIES s LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID WHERE s.DELETED_DATE IS NULL AND LOWER(COALESCE(sm.TITLE, s.NAME)) LIKE LOWER(?) ORDER BY TITLE COLLATE NOCASE ASC, s.ID ASC LIMIT 20",
    )
    .bind(&pattern)
    .fetch_all(&pool)
    .await?;

    let book_rows = sqlx::query(
        "SELECT b.ID, b.LIBRARY_ID, COALESCE(bm.TITLE, b.NAME) AS TITLE FROM BOOK b LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID WHERE b.DELETED_DATE IS NULL AND LOWER(COALESCE(bm.TITLE, b.NAME)) LIKE LOWER(?) ORDER BY TITLE COLLATE NOCASE ASC, b.ID ASC LIMIT 20",
    )
    .bind(&pattern)
    .fetch_all(&pool)
    .await?;

    let readlist_rows = sqlx::query(
        "SELECT ID, NAME FROM READLIST WHERE LOWER(NAME) LIKE LOWER(?) ORDER BY NAME COLLATE NOCASE ASC, ID ASC LIMIT 20",
    )
    .bind(&pattern)
    .fetch_all(&pool)
    .await?;

    pool.close().await;

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
        readlist_rows
            .into_iter()
            .map(|row| PersistedReadlistSearchResult {
                id: row.get::<String, _>("ID"),
                name: row.get::<String, _>("NAME"),
            })
            .collect(),
    ))
}

async fn opds_v1_series_xml(headers: &HeaderMap, database_file: &Path) -> String {
    let host = request_host(headers);
    let self_href = format!("http://{host}/opds/v1.2/series");
    let start_href = format!("http://{host}/opds/v1.2/catalog");
    let Some(allowed_library_ids) = allowed_library_ids(headers) else {
        return format!(
            "<feed xmlns=\"http://www.w3.org/2005/Atom\"><id>allSeries</id><title>All series</title><updated>2026-01-01T00:00:00Z</updated><author><name>Komga</name><uri>https://github.com/gotson/komga</uri></author><link type=\"application/atom+xml;profile=opds-catalog;kind=navigation\" rel=\"self\" href=\"{self_href}\"/><link type=\"application/atom+xml;profile=opds-catalog;kind=navigation\" rel=\"start\" href=\"{start_href}\"/></feed>"
        );
    };

    let entries =
        match load_opds_v1_series_entries(headers, database_file, &allowed_library_ids).await {
            Ok(entries) => entries,
            Err(_) => String::new(),
        };

    format!(
        "<feed xmlns=\"http://www.w3.org/2005/Atom\"><id>allSeries</id><title>All series</title><updated>2026-01-01T00:00:00Z</updated><author><name>Komga</name><uri>https://github.com/gotson/komga</uri></author><link type=\"application/atom+xml;profile=opds-catalog;kind=navigation\" rel=\"self\" href=\"{self_href}\"/><link type=\"application/atom+xml;profile=opds-catalog;kind=navigation\" rel=\"start\" href=\"{start_href}\"/>{entries}</feed>"
    )
}

async fn load_opds_v1_series_entries(
    headers: &HeaderMap,
    database_file: &Path,
    allowed_library_ids: &Option<HashSet<String>>,
) -> Result<String, sqlx::Error> {
    if !database_file.exists() {
        return Ok(String::new());
    }

    let pool = connect_pool(database_file, 1).await?;
    let rows = sqlx::query(
        "SELECT s.ID, COALESCE(sm.TITLE, s.NAME) AS TITLE, s.LIBRARY_ID FROM SERIES s LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID WHERE s.DELETED_DATE IS NULL ORDER BY TITLE COLLATE NOCASE ASC, s.ID ASC",
    )
    .fetch_all(&pool)
    .await?;
    pool.close().await;

    let host = request_host(headers);
    let mut entries = String::new();
    for row in rows {
        let library_id = row.get::<String, _>("LIBRARY_ID");
        if !library_visible(allowed_library_ids, &library_id) {
            continue;
        }

        let series_id = row.get::<String, _>("ID");
        let title = row.get::<String, _>("TITLE");
        entries.push_str(
            format!(
                "<entry><title>{title}</title><updated>2026-01-01T00:00:00Z</updated><id>{series_id}</id><content></content><link type=\"application/atom+xml;profile=opds-catalog;kind=navigation\" rel=\"subsection\" href=\"http://{host}/opds/v1.2/series/{series_id}\"/></entry>"
            )
            .as_str(),
        );
    }

    Ok(entries)
}
