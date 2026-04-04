#[derive(Clone)]
pub struct AuthorEntry {
    pub name: String,
    pub role: String,
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
    pub title: String,
    pub created: String,
    pub last_modified: String,
    pub media_status: String,
    pub media_type: String,
    pub read_status: String,
    pub metadata_number_sort: Option<f64>,
    pub metadata_release_date: Option<String>,
    pub deleted: bool,
    pub oneshot: bool,
    pub genres: Vec<String>,
    pub language: Option<String>,
    pub publisher: Option<String>,
    pub age_rating: Option<u16>,
    pub metadata_tags: Vec<String>,
    pub metadata_authors: Vec<String>,
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
