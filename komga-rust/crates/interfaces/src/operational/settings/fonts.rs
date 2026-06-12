use axum::Json;
use axum::extract::Path as AxumPath;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use komga_application::operational::{
    FontPort, build_font_family_css, font_media_type, is_supported_font_file,
};
use rust_embed::Embed;
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use crate::identity_access::auth::Authenticated;
use crate::media_assets::http_helpers::attachment_disposition;
use crate::state::OperationalApiState;

#[derive(Embed)]
#[folder = "../../../komga/src/main/resources/embeddedFonts"]
struct EmbeddedFonts;

struct EmbeddedFontPath<'a> {
    family: &'a str,
    file_name: &'a str,
}

impl<'a> EmbeddedFontPath<'a> {
    fn parse(path: &'a str) -> Option<Self> {
        path.split_once('/')
            .map(|(family, file_name)| Self { family, file_name })
    }
}

pub(crate) async fn get_fonts_families(
    State(app): State<OperationalApiState>,
    _: Authenticated,
) -> Response {
    let families = match merged_font_families(
        app.fonts.as_ref(),
        app.operational.runtime.fonts_data_directory.as_path(),
    ) {
        Ok(families) => families,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    Json(Value::Array(
        families.into_iter().map(Value::String).collect(),
    ))
    .into_response()
}

fn merged_font_families(
    fonts: &dyn FontPort,
    fonts_directory: &Path,
) -> Result<Vec<String>, String> {
    let mut families = embedded_font_families()
        .into_iter()
        .collect::<BTreeSet<_>>();
    families.extend(fonts.list_font_families(fonts_directory)?);
    Ok(families.into_iter().collect())
}

fn embedded_font_families() -> Vec<String> {
    EmbeddedFonts::iter()
        .filter_map(|path| {
            let path = path.as_ref();
            is_supported_font_file(path).then_some(())?;
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
    let font_files = EmbeddedFonts::iter()
        .filter_map(|path| {
            let path = path.as_ref();
            let font_path = EmbeddedFontPath::parse(path)?;
            if font_path.family != font_family || !is_supported_font_file(font_path.file_name) {
                return None;
            }
            Some(font_path.file_name.to_string())
        })
        .collect::<Vec<_>>();

    build_font_family_css(font_family, font_files)
}

fn filesystem_font_family_exists(
    fonts_directory: &Path,
    font_family: &str,
) -> Result<bool, String> {
    let path = fonts_directory.join(font_family);
    match fs::metadata(&path) {
        Ok(metadata) => Ok(metadata.is_dir()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!(
            "read font family metadata '{}': {error}",
            path.display()
        )),
    }
}

pub(crate) async fn get_font_file(
    State(app): State<OperationalApiState>,
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

    let fonts_directory = app.operational.runtime.fonts_data_directory.as_path();
    let filesystem_family_exists =
        match filesystem_font_family_exists(fonts_directory, &font_family) {
            Ok(exists) => exists,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };
    let bytes = if filesystem_family_exists {
        match app
            .fonts
            .load_font_file(fonts_directory, &font_family, &font_file)
        {
            Ok(bytes) => bytes,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    } else {
        match load_embedded_font_file(&font_family, &font_file) {
            Some(bytes) => Some(bytes),
            None => match app
                .fonts
                .load_font_file(fonts_directory, &font_family, &font_file)
            {
                Ok(bytes) => bytes,
                Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            },
        }
    };

    let Some(bytes) = bytes else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let content_disposition = font_attachment_disposition(&font_file);
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
    State(app): State<OperationalApiState>,
    AxumPath(font_family): AxumPath<String>,
) -> Response {
    if font_family.contains('/') || font_family.contains('\\') {
        return StatusCode::NOT_FOUND.into_response();
    }

    let fonts_directory = app.operational.runtime.fonts_data_directory.as_path();
    let filesystem_family_exists =
        match filesystem_font_family_exists(fonts_directory, &font_family) {
            Ok(exists) => exists,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };
    let css = if filesystem_family_exists {
        match app
            .fonts
            .load_font_family_css(fonts_directory, &font_family)
        {
            Ok(css) => css,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    } else {
        match load_embedded_font_family_css(&font_family) {
            Some(css) => Some(css),
            None => match app
                .fonts
                .load_font_family_css(fonts_directory, &font_family)
            {
                Ok(css) => css,
                Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            },
        }
    };

    let Some(css) = css else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let content_disposition = font_attachment_disposition(&format!("{font_family}.css"));
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

fn font_attachment_disposition(file_name: &str) -> String {
    if file_name.chars().all(is_plain_content_disposition_filename) {
        return format!("attachment; filename=\"{file_name}\"");
    }

    attachment_disposition(file_name)
}

fn is_plain_content_disposition_filename(character: char) -> bool {
    matches!(character, ' '..='~') && !matches!(character, '"' | '\\' | ';')
}
