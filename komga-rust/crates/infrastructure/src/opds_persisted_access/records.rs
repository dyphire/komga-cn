use sqlx::Row;

#[derive(Clone)]
pub struct PersistedLibraryRecord {
    pub id: String,
    pub name: String,
    pub last_modified: String,
}

pub struct PersistedSeriesRecord {
    pub id: String,
    pub library_id: String,
    pub title: String,
    pub summary: String,
    pub age_rating: Option<u16>,
    pub sharing_labels: Vec<String>,
    pub last_modified: String,
}

pub struct PersistedSeriesBookRecord {
    pub id: String,
    pub series_id: String,
    pub title: String,
    pub series_title: String,
    pub number: String,
    pub number_sort: f64,
    pub summary: String,
    pub isbn: Option<String>,
    pub authors: Vec<PersistedBookAuthorRecord>,
    pub tags: Vec<String>,
    pub file_name: String,
    pub file_size: i64,
    pub media_type: String,
    pub media_status: Option<String>,
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

pub struct PersistedReadlistRecord {
    pub id: String,
    pub name: String,
    pub last_modified: String,
    pub ordered: bool,
}

pub struct PersistedBookAuthorRecord {
    pub name: String,
    pub role: String,
}

pub struct PersistedReadlistBookRecord {
    pub id: String,
    pub series_id: String,
    pub title: String,
    pub series_title: String,
    pub number: String,
    pub number_sort: f64,
    pub summary: String,
    pub isbn: Option<String>,
    pub authors: Vec<PersistedBookAuthorRecord>,
    pub tags: Vec<String>,
    pub file_name: String,
    pub file_size: i64,
    pub media_type: String,
    pub media_status: Option<String>,
    pub page_count: i64,
    pub epub_divina_compatible: bool,
    pub library_id: String,
    pub age_rating: Option<u16>,
    pub sharing_labels: Vec<String>,
    pub last_modified: String,
    pub release_date: Option<String>,
}

pub struct PersistedSeriesSearchRecord {
    pub id: String,
    pub title: String,
    pub library_id: String,
    pub age_rating: Option<u16>,
    pub sharing_labels: Vec<String>,
    pub last_modified: String,
}

pub struct PersistedBookSearchRecord {
    pub id: String,
    pub series_id: String,
    pub title: String,
    pub series_title: String,
    pub number: String,
    pub number_sort: f64,
    pub summary: String,
    pub isbn: Option<String>,
    pub authors: Vec<PersistedBookAuthorRecord>,
    pub tags: Vec<String>,
    pub file_name: String,
    pub file_size: i64,
    pub media_type: String,
    pub page_count: i64,
    pub epub_divina_compatible: bool,
    pub library_id: String,
    pub age_rating: Option<u16>,
    pub sharing_labels: Vec<String>,
    pub last_modified: String,
    pub release_date: Option<String>,
}

pub struct PersistedNamedRecord {
    pub id: String,
    pub name: String,
    pub last_modified: String,
    pub ordered: bool,
}

pub struct PersistedBookFeedRecord {
    pub id: String,
    pub title: String,
    pub file_name: String,
    pub media_type: String,
    pub library_id: String,
    pub age_rating: Option<u16>,
    pub sharing_labels: Vec<String>,
    pub last_modified: String,
}

pub(crate) fn parsed_age_rating(row: &sqlx::sqlite::SqliteRow) -> Option<u16> {
    row.try_get::<i64, _>("AGE_RATING")
        .ok()
        .and_then(|value| u16::try_from(value).ok())
}

pub(crate) fn parsed_sharing_labels(row: &sqlx::sqlite::SqliteRow) -> Vec<String> {
    row.get::<String, _>("SHARING_LABELS")
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

pub(crate) fn parsed_book_author_records(
    row: &sqlx::sqlite::SqliteRow,
) -> Vec<PersistedBookAuthorRecord> {
    row.get::<String, _>("AUTHORS")
        .split('\u{001e}')
        .filter(|value| !value.is_empty())
        .map(|value| {
            let mut parts = value.splitn(2, '\u{001f}');
            let name = parts.next().unwrap_or_default().trim().to_string();
            let role = parts.next().unwrap_or_default().trim().to_string();
            PersistedBookAuthorRecord { name, role }
        })
        .filter(|author| !author.name.is_empty())
        .collect()
}

pub(crate) fn parsed_book_tags(row: &sqlx::sqlite::SqliteRow) -> Vec<String> {
    row.get::<String, _>("TAGS")
        .split('\u{001e}')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

pub(crate) fn placeholder_list(count: usize) -> String {
    std::iter::repeat_n("?", count)
        .collect::<Vec<_>>()
        .join(", ")
}
