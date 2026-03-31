use axum::extract::Extension;
use axum::extract::Path as AxumPath;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::json;
use std::path::Path;

use super::{super::OperationalState, WebUiAssets};

pub(crate) async fn webui_entrypoint(Extension(_state): Extension<OperationalState>) -> Response {
    serve_webui_asset("")
}

pub(crate) async fn webui_asset(
    AxumPath(webui_path): AxumPath<String>,
    Extension(_state): Extension<OperationalState>,
) -> Response {
    if is_runtime_owned_prefix(webui_path.as_str()) {
        return StatusCode::NOT_FOUND.into_response();
    }
    serve_webui_asset(webui_path.as_str())
}

fn serve_webui_asset(webui_path: &str) -> Response {
    let Some(asset_path) = resolve_embedded_asset_path(webui_path) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(asset) = WebUiAssets::get(asset_path.as_str()) else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            [(header::CONTENT_TYPE, "application/json")],
            axum::Json(json!({
                "message": format!("embedded webui asset missing: {asset_path}"),
            })),
        )
            .into_response();
    };

    (
        [
            (
                header::CONTENT_TYPE,
                content_type_for(Path::new(asset_path.as_str())),
            ),
            (
                header::CACHE_CONTROL,
                cache_control_for(asset_path.as_str()).to_string(),
            ),
        ],
        asset.data,
    )
        .into_response()
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
        WebUiAssets, content_type_for, is_runtime_owned_prefix, resolve_embedded_asset_path,
        serve_webui_asset,
    };
    use axum::body::to_bytes;
    use axum::http::{StatusCode, header};
    use std::path::Path;

    #[tokio::test]
    async fn webui_entrypoint_serves_embedded_index_html() {
        let response = serve_webui_asset("");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            content_type_for(Path::new("index.html")).as_str(),
        );

        let response_body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("webui entrypoint body should be readable");
        let index_html = WebUiAssets::get("index.html").expect("embedded index.html should exist");

        assert_eq!(response_body.as_ref(), index_html.data.as_ref());
    }

    #[tokio::test]
    async fn extensionless_spa_routes_fall_back_to_embedded_index_html() {
        let response = serve_webui_asset("series/123");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            content_type_for(Path::new("index.html")).as_str(),
        );

        let response_body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("spa fallback body should be readable");
        let index_html = WebUiAssets::get("index.html").expect("embedded index.html should exist");

        assert_eq!(response_body.as_ref(), index_html.data.as_ref());
    }

    #[tokio::test]
    async fn root_level_embedded_assets_are_served_from_embed_storage() {
        for asset_path in ["manifest.json", "android-chrome-192x192.png"] {
            let response = serve_webui_asset(asset_path);

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

            assert_eq!(response_body.as_ref(), embedded_asset.data.as_ref());
        }
    }

    #[tokio::test]
    async fn html_entry_assets_are_served_with_no_store_cache_control() {
        for asset_path in ["", "index.html", "manifest.json"] {
            let response = serve_webui_asset(asset_path);
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
    async fn versioned_static_assets_are_served_with_long_lived_public_cache_control() {
        let static_asset = WebUiAssets::iter()
            .find(|path| path.contains('/'))
            .expect("embedded webui should expose at least one nested static asset");

        let response = serve_webui_asset(static_asset.as_ref());
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(header::CACHE_CONTROL)
                .and_then(|value| value.to_str().ok()),
            Some("max-age=31536000, public"),
            "nested hashed static assets should be cacheable long-term",
        );
    }

    #[tokio::test]
    async fn missing_extensionful_assets_return_not_found() {
        let response = serve_webui_asset("missing.js");

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
}
