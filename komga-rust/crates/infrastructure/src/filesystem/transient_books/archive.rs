use std::fs;
use std::path::Path;

use zip::ZipArchive;

use super::TransientBookPage;
use super::detection::{is_supported_page_image_file_name, transient_entry_media_type};
use super::image_analysis::{image_dimensions_from_bytes, image_dimensions_from_reader};
use crate::rar_support::{list_rar_entries, read_rar_entry_bytes};

pub(super) fn analyze_transient_zip_archive(
    path: &str,
) -> Result<(Vec<TransientBookPage>, Vec<String>), String> {
    let file = fs::File::open(path).map_err(|error| format!("open archive: {error}"))?;
    let mut archive = ZipArchive::new(file).map_err(|error| format!("read archive: {error}"))?;

    let mut files = Vec::new();
    let mut pages = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("read archive entry: {error}"))?;
        let file_name = entry
            .name()
            .map_err(|error| format!("read archive entry name: {error}"))?
            .trim()
            .to_string();
        if file_name.is_empty() || file_name.ends_with('/') {
            continue;
        }

        files.push(file_name.clone());
        if !is_supported_page_image_file_name(&file_name) {
            continue;
        }

        let dimensions = image_dimensions_from_reader(&mut entry);
        pages.push(TransientBookPage {
            number: (pages.len() as u32) + 1,
            file_name: file_name.clone(),
            media_type: transient_entry_media_type(&file_name),
            width: dimensions.map(|(width, _)| width),
            height: dimensions.map(|(_, height)| height),
            size_bytes: Some(entry.size()),
        });
    }

    files.sort();
    Ok((pages, files))
}

pub(super) fn analyze_transient_rar_archive(
    path: &str,
) -> Result<(Vec<TransientBookPage>, Vec<String>), String> {
    let entries =
        list_rar_entries(Path::new(path)).map_err(|_| "Book analysis failed".to_string())?;
    let mut files = entries
        .iter()
        .map(|entry| entry.file_name.clone())
        .collect::<Vec<_>>();
    files.sort();

    let pages = entries
        .into_iter()
        .filter(|entry| is_supported_page_image_file_name(&entry.file_name))
        .enumerate()
        .map(|(index, entry)| {
            let entry_bytes = read_rar_entry_bytes(Path::new(path), &entry.file_name)
                .ok()
                .flatten();
            let dimensions = entry_bytes.as_deref().and_then(image_dimensions_from_bytes);
            TransientBookPage {
                number: (index as u32) + 1,
                file_name: entry.file_name.clone(),
                media_type: transient_entry_media_type(&entry.file_name),
                width: dimensions.map(|(width, _)| width),
                height: dimensions.map(|(_, height)| height),
                size_bytes: Some(entry.unpacked_size),
            }
        })
        .collect::<Vec<_>>();

    Ok((pages, files))
}
