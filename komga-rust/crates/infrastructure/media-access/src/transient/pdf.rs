use anyhow::Context;
use pdfium_render::prelude::*;

use komga_infrastructure_media_core::formats::pdfium::load_pdfium;

pub(super) fn render_pdf_page_image_bytes(path: &str, page_number: u32) -> anyhow::Result<Vec<u8>> {
    if page_number == 0 {
        return Err(anyhow::anyhow!(
            "render transient pdf page 0: invalid page number"
        ));
    }

    let pdfium = load_pdfium()?;
    let document = pdfium
        .load_pdf_from_file(path, None)
        .map_err(|error| anyhow::anyhow!(error).context(format!("open transient pdf '{path}'")))?;
    let page = document
        .pages()
        .get(
            i32::try_from(page_number.saturating_sub(1))
                .context("convert transient pdf page number")?,
        )
        .map_err(|error| {
            anyhow::anyhow!(error).context(format!(
                "load transient pdf page {page_number} from '{path}': "
            ))
        })?;
    let rendered = page
        .render_with_config(
            &PdfRenderConfig::new()
                .set_target_width(1600)
                .set_maximum_height(1600),
        )
        .map_err(|error| {
            anyhow::anyhow!(error).context(format!(
                "render transient pdf page {page_number} from '{path}': "
            ))
        })?
        .as_image()
        .map_err(|error| {
            anyhow::anyhow!(error).context(format!(
                "convert transient pdf page {page_number} from '{path}' to image: "
            ))
        })?
        .into_rgb8();

    let mut output = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(rendered)
        .write_to(&mut output, image::ImageFormat::Jpeg)
        .map_err(|error| {
            anyhow::anyhow!(error).context(format!(
                "encode transient pdf page {page_number} from '{path}' as jpeg: "
            ))
        })?;
    Ok(output.into_inner())
}
