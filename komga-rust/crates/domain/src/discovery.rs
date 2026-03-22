#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgeRestrictionKind {
    AllowOnly,
    Exclude,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryRestrictions {
    pub age: Option<u16>,
    pub age_restriction: Option<AgeRestrictionKind>,
    pub labels_allow: Vec<String>,
    pub labels_exclude: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveryQueryContext {
    pub user_id: Option<String>,
    pub is_admin: bool,
    pub authorized_library_ids: Option<Vec<String>>,
    pub restrictions: Option<QueryRestrictions>,
}

impl DiscoveryQueryContext {
    pub fn allow_all() -> Self {
        Self {
            user_id: None,
            is_admin: true,
            authorized_library_ids: None,
            restrictions: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LibraryReadModel {
    pub id: String,
    pub name: String,
    pub root: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeriesReadModel {
    pub id: String,
    pub library_id: String,
    pub title: String,
    pub labels: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeriesResourceReadModel {
    pub id: String,
    pub library_id: String,
    pub age_rating: Option<u16>,
    pub labels: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeriesDetailReadModel {
    pub id: String,
    pub library_id: String,
    pub title: String,
    pub url: String,
    pub created: String,
    pub last_modified: String,
    pub file_last_modified: String,
    pub books_count: u32,
    pub books_read_count: u32,
    pub books_unread_count: u32,
    pub books_in_progress_count: u32,
    pub status: String,
    pub summary: String,
    pub reading_direction: String,
    pub publisher: String,
    pub age_rating: Option<u16>,
    pub language: String,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
    pub total_book_count: Option<u32>,
    pub sharing_labels: Vec<String>,
    pub alternate_titles: Vec<String>,
    pub metadata_created: String,
    pub metadata_last_modified: String,
    pub books_metadata_authors: Vec<String>,
    pub books_metadata_tags: Vec<String>,
    pub books_metadata_release_date: Option<String>,
    pub books_metadata_summary: String,
    pub books_metadata_summary_number: String,
    pub books_metadata_created: String,
    pub books_metadata_last_modified: String,
    pub deleted: bool,
    pub oneshot: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollectionReadModel {
    pub id: String,
    pub name: String,
    pub ordered: bool,
    pub series_ids: Vec<String>,
    pub created_date: String,
    pub last_modified_date: String,
    pub filtered: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookReadModel {
    pub id: String,
    pub series_id: String,
    pub series_title: String,
    pub library_id: String,
    pub title: String,
    pub url: String,
    pub created: String,
    pub last_modified: String,
    pub file_last_modified: String,
    pub size_bytes: u64,
    pub media_status: String,
    pub media_type: String,
    pub media_pages_count: u32,
    pub metadata_release_date: Option<String>,
    pub deleted: bool,
    pub oneshot: bool,
    pub labels: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadProgressReadModel {
    pub page: u32,
    pub completed: bool,
    pub read_date: String,
    pub created: String,
    pub last_modified: String,
    pub device_id: String,
    pub device_name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookResourceReadModel {
    pub id: String,
    pub library_id: String,
    pub age_rating: Option<u16>,
    pub labels: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BookDetailReadModel {
    pub id: String,
    pub series_id: String,
    pub series_title: String,
    pub library_id: String,
    pub name: String,
    pub url: String,
    pub number: i32,
    pub created: String,
    pub last_modified: String,
    pub file_last_modified: String,
    pub size_bytes: u64,
    pub media_status: String,
    pub media_type: String,
    pub media_pages_count: u32,
    pub media_comment: String,
    pub metadata_title: String,
    pub metadata_summary: String,
    pub metadata_number: String,
    pub metadata_number_sort: f64,
    pub metadata_release_date: Option<String>,
    pub metadata_authors: Vec<String>,
    pub metadata_tags: Vec<String>,
    pub metadata_isbn: String,
    pub metadata_created: String,
    pub metadata_last_modified: String,
    pub read_progress: Option<ReadProgressReadModel>,
    pub deleted: bool,
    pub file_hash: String,
    pub oneshot: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadListReadModel {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub ordered: bool,
    pub book_ids: Vec<String>,
    pub created_date: String,
    pub last_modified_date: String,
    pub filtered: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PageEnvelope<T> {
    pub content: Vec<T>,
    pub page: usize,
    pub size: usize,
    pub total_elements: usize,
    pub total_pages: usize,
}

impl<T> PageEnvelope<T> {
    pub fn from_slice(content: Vec<T>, page: usize, size: usize, total_elements: usize) -> Self {
        let safe_size = size.max(1);
        let total_pages = if total_elements == 0 {
            0
        } else {
            ((total_elements - 1) / safe_size) + 1
        };
        Self {
            content,
            page,
            size,
            total_elements,
            total_pages,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SeriesSort {
    MetadataTitleSort,
    CreatedDate,
    LastModifiedDate,
    BooksMetadataReleaseDate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BookSort {
    MetadataTitle,
    CreatedDate,
    LastModifiedDate,
    MetadataReleaseDate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DirectBrowseBooksListFamily {
    BrowseSeriesPaged,
    BrowseBookSiblingsUnpaged,
    BrowseOneshotBootstrap,
}

pub const SUPPORTED_SERIES_CONDITION_TYPES: &[&str] = &[
    "LibraryId",
    "AnyOfSeries",
    "AllOfSeries",
    "OneShot",
    "Deleted",
    "ReadStatus",
    "Genre",
    "Tag",
    "Language",
    "Publisher",
    "AgeRating",
    "ReleaseDate",
    "SharingLabel",
    "SeriesStatus",
    "Complete",
    "Author",
];

pub const SUPPORTED_BOOK_CONDITION_TYPES: &[&str] = &[
    "SeriesId",
    "LibraryId",
    "AnyOfBook",
    "AllOfBook",
    "OneShot",
    "Deleted",
    "ReadStatus",
    "Tag",
    "MediaProfile",
    "MediaStatus",
    "Author",
    "ReleaseDate",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NonNativeRequestShape {
    UnsupportedSeriesSort(String),
    UnsupportedSeriesFilter(String),
    UnsupportedBookSort(String),
    UnsupportedBookFilter(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiscoveryError {
    NonNativeRequestShape(NonNativeRequestShape),
    InvalidRequest(String),
    Persistence(String),
}

pub fn classify_series_sorts(raw: &[String]) -> Result<Vec<SeriesSort>, DiscoveryError> {
    raw.iter()
        .map(|candidate| {
            let property = sort_property(candidate);
            match property {
                "metadata.titleSort" => Ok(SeriesSort::MetadataTitleSort),
                "createdDate" => Ok(SeriesSort::CreatedDate),
                "lastModifiedDate" => Ok(SeriesSort::LastModifiedDate),
                "booksMetadata.releaseDate" => Ok(SeriesSort::BooksMetadataReleaseDate),
                unsupported => Err(DiscoveryError::NonNativeRequestShape(
                    NonNativeRequestShape::UnsupportedSeriesSort(unsupported.to_string()),
                )),
            }
        })
        .collect()
}

pub fn classify_book_sorts(raw: &[String]) -> Result<Vec<BookSort>, DiscoveryError> {
    raw.iter()
        .map(|candidate| {
            let property = sort_property(candidate);
            match property {
                "metadata.title" => Ok(BookSort::MetadataTitle),
                "createdDate" => Ok(BookSort::CreatedDate),
                "lastModifiedDate" => Ok(BookSort::LastModifiedDate),
                "metadata.releaseDate" => Ok(BookSort::MetadataReleaseDate),
                unsupported => Err(DiscoveryError::NonNativeRequestShape(
                    NonNativeRequestShape::UnsupportedBookSort(unsupported.to_string()),
                )),
            }
        })
        .collect()
}

pub fn classify_direct_browse_books_list_sort(raw: &[String]) -> Result<(), DiscoveryError> {
    if raw.len() != 1 {
        return Err(DiscoveryError::NonNativeRequestShape(
            NonNativeRequestShape::UnsupportedBookSort(raw.first().cloned().unwrap_or_default()),
        ));
    }

    let sort = raw.first().cloned().unwrap_or_default();
    let (property, order) = split_sort_candidate(&sort);
    if property != "metadata.numberSort" {
        return Err(DiscoveryError::NonNativeRequestShape(
            NonNativeRequestShape::UnsupportedBookSort(sort),
        ));
    }

    if let Some(order) = order
        && !order.eq_ignore_ascii_case("asc")
    {
        return Err(DiscoveryError::NonNativeRequestShape(
            NonNativeRequestShape::UnsupportedBookSort(raw[0].clone()),
        ));
    }

    Ok(())
}

fn sort_property(candidate: &str) -> &str {
    candidate.split(',').next().unwrap_or(candidate).trim()
}

fn split_sort_candidate(candidate: &str) -> (&str, Option<&str>) {
    let mut parts = candidate.splitn(2, ',').map(str::trim);
    let property = parts.next().unwrap_or_default();
    let order = parts.next();
    (property, order)
}
