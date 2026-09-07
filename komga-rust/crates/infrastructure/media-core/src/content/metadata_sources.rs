use std::io::{Read, Seek};

use anyhow::Context;
use zip::ZipArchive;

use crate::formats::rar::RarEntryBytesRecord;

pub const COMICINFO_FILE_NAME: &str = "ComicInfo.xml";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MetadataSourceRequest {
    pub comicinfo: bool,
    pub epub: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum CapturedMetadataDocument {
    #[default]
    NotRequested,
    NotApplicable,
    Absent,
    Present(Vec<u8>),
    Failed(String),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CapturedMetadataSources {
    pub comicinfo: CapturedMetadataDocument,
    pub epub: CapturedMetadataDocument,
}

pub fn is_comicinfo_entry(entry_name: &str) -> bool {
    let entry_name = entry_name.replace('\\', "/");
    entry_name == COMICINFO_FILE_NAME
        || entry_name
            .rsplit('/')
            .next()
            .is_some_and(|name| name == COMICINFO_FILE_NAME)
}

pub fn read_comicinfo_from_zip_archive<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
) -> anyhow::Result<Option<Vec<u8>>> {
    let mut fallback_index = None;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .with_context(|| format!("read ComicInfo archive entry at index {index}"))?;
        if entry.is_dir() {
            continue;
        }
        let entry_name = entry
            .name()
            .with_context(|| format!("decode ComicInfo archive entry at index {index}"))?
            .replace('\\', "/");
        if entry_name == COMICINFO_FILE_NAME {
            drop(entry);
            return read_zip_entry(archive, index).map(Some);
        }
        if fallback_index.is_none() && is_comicinfo_entry(&entry_name) {
            fallback_index = Some(index);
        }
    }

    fallback_index
        .map(|index| read_zip_entry(archive, index))
        .transpose()
}

fn read_zip_entry<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    index: usize,
) -> anyhow::Result<Vec<u8>> {
    let mut entry = archive
        .by_index(index)
        .with_context(|| format!("open ComicInfo archive entry at index {index}"))?;
    let mut bytes = Vec::new();
    entry
        .read_to_end(&mut bytes)
        .with_context(|| format!("read ComicInfo archive entry at index {index}"))?;
    Ok(bytes)
}

pub fn take_comicinfo_from_rar_entries(entries: &mut [RarEntryBytesRecord]) -> Option<Vec<u8>> {
    let index = entries
        .iter()
        .position(|entry| entry.file_name == COMICINFO_FILE_NAME)
        .or_else(|| {
            entries
                .iter()
                .position(|entry| is_comicinfo_entry(&entry.file_name))
        })?;
    Some(std::mem::take(&mut entries[index].bytes))
}
