#[derive(Clone)]
pub struct PersistedBookResourceRecord {
    pub library_id: String,
    pub age_rating: Option<u16>,
    pub sharing_labels: String,
}

#[derive(Clone)]
pub struct PersistedBookDetailRecord {
    pub id: String,
    pub series_id: String,
    pub series_title: String,
    pub series_title_sort: String,
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
    pub metadata_title_lock: bool,
    pub metadata_summary_lock: bool,
    pub metadata_number_lock: bool,
    pub metadata_number_sort_lock: bool,
    pub metadata_release_date_lock: bool,
    pub metadata_authors: String,
    pub metadata_authors_lock: bool,
    pub metadata_tags: String,
    pub metadata_tags_lock: bool,
    pub metadata_isbn: String,
    pub metadata_isbn_lock: bool,
    pub metadata_links: String,
    pub metadata_links_lock: bool,
    pub metadata_created: String,
    pub metadata_last_modified: String,
    pub media_epub_divina_compatible: bool,
    pub media_epub_is_kepub: bool,
    pub read_progress: Option<PersistedReadProgressRecord>,
    pub deleted: bool,
    pub file_hash: String,
    pub oneshot: bool,
}

#[derive(Clone)]
pub struct PersistedReadProgressRecord {
    pub page: i32,
    pub completed: bool,
    pub read_date: Option<String>,
    pub created: String,
    pub last_modified: String,
    pub device_id: String,
    pub device_name: String,
}

#[derive(Clone, Copy)]
pub enum PersistedBookSiblingDirectionRecord {
    Previous,
    Next,
}
