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

    pub fn with_root(mut self, root: &str) -> Self {
        self.root = root.to_string();
        self
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

    pub fn with_url(mut self, url: &str) -> Self {
        self.url = url.to_string();
        self
    }

    pub fn with_last_modified(mut self, last_modified: &str) -> Self {
        self.last_modified = last_modified.to_string();
        self
    }

    pub fn with_file_last_modified(mut self, file_last_modified: &str) -> Self {
        self.file_last_modified = file_last_modified.to_string();
        self
    }

    pub fn with_labels<I, S>(mut self, labels: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.labels = labels
            .into_iter()
            .map(|it| it.as_ref().to_ascii_lowercase())
            .collect();
        self
    }

    pub fn with_age_rating(mut self, age_rating: u16) -> Self {
        self.age_rating = Some(age_rating);
        self
    }

    pub fn with_genres<I, S>(mut self, genres: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.genres = genres
            .into_iter()
            .map(|it| it.as_ref().to_ascii_lowercase())
            .collect();
        self
    }

    pub fn with_tags<I, S>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.tags = tags
            .into_iter()
            .map(|it| it.as_ref().to_ascii_lowercase())
            .collect();
        self
    }

    pub fn with_language(mut self, language: &str) -> Self {
        self.language = language.to_ascii_lowercase();
        self
    }

    pub fn with_publisher(mut self, publisher: &str) -> Self {
        self.publisher = publisher.to_ascii_lowercase();
        self
    }

    pub fn with_release_date(mut self, release_date: &str) -> Self {
        self.release_date = Some(release_date.to_string());
        self
    }

    pub fn with_status(mut self, status: &str) -> Self {
        self.status = status.to_ascii_lowercase();
        self
    }

    pub fn with_complete(mut self, complete: bool) -> Self {
        self.complete = complete;
        self
    }

    pub fn with_read_status(mut self, read_status: &str) -> Self {
        self.read_status = read_status.to_ascii_lowercase();
        self
    }

    pub fn with_authors<I, S>(mut self, authors: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.authors = authors
            .into_iter()
            .map(|it| it.as_ref().to_ascii_lowercase())
            .collect();
        self
    }

    pub fn with_deleted(mut self, deleted: bool) -> Self {
        self.deleted = deleted;
        self
    }

    pub fn with_oneshot(mut self, oneshot: bool) -> Self {
        self.oneshot = oneshot;
        self
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

    pub fn with_summary(mut self, summary: &str) -> Self {
        self.summary = summary.to_string();
        self
    }

    pub fn with_ordered(mut self, ordered: bool) -> Self {
        self.ordered = ordered;
        self
    }

    pub fn with_book_ids<I, S>(mut self, book_ids: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.book_ids = book_ids
            .into_iter()
            .map(|it| it.as_ref().to_string())
            .collect();
        self
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

    pub fn with_ordered(mut self, ordered: bool) -> Self {
        self.ordered = ordered;
        self
    }

    pub fn with_series_ids<I, S>(mut self, series_ids: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.series_ids = series_ids
            .into_iter()
            .map(|it| it.as_ref().to_string())
            .collect();
        self
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

    pub fn with_read_date(mut self, read_date: &str) -> Self {
        self.read_date = read_date.to_string();
        self
    }

    pub fn with_created(mut self, created: &str) -> Self {
        self.created = created.to_string();
        self
    }

    pub fn with_last_modified(mut self, last_modified: &str) -> Self {
        self.last_modified = last_modified.to_string();
        self
    }

    pub fn with_device(mut self, device_id: &str, device_name: &str) -> Self {
        self.device_id = device_id.to_string();
        self.device_name = device_name.to_string();
        self
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

    pub fn with_url(mut self, url: &str) -> Self {
        self.url = url.to_string();
        self
    }

    pub fn with_last_modified(mut self, last_modified: &str) -> Self {
        self.last_modified = last_modified.to_string();
        self
    }

    pub fn with_size_bytes(mut self, size_bytes: u64) -> Self {
        self.size_bytes = size_bytes;
        self
    }

    pub fn with_media(mut self, status: &str, media_type: &str, pages_count: u32) -> Self {
        self.media_status = status.to_string();
        self.media_type = media_type.to_string();
        self.media_pages_count = pages_count;
        self
    }

    pub fn with_media_profile(mut self, media_profile: &str) -> Self {
        self.media_profile = media_profile.to_ascii_lowercase();
        self
    }

    pub fn with_release_date(mut self, release_date: &str) -> Self {
        self.metadata_release_date = Some(release_date.to_string());
        self
    }

    pub fn with_number_sort(mut self, number_sort: i32) -> Self {
        self.number_sort = number_sort;
        self
    }

    pub fn with_deleted(mut self, deleted: bool) -> Self {
        self.deleted = deleted;
        self
    }

    pub fn with_oneshot(mut self, oneshot: bool) -> Self {
        self.oneshot = oneshot;
        self
    }

    pub fn with_tags<I, S>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.tags = tags
            .into_iter()
            .map(|it| it.as_ref().to_ascii_lowercase())
            .collect();
        self
    }

    pub fn with_read_status(mut self, read_status: &str) -> Self {
        self.read_status = read_status.to_ascii_lowercase();
        self
    }

    pub fn with_authors<I, S>(mut self, authors: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.authors = authors
            .into_iter()
            .map(|it| it.as_ref().to_ascii_lowercase())
            .collect();
        self
    }
}
