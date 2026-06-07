use pdfium_render::prelude::*;

use crate::load_pdfium;

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
