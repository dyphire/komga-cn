use pdfium_render::prelude::*;

use crate::load_pdfium;

pub(super) fn render_pdf_page_image_bytes(path: &str, page_number: u32) -> Result<Vec<u8>, String> {
    if page_number == 0 {
        return Err("render transient pdf page 0: invalid page number".to_string());
    }

    let pdfium = load_pdfium()?;
    let document = pdfium
        .load_pdf_from_file(path, None)
        .map_err(|error| format!("open transient pdf '{path}': {error}"))?;
    let page = document
        .pages()
        .get(
            i32::try_from(page_number.saturating_sub(1))
                .map_err(|error| format!("convert transient pdf page number: {error}"))?,
        )
        .map_err(|error| format!("load transient pdf page {page_number} from '{path}': {error}"))?;
    let rendered = page
        .render_with_config(
            &PdfRenderConfig::new()
                .set_target_width(1600)
                .set_maximum_height(1600),
        )
        .map_err(|error| format!("render transient pdf page {page_number} from '{path}': {error}"))?
        .as_image()
        .map_err(|error| {
            format!("convert transient pdf page {page_number} from '{path}' to image: {error}")
        })?
        .into_rgb8();

    let mut output = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(rendered)
        .write_to(&mut output, image::ImageFormat::Jpeg)
        .map_err(|error| {
            format!("encode transient pdf page {page_number} from '{path}' as jpeg: {error}")
        })?;
    Ok(output.into_inner())
}
