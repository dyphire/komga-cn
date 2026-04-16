use std::path::Path;

use axum::Json;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use crate::http::discovery::detail::load_persisted_book_series_id;
use crate::http::identity_access::auth::require_auth;
use crate::http::media_assets::manifest_persistence::build_persisted_book_manifest;
use crate::http::media_assets::types::{ManifestBuildOutcome, ManifestVariant};
use crate::http::request_urls::app_absolute_url;

const OPDS_MANIFEST_CONTENT_TYPE: &str = "application/opds-publication+json";
const OPDS_AUTH_CONTENT_TYPE: &str = "application/opds-authentication+json";
const PROGRESSION_REL: &str = "http://www.cantook.com/api/progression";
const PROGRESSION_CONTENT_TYPE: &str = "application/vnd.readium.progression+json";

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

    let Some(variant) = manifest_variant(profile) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    match build_persisted_book_manifest(database_file, &headers, book_id, variant).await {
        Ok(ManifestBuildOutcome::Found(_, mut payload)) => {
            let series_id = load_persisted_book_series_id(database_file, book_id)
                .await
                .ok()
                .flatten();
            adapt_manifest_payload_to_opds(&mut payload, &headers, book_id, series_id.as_deref());
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, OPDS_MANIFEST_CONTENT_TYPE)],
                Json(payload),
            )
                .into_response()
        }
        Ok(ManifestBuildOutcome::BadRequest(message)) => {
            (StatusCode::BAD_REQUEST, Json(json!({ "error": message }))).into_response()
        }
        Ok(ManifestBuildOutcome::NotFound) => StatusCode::NOT_FOUND.into_response(),
        Ok(ManifestBuildOutcome::Forbidden) => StatusCode::FORBIDDEN.into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("load persisted OPDS manifest: {error}") })),
        )
            .into_response(),
    }
}

fn manifest_variant(profile: Option<&str>) -> Option<ManifestVariant> {
    match profile {
        None => Some(ManifestVariant::Default),
        Some("epub") => Some(ManifestVariant::Epub),
        Some("pdf") => Some(ManifestVariant::Pdf),
        Some("divina") => Some(ManifestVariant::Divina),
        Some(_) => None,
    }
}

fn adapt_manifest_payload_to_opds(
    payload: &mut Value,
    headers: &HeaderMap,
    book_id: &str,
    series_id: Option<&str>,
) {
    rewrite_api_hrefs_to_opds(payload);
    add_series_links_to_belongs_to(payload, headers, series_id);
    add_auth_properties_to_manifest_links(payload, headers);
    add_auth_properties_to_thumbnail_resources(payload, headers);
    add_progression_link(payload, headers, book_id);
}

fn rewrite_api_hrefs_to_opds(value: &mut Value) {
    match value {
        Value::Array(entries) => {
            for entry in entries {
                rewrite_api_hrefs_to_opds(entry);
            }
        }
        Value::Object(entries) => {
            if let Some(Value::String(href)) = entries.get_mut("href") {
                *href = href.replacen("/api/v1/", "/opds/v2/", 1);
            }
            for entry in entries.values_mut() {
                rewrite_api_hrefs_to_opds(entry);
            }
        }
        _ => {}
    }
}

fn add_series_links_to_belongs_to(
    payload: &mut Value,
    headers: &HeaderMap,
    series_id: Option<&str>,
) {
    let Some(series_id) = series_id.filter(|value| !value.is_empty()) else {
        return;
    };
    let Some(series_entries) = payload
        .get_mut("metadata")
        .and_then(Value::as_object_mut)
        .and_then(|metadata| metadata.get_mut("belongsTo"))
        .and_then(Value::as_object_mut)
        .and_then(|belongs_to| belongs_to.get_mut("series"))
        .and_then(Value::as_array_mut)
    else {
        return;
    };

    for entry in series_entries {
        let Some(entry_object) = entry.as_object_mut() else {
            continue;
        };
        entry_object.insert(
            "links".to_string(),
            Value::Array(vec![json!({
                "href": app_absolute_url(headers, format!("/opds/v2/series/{series_id}").as_str()),
                "type": "application/opds+json",
            })]),
        );
    }
}

fn add_auth_properties_to_manifest_links(payload: &mut Value, headers: &HeaderMap) {
    let Some(links) = payload.get_mut("links").and_then(Value::as_array_mut) else {
        return;
    };
    for link in links {
        insert_auth_properties(link, headers);
    }
}

fn add_auth_properties_to_thumbnail_resources(payload: &mut Value, headers: &HeaderMap) {
    let Some(resources) = payload.get_mut("resources").and_then(Value::as_array_mut) else {
        return;
    };
    for resource in resources {
        let is_thumbnail = resource
            .get("href")
            .and_then(Value::as_str)
            .is_some_and(|href| href.ends_with("/thumbnail"));
        if is_thumbnail {
            insert_auth_properties(resource, headers);
        }
    }
}

fn insert_auth_properties(value: &mut Value, headers: &HeaderMap) {
    let Some(entry) = value.as_object_mut() else {
        return;
    };
    entry.insert(
        "properties".to_string(),
        json!({
            "authenticate": {
                "href": app_absolute_url(headers, "/opds/v2/auth"),
                "type": OPDS_AUTH_CONTENT_TYPE,
            }
        }),
    );
}

fn add_progression_link(payload: &mut Value, headers: &HeaderMap, book_id: &str) {
    let Some(links) = payload.get_mut("links").and_then(Value::as_array_mut) else {
        return;
    };
    if links
        .iter()
        .any(|link| link.get("rel").and_then(Value::as_str) == Some(PROGRESSION_REL))
    {
        return;
    }
    links.push(json!({
        "rel": PROGRESSION_REL,
        "href": app_absolute_url(headers, format!("/opds/v2/books/{book_id}/progression").as_str()),
        "type": PROGRESSION_CONTENT_TYPE,
        "properties": {
            "authenticate": {
                "href": app_absolute_url(headers, "/opds/v2/auth"),
                "type": OPDS_AUTH_CONTENT_TYPE,
            }
        }
    }));
}
