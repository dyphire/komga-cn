use anyhow::{Context, Result};
use komga_application::discovery::{
    BookMetadataAuthorReadModel, BookMetadataLinkReadModel, BookReadModel,
    BookReadProgressReadModel, CollectionReadModel, ComicRackReadListMatchResult,
    ReadListReadModel, SeriesAlphabeticalGroup, SeriesAlternateTitleRecord,
    SeriesMetadataLinkRecord, SeriesReadModel,
};
use komga_domain::discovery::MediaProfile;
use serde::Serialize;

use super::common::{KotlinLocalDate, KotlinUtcDateTime};
use crate::discovery::detail::SeriesDetailReadModel;
use crate::helpers::{api_file_path, restricted_book_url};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionDto {
    pub id: String,
    pub name: String,
    pub ordered: bool,
    pub series_ids: Vec<String>,
    pub created_date: KotlinUtcDateTime,
    pub last_modified_date: KotlinUtcDateTime,
    pub filtered: bool,
}

impl CollectionDto {
    pub fn from_read_model(collection: &CollectionReadModel) -> Result<Self> {
        Ok(Self {
            id: collection.id.clone(),
            name: collection.name.clone(),
            ordered: collection.ordered,
            series_ids: collection.series_ids.clone(),
            created_date: parse_datetime("collection.createdDate", &collection.created_date)?,
            last_modified_date: parse_datetime(
                "collection.lastModifiedDate",
                &collection.last_modified_date,
            )?,
            filtered: collection.filtered,
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadListDto {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub ordered: bool,
    pub book_ids: Vec<String>,
    pub created_date: KotlinUtcDateTime,
    pub last_modified_date: KotlinUtcDateTime,
    pub filtered: bool,
}

impl ReadListDto {
    pub fn from_read_model(readlist: &ReadListReadModel) -> Result<Self> {
        Ok(Self {
            id: readlist.id.clone(),
            name: readlist.name.clone(),
            summary: readlist.summary.clone(),
            ordered: readlist.ordered,
            book_ids: readlist.book_ids.clone(),
            created_date: parse_datetime("readList.createdDate", &readlist.created_date)?,
            last_modified_date: parse_datetime(
                "readList.lastModifiedDate",
                &readlist.last_modified_date,
            )?,
            filtered: readlist.filtered,
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadListRequestMatchDto {
    pub read_list_match: ReadListMatchDto,
    pub requests: Vec<ReadListRequestBookMatchesDto>,
    pub error_code: String,
}

impl ReadListRequestMatchDto {
    pub fn from_result(result: &ComicRackReadListMatchResult) -> Result<Self> {
        Ok(Self {
            read_list_match: ReadListMatchDto {
                name: result.name.clone(),
                error_code: result
                    .error
                    .map(|error| error.error_code().to_string())
                    .unwrap_or_default(),
            },
            requests: result
                .requests
                .iter()
                .map(ReadListRequestBookMatchesDto::from_result)
                .collect::<Result<Vec<_>>>()?,
            error_code: String::new(),
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadListMatchDto {
    pub name: String,
    pub error_code: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadListRequestBookMatchesDto {
    pub request: ReadListRequestBookDto,
    pub matches: Vec<ReadListRequestBookMatchDto>,
}

impl ReadListRequestBookMatchesDto {
    fn from_result(
        request: &komga_application::discovery::ComicRackReadListRequestMatch,
    ) -> Result<Self> {
        Ok(Self {
            request: ReadListRequestBookDto {
                series: request.request.series_candidates.clone(),
                number: request.request.number.clone(),
            },
            matches: request
                .matches
                .iter()
                .map(ReadListRequestBookMatchDto::from_result)
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadListRequestBookDto {
    pub series: Vec<String>,
    pub number: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadListRequestBookMatchDto {
    pub series: ReadListRequestBookMatchSeriesDto,
    pub books: Vec<ReadListRequestBookMatchBookDto>,
}

impl ReadListRequestBookMatchDto {
    fn from_result(
        group: &komga_application::discovery::ComicRackReadListMatchGroup,
    ) -> Result<Self> {
        Ok(Self {
            series: ReadListRequestBookMatchSeriesDto {
                series_id: group.series.series_id.clone(),
                title: group.series.title.clone(),
                release_date: group
                    .series
                    .release_date
                    .as_deref()
                    .map(KotlinLocalDate::parse)
                    .transpose()
                    .context("readListMatch.series.releaseDate")?,
            },
            books: group
                .books
                .iter()
                .map(|book| ReadListRequestBookMatchBookDto {
                    book_id: book.book_id.clone(),
                    number: book.number.clone(),
                    title: book.title.clone(),
                })
                .collect(),
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadListRequestBookMatchSeriesDto {
    pub series_id: String,
    pub title: String,
    pub release_date: Option<KotlinLocalDate>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadListRequestBookMatchBookDto {
    pub book_id: String,
    pub number: String,
    pub title: String,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum FacetValueDto {
    String(String),
    Integer(i64),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesAlphabeticalGroupDto {
    pub group: String,
    pub count: i64,
}

impl From<SeriesAlphabeticalGroup> for SeriesAlphabeticalGroupDto {
    fn from(group: SeriesAlphabeticalGroup) -> Self {
        Self {
            group: group.group,
            count: group.count,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesDto {
    pub id: String,
    pub library_id: String,
    pub name: String,
    pub url: String,
    pub created: KotlinUtcDateTime,
    pub last_modified: KotlinUtcDateTime,
    pub file_last_modified: KotlinUtcDateTime,
    pub books_count: u64,
    pub books_read_count: u64,
    pub books_unread_count: u64,
    pub books_in_progress_count: u64,
    pub metadata: SeriesMetadataDto,
    pub books_metadata: BookMetadataAggregationDto,
    pub deleted: bool,
    pub oneshot: bool,
}

impl SeriesDto {
    pub fn from_read_model(series: &SeriesReadModel, is_admin: bool) -> Result<Self> {
        Ok(Self {
            id: series.id.clone(),
            library_id: series.library_id.clone(),
            name: series.name.clone(),
            url: if is_admin {
                api_file_path(&series.url)
            } else {
                String::new()
            },
            created: parse_datetime("series.created", &series.created)?,
            last_modified: parse_datetime("series.lastModified", &series.last_modified)?,
            file_last_modified: parse_datetime(
                "series.fileLastModified",
                &series.file_last_modified,
            )?,
            books_count: series.books_count,
            books_read_count: series.books_read_count,
            books_unread_count: series.books_unread_count,
            books_in_progress_count: series.books_in_progress_count,
            metadata: SeriesMetadataDto::from_read_model(series)?,
            books_metadata: BookMetadataAggregationDto::from_read_model(series)?,
            deleted: series.deleted,
            oneshot: series.oneshot,
        })
    }

    pub(crate) fn from_detail(series: &SeriesDetailReadModel, is_admin: bool) -> Result<Self> {
        Ok(Self {
            id: series.id.clone(),
            library_id: series.library_id.clone(),
            name: series.name.clone(),
            url: if is_admin {
                api_file_path(&series.url)
            } else {
                String::new()
            },
            created: parse_datetime("series.created", &series.created)?,
            last_modified: parse_datetime("series.lastModified", &series.last_modified)?,
            file_last_modified: parse_datetime(
                "series.fileLastModified",
                &series.file_last_modified,
            )?,
            books_count: u64::from(series.books_count),
            books_read_count: u64::from(series.books_read_count),
            books_unread_count: u64::from(series.books_unread_count),
            books_in_progress_count: u64::from(series.books_in_progress_count),
            metadata: SeriesMetadataDto::from_detail(series)?,
            books_metadata: BookMetadataAggregationDto::from_detail(series)?,
            deleted: series.deleted,
            oneshot: series.oneshot,
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesMetadataDto {
    pub status: String,
    pub status_lock: bool,
    pub title: String,
    pub title_lock: bool,
    pub title_sort: String,
    pub title_sort_lock: bool,
    pub summary: String,
    pub summary_lock: bool,
    pub reading_direction: String,
    pub reading_direction_lock: bool,
    pub publisher: String,
    pub publisher_lock: bool,
    pub age_rating: Option<u32>,
    pub age_rating_lock: bool,
    pub language: String,
    pub language_lock: bool,
    pub genres: Vec<String>,
    pub genres_lock: bool,
    pub tags: Vec<String>,
    pub tags_lock: bool,
    pub total_book_count: Option<u64>,
    pub total_book_count_lock: bool,
    pub sharing_labels: Vec<String>,
    pub sharing_labels_lock: bool,
    pub links: Vec<WebLinkDto>,
    pub links_lock: bool,
    pub alternate_titles: Vec<AlternateTitleDto>,
    pub alternate_titles_lock: bool,
    pub created: KotlinUtcDateTime,
    pub last_modified: KotlinUtcDateTime,
}

impl SeriesMetadataDto {
    fn from_read_model(series: &SeriesReadModel) -> Result<Self> {
        Ok(Self {
            status: series.status.persisted_name().to_string(),
            status_lock: series.status_lock,
            title: series.title.clone(),
            title_lock: series.title_lock,
            title_sort: series.title_sort.clone(),
            title_sort_lock: series.title_sort_lock,
            summary: series.summary.clone(),
            summary_lock: series.summary_lock,
            reading_direction: series
                .reading_direction
                .map(|value| value.persisted_name().to_string())
                .unwrap_or_default(),
            reading_direction_lock: series.reading_direction_lock,
            publisher: series.publisher.clone(),
            publisher_lock: series.publisher_lock,
            age_rating: series.age_rating,
            age_rating_lock: series.age_rating_lock,
            language: series.language.clone(),
            language_lock: series.language_lock,
            genres: series.genres.clone(),
            genres_lock: series.genres_lock,
            tags: series.tags.clone(),
            tags_lock: series.tags_lock,
            total_book_count: series.total_book_count.map(u64::from),
            total_book_count_lock: series.total_book_count_lock,
            sharing_labels: series.labels.clone(),
            sharing_labels_lock: series.sharing_labels_lock,
            links: series
                .links
                .iter()
                .map(WebLinkDto::from_series_record)
                .collect(),
            links_lock: series.links_lock,
            alternate_titles: series
                .alternate_titles
                .iter()
                .map(AlternateTitleDto::from_series_record)
                .collect(),
            alternate_titles_lock: series.alternate_titles_lock,
            created: parse_datetime("series.metadata.created", &series.metadata_created)?,
            last_modified: parse_datetime(
                "series.metadata.lastModified",
                &series.metadata_last_modified,
            )?,
        })
    }

    fn from_detail(series: &SeriesDetailReadModel) -> Result<Self> {
        Ok(Self {
            status: series.status.persisted_name().to_string(),
            status_lock: series.status_lock,
            title: series.title.clone(),
            title_lock: series.title_lock,
            title_sort: series.title_sort.clone(),
            title_sort_lock: series.title_sort_lock,
            summary: series.summary.clone(),
            summary_lock: series.summary_lock,
            reading_direction: series
                .reading_direction
                .map(|value| value.persisted_name().to_string())
                .unwrap_or_default(),
            reading_direction_lock: series.reading_direction_lock,
            publisher: series.publisher.clone(),
            publisher_lock: series.publisher_lock,
            age_rating: series.age_rating,
            age_rating_lock: series.age_rating_lock,
            language: series.language.clone(),
            language_lock: series.language_lock,
            genres: series.genres.clone(),
            genres_lock: series.genres_lock,
            tags: series.tags.clone(),
            tags_lock: series.tags_lock,
            total_book_count: series.total_book_count.map(u64::from),
            total_book_count_lock: series.total_book_count_lock,
            sharing_labels: series.sharing_labels.clone(),
            sharing_labels_lock: series.sharing_labels_lock,
            links: series
                .links
                .iter()
                .map(WebLinkDto::from_series_record)
                .collect(),
            links_lock: series.links_lock,
            alternate_titles: series
                .alternate_titles
                .iter()
                .map(AlternateTitleDto::from_series_record)
                .collect(),
            alternate_titles_lock: series.alternate_titles_lock,
            created: parse_datetime("series.metadata.created", &series.metadata_created)?,
            last_modified: parse_datetime(
                "series.metadata.lastModified",
                &series.metadata_last_modified,
            )?,
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookMetadataAggregationDto {
    pub authors: Vec<AuthorDto>,
    pub tags: Vec<String>,
    pub release_date: Option<KotlinLocalDate>,
    pub summary: String,
    pub summary_number: String,
    pub created: KotlinUtcDateTime,
    pub last_modified: KotlinUtcDateTime,
}

impl BookMetadataAggregationDto {
    fn from_read_model(series: &SeriesReadModel) -> Result<Self> {
        Ok(Self {
            authors: series
                .books_metadata_authors
                .iter()
                .map(|value| AuthorDto::from_persisted(value))
                .collect(),
            tags: series.books_metadata_tags.clone(),
            release_date: series
                .books_metadata_release_date
                .as_deref()
                .map(KotlinLocalDate::parse)
                .transpose()
                .context("series.booksMetadata.releaseDate")?,
            summary: series.books_metadata_summary.clone(),
            summary_number: series.books_metadata_summary_number.clone(),
            created: parse_datetime(
                "series.booksMetadata.created",
                &series.books_metadata_created,
            )?,
            last_modified: parse_datetime(
                "series.booksMetadata.lastModified",
                &series.books_metadata_last_modified,
            )?,
        })
    }

    fn from_detail(series: &SeriesDetailReadModel) -> Result<Self> {
        Ok(Self {
            authors: series
                .books_metadata_authors
                .iter()
                .map(AuthorDto::from_read_model)
                .collect(),
            tags: series.books_metadata_tags.clone(),
            release_date: series
                .books_metadata_release_date
                .as_deref()
                .map(KotlinLocalDate::parse)
                .transpose()
                .context("series.booksMetadata.releaseDate")?,
            summary: series.books_metadata_summary.clone(),
            summary_number: series.books_metadata_summary_number.clone(),
            created: parse_datetime(
                "series.booksMetadata.created",
                &series.books_metadata_created,
            )?,
            last_modified: parse_datetime(
                "series.booksMetadata.lastModified",
                &series.books_metadata_last_modified,
            )?,
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlternateTitleDto {
    pub label: String,
    pub title: String,
}

impl AlternateTitleDto {
    fn from_series_record(value: &SeriesAlternateTitleRecord) -> Self {
        Self {
            label: value.label.clone(),
            title: value.title.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookDto {
    pub id: String,
    pub series_id: String,
    pub series_title: String,
    pub library_id: String,
    pub name: String,
    pub url: String,
    pub number: i32,
    pub created: KotlinUtcDateTime,
    pub last_modified: KotlinUtcDateTime,
    pub file_last_modified: KotlinUtcDateTime,
    pub size_bytes: u64,
    pub size: String,
    pub media: MediaDto,
    pub metadata: BookMetadataDto,
    pub read_progress: Option<ReadProgressDto>,
    pub deleted: bool,
    pub file_hash: String,
    pub oneshot: bool,
}

impl BookDto {
    pub fn from_read_model(book: &BookReadModel, is_admin: bool) -> Result<Self> {
        let url = restricted_book_url(&api_file_path(&book.url), is_admin);

        Ok(Self {
            id: book.id.clone(),
            series_id: book.series_id.clone(),
            series_title: book.series_title.clone(),
            library_id: book.library_id.clone(),
            name: book.name.clone(),
            url,
            number: book.number,
            created: parse_datetime("book.created", &book.created)?,
            last_modified: parse_datetime("book.lastModified", &book.last_modified)?,
            file_last_modified: parse_datetime("book.fileLastModified", &book.file_last_modified)?,
            size: format_size_bytes(book.size_bytes),
            size_bytes: book.size_bytes,
            media: MediaDto::from_read_model(book),
            metadata: BookMetadataDto::from_read_model(book)?,
            read_progress: book
                .read_progress
                .as_ref()
                .map(ReadProgressDto::from_read_model)
                .transpose()?,
            deleted: book.deleted,
            file_hash: book.file_hash.clone(),
            oneshot: book.oneshot,
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaDto {
    pub status: String,
    pub media_type: String,
    pub pages_count: u32,
    pub comment: String,
    pub epub_divina_compatible: bool,
    pub epub_is_kepub: bool,
    pub media_profile: String,
}

impl MediaDto {
    fn from_read_model(book: &BookReadModel) -> Self {
        Self {
            status: book.media_status.persisted_name().to_string(),
            media_type: book.media_type.clone(),
            pages_count: book.media_pages_count,
            comment: book.media_comment.clone(),
            epub_divina_compatible: book.media_epub_divina_compatible,
            epub_is_kepub: book.media_epub_is_kepub,
            media_profile: MediaProfile::from_media_type(&book.media_type)
                .map(MediaProfile::api_name)
                .unwrap_or_default()
                .to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookMetadataDto {
    pub title: String,
    pub title_lock: bool,
    pub summary: String,
    pub summary_lock: bool,
    pub number: String,
    pub number_lock: bool,
    pub number_sort: f64,
    pub number_sort_lock: bool,
    pub release_date: Option<KotlinLocalDate>,
    pub release_date_lock: bool,
    pub authors: Vec<AuthorDto>,
    pub authors_lock: bool,
    pub tags: Vec<String>,
    pub tags_lock: bool,
    pub isbn: String,
    pub isbn_lock: bool,
    pub links: Vec<WebLinkDto>,
    pub links_lock: bool,
    pub created: KotlinUtcDateTime,
    pub last_modified: KotlinUtcDateTime,
}

impl BookMetadataDto {
    fn from_read_model(book: &BookReadModel) -> Result<Self> {
        Ok(Self {
            title: book.metadata_title.clone(),
            title_lock: book.metadata_title_lock,
            summary: book.metadata_summary.clone(),
            summary_lock: book.metadata_summary_lock,
            number: book.metadata_number.clone(),
            number_lock: book.metadata_number_lock,
            number_sort: book.metadata_number_sort,
            number_sort_lock: book.metadata_number_sort_lock,
            release_date: book
                .metadata_release_date
                .as_deref()
                .map(KotlinLocalDate::parse)
                .transpose()
                .context("book.metadata.releaseDate")?,
            release_date_lock: book.metadata_release_date_lock,
            authors: book
                .metadata_authors
                .iter()
                .map(AuthorDto::from_read_model)
                .collect(),
            authors_lock: book.metadata_authors_lock,
            tags: book.metadata_tags.clone(),
            tags_lock: book.metadata_tags_lock,
            isbn: book.metadata_isbn.clone(),
            isbn_lock: book.metadata_isbn_lock,
            links: book
                .metadata_links
                .iter()
                .map(WebLinkDto::from_read_model)
                .collect(),
            links_lock: book.metadata_links_lock,
            created: parse_datetime("book.metadata.created", &book.metadata_created)?,
            last_modified: parse_datetime(
                "book.metadata.lastModified",
                &book.metadata_last_modified,
            )?,
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorDto {
    pub name: String,
    pub role: String,
}

impl AuthorDto {
    fn from_read_model(author: &BookMetadataAuthorReadModel) -> Self {
        Self {
            name: author.name.clone(),
            role: author.role.clone(),
        }
    }

    fn from_persisted(value: &str) -> Self {
        let (name, role) = value
            .split_once("::")
            .map_or((value, ""), |(name, role)| (name, role));

        Self {
            name: name.to_string(),
            role: role.to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebLinkDto {
    pub label: String,
    pub url: String,
}

impl WebLinkDto {
    fn from_read_model(link: &BookMetadataLinkReadModel) -> Self {
        Self {
            label: link.label.clone(),
            url: link.url.clone(),
        }
    }

    fn from_series_record(link: &SeriesMetadataLinkRecord) -> Self {
        Self {
            label: link.label.clone(),
            url: link.url.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadProgressDto {
    pub page: i32,
    pub completed: bool,
    pub read_date: KotlinUtcDateTime,
    pub created: KotlinUtcDateTime,
    pub last_modified: KotlinUtcDateTime,
    pub device_id: String,
    pub device_name: String,
}

impl ReadProgressDto {
    fn from_read_model(progress: &BookReadProgressReadModel) -> Result<Self> {
        let read_date = progress
            .read_date
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                (!progress.last_modified.trim().is_empty())
                    .then_some(progress.last_modified.as_str())
            })
            .unwrap_or(progress.created.as_str());

        Ok(Self {
            page: progress.page,
            completed: progress.completed,
            read_date: parse_datetime("book.readProgress.readDate", read_date)?,
            created: parse_datetime("book.readProgress.created", &progress.created)?,
            last_modified: parse_datetime(
                "book.readProgress.lastModified",
                &progress.last_modified,
            )?,
            device_id: progress.device_id.clone(),
            device_name: progress.device_name.clone(),
        })
    }
}

fn parse_datetime(field: &str, raw: &str) -> Result<KotlinUtcDateTime> {
    KotlinUtcDateTime::parse(raw).with_context(|| format!("invalid {field}: {raw}"))
}

pub fn format_size_bytes(size_bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];

    if size_bytes < 1024 {
        return format!("{size_bytes} B");
    }

    let mut size = size_bytes as f64;
    let mut unit_index = 0usize;
    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if (size - size.round()).abs() < 0.05 {
        format!("{} {}", size.round() as u64, UNITS[unit_index])
    } else {
        format!("{size:.1} {}", UNITS[unit_index])
    }
}
