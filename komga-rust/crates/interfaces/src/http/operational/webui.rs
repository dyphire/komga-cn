use crate::http::request_urls::request_context_path;
use axum::body::Bytes;
use axum::extract::Extension;
use axum::extract::Path as AxumPath;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::json;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{LazyLock, RwLock};

use super::{super::OperationalState, webui_assets::WebUiAssets};

const RESOURCE_BASE_URL_TEMPLATE_MARKER: &str =
    concat!("/*[(${", "\"'\" + baseUrl + \"'\"", "})]*/ '/'",);
const INDEX_HTML_CACHE_MAX_ENTRIES: usize = 16;

static REWRITTEN_INDEX_HTML_CACHE: LazyLock<RwLock<IndexHtmlCache>> =
    LazyLock::new(|| RwLock::new(IndexHtmlCache::default()));

#[derive(Default)]
struct IndexHtmlCache {
    entries: HashMap<String, Bytes>,
    usage_order: Vec<String>,
}

impl IndexHtmlCache {
    fn get(&mut self, resource_base_url: &str) -> Option<Bytes> {
        let cached = self.entries.get(resource_base_url).cloned()?;
        self.touch(resource_base_url);
        Some(cached)
    }

    fn insert(&mut self, resource_base_url: String, value: Bytes) {
        if self.entries.contains_key(resource_base_url.as_str()) {
            self.entries.insert(resource_base_url.clone(), value);
            self.touch(resource_base_url.as_str());
            return;
        }

        if self.entries.len() >= INDEX_HTML_CACHE_MAX_ENTRIES
            && let Some(oldest) = self.usage_order.first().cloned()
        {
            self.entries.remove(oldest.as_str());
            self.usage_order.remove(0);
        }

        self.entries.insert(resource_base_url.clone(), value);
        self.usage_order.push(resource_base_url);
    }

    fn touch(&mut self, resource_base_url: &str) {
        if let Some(index) = self
            .usage_order
            .iter()
            .position(|existing| existing == resource_base_url)
        {
            let key = self.usage_order.remove(index);
            self.usage_order.push(key);
        }
    }
}

pub(crate) async fn webui_entrypoint(
    headers: HeaderMap,
    Extension(_state): Extension<OperationalState>,
) -> Response {
    let resource_base_url = request_scoped_resource_base_url(&headers);
    serve_webui_asset("", resource_base_url.as_str())
}

pub(crate) async fn webui_asset(
    AxumPath(webui_path): AxumPath<String>,
    headers: HeaderMap,
    Extension(_state): Extension<OperationalState>,
) -> Response {
    if is_runtime_owned_prefix(webui_path.as_str()) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let resource_base_url = request_scoped_resource_base_url(&headers);
    serve_webui_asset(webui_path.as_str(), resource_base_url.as_str())
}

fn serve_webui_asset(webui_path: &str, resource_base_url: &str) -> Response {
    let Some(asset_path) = resolve_embedded_asset_path(webui_path) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(asset_data) = WebUiAssets::get(asset_path.as_str()) else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            [(header::CONTENT_TYPE, "application/json")],
            axum::Json(json!({
                "message": format!("embedded webui asset missing: {asset_path}"),
            })),
        )
            .into_response();
    };

    let response_headers = [
        (
            header::CONTENT_TYPE,
            content_type_for(Path::new(asset_path.as_str())),
        ),
        (
            header::CACHE_CONTROL,
            cache_control_for(asset_path.as_str()).to_string(),
        ),
    ];

    if asset_path == "index.html" {
        return (
            response_headers,
            cached_rewritten_index_html(resource_base_url),
        )
            .into_response();
    }

    (response_headers, asset_data).into_response()
}

fn request_scoped_resource_base_url(headers: &HeaderMap) -> String {
    let prefix = request_context_path(headers);
    if prefix.is_empty() || !is_safe_resource_path_prefix(prefix.as_str()) {
        "/".to_string()
    } else {
        format!("{prefix}/")
    }
}

fn cached_rewritten_index_html(resource_base_url: &str) -> Bytes {
    if let Some(cached) = REWRITTEN_INDEX_HTML_CACHE
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(resource_base_url)
    {
        return cached;
    }

    let index_html = WebUiAssets::get("index.html").expect("embedded index.html should exist");
    let rewritten = Bytes::from(rewrite_index_html(index_html.as_ref(), resource_base_url));
    let mut cache = REWRITTEN_INDEX_HTML_CACHE
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(cached) = cache.get(resource_base_url) {
        return cached;
    }

    // Bound cache cardinality because X-Forwarded-Prefix is request-controlled; in practice
    // deployments use a tiny fixed set of prefixes, so a small cache keeps hot paths fast
    // without letting hostile prefixes grow memory unbounded.
    cache.insert(resource_base_url.to_string(), rewritten.clone());
    rewritten
}

fn rewrite_index_html(asset_data: &[u8], resource_base_url: &str) -> Vec<u8> {
    let html = String::from_utf8_lossy(asset_data).into_owned();
    let html = html.replace(
        RESOURCE_BASE_URL_TEMPLATE_MARKER,
        format!("'{resource_base_url}'").as_str(),
    );
    let html = rewrite_attribute_values(html, "src", resource_base_url);
    let html = rewrite_attribute_values(html, "href", resource_base_url);
    let html = rewrite_attribute_values(html, "content", resource_base_url);
    html.into_bytes()
}

fn rewrite_attribute_values(input: String, attribute: &str, resource_base_url: &str) -> String {
    let needle = format!(r#"{attribute}=""#);
    let mut rewritten = String::with_capacity(input.len());
    let mut remaining = input.as_str();

    while let Some(offset) = remaining.find(needle.as_str()) {
        let (before, after_attribute) = remaining.split_at(offset);
        rewritten.push_str(before);
        rewritten.push_str(needle.as_str());

        let value_start = &after_attribute[needle.len()..];
        let Some(value_end) = value_start.find('"') else {
            rewritten.push_str(value_start);
            return rewritten;
        };

        let (value, after_value) = value_start.split_at(value_end);
        if let Some(rewritten_value) = rewrite_embedded_asset_reference(value, resource_base_url) {
            rewritten.push_str(rewritten_value.as_str());
        } else {
            rewritten.push_str(value);
        }
        rewritten.push('"');
        remaining = &after_value[1..];
    }

    rewritten.push_str(remaining);
    rewritten
}

fn rewrite_embedded_asset_reference(value: &str, resource_base_url: &str) -> Option<String> {
    if value.is_empty()
        || value.starts_with('#')
        || value.starts_with("//")
        || value.contains("://")
        || value.starts_with("data:")
        || value.starts_with("mailto:")
        || value.starts_with("javascript:")
    {
        return None;
    }

    let (asset_path, suffix) = split_asset_reference(value);
    let normalized_asset_path = asset_path.trim_start_matches('/');
    if normalized_asset_path.is_empty()
        || normalized_asset_path.starts_with('.')
        || WebUiAssets::get(normalized_asset_path).is_none()
    {
        return None;
    }

    Some(format!(
        "{}{suffix}",
        prefixed_asset_path(resource_base_url, normalized_asset_path)
    ))
}

fn split_asset_reference(value: &str) -> (&str, &str) {
    let suffix_start = value.find(['?', '#']).unwrap_or(value.len());
    value.split_at(suffix_start)
}

fn prefixed_asset_path(resource_base_url: &str, asset_path: &str) -> String {
    if resource_base_url == "/" {
        format!("/{asset_path}")
    } else {
        format!("{resource_base_url}{asset_path}")
    }
}

fn is_safe_resource_path_prefix(value: &str) -> bool {
    if value.is_empty() || !value.starts_with('/') || value.ends_with('/') {
        return false;
    }

    value
        .chars()
        .all(|ch| ch == '/' || ch == '-' || ch == '_' || ch.is_ascii_alphanumeric())
}

fn cache_control_for(asset_path: &str) -> &'static str {
    match asset_path {
        "index.html"
        | "favicon.ico"
        | "favicon-16x16.png"
        | "favicon-32x32.png"
        | "mstile-144x144.png"
        | "apple-touch-icon.png"
        | "apple-touch-icon-180x180.png"
        | "android-chrome-192x192.png"
        | "android-chrome-512x512.png"
        | "manifest.json" => "no-store",
        _ => "max-age=31536000, public",
    }
}

fn resolve_embedded_asset_path(webui_path: &str) -> Option<String> {
    let normalized = webui_path.trim_matches('/');
    if normalized
        .split('/')
        .any(|segment| segment == ".." || segment.contains('\\'))
    {
        return None;
    }

    if normalized.is_empty() {
        return Some("index.html".to_string());
    }

    if is_index_html_candidate(normalized) {
        if WebUiAssets::get(normalized).is_some() {
            return Some(normalized.to_string());
        }
        return Some("index.html".to_string());
    }

    if WebUiAssets::get(normalized).is_some() {
        Some(normalized.to_string())
    } else {
        None
    }
}

fn is_index_html_candidate(path: &str) -> bool {
    Path::new(path).extension().is_none()
}

fn is_runtime_owned_prefix(path: &str) -> bool {
    let normalized = path.trim_matches('/');
    normalized == "api"
        || normalized.starts_with("api/")
        || normalized == "opds"
        || normalized.starts_with("opds/")
        || normalized == "kobo"
        || normalized.starts_with("kobo/")
        || normalized == "koreader"
        || normalized.starts_with("koreader/")
        || normalized == "sse"
        || normalized.starts_with("sse/")
        || normalized == "health"
        || normalized.starts_with("health/")
        || normalized == "metrics"
        || normalized.starts_with("metrics/")
        || normalized == "actuator"
        || normalized.starts_with("actuator/")
        || normalized == "oauth2"
        || normalized.starts_with("oauth2/")
        || normalized.starts_with("login/oauth2/")
}

fn content_type_for(path: &Path) -> String {
    mime_guess::from_path(path)
        .first_or_octet_stream()
        .essence_str()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        INDEX_HTML_CACHE_MAX_ENTRIES, IndexHtmlCache, content_type_for, is_runtime_owned_prefix,
        request_scoped_resource_base_url, resolve_embedded_asset_path, serve_webui_asset,
    };
    #[cfg(webui_dist_present)]
    use super::{cached_rewritten_index_html, rewrite_index_html};
    use axum::body::Bytes;
    #[cfg(webui_dist_present)]
    use axum::body::to_bytes;
    #[cfg(webui_dist_present)]
    use axum::http::header;
    use axum::http::{HeaderMap, StatusCode};
    use std::path::Path;
    #[cfg(webui_dist_present)]
    use super::super::webui_assets::WebUiAssets;

    #[cfg(webui_dist_present)]
    #[tokio::test]
    async fn webui_entrypoint_serves_embedded_index_html() {
        let response = serve_webui_asset("", "/");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            content_type_for(Path::new("index.html")).as_str(),
        );

        let response_body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("webui entrypoint body should be readable");
        let index_html = rewritten_embedded_index_html("/");

        assert_eq!(response_body.as_ref(), index_html.as_slice());
    }

    #[cfg(webui_dist_present)]
    #[tokio::test]
    async fn extensionless_spa_routes_fall_back_to_embedded_index_html() {
        let response = serve_webui_asset("series/123", "/");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            content_type_for(Path::new("index.html")).as_str(),
        );

        let response_body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("spa fallback body should be readable");
        let index_html = rewritten_embedded_index_html("/");

        assert_eq!(response_body.as_ref(), index_html.as_slice());
    }

    #[cfg(webui_dist_present)]
    #[tokio::test]
    async fn root_level_embedded_assets_are_served_from_embed_storage() {
        for asset_path in ["manifest.json", "android-chrome-192x192.png"] {
            let response = serve_webui_asset(asset_path, "/");

            assert_eq!(
                response.status(),
                StatusCode::OK,
                "{asset_path} should be served"
            );
            assert_eq!(
                response.headers().get(header::CONTENT_TYPE).unwrap(),
                content_type_for(Path::new(asset_path)).as_str(),
                "{asset_path} should use mime_guess content type",
            );

            let response_body = to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("embedded asset body should be readable");
            let embedded_asset =
                WebUiAssets::get(asset_path).expect("embedded asset should exist in rust-embed");

            assert_eq!(response_body.as_ref(), embedded_asset.as_ref());
        }
    }

    #[cfg(webui_dist_present)]
    #[tokio::test]
    async fn html_entry_assets_are_served_with_no_store_cache_control() {
        for asset_path in ["", "index.html", "manifest.json"] {
            let response = serve_webui_asset(asset_path, "/");
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(
                response
                    .headers()
                    .get(header::CACHE_CONTROL)
                    .and_then(|value| value.to_str().ok()),
                Some("no-store"),
                "{asset_path:?} should disable caching like Kotlin entry resources",
            );
        }
    }

    #[tokio::test]
    async fn missing_extensionful_assets_return_not_found() {
        let response = serve_webui_asset("missing.js", "/");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn resolve_embedded_asset_path_rejects_traversal_and_hidden_nested_prefix_stripping() {
        assert_eq!(resolve_embedded_asset_path("../index.html"), None);
        assert_eq!(resolve_embedded_asset_path("folder\\index.html"), None);
        assert_eq!(resolve_embedded_asset_path("library/1/js/app.js"), None);
    }

    #[test]
    fn runtime_owned_prefix_filter_keeps_login_spa_route_while_reserving_oauth_callback_path() {
        assert!(
            !is_runtime_owned_prefix("login"),
            "runtime WebUI must keep /login as SPA route; only /login/oauth2/* callback paths are runtime-owned",
        );
        assert!(
            is_runtime_owned_prefix("login/oauth2/code/provider-a"),
            "runtime must continue reserving /login/oauth2/code/{{id}} callback endpoint ownership",
        );
    }

    #[test]
    fn content_type_for_uses_octet_stream_fallback_for_unknown_extensions() {
        assert_eq!(
            content_type_for(Path::new("asset.unknown-extension")),
            "application/octet-stream",
        );
    }

    #[test]
    fn request_scoped_resource_base_url_accepts_safe_prefix_and_rejects_unsafe_values() {
        let mut safe_headers = HeaderMap::new();
        safe_headers.insert("x-forwarded-prefix", "/komga".parse().unwrap());
        assert_eq!(request_scoped_resource_base_url(&safe_headers), "/komga/");

        let mut unsafe_headers = HeaderMap::new();
        unsafe_headers.insert("x-forwarded-prefix", "/../komga".parse().unwrap());
        assert_eq!(request_scoped_resource_base_url(&unsafe_headers), "/");
    }

    #[cfg(webui_dist_present)]
    #[test]
    fn cached_rewritten_index_html_matches_direct_rewrite_for_same_prefix() {
        let expected = rewritten_embedded_index_html("/komga/");

        assert_eq!(
            cached_rewritten_index_html("/komga/").as_ref(),
            expected.as_slice()
        );
    }

    #[test]
    fn index_html_cache_evicts_oldest_entry_after_recent_access_reorders_usage() {
        let mut cache = IndexHtmlCache::default();
        let stable_entry = "/stable/".to_string();

        for index in 0..INDEX_HTML_CACHE_MAX_ENTRIES {
            let key = if index == 0 {
                stable_entry.clone()
            } else {
                format!("/entry-{index}/")
            };
            cache.insert(key, Bytes::from(format!("value-{index}")));
        }

        assert!(cache.get(stable_entry.as_str()).is_some());

        cache.insert("/fresh/".to_string(), Bytes::from_static(b"fresh"));

        assert!(cache.get(stable_entry.as_str()).is_some());
        assert!(cache.get("/entry-1/").is_none());
        assert!(cache.get("/fresh/").is_some());
    }

    #[cfg(webui_dist_present)]
    fn rewritten_embedded_index_html(resource_base_url: &str) -> Vec<u8> {
        let index_html = WebUiAssets::get("index.html").expect("embedded index.html should exist");
        rewrite_index_html(index_html.as_ref(), resource_base_url)
    }
}
