use axum::Json;
use axum::extract::Path as AxumPath;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use rust_embed::Embed;
use serde_json::Value;
use std::collections::BTreeSet;
use std::sync::Arc;

use crate::identity_access::auth::require_request_auth;
use crate::state::HttpAppState;

#[derive(Embed)]
#[folder = "../../../komga/src/main/resources/embeddedFonts"]
struct EmbeddedFonts;

pub(crate) async fn get_fonts_families(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_request_auth(&*app.services.runtime_identity, &headers).await {
        return response;
    }

    let families = merged_font_families(&app);
    Json(Value::Array(
        families.into_iter().map(Value::String).collect(),
    ))
    .into_response()
}

fn merged_font_families(app: &HttpAppState) -> Vec<String> {
    let mut families = embedded_font_families()
        .into_iter()
        .collect::<BTreeSet<_>>();
    families.extend(
        app.services
            .operational_settings
            .list_font_families(app.operational.runtime.fonts_data_directory.clone()),
    );
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

fn load_embedded_font_family_css(font_family: &str) -> Option<String> {
    let mut font_files = EmbeddedFonts::iter()
        .filter_map(|path| {
            let path = path.as_ref();
            let (family, file_name) = path.split_once('/')?;
            if family != font_family || font_extension(file_name).is_none() {
                return None;
            }
            Some(file_name.to_string())
        })
        .collect::<Vec<_>>();

    if font_files.is_empty() {
        return None;
    }

    font_files.sort_by_key(|file_name| file_name.to_ascii_lowercase());
    let mut groups: Vec<(FontCharacteristics, Vec<String>)> = Vec::new();
    for file_name in font_files {
        let characteristics = font_characteristics(&file_name);
        if let Some((_, files)) = groups
            .iter_mut()
            .find(|(current, _)| *current == characteristics)
        {
            files.push(file_name);
        } else {
            groups.push((characteristics, vec![file_name]));
        }
    }

    Some(
        groups
            .into_iter()
            .map(|(characteristics, files)| {
                build_font_face_block(font_family, &characteristics, &files)
            })
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

fn filesystem_font_family_exists(app: &HttpAppState, font_family: &str) -> bool {
    app.operational
        .runtime
        .fonts_data_directory
        .join(font_family)
        .is_dir()
}

#[derive(Clone, PartialEq, Eq)]
struct FontCharacteristics {
    style: &'static str,
    weight: &'static str,
}

pub(crate) async fn get_font_file(
    State(app): State<Arc<HttpAppState>>,
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

    let fonts_directory = app.operational.runtime.fonts_data_directory.clone();
    let bytes = if filesystem_font_family_exists(&app, &font_family) {
        app.services.operational_settings.load_font_file(
            fonts_directory.clone(),
            font_family.clone(),
            font_file.clone(),
        )
    } else {
        load_embedded_font_file(&font_family, &font_file).or_else(|| {
            app.services.operational_settings.load_font_file(
                fonts_directory.clone(),
                font_family.clone(),
                font_file.clone(),
            )
        })
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
    State(app): State<Arc<HttpAppState>>,
    AxumPath(font_family): AxumPath<String>,
) -> Response {
    if font_family.contains('/') || font_family.contains('\\') {
        return StatusCode::NOT_FOUND.into_response();
    }

    let fonts_directory = app.operational.runtime.fonts_data_directory.clone();
    let css = if filesystem_font_family_exists(&app, &font_family) {
        app.services
            .operational_settings
            .load_font_family_css(fonts_directory.clone(), font_family.clone())
    } else {
        load_embedded_font_family_css(&font_family).or_else(|| {
            app.services
                .operational_settings
                .load_font_family_css(fonts_directory.clone(), font_family.clone())
        })
    };

    let Some(css) = css else {
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

fn font_format(file_name: &str) -> Option<&'static str> {
    match font_extension(file_name) {
        Some("ttf") => Some("truetype"),
        Some("otf") => Some("opentype"),
        Some("woff") => Some("woff"),
        Some("woff2") => Some("woff2"),
        _ => None,
    }
}

fn font_characteristics(file_name: &str) -> FontCharacteristics {
    let lower = file_name.to_ascii_lowercase();
    FontCharacteristics {
        style: if lower.contains("italic") {
            "italic"
        } else {
            "normal"
        },
        weight: if lower.contains("bold") {
            "bold"
        } else {
            "normal"
        },
    }
}

fn build_font_face_block(
    font_family: &str,
    characteristics: &FontCharacteristics,
    files: &[String],
) -> String {
    let src = files
        .iter()
        .map(|file_name| {
            format!(
                "url('{}') format('{}')",
                file_name,
                font_format(file_name).expect("font format should exist for grouped files")
            )
        })
        .collect::<Vec<_>>()
        .join(",");

    format!(
        "@font-face {{\n    font-family: '{}';\n    src: {};\n    font-weight: {};\n    font-style: {};\n}}\n",
        font_family, src, characteristics.weight, characteristics.style,
    )
}
