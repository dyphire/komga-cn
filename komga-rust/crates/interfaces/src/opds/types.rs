#[derive(Clone)]
pub(super) struct PersistedLibrary {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) last_modified: String,
}

#[derive(Clone)]
pub(super) struct PersistedSeries {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) last_modified: String,
}

pub(super) struct PersistedSeriesSearchResult {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) library_id: String,
    pub(super) age_rating: Option<u32>,
    pub(super) sharing_labels: Vec<String>,
    pub(super) last_modified: String,
}

pub(super) struct PersistedBookFeedItem {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) series_title: String,
    pub(super) number: String,
    pub(super) summary: String,
    pub(super) authors: Vec<String>,
    pub(super) file_name: String,
    pub(super) file_size: i64,
    pub(super) media_type: String,
    pub(super) page_count: i64,
    pub(super) epub_divina_compatible: bool,
    pub(super) last_read: Option<i64>,
    pub(super) last_read_date: Option<String>,
    pub(super) last_modified: String,
}
