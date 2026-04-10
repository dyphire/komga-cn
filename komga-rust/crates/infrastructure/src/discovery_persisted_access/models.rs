#[derive(Clone)]
pub struct AuthorEntry {
    pub name: String,
    pub role: String,
}

#[derive(Clone)]
pub struct WebLinkEntry {
    pub label: String,
    pub url: String,
}

pub enum AuthorsScope {
    All,
    Libraries(Vec<String>),
    Collection(String),
    Series(String),
    ReadList(String),
}

#[derive(Clone)]
pub struct BookBrowseEntry {
    pub id: String,
    pub library_id: String,
    pub name: String,
    pub title: String,
}

pub enum BookTagsScope {
    All,
    Series(String),
    Libraries(Vec<String>),
    ReadList(String),
}

#[derive(Clone)]
pub struct BookPosterSummary {
    pub thumbnail_type: String,
    pub selected: bool,
}

#[derive(Clone)]
pub struct BookSummary {
    pub id: String,
    pub series_id: String,
    pub library_id: String,
    pub series_title: String,
    pub title: String,
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
    pub media_epub_divina_compatible: bool,
    pub media_epub_is_kepub: bool,
    pub read_status: String,
    pub metadata_title_lock: bool,
    pub metadata_summary: String,
    pub metadata_summary_lock: bool,
    pub metadata_number: String,
    pub metadata_number_lock: bool,
    pub metadata_number_sort: f64,
    pub metadata_number_sort_lock: bool,
    pub metadata_release_date: Option<String>,
    pub metadata_release_date_lock: bool,
    pub metadata_authors_lock: bool,
    pub metadata_tags_lock: bool,
    pub metadata_isbn: String,
    pub metadata_isbn_lock: bool,
    pub metadata_links_lock: bool,
    pub metadata_created: String,
    pub metadata_last_modified: String,
    pub file_hash: String,
    pub read_progress: Option<ReadProgressSummary>,
    pub deleted: bool,
    pub oneshot: bool,
    pub genres: Vec<String>,
    pub language: Option<String>,
    pub publisher: Option<String>,
    pub age_rating: Option<u16>,
    pub metadata_tags: Vec<String>,
    pub metadata_authors: Vec<AuthorEntry>,
    pub metadata_links: Vec<WebLinkEntry>,
}

#[derive(Clone)]
pub struct ReadProgressSummary {
    pub page: i32,
    pub completed: bool,
    pub read_date: Option<String>,
    pub created: String,
    pub last_modified: String,
    pub device_id: String,
    pub device_name: String,
}

#[derive(Clone)]
pub struct SeriesSummary {
    pub id: String,
    pub library_id: String,
    pub title: String,
    pub title_sort: String,
    pub labels: Vec<String>,
    pub created: String,
    pub last_modified: String,
    pub file_last_modified: String,
    pub books_count: u64,
    pub books_read_count: u64,
    pub books_unread_count: u64,
    pub books_in_progress_count: u64,
    pub status: String,
    pub summary: String,
    pub reading_direction: String,
    pub publisher: String,
    pub age_rating: Option<u16>,
    pub language: String,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
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
