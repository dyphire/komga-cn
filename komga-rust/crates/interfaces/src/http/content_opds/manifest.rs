use std::path::Path;

use axum::Json;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use crate::http::identity_access::auth::require_auth;
use crate::http::request_urls::app_absolute_url;
use crate::media_assets_runtime_access::{
    load_archive_page_rows, load_persisted_book_media, load_persisted_book_pages,
};
use crate::opds_manifest_access;

pub(crate) async fn opds_manifest(
    headers: HeaderMap,
    database_file: &Path,
    book_id: &str,
) -> Response {
    opds_manifest_variant(headers, database_file, book_id, None).await
}

pub(crate) async fn opds_manifest_with_profile(
    headers: HeaderMap,
    database_file: &Path,
    book_id: &str,
    profile: &str,
) -> Response {
    opds_manifest_variant(headers, database_file, book_id, Some(profile)).await
}

async fn opds_manifest_variant(
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
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/opds-publication+json")],
                Json(manifest),
            )
                .into_response()
        }
        Ok(None) => {
            (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "error": format!(
                    "OPDS manifest requires persisted/domain-backed data; snapshot path removed for book {book_id}",
                    ),
                })),
            )
                .into_response()
        }
        Err(error) => {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load persisted OPDS manifest: {error}") })),
            )
                .into_response()
        }
    }
}

async fn persisted_opds_manifest(
    database_file: &Path,
    headers: &HeaderMap,
    book_id: &str,
    profile: Option<&str>,
) -> Result<Option<Value>, String> {
    let Some(row) = opds_manifest_access::load_manifest_book_record(database_file, book_id).await?
    else {
        return Ok(None);
    };

    let title = row.title;
    let file_name = row.file_name;
    let media_type = row
        .media_type
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| content_type_from_filename(&file_name, "application/octet-stream"));
    if !manifest_profile_matches_media(profile, &media_type) {
        return Ok(None);
    }
    let page_count = row.page_count.max(1) as usize;
    let page_media_types =
        if profile.unwrap_or_else(|| manifest_profile_name(&media_type)) == "divina" {
            Some(load_divina_page_media_types(database_file, book_id).await)
        } else {
            None
        };
    let manifest = persisted_manifest_payload(
        headers,
        book_id,
        &title,
        &media_type,
        page_count,
        profile,
        page_media_types.as_deref(),
    );

    Ok(Some(manifest))
}

async fn load_divina_page_media_types(database_file: &Path, book_id: &str) -> Vec<String> {
    let persisted = load_persisted_book_pages(database_file, book_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|page| {
            if page.media_type.is_empty() {
                content_type_from_filename(&page.file_name, "image/jpeg")
            } else {
                page.media_type
            }
        })
        .collect::<Vec<_>>();
    if !persisted.is_empty() {
        return persisted;
    }

    let Ok(Some(media)) = load_persisted_book_media(database_file, book_id).await else {
        return vec![];
    };

    let media_content_type = content_type_from_filename(&media.file_name, &media.media_type);
    if media_content_type.starts_with("image/") {
        return vec![media_content_type];
    }

    load_archive_page_rows(&media)
        .unwrap_or_default()
        .into_iter()
        .map(|page| {
            if page.media_type.is_empty() {
                content_type_from_filename(&page.file_name, "image/jpeg")
            } else {
                page.media_type
            }
        })
        .collect()
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
    page_media_types: Option<&[String]>,
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
        "pdf" => {
            for page in 1..=page_count {
                let href = app_absolute_url(
                    headers,
                    format!("/opds/v2/books/{book_id}/pages/{page}/raw").as_str(),
                );
                reading_order.push(json!({
                    "href": href,
                    "type": "application/pdf",
                }));
                page_list.push(json!({
                    "href": href,
                    "title": format!("Page {page}"),
                }));
            }
        }
        "divina" => {
            for page in 1..=page_count {
                let href = app_absolute_url(
                    headers,
                    format!("/opds/v2/books/{book_id}/pages/{page}?contentNegotiation=false")
                        .as_str(),
                );
                let page_media_type = page_media_types
                    .and_then(|types| types.get(page - 1))
                    .map(String::as_str)
                    .unwrap_or("image/jpeg");
                reading_order.push(json!({
                    "href": href,
                    "type": page_media_type,
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

fn content_type_from_filename(file_name: &str, default_mime_type: &str) -> String {
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
        _ => default_mime_type.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue, header};

    use super::persisted_manifest_payload;

    fn fixture_headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("komga.example"));
        headers
    }

    #[test]
    fn persisted_manifest_payload_uses_pdf_raw_pages_with_pdf_type() {
        let payload = persisted_manifest_payload(
            &fixture_headers(),
            "book-1",
            "Fixture PDF",
            "application/pdf",
            2,
            Some("pdf"),
            None,
        );

        let first_entry = &payload["readingOrder"][0];
        assert_eq!(
            first_entry["href"].as_str(),
            Some("http://komga.example/opds/v2/books/book-1/pages/1/raw")
        );
        assert_eq!(first_entry["type"].as_str(), Some("application/pdf"));
    }

    #[test]
    fn persisted_manifest_payload_uses_divina_page_route_without_raw_suffix() {
        let payload = persisted_manifest_payload(
            &fixture_headers(),
            "book-1",
            "Fixture Divina",
            "application/vnd.comicbook+zip",
            2,
            Some("divina"),
            Some(&["image/png".to_string(), "image/png".to_string()]),
        );

        let first_entry = &payload["readingOrder"][0];
        assert_eq!(
            first_entry["href"].as_str(),
            Some("http://komga.example/opds/v2/books/book-1/pages/1?contentNegotiation=false")
        );
        assert_eq!(first_entry["type"].as_str(), Some("image/png"));
    }
}
