use std::path::Path;

use async_trait::async_trait;

use super::{
    ArchiveEntry, BookMediaPort, BookMediaRecord, ContentAccessPort, ContentResolverPort,
    SeriesArchiveEntries,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArchiveDelivery {
    Asset(ArchiveDeliveryAsset),
    NotFound,
    Internal(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveDeliveryAsset {
    pub bytes: Vec<u8>,
    pub file_name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveFileEntry {
    pub file_name: String,
    pub bytes: Vec<u8>,
}

pub struct ArchiveDeliveryService<'a, R, C, B>
where
    R: ArchiveReaderPort + ?Sized,
    C: ArchiveContentPort + ?Sized,
    B: ArchiveBuilderPort + ?Sized,
{
    reader: &'a R,
    content: &'a C,
    builder: &'a B,
}

#[async_trait]
pub trait ArchiveReaderPort: Send + Sync {
    async fn readlist_name(&self, readlist_id: &str) -> Result<Option<String>, String>;

    async fn book_media(&self, book_id: &str) -> Result<Option<BookMediaRecord>, String>;

    async fn series_archive_entries(
        &self,
        series_id: &str,
    ) -> Result<Option<SeriesArchiveEntries>, String>;
}

#[async_trait]
impl<T> ArchiveReaderPort for T
where
    T: BookMediaPort + ContentAccessPort + Send + Sync + ?Sized,
{
    async fn readlist_name(&self, readlist_id: &str) -> Result<Option<String>, String> {
        ContentAccessPort::readlist_name(self, readlist_id).await
    }

    async fn book_media(&self, book_id: &str) -> Result<Option<BookMediaRecord>, String> {
        BookMediaPort::book_media(self, book_id).await
    }

    async fn series_archive_entries(
        &self,
        series_id: &str,
    ) -> Result<Option<SeriesArchiveEntries>, String> {
        ContentAccessPort::series_archive_entries(self, series_id).await
    }
}

#[async_trait]
pub trait ArchiveContentPort: Send + Sync {
    async fn read_media_file_bytes(&self, path: &Path) -> Result<Option<Vec<u8>>, String>;
}

#[async_trait]
impl<T> ArchiveContentPort for T
where
    T: ContentResolverPort + Send + Sync + ?Sized,
{
    async fn read_media_file_bytes(&self, path: &Path) -> Result<Option<Vec<u8>>, String> {
        ContentResolverPort::read_media_file_bytes(self, path).await
    }
}

pub trait ArchiveBuilderPort: Send + Sync {
    fn build_archive(&self, entries: Vec<ArchiveFileEntry>) -> Result<Vec<u8>, String>;
}

impl<'a, R, C, B> ArchiveDeliveryService<'a, R, C, B>
where
    R: ArchiveReaderPort + ?Sized,
    C: ArchiveContentPort + ?Sized,
    B: ArchiveBuilderPort + ?Sized,
{
    pub fn new(reader: &'a R, content: &'a C, builder: &'a B) -> Self {
        Self {
            reader,
            content,
            builder,
        }
    }

    pub async fn readlist_archive(
        &self,
        readlist_id: &str,
        visible_book_ids: Vec<String>,
    ) -> ArchiveDelivery {
        if visible_book_ids.is_empty() {
            return ArchiveDelivery::NotFound;
        }

        let readlist_name = match self.reader.readlist_name(readlist_id).await {
            Ok(Some(name)) => name,
            Ok(None) => return ArchiveDelivery::NotFound,
            Err(error) => return ArchiveDelivery::Internal(error),
        };

        let mut entries = Vec::new();
        for (index, book_id) in visible_book_ids.into_iter().enumerate() {
            let media = match self.reader.book_media(&book_id).await {
                Ok(Some(media)) => media,
                Ok(None) => continue,
                Err(error) => return ArchiveDelivery::Internal(error),
            };

            let bytes = match self.content.read_media_file_bytes(&media.file_path).await {
                Ok(Some(bytes)) => bytes,
                Ok(None) => continue,
                Err(error) => return ArchiveDelivery::Internal(error),
            };
            entries.push(ArchiveFileEntry {
                file_name: readlist_archive_entry_name(index, &media.file_name),
                bytes,
            });
        }

        archive_asset(self.builder, format!("{readlist_name}.zip"), entries)
    }

    pub async fn series_archive(&self, series_id: &str) -> ArchiveDelivery {
        let archive = match self.reader.series_archive_entries(series_id).await {
            Ok(Some(archive_entries)) => archive_entries,
            Ok(None) => return ArchiveDelivery::NotFound,
            Err(error) => return ArchiveDelivery::Internal(error),
        };

        self.series_archive_from_entries(archive.series_title, archive.entries)
            .await
    }

    pub async fn series_archive_from_entries(
        &self,
        series_title: String,
        entries: Vec<ArchiveEntry>,
    ) -> ArchiveDelivery {
        let mut archive_entries = Vec::new();
        for entry in entries {
            let bytes = match self.content.read_media_file_bytes(&entry.file_path).await {
                Ok(Some(bytes)) => bytes,
                Ok(None) => continue,
                Err(error) => return ArchiveDelivery::Internal(error),
            };
            archive_entries.push(ArchiveFileEntry {
                file_name: entry.file_name,
                bytes,
            });
        }

        archive_asset(self.builder, format!("{series_title}.zip"), archive_entries)
    }
}

fn archive_asset<B>(
    builder: &B,
    file_name: String,
    entries: Vec<ArchiveFileEntry>,
) -> ArchiveDelivery
where
    B: ArchiveBuilderPort + ?Sized,
{
    match builder.build_archive(entries) {
        Ok(bytes) => ArchiveDelivery::Asset(ArchiveDeliveryAsset { bytes, file_name }),
        Err(error) => ArchiveDelivery::Internal(error),
    }
}

fn readlist_archive_entry_name(index: usize, file_name: &str) -> String {
    let visible_name = file_name
        .rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(file_name);
    format!("{} - {}", index + 1, visible_name)
}
