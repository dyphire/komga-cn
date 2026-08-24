use sqlx::Row;

use komga_infrastructure_base::sqlite::codecs::{
    clamp_kotlin_int_u32, parse_sqlite_group_concat_values,
};

use komga_application::opds::OpdsPersistedBookAuthorRecord;

pub(crate) fn parsed_age_rating(row: &sqlx::sqlite::SqliteRow) -> Option<u32> {
    row.get::<Option<i64>, _>("AGE_RATING")
        .map(clamp_kotlin_int_u32)
}

pub(crate) fn parsed_sharing_labels(row: &sqlx::sqlite::SqliteRow) -> Vec<String> {
    parse_sqlite_group_concat_values(&row.get::<String, _>("SHARING_LABELS"))
}

pub(crate) fn parsed_book_author_records(
    row: &sqlx::sqlite::SqliteRow,
) -> Vec<OpdsPersistedBookAuthorRecord> {
    row.get::<String, _>("AUTHORS")
        .split('\u{001e}')
        .filter(|value| !value.is_empty())
        .map(|value| {
            let mut parts = value.splitn(2, '\u{001f}');
            let name = parts.next().unwrap_or_default().trim().to_string();
            let role = parts.next().unwrap_or_default().trim().to_string();
            OpdsPersistedBookAuthorRecord { name, role }
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
