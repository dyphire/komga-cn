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

fn sort_property(candidate: &str) -> &str {
    candidate.split(',').next().unwrap_or(candidate).trim()
}
