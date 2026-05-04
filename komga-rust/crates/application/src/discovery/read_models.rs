#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LibraryReadModel {
    pub id: String,
    pub name: String,
    pub root: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeriesReadModel {
    pub id: String,
    pub name: String,
    pub title: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BookMetadataAuthorReadModel {
    pub name: String,
    pub role: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BookMetadataLinkReadModel {
    pub label: String,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BookReadProgressReadModel {
    pub page: i32,
    pub completed: bool,
    pub read_date: Option<String>,
    pub created: String,
    pub last_modified: String,
    pub device_id: String,
    pub device_name: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BookReadModel {
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
    pub media_epub_divina_compatible: bool,
    pub media_epub_is_kepub: bool,
    pub metadata_title: String,
    pub metadata_title_lock: bool,
    pub metadata_summary: String,
    pub metadata_summary_lock: bool,
    pub metadata_number: String,
    pub metadata_number_lock: bool,
    pub metadata_number_sort: f64,
    pub metadata_number_sort_lock: bool,
    pub metadata_release_date: Option<String>,
    pub metadata_release_date_lock: bool,
    pub metadata_authors: Vec<BookMetadataAuthorReadModel>,
    pub metadata_authors_lock: bool,
    pub metadata_tags: Vec<String>,
    pub metadata_tags_lock: bool,
    pub metadata_isbn: String,
    pub metadata_isbn_lock: bool,
    pub metadata_links: Vec<BookMetadataLinkReadModel>,
    pub metadata_links_lock: bool,
    pub metadata_created: String,
    pub metadata_last_modified: String,
    pub read_progress: Option<BookReadProgressReadModel>,
    pub deleted: bool,
    pub file_hash: String,
    pub oneshot: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadListReadModel {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollectionReadModel {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeriesDetailReadModel {
    pub id: String,
    pub title: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookDetailReadModel {
    pub id: String,
    pub series_id: String,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeriesResourceReadModel {
    pub id: String,
    pub url: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookResourceReadModel {
    pub id: String,
    pub url: String,
}
