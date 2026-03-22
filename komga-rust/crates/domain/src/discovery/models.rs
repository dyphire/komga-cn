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
