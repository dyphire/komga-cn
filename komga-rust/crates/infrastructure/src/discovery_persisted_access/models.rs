use komga_application::discovery::{SeriesAlternateTitleRecord, SeriesMetadataLinkRecord};
use komga_domain::discovery::{MediaStatus, ReadStatus};
use komga_domain::media_assets::ThumbnailType;

#[derive(Clone)]
pub(super) struct AuthorEntry {
    pub(super) name: String,
    pub(super) role: String,
}

#[derive(Clone)]
pub(super) struct WebLinkEntry {
    pub(super) label: String,
    pub(super) url: String,
}

pub(super) enum AuthorsScope {
    All,
    Libraries(Vec<String>),
    Collections(Vec<String>),
    Series(Vec<String>),
    ReadLists(Vec<String>),
}

#[derive(Clone)]
pub(super) struct BookBrowseEntry {
    pub(super) id: String,
    pub(super) library_id: String,
    pub(super) name: String,
    pub(super) title: String,
}

pub(super) enum BookTagsScope {
    All,
    Series(String),
    Libraries(Vec<String>),
    ReadList(String),
}

#[derive(Clone)]
pub(super) struct BookPosterSummary {
    pub(super) thumbnail_type: ThumbnailType,
    pub(super) selected: bool,
}

#[derive(Clone)]
pub(super) struct BookSummary {
    pub(super) id: String,
    pub(super) series_id: String,
    pub(super) library_id: String,
    pub(super) series_title: String,
    pub(super) series_title_sort: String,
    pub(super) title: String,
    pub(super) name: String,
    pub(super) url: String,
    pub(super) number: i32,
    pub(super) created: String,
    pub(super) last_modified: String,
    pub(super) file_last_modified: String,
    pub(super) size_bytes: u64,
    pub(super) media_status: MediaStatus,
    pub(super) media_type: String,
    pub(super) media_pages_count: u32,
    pub(super) media_comment: String,
    pub(super) media_epub_divina_compatible: bool,
    pub(super) media_epub_is_kepub: bool,
    pub(super) read_status: ReadStatus,
    pub(super) metadata_title_lock: bool,
    pub(super) metadata_summary: String,
    pub(super) metadata_summary_lock: bool,
    pub(super) metadata_number: String,
    pub(super) metadata_number_lock: bool,
    pub(super) metadata_number_sort: f64,
    pub(super) metadata_number_sort_lock: bool,
    pub(super) metadata_release_date: Option<String>,
    pub(super) metadata_release_date_lock: bool,
    pub(super) metadata_authors_lock: bool,
    pub(super) metadata_tags_lock: bool,
    pub(super) metadata_isbn: String,
    pub(super) metadata_isbn_lock: bool,
    pub(super) metadata_links_lock: bool,
    pub(super) metadata_created: String,
    pub(super) metadata_last_modified: String,
    pub(super) file_hash: String,
    pub(super) read_progress: Option<ReadProgressSummary>,
    pub(super) deleted: bool,
    pub(super) oneshot: bool,
    pub(super) genres: Vec<String>,
    pub(super) language: Option<String>,
    pub(super) publisher: Option<String>,
    pub(super) age_rating: Option<u32>,
    pub(super) metadata_tags: Vec<String>,
    pub(super) metadata_authors: Vec<AuthorEntry>,
    pub(super) metadata_links: Vec<WebLinkEntry>,
}

#[derive(Clone)]
pub(super) struct ReadProgressSummary {
    pub(super) page: i32,
    pub(super) completed: bool,
    pub(super) read_date: Option<String>,
    pub(super) created: String,
    pub(super) last_modified: String,
    pub(super) device_id: String,
    pub(super) device_name: String,
}

#[derive(Clone)]
pub(super) struct SeriesSummary {
    pub(super) id: String,
    pub(super) library_id: String,
    pub(super) name: String,
    pub(super) url: String,
    pub(super) title: String,
    pub(super) title_sort: String,
    pub(super) labels: Vec<String>,
    pub(super) created: String,
    pub(super) last_modified: String,
    pub(super) file_last_modified: String,
    pub(super) books_count: u64,
    pub(super) books_read_count: u64,
    pub(super) books_unread_count: u64,
    pub(super) books_in_progress_count: u64,
    pub(super) status: String,
    pub(super) status_lock: bool,
    pub(super) summary: String,
    pub(super) summary_lock: bool,
    pub(super) reading_direction: String,
    pub(super) reading_direction_lock: bool,
    pub(super) publisher: String,
    pub(super) publisher_lock: bool,
    pub(super) age_rating: Option<u32>,
    pub(super) age_rating_lock: bool,
    pub(super) language: String,
    pub(super) language_lock: bool,
    pub(super) genres: Vec<String>,
    pub(super) genres_lock: bool,
    pub(super) tags: Vec<String>,
    pub(super) tags_lock: bool,
    pub(super) total_book_count: Option<u32>,
    pub(super) total_book_count_lock: bool,
    pub(super) sharing_labels_lock: bool,
    pub(super) links: Vec<SeriesMetadataLinkRecord>,
    pub(super) links_lock: bool,
    pub(super) alternate_titles: Vec<SeriesAlternateTitleRecord>,
    pub(super) alternate_titles_lock: bool,
    pub(super) title_lock: bool,
    pub(super) title_sort_lock: bool,
    pub(super) metadata_created: String,
    pub(super) metadata_last_modified: String,
    pub(super) books_metadata_authors: Vec<String>,
    pub(super) books_metadata_tags: Vec<String>,
    pub(super) books_metadata_release_date: Option<String>,
    pub(super) books_metadata_summary: String,
    pub(super) books_metadata_summary_number: String,
    pub(super) books_metadata_created: String,
    pub(super) books_metadata_last_modified: String,
    pub(super) deleted: bool,
    pub(super) oneshot: bool,
}
