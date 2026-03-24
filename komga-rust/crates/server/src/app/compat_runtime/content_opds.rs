use axum::Json;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use komga_persistence::sqlite::connect_pool;
use serde_json::json;
use serde_json::Value;
use sqlx::Row;
use std::path::Path;

use crate::app::CompatProfile;
use crate::app::placeholder_auth::require_auth;
use crate::app::snapshots::{opds_auth_json, request_host, snapshot_json};

pub(super) async fn opds_manifest(
    profile: CompatProfile,
    headers: HeaderMap,
    database_file: &Path,
    book_id: &str,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if let Ok(Some(manifest)) = persisted_opds_manifest(database_file, &headers, book_id).await {
        return (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/opds-publication+json")],
            Json(manifest),
        )
            .into_response();
    }

    let _ = profile;

    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/opds-publication+json")],
        Json(snapshot_json(
            "opds-v2-manifest.json",
            CompatProfile::SnapshotAligned,
        )),
    )
        .into_response()
}

async fn persisted_opds_manifest(
    database_file: &Path,
    headers: &HeaderMap,
    book_id: &str,
) -> Result<Option<Value>, sqlx::Error> {
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
    let host = request_host(headers);
    let manifest = persisted_manifest_payload(&host, book_id, &title, &media_type);

    Ok(Some(manifest))
}

fn persisted_manifest_payload(host: &str, book_id: &str, title: &str, media_type: &str) -> Value {
    json!({
      "context": "https://readium.org/webpub-manifest/context.jsonld",
      "metadata": {
        "title": title,
      },
      "links": [
        {
          "rel": "self",
          "href": format!("http://{host}/opds/v2/books/{book_id}/manifest"),
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

pub(super) async fn opds_v1_series(profile: CompatProfile, headers: HeaderMap) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/atom+xml"),
        )],
        opds_v1_series_xml(profile, &headers),
    )
        .into_response()
}

fn opds_v1_series_xml(profile: CompatProfile, headers: &HeaderMap) -> String {
    let host = request_host(headers);
    let self_href = format!("http://{host}/opds/v1.2/series");
    let start_href = format!("http://{host}/opds/v1.2/catalog");
    let entry = if profile == CompatProfile::JavaLiveLocaldb {
        format!(
            "<entry><title>series</title><updated>2026-01-01T00:00:00Z</updated><id>series-1</id><content></content><link type=\"application/atom+xml;profile=opds-catalog;kind=navigation\" rel=\"subsection\" href=\"http://{host}/opds/v1.2/series/series-1\"/></entry>"
        )
    } else {
        String::new()
    };

    format!(
        "<feed xmlns=\"http://www.w3.org/2005/Atom\"><id>allSeries</id><title>All series</title><updated>2026-01-01T00:00:00Z</updated><author><name>Komga</name><uri>https://github.com/gotson/komga</uri></author><link type=\"application/atom+xml;profile=opds-catalog;kind=navigation\" rel=\"self\" href=\"{self_href}\"/><link type=\"application/atom+xml;profile=opds-catalog;kind=navigation\" rel=\"start\" href=\"{start_href}\"/>{entry}</feed>"
    )
}
