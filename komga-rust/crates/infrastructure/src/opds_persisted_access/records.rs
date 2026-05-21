use sqlx::Row;

pub use komga_application::opds::{
    OpdsPersistedBookAuthorRecord as PersistedBookAuthorRecord, PersistedBookFeedRecord,
    PersistedBookSearchRecord, PersistedLibraryRecord, PersistedNamedRecord,
    PersistedReadlistBookRecord, PersistedReadlistRecord, PersistedSeriesBookRecord,
    PersistedSeriesRecord, PersistedSeriesSearchRecord,
};

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
