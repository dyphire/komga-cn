use std::path::PathBuf;

use lopdf::{Document as PdfDocument, Object};
use pdfium_render::prelude::*;

use super::{KOTLIN_PDF_MIN_EDGE, TransientBookPage};
use crate::load_pdfium;

pub(super) fn analyze_transient_pdf(
    path: &str,
) -> Result<(Vec<TransientBookPage>, Vec<String>), String> {
    let document = PdfDocument::load(path).map_err(|error| format!("open pdf: {error}"))?;
    let page_count = document.get_pages().len() as u32;
    let pages = (1..=page_count)
        .map(|number| {
            let dimensions = pdf_page_dimensions(&document, number).map(scale_pdf_page_dimensions);
            TransientBookPage {
                number,
                file_name: number.to_string(),
                media_type: "image/jpeg".to_string(),
                width: dimensions.map(|(width, _)| width),
                height: dimensions.map(|(_, height)| height),
                size_bytes: None,
            }
        })
        .collect::<Vec<_>>();

    let file_name = PathBuf::from(path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string();

    Ok((pages, vec![file_name]))
}

pub(super) fn render_pdf_page_image_bytes(path: &str, page_number: u32) -> Option<Vec<u8>> {
    if page_number == 0 {
        return None;
    }

    let pdfium = load_pdfium().ok()?;
    let document = pdfium.load_pdf_from_file(path, None).ok()?;
    let page = document
        .pages()
        .get(i32::try_from(page_number.saturating_sub(1)).ok()?)
        .ok()?;
    let rendered = page
        .render_with_config(
            &PdfRenderConfig::new()
                .set_target_width(1600)
                .set_maximum_height(1600),
        )
        .ok()?
        .as_image()
        .ok()?
        .into_rgb8();

    let mut output = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(rendered)
        .write_to(&mut output, image::ImageFormat::Jpeg)
        .ok()?;
    Some(output.into_inner())
}

fn pdf_page_dimensions(document: &PdfDocument, page_number: u32) -> Option<(u32, u32)> {
    let object_id = *document.get_pages().get(&page_number)?;
    let page = document.get_dictionary(object_id).ok()?;
    let media_box = page.get(b"MediaBox").ok()?.as_array().ok()?;
    if media_box.len() != 4 {
        return None;
    }

    let left = pdf_numeric_value(&media_box[0])?;
    let bottom = pdf_numeric_value(&media_box[1])?;
    let right = pdf_numeric_value(&media_box[2])?;
    let top = pdf_numeric_value(&media_box[3])?;
    let width = (right - left).abs().round();
    let height = (top - bottom).abs().round();
    if width <= 0.0 || height <= 0.0 {
        return None;
    }

    Some((width as u32, height as u32))
}

fn pdf_numeric_value(object: &Object) -> Option<f64> {
    match object {
        Object::Integer(value) => Some(*value as f64),
        Object::Real(value) => Some((*value).into()),
        _ => None,
    }
}

fn scale_pdf_page_dimensions((width, height): (u32, u32)) -> (u32, u32) {
    let min_edge = f64::from(width.min(height));
    if min_edge <= 0.0 {
        return (width, height);
    }

    let scale = KOTLIN_PDF_MIN_EDGE / min_edge;
    let scaled_width = (f64::from(width) * scale).round().max(1.0) as u32;
    let scaled_height = (f64::from(height) * scale).round().max(1.0) as u32;
    (scaled_width, scaled_height)
}
