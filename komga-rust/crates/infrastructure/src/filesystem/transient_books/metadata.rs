use std::fs;
use std::io::Read;
use std::path::Path;

use zip::ZipArchive;
use zip::result::ZipError;

use super::{TransientEpubManifestItem, TransientMetadataInference, epub};
use crate::metadata::{
    infer_transient_comicinfo_provider_metadata, infer_transient_epub_provider_metadata,
};
use crate::rar_support::read_rar_entry_bytes;
use detection::transient_book_media_type;

use super::detection;

pub(super) fn infer_transient_metadata(
    path_or_name: &str,
) -> anyhow::Result<TransientMetadataInference> {
    let media_type = transient_book_media_type(path_or_name);
    if media_type == "application/epub+zip"
        && let Some(inferred) = infer_transient_epub_metadata_from_path(path_or_name)?
    {
        return Ok(inferred);
    }

    if matches!(
        media_type.as_str(),
        "application/zip"
            | "application/vnd.comicbook-rar"
            | "application/x-rar-compressed"
            | "application/x-rar-compressed; version=4"
            | "application/x-rar-compressed; version=5"
    ) && let Some(inferred) =
        infer_transient_comicinfo_provider_metadata_from_path(path_or_name, media_type.as_str())?
    {
        return Ok(inferred);
    }

    Ok(TransientMetadataInference::default())
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
) -> anyhow::Result<Option<TransientMetadataInference>> {
    let comicinfo_bytes = if media_type == "application/zip" {
        let file = fs::File::open(path).map_err(|error| {
            anyhow::anyhow!(error).context(format!("open transient metadata archive '{path}'"))
        })?;
        let mut archive = ZipArchive::new(file).map_err(|error| {
            anyhow::anyhow!(error).context(format!("read transient metadata archive '{path}'"))
        })?;
        let Some(bytes) = read_zip_entry_bytes_for_metadata(&mut archive, "ComicInfo.xml", path)?
        else {
            return Ok(None);
        };
        bytes
    } else {
        let Some(bytes) =
            read_rar_entry_bytes(Path::new(path), "ComicInfo.xml").map_err(|error| {
                anyhow::anyhow!(error).context(format!("read transient metadata archive '{path}'"))
            })?
        else {
            return Ok(None);
        };
        bytes
    };
    let Ok(comicinfo_xml) = String::from_utf8(comicinfo_bytes) else {
        return Ok(None);
    };
    Ok(Some(transient_metadata_inference_from_provider(
        infer_transient_comicinfo_provider_metadata(&comicinfo_xml),
    )))
}

fn infer_transient_epub_metadata_from_path(
    path: &str,
) -> anyhow::Result<Option<TransientMetadataInference>> {
    let file = fs::File::open(path).map_err(|error| {
        anyhow::anyhow!(error).context(format!("open transient metadata archive '{path}'"))
    })?;
    let mut archive = ZipArchive::new(file).map_err(|error| {
        anyhow::anyhow!(error).context(format!("read transient metadata archive '{path}'"))
    })?;
    let Some(container_xml) =
        read_zip_entry_bytes_for_metadata(&mut archive, "META-INF/container.xml", path)?
    else {
        return Err(anyhow::anyhow!(format!(
            "missing transient epub container in '{path}'"
        )));
    };
    let rootfile_path = epub::parse_transient_epub_rootfile_path(&container_xml)
        .ok_or_else(|| anyhow::anyhow!(format!("parse transient epub container in '{path}'")))?;
    let Some(package_document) =
        read_zip_entry_bytes_for_metadata(&mut archive, &rootfile_path, path)?
    else {
        return Err(anyhow::anyhow!(format!(
            "missing transient epub package '{rootfile_path}' in '{path}'"
        )));
    };
    let manifest = epub::parse_transient_epub_manifest_items(&package_document, &rootfile_path)
        .map_err(|error| {
            anyhow::anyhow!(error).context(format!(
                "parse transient epub package '{rootfile_path}' in '{path}': "
            ))
        })?;
    let mut inferred = transient_metadata_inference_from_provider(
        infer_transient_epub_provider_metadata(&package_document)?,
    );
    inferred.number = None;

    if let Some(comicinfo_inference) =
        infer_transient_comicinfo_provider_metadata_from_epub_archive(
            &mut archive,
            &manifest,
            path,
        )?
    {
        merge_transient_metadata_inference(&mut inferred, comicinfo_inference);
    }

    Ok(Some(inferred))
}

fn infer_transient_comicinfo_provider_metadata_from_epub_archive<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    manifest: &std::collections::HashMap<String, TransientEpubManifestItem>,
    path: &str,
) -> anyhow::Result<Option<TransientMetadataInference>> {
    let comicinfo_path = manifest
        .values()
        .find(|item| item.href == "ComicInfo.xml")
        .map(|item| item.href.as_str());
    let Some(comicinfo_path) = comicinfo_path else {
        return Ok(None);
    };
    let Some(comicinfo_bytes) = read_zip_entry_bytes_for_metadata(archive, comicinfo_path, path)?
    else {
        return Ok(None);
    };
    let Ok(comicinfo_xml) = String::from_utf8(comicinfo_bytes) else {
        return Ok(None);
    };
    Ok(Some(transient_metadata_inference_from_provider(
        infer_transient_comicinfo_provider_metadata(&comicinfo_xml),
    )))
}

fn read_zip_entry_bytes_for_metadata<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    entry_path: &str,
    archive_path: &str,
) -> anyhow::Result<Option<Vec<u8>>> {
    let mut entry = match archive.by_name(entry_path) {
        Ok(entry) => entry,
        Err(ZipError::FileNotFound) => return Ok(None),
        Err(error) => {
            return Err(anyhow::anyhow!(format!(
                "read transient metadata archive entry '{entry_path}' from '{archive_path}': {error}"
            )));
        }
    };
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes).map_err(|error| {
        anyhow::anyhow!(error).context(format!(
            "read transient metadata archive entry '{entry_path}' from '{archive_path}': "
        ))
    })?;
    Ok(Some(bytes))
}
