use std::io::Read;

use image::GenericImageView;

use super::TransientBookPage;
use super::detection::transient_entry_media_type;

pub(super) fn analyze_transient_image(path: &str) -> (Vec<TransientBookPage>, Vec<String>) {
    let file_name = std::path::PathBuf::from(path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string();
    let size_bytes = std::fs::metadata(path).ok().map(|meta| meta.len());
    let (width, height) = std::fs::read(path)
        .ok()
        .and_then(|bytes| image_dimensions_from_bytes(&bytes))
        .map(|(width, height)| (Some(width), Some(height)))
        .unwrap_or((None, None));

    (
        vec![TransientBookPage {
            number: 1,
            file_name: file_name.clone(),
            media_type: transient_entry_media_type(&file_name),
            width,
            height,
            size_bytes,
        }],
        vec![file_name],
    )
}

pub(super) fn image_dimensions_from_bytes(bytes: &[u8]) -> Option<(u32, u32)> {
    let image = image::load_from_memory(bytes).ok()?;
    Some(image.dimensions())
}

pub(super) fn image_dimensions_from_reader(reader: &mut dyn Read) -> Option<(u32, u32)> {
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).ok()?;
    image_dimensions_from_bytes(&bytes)
}
