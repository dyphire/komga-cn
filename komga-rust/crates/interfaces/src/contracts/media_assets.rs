use komga_application::media_assets::{
    BookPageRecord, BookProgressionRecord, CollectionThumbnailRecord, EntityThumbnailRecord,
    ReadlistTachiyomiCounters, ReadlistThumbnailRecord, SeriesTachiyomiProgress,
    SeriesThumbnailRecord,
};
use serde::Serialize;
use serde_json::Value;

use crate::media_assets::http_helpers::format_size_bytes;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThumbnailFieldsDto {
    #[serde(rename = "type")]
    pub thumbnail_type: String,
    pub selected: bool,
    pub media_type: String,
    pub file_size: i64,
    pub width: i64,
    pub height: i64,
}

impl ThumbnailFieldsDto {
    fn new(
        thumbnail_type: &komga_domain::media_assets::ThumbnailType,
        selected: bool,
        media_type: &str,
        file_size: i64,
        width: i64,
        height: i64,
    ) -> Self {
        Self {
            thumbnail_type: thumbnail_type.persisted_name().to_string(),
            selected,
            media_type: media_type.to_string(),
            file_size,
            width,
            height,
        }
    }

    pub fn from_book_record(record: &EntityThumbnailRecord) -> Self {
        Self::new(
            &record.thumbnail_type,
            record.selected,
            &record.media_type,
            record.file_size,
            record.width,
            record.height,
        )
    }

    pub fn from_series_record(record: &SeriesThumbnailRecord) -> Self {
        Self::new(
            &record.thumbnail_type,
            record.selected,
            &record.media_type,
            record.file_size,
            record.width,
            record.height,
        )
    }

    pub fn from_readlist_record(record: &ReadlistThumbnailRecord) -> Self {
        Self::new(
            &record.thumbnail_type,
            record.selected,
            &record.media_type,
            record.file_size,
            record.width,
            record.height,
        )
    }

    pub fn from_collection_record(record: &CollectionThumbnailRecord) -> Self {
        Self::new(
            &record.thumbnail_type,
            record.selected,
            &record.media_type,
            record.file_size,
            record.width,
            record.height,
        )
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookThumbnailDto {
    pub id: String,
    pub book_id: String,
    #[serde(flatten)]
    pub fields: ThumbnailFieldsDto,
}

impl BookThumbnailDto {
    pub fn from_record(record: &EntityThumbnailRecord) -> Self {
        Self {
            id: record.id.clone(),
            book_id: record.book_id.clone(),
            fields: ThumbnailFieldsDto::from_book_record(record),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesThumbnailDto {
    pub id: String,
    pub series_id: String,
    #[serde(flatten)]
    pub fields: ThumbnailFieldsDto,
}

impl SeriesThumbnailDto {
    pub fn from_record(record: &SeriesThumbnailRecord) -> Self {
        Self {
            id: record.id.clone(),
            series_id: record.series_id.clone(),
            fields: ThumbnailFieldsDto::from_series_record(record),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadListThumbnailDto {
    pub id: String,
    #[serde(rename = "readListId")]
    pub readlist_id: String,
    #[serde(flatten)]
    pub fields: ThumbnailFieldsDto,
}

impl ReadListThumbnailDto {
    pub fn from_record(record: &ReadlistThumbnailRecord) -> Self {
        Self {
            id: record.id.clone(),
            readlist_id: record.readlist_id.clone(),
            fields: ThumbnailFieldsDto::from_readlist_record(record),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionThumbnailDto {
    pub id: String,
    pub collection_id: String,
    #[serde(flatten)]
    pub fields: ThumbnailFieldsDto,
}

impl CollectionThumbnailDto {
    pub fn from_record(record: &CollectionThumbnailRecord) -> Self {
        Self {
            id: record.id.clone(),
            collection_id: record.collection_id.clone(),
            fields: ThumbnailFieldsDto::from_collection_record(record),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ReadiumPositionListDto {
    pub total: usize,
    pub positions: Vec<Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookPageDto {
    pub number: u64,
    pub file_name: String,
    pub media_type: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub size_bytes: Option<i64>,
    pub size: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebPubManifestDto {
    pub context: String,
    pub metadata: WebPubManifestMetadataDto,
    pub links: Vec<WebPubManifestLinkDto>,
    pub reading_order: Vec<WebPubManifestLinkDto>,
    pub resources: Vec<WebPubManifestLinkDto>,
    pub toc: Vec<WebPubManifestNavigationDto>,
    pub landmarks: Vec<WebPubManifestNavigationDto>,
    pub page_list: Vec<WebPubManifestNavigationDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebPubManifestMetadataDto {
    pub title: String,
    pub conforms_to: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identifier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number_of_pages: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<crate::contracts::common::KotlinUtcDateTime>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub translator: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub editor: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub illustrator: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub letterer: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub penciler: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub colorist: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inker: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contributor: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub belongs_to: Option<WebPubManifestBelongsToDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reading_progression: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rendition: Option<WebPubManifestRenditionDto>,
}

#[derive(Debug, Serialize)]
pub struct WebPubManifestBelongsToDto {
    pub series: Vec<WebPubManifestSeriesDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebPubManifestSeriesDto {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub links: Option<Vec<WebPubManifestLinkDto>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebPubManifestRenditionDto {
    pub layout: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebPubManifestLinkDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rel: Option<String>,
    pub href: String,
    #[serde(rename = "type")]
    pub media_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<WebPubManifestLinkPropertiesDto>,
    #[serde(flatten)]
    pub dimensions: Option<WebPubManifestLinkDimensionsDto>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub alternate: Vec<WebPubManifestLinkDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebPubManifestLinkPropertiesDto {
    pub authenticate: WebPubManifestAuthenticationLinkDto,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebPubManifestAuthenticationLinkDto {
    pub href: String,
    #[serde(rename = "type")]
    pub media_type: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebPubManifestLinkDimensionsDto {
    pub width: Option<i64>,
    pub height: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebPubManifestNavigationDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub href: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<WebPubManifestNavigationDto>,
}

impl From<BookPageRecord> for BookPageDto {
    fn from(page: BookPageRecord) -> Self {
        let (size_bytes, size) = if page.file_size < 0 {
            (None, String::new())
        } else {
            (
                Some(page.file_size),
                format_size_bytes(page.file_size as u64),
            )
        };

        Self {
            number: page.number,
            file_name: page.file_name,
            media_type: page.media_type,
            width: page.width,
            height: page.height,
            size_bytes,
            size,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TachiyomiSeriesProgressDto {
    pub books_count: u64,
    pub books_read_count: u64,
    pub books_unread_count: u64,
    pub books_in_progress_count: u64,
    pub last_read_continuous_number_sort: f64,
    pub max_number_sort: f64,
}

impl From<SeriesTachiyomiProgress> for TachiyomiSeriesProgressDto {
    fn from(progress: SeriesTachiyomiProgress) -> Self {
        Self {
            books_count: progress.books_count,
            books_read_count: progress.books_read_count,
            books_unread_count: progress.books_unread_count,
            books_in_progress_count: progress.books_in_progress_count,
            last_read_continuous_number_sort: progress.last_read_continuous_number_sort,
            max_number_sort: progress.max_number_sort,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TachiyomiReadListProgressDto {
    pub books_count: u64,
    pub books_read_count: u64,
    pub books_unread_count: u64,
    pub books_in_progress_count: u64,
    pub last_read_continuous_index: u64,
}

impl From<ReadlistTachiyomiCounters> for TachiyomiReadListProgressDto {
    fn from(counters: ReadlistTachiyomiCounters) -> Self {
        Self {
            books_count: counters.books_count,
            books_read_count: counters.books_read_count,
            books_unread_count: counters.books_unread_count,
            books_in_progress_count: counters.books_in_progress_count,
            last_read_continuous_index: counters.last_read_continuous_index,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookProgressionDeviceDto {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookProgressionDto {
    pub modified: String,
    pub device: BookProgressionDeviceDto,
    pub locator: Value,
}

impl From<BookProgressionRecord> for BookProgressionDto {
    fn from(progression: BookProgressionRecord) -> Self {
        Self {
            modified: progression.modified,
            device: BookProgressionDeviceDto {
                id: progression.device_id,
                name: progression.device_name,
            },
            locator: progression.locator,
        }
    }
}
