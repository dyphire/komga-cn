use std::fs;
use std::io::Read;
use std::path::Path;

use zip::ZipArchive;

use super::{TransientEpubManifestItem, TransientMetadataInference, epub};
use crate::metadata::{
    infer_transient_comicinfo_provider_metadata, infer_transient_epub_provider_metadata,
};
use crate::rar_support::read_rar_entry_bytes;
use detection::transient_book_media_type;

use super::detection;

pub(super) fn infer_transient_metadata(path_or_name: &str) -> TransientMetadataInference {
    let media_type = transient_book_media_type(path_or_name);
    if media_type == "application/epub+zip"
        && let Some(inferred) = infer_transient_epub_metadata_from_path(path_or_name)
    {
        return inferred;
    }

    if matches!(
        media_type.as_str(),
        "application/zip"
            | "application/vnd.comicbook-rar"
            | "application/x-rar-compressed"
            | "application/x-rar-compressed; version=4"
            | "application/x-rar-compressed; version=5"
    ) && let Some(inferred) =
        infer_transient_comicinfo_provider_metadata_from_path(path_or_name, media_type.as_str())
    {
        return inferred;
    }

    TransientMetadataInference::default()
}

fn merge_transient_metadata_inference(
    target: &mut TransientMetadataInference,
    incoming: TransientMetadataInference,
) {
    for title in incoming.series_titles {
        if !title.trim().is_empty()
            && !target
                .series_titles
                .iter()
                .any(|existing| existing == &title)
        {
            target.series_titles.push(title);
        }
    }

    if target.number.is_none() {
        target.number = incoming.number;
    }
}

fn transient_metadata_inference_from_provider(
    provider_inference: crate::metadata::TransientMetadataProviderInference,
) -> TransientMetadataInference {
    TransientMetadataInference {
        series_titles: provider_inference.series_titles,
        number: provider_inference.number,
    }
}

fn infer_transient_comicinfo_provider_metadata_from_path(
    path: &str,
    media_type: &str,
) -> Option<TransientMetadataInference> {
    let comicinfo_bytes = if media_type == "application/zip" {
        let file = fs::File::open(path).ok()?;
        let mut archive = ZipArchive::new(file).ok()?;
        let mut entry = archive.by_name("ComicInfo.xml").ok()?;
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).ok()?;
        bytes
    } else {
        read_rar_entry_bytes(Path::new(path), "ComicInfo.xml")
            .ok()
            .flatten()?
    };
    let comicinfo_xml = String::from_utf8(comicinfo_bytes).ok()?;
    Some(transient_metadata_inference_from_provider(
        infer_transient_comicinfo_provider_metadata(&comicinfo_xml),
    ))
}

fn infer_transient_epub_metadata_from_path(path: &str) -> Option<TransientMetadataInference> {
    let file = fs::File::open(path).ok()?;
    let mut archive = ZipArchive::new(file).ok()?;
    let container_xml =
        epub::read_zip_entry_bytes_normalized(&mut archive, "META-INF/container.xml")?;
    let rootfile_path = epub::parse_transient_epub_rootfile_path(&container_xml)?;
    let package_document = epub::read_zip_entry_bytes_normalized(&mut archive, &rootfile_path)?;
    let manifest = epub::parse_transient_epub_manifest_items(&package_document, &rootfile_path);
    let mut inferred = transient_metadata_inference_from_provider(
        infer_transient_epub_provider_metadata(&package_document),
    );
    inferred.number = None;

    if let Some(comicinfo_inference) =
        infer_transient_comicinfo_provider_metadata_from_epub_archive(&mut archive, &manifest)
    {
        merge_transient_metadata_inference(&mut inferred, comicinfo_inference);
    }

    Some(inferred)
}

fn infer_transient_comicinfo_provider_metadata_from_epub_archive<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    manifest: &std::collections::HashMap<String, TransientEpubManifestItem>,
) -> Option<TransientMetadataInference> {
    let comicinfo_path = manifest
        .values()
        .find(|item| item.href == "ComicInfo.xml")
        .map(|item| item.href.as_str())?;
    let comicinfo_bytes = epub::read_zip_entry_bytes_normalized(archive, comicinfo_path)?;
    let comicinfo_xml = String::from_utf8(comicinfo_bytes).ok()?;
    Some(transient_metadata_inference_from_provider(
        infer_transient_comicinfo_provider_metadata(&comicinfo_xml),
    ))
}
