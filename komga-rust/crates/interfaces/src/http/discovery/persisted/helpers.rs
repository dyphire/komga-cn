use super::*;

pub(crate) fn media_profile_for_media_type(media_type: &str) -> &'static str {
    match media_type {
        "application/zip"
        | "application/x-rar-compressed"
        | "application/x-rar-compressed; version=4"
        | "application/x-rar-compressed; version=5" => "divina",
        "application/epub+zip" => "epub",
        "application/pdf" => "pdf",
        _ => "",
    }
}

pub(crate) fn series_matches_read_status(
    row: &PersistedSeriesSummary,
    read_progress: Option<(i64, i64)>,
    status: &str,
) -> bool {
    match status.to_ascii_lowercase().as_str() {
        "unread" => read_progress.is_none(),
        "read" => read_progress
            .map(|(read_count, _)| read_count.max(0) as u64 == row.books_count)
            .unwrap_or(false),
        "in_progress" | "inprogress" => read_progress
            .map(|(read_count, _)| read_count.max(0) as u64 != row.books_count)
            .unwrap_or(false),
        _ => false,
    }
}

pub(crate) fn poster_matches(
    poster: &PersistedBookPosterSummary,
    poster_types: Option<&Vec<String>>,
    poster_selected: Option<bool>,
) -> bool {
    let type_matches = poster_types
        .map(|values| {
            values
                .iter()
                .any(|value| value.eq_ignore_ascii_case(&poster.thumbnail_type))
        })
        .unwrap_or(true);
    let selected_matches = poster_selected
        .map(|value| poster.selected == value)
        .unwrap_or(true);
    type_matches && selected_matches
}

pub(crate) fn normalized_author_filter_value(name: &str, role: &str) -> String {
    if role.is_empty() {
        name.to_ascii_lowercase()
    } else {
        format!("{name}::{role}").to_ascii_lowercase()
    }
}

pub(crate) fn author_matches_filter_value(author: &str, expected: &[String]) -> bool {
    let normalized = author.to_ascii_lowercase();
    expected
        .iter()
        .any(|value| author_value_matches(&normalized, value))
}

pub(crate) fn author_matches_filter(name: &str, role: &str, expected: &[String]) -> bool {
    author_matches_filter_value(&normalized_author_filter_value(name, role), expected)
}

pub(crate) fn author_value_matches(author: &str, expected: &str) -> bool {
    if let Some((expected_name, expected_role)) = expected.split_once("::") {
        let (author_name, author_role) = author
            .split_once("::")
            .map(|(name, role)| (name, Some(role)))
            .unwrap_or((author, None));

        if expected_name.is_empty() {
            return author_role
                .map(|role| role.eq_ignore_ascii_case(expected_role))
                .unwrap_or(false);
        }

        if expected_role.is_empty() {
            return author_name.eq_ignore_ascii_case(expected_name);
        }

        return author_name.eq_ignore_ascii_case(expected_name)
            && author_role
                .map(|role| role.eq_ignore_ascii_case(expected_role))
                .unwrap_or(false);
    }

    author.contains(expected)
}
