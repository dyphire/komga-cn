pub struct BrowseSeriesNavigationEntry {
    pub id: String,
    pub title: String,
}

pub struct BrowsePublisherEntry {
    pub publisher: String,
}

pub struct OpdsBookAuthorEntry {
    pub name: String,
    pub role: String,
}

pub struct OpdsBookFeedEntry {
    pub id: String,
    pub series_id: String,
    pub title: String,
    pub series_title: String,
    pub number: String,
    pub number_sort: f64,
    pub summary: String,
    pub isbn: Option<String>,
    pub authors: Vec<OpdsBookAuthorEntry>,
    pub tags: Vec<String>,
    pub file_name: String,
    pub file_size: i64,
    pub media_type: String,
    pub page_count: i64,
    pub epub_divina_compatible: bool,
    pub last_read: Option<i64>,
    pub last_read_date: Option<String>,
    pub library_id: String,
    pub age_rating: Option<u16>,
    pub sharing_labels: Vec<String>,
    pub last_modified: String,
    pub release_date: Option<String>,
}

pub struct OpdsSeriesEntry {
    pub id: String,
    pub library_id: String,
    pub title: String,
    pub one_shot: bool,
    pub age_rating: Option<u16>,
    pub sharing_labels: Vec<String>,
    pub last_modified: String,
}

pub struct OpdsReadlistEntry {
    pub id: String,
    pub name: String,
    pub last_modified: String,
}
