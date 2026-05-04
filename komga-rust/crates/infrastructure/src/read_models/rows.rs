#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LibraryRow {
    pub id: String,
    pub name: String,
    pub root: String,
}

impl LibraryRow {
    pub fn new(id: &str, name: &str) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            root: String::new(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeriesRow {
    pub id: String,
    pub library_id: String,
    pub title: String,
    pub labels: Vec<String>,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
    pub language: String,
    pub publisher: String,
    pub age_rating: Option<u16>,
    pub release_date: Option<String>,
    pub status: String,
    pub complete: bool,
    pub read_status: String,
    pub authors: Vec<String>,
    pub deleted: bool,
    pub oneshot: bool,
    pub created: String,
    pub last_modified: String,
    pub file_last_modified: String,
    pub url: String,
}

impl SeriesRow {
    pub fn new(id: &str, library_id: &str, title: &str) -> Self {
        Self {
            id: id.to_string(),
            library_id: library_id.to_string(),
            title: title.to_string(),
            labels: vec![],
            genres: vec![],
            tags: vec![],
            language: String::new(),
            publisher: String::new(),
            age_rating: None,
            release_date: None,
            status: String::new(),
            complete: false,
            read_status: String::new(),
            authors: vec![],
            deleted: false,
            oneshot: false,
            created: "2026-01-01T00:00:00Z".to_string(),
            last_modified: "2026-01-01T00:00:00Z".to_string(),
            file_last_modified: "2024-01-02T03:04:05Z".to_string(),
            url: format!("/library/{library_id}/{id}"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollectionRow {
    pub id: String,
    pub name: String,
    pub ordered: bool,
    pub series_ids: Vec<String>,
    pub created_date: String,
    pub last_modified_date: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadListRow {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub ordered: bool,
    pub book_ids: Vec<String>,
    pub created_date: String,
    pub last_modified_date: String,
}

impl ReadListRow {
    pub fn new(id: &str, name: &str) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            summary: String::new(),
            ordered: true,
            book_ids: vec![],
            created_date: "2026-01-01T00:00:00Z".to_string(),
            last_modified_date: "2026-01-01T00:00:00Z".to_string(),
        }
    }
}

impl CollectionRow {
    pub fn new(id: &str, name: &str) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            ordered: false,
            series_ids: vec![],
            created_date: "2026-01-01T00:00:00Z".to_string(),
            last_modified_date: "2026-01-01T00:00:00Z".to_string(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookRow {
    pub id: String,
    pub series_id: String,
    pub library_id: String,
    pub title: String,
    pub url: String,
    pub created: String,
    pub last_modified: String,
    pub file_last_modified: String,
    pub size_bytes: u64,
    pub media_status: String,
    pub media_profile: String,
    pub media_type: String,
    pub media_pages_count: u32,
    pub metadata_release_date: Option<String>,
    pub number_sort: i32,
    pub deleted: bool,
    pub oneshot: bool,
    pub tags: Vec<String>,
    pub read_status: String,
    pub authors: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadProgressRow {
    pub book_id: String,
    pub user_id: String,
    pub page: u32,
    pub completed: bool,
    pub read_date: String,
    pub created: String,
    pub last_modified: String,
    pub device_id: String,
    pub device_name: String,
}

impl ReadProgressRow {
    pub fn new(book_id: &str, user_id: &str, page: u32, completed: bool) -> Self {
        Self {
            book_id: book_id.to_string(),
            user_id: user_id.to_string(),
            page,
            completed,
            read_date: "2024-01-01T00:00:00Z".to_string(),
            created: "2024-01-01T00:00:00Z".to_string(),
            last_modified: "2024-01-01T00:00:00Z".to_string(),
            device_id: "device-1".to_string(),
            device_name: "Device 1".to_string(),
        }
    }
}

impl BookRow {
    pub fn new(id: &str, series_id: &str, library_id: &str, title: &str) -> Self {
        Self {
            id: id.to_string(),
            series_id: series_id.to_string(),
            library_id: library_id.to_string(),
            title: title.to_string(),
            url: format!("/library/{library_id}/{title}"),
            created: "2024-01-02T03:04:05Z".to_string(),
            last_modified: "2024-01-02T03:04:05Z".to_string(),
            file_last_modified: "2024-01-02T08:04:05Z".to_string(),
            size_bytes: 0,
            media_status: "UNKNOWN".to_string(),
            media_profile: String::new(),
            media_type: String::new(),
            media_pages_count: 0,
            metadata_release_date: None,
            number_sort: 1,
            deleted: false,
            oneshot: false,
            tags: vec![],
            read_status: String::new(),
            authors: vec![],
        }
    }
}
