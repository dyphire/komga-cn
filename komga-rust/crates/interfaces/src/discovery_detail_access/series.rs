#[derive(Clone)]
pub struct PersistedSeriesResourceRecord {
    pub library_id: String,
    pub age_rating: Option<u32>,
    pub sharing_labels: String,
}

#[derive(Clone)]
pub struct PersistedSeriesDetailRecord {
    pub id: String,
    pub library_id: String,
    pub name: String,
    pub title: String,
    pub title_sort: String,
    pub url: String,
    pub created: String,
    pub last_modified: String,
    pub file_last_modified: String,
    pub books_count: u32,
    pub status: String,
    pub summary: String,
    pub reading_direction: String,
    pub publisher: String,
    pub age_rating: Option<u32>,
    pub language: String,
    pub sharing_labels: String,
    pub metadata_created: String,
    pub metadata_last_modified: String,
    pub deleted: bool,
    pub oneshot: bool,
}

#[derive(Clone)]
pub struct PersistedCollectionRecord {
    pub id: String,
    pub name: String,
    pub ordered: bool,
    pub series_ids: Vec<String>,
    pub created_date: String,
    pub last_modified_date: String,
}

pub struct ExistingSeriesMetadataRecord {
    pub status: String,
    pub status_lock: bool,
    pub title: String,
    pub title_lock: bool,
    pub title_sort: String,
    pub title_sort_lock: bool,
    pub summary: String,
    pub summary_lock: bool,
    pub reading_direction: Option<String>,
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
    pub total_book_count: Option<u32>,
    pub total_book_count_lock: bool,
    pub sharing_labels: Vec<String>,
    pub sharing_labels_lock: bool,
    pub links: Vec<SeriesMetadataLinkRecord>,
    pub links_lock: bool,
    pub alternate_titles: Vec<SeriesAlternateTitleRecord>,
    pub alternate_titles_lock: bool,
}

#[derive(Clone)]
pub struct SeriesMetadataLinkRecord {
    pub label: String,
    pub url: String,
}

#[derive(Clone)]
pub struct SeriesAlternateTitleRecord {
    pub label: String,
    pub title: String,
}

#[derive(Clone)]
pub struct SeriesMetadataUpdateRecord {
    pub status: String,
    pub status_lock: bool,
    pub title: String,
    pub title_lock: bool,
    pub title_sort: String,
    pub title_sort_lock: bool,
    pub summary: String,
    pub summary_lock: bool,
    pub reading_direction: Option<String>,
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
    pub total_book_count: Option<u32>,
    pub total_book_count_lock: bool,
    pub sharing_labels: Vec<String>,
    pub sharing_labels_lock: bool,
    pub links: Vec<SeriesMetadataLinkRecord>,
    pub links_lock: bool,
    pub alternate_titles: Vec<SeriesAlternateTitleRecord>,
    pub alternate_titles_lock: bool,
}

#[derive(Clone)]
pub struct SeriesSummaryRecord {
    pub id: String,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
    pub alternate_titles: Vec<String>,
    pub books_metadata_authors: Vec<String>,
    pub books_metadata_tags: Vec<String>,
    pub books_metadata_release_date: Option<String>,
    pub books_metadata_summary: String,
    pub books_metadata_summary_number: String,
    pub books_metadata_created: String,
    pub books_metadata_last_modified: String,
}
