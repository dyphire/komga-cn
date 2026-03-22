use axum::Json;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use reqwest::header::{AUTHORIZATION, COOKIE};
use serde_json::Value;

use crate::app::CompatProfile;
use crate::app::placeholder_auth::{
    require_auth, resolved_auth_user, user_is_admin, user_shared_all_libraries,
};
use crate::app::snapshots::{java_live_opds_manifest, opds_auth_json, request_host, snapshot_json};

pub(super) async fn opds_manifest(profile: CompatProfile, headers: HeaderMap) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if profile == CompatProfile::JavaLiveLocaldb {
        let manifest = fetch_java_live_manifest(&headers)
            .await
            .unwrap_or_else(|| java_live_opds_manifest(&headers));

        return (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/opds-publication+json")],
            Json(manifest),
        )
            .into_response();
    }

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

async fn fetch_java_live_manifest(headers: &HeaderMap) -> Option<Value> {
    let base_url = std::env::var("KOMGA_RUST_JAVA_LIVE_BASE_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())?;
    let normalized_base_url = base_url.trim_end_matches('/').to_string();
    let user = resolved_auth_user(headers)?;
    let client = reqwest::Client::builder().build().ok()?;
    let bootstrap = client
        .get(format!("{}/api/v2/users/me", normalized_base_url))
        .header(AUTHORIZATION, java_live_basic_auth_header(user))
        .header("X-Auth-Token", "")
        .send()
        .await
        .ok()?;

    if !bootstrap.status().is_success() {
        return None;
    }

    let manifest_request = client.get(format!(
        "{}/opds/v2/books/book-1/manifest",
        normalized_base_url
    ));
    let manifest = match extract_java_live_session_cookie(bootstrap.headers()) {
        Some(cookie) => manifest_request.header(COOKIE, cookie),
        None => manifest_request.header(
            "X-Auth-Token",
            extract_java_live_session_token(bootstrap.headers())?,
        ),
    }
    .send()
    .await
    .ok()?;

    if !manifest.status().is_success() {
        return None;
    }

    let mut manifest = manifest.json::<Value>().await.ok()?;
    rewrite_manifest_urls(
        &mut manifest,
        &normalized_base_url,
        &format!("http://{}", request_host(headers)),
    );

    Some(manifest)
}

fn java_live_basic_auth_header(
    user: crate::app::placeholder_auth::PlaceholderUser,
) -> &'static str {
    if user_is_admin(user) {
        "Basic YWRtaW5AZXhhbXBsZS5vcmc6YWRtaW4="
    } else if user_shared_all_libraries(user) {
        "Basic dXNlckBleGFtcGxlLm9yZzp1c2Vy"
    } else {
        "Basic bGltaXRlZEBleGFtcGxlLm9yZzpsaW1pdGVk"
    }
}

fn extract_java_live_session_cookie(headers: &reqwest::header::HeaderMap) -> Option<String> {
    headers
        .get_all(header::SET_COOKIE)
        .iter()
        .find_map(|value| {
            value.to_str().ok().and_then(|cookie| {
                cookie
                    .split(';')
                    .map(str::trim)
                    .find(|part| part.starts_with("KOMGA-SESSION="))
                    .map(str::to_string)
            })
        })
}

fn extract_java_live_session_token(headers: &reqwest::header::HeaderMap) -> Option<String> {
    headers
        .get("X-Auth-Token")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn rewrite_manifest_urls(value: &mut Value, from: &str, to: &str) {
    match value {
        Value::String(url) => {
            if let Some(suffix) = url.strip_prefix(from) {
                *url = format!("{to}{suffix}");
            }
        }
        Value::Array(items) => {
            for item in items {
                rewrite_manifest_urls(item, from, to);
            }
        }
        Value::Object(map) => {
            for item in map.values_mut() {
                rewrite_manifest_urls(item, from, to);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
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
