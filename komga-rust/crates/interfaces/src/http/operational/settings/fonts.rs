use axum::Json;
use axum::extract::Extension;
use axum::extract::Path as AxumPath;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use rust_embed::Embed;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::Path;

use crate::http::identity_access::auth::require_auth;
use crate::operational_settings_access::fonts::{
    list_font_families, load_font_family_css, load_font_file,
};

use super::super::super::OperationalState;

#[derive(Embed)]
#[folder = "../../../komga/src/main/resources/embeddedFonts"]
struct EmbeddedFonts;

pub(crate) async fn get_fonts_families(
    headers: HeaderMap,
    Extension(state): Extension<OperationalState>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let families = merged_font_families(state.runtime.fonts_data_directory.as_path());
    Json(Value::Array(
        families.into_iter().map(Value::String).collect(),
    ))
    .into_response()
}

fn merged_font_families(fonts_directory: &Path) -> Vec<String> {
    let mut families = embedded_font_families()
        .into_iter()
        .collect::<BTreeSet<_>>();
    families.extend(list_font_families(fonts_directory));
    families.into_iter().collect()
}

fn embedded_font_families() -> Vec<String> {
    EmbeddedFonts::iter()
        .filter_map(|path| {
            let path = path.as_ref();
            font_extension(path)?;
            path.split('/').next().map(str::to_string)
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn load_embedded_font_file(font_family: &str, font_file: &str) -> Option<Vec<u8>> {
    let path = format!("{font_family}/{font_file}");
    EmbeddedFonts::get(&path).map(|file| file.data.into_owned())
}

fn filesystem_font_family_exists(fonts_directory: &Path, font_family: &str) -> bool {
    fonts_directory.join(font_family).is_dir()
}

pub(crate) async fn get_font_file(
    Extension(state): Extension<OperationalState>,
    AxumPath((font_family, font_file)): AxumPath<(String, String)>,
) -> Response {
    if font_family.contains('/')
        || font_family.contains('\\')
        || font_file.contains('/')
        || font_file.contains('\\')
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    let Some(media_type) = font_media_type(&font_file) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let fonts_directory = state.runtime.fonts_data_directory.as_path();
    let bytes = if filesystem_font_family_exists(fonts_directory, &font_family) {
        load_font_file(fonts_directory, &font_family, &font_file)
    } else {
        load_embedded_font_file(&font_family, &font_file)
            .or_else(|| load_font_file(fonts_directory, &font_family, &font_file))
    };

    let Some(bytes) = bytes else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let content_disposition = format!("attachment; filename=\"{}\"", font_file);
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, media_type),
            (header::CONTENT_DISPOSITION, content_disposition.as_str()),
        ],
        bytes,
    )
        .into_response()
}

pub(crate) async fn get_font_family_css(
    Extension(state): Extension<OperationalState>,
    AxumPath(font_family): AxumPath<String>,
) -> Response {
    if font_family.contains('/') || font_family.contains('\\') {
        return StatusCode::NOT_FOUND.into_response();
    }

    let Some(css) =
        load_font_family_css(state.runtime.fonts_data_directory.as_path(), &font_family)
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let content_disposition = format!("attachment; filename=\"{}.css\"", font_family);
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/css"),
            (header::CONTENT_DISPOSITION, content_disposition.as_str()),
        ],
        css,
    )
        .into_response()
}

fn font_extension(file_name: &str) -> Option<&str> {
    file_name
        .rsplit_once('.')
        .map(|(_, extension)| extension)
        .map(|extension| extension.to_ascii_lowercase())
        .filter(|extension| matches!(extension.as_str(), "woff" | "woff2" | "ttf" | "otf"))
        .map(|extension| {
            if extension == "woff2" {
                "woff2"
            } else if extension == "woff" {
                "woff"
            } else if extension == "ttf" {
                "ttf"
            } else {
                "otf"
            }
        })
}

fn font_media_type(file_name: &str) -> Option<&'static str> {
    match font_extension(file_name) {
        Some("woff") => Some("font/woff"),
        Some("woff2") => Some("font/woff2"),
        Some("ttf") => Some("font/ttf"),
        Some("otf") => Some("font/otf"),
        _ => None,
    }
}
