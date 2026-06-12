use super::models::{BookPosterRow, SeriesReadProgressCounts};
use komga_domain::discovery::ReadStatus;
use komga_domain::media_assets::ThumbnailType;

pub(super) fn series_matches_read_status(
    books_count: u64,
    read_progress: Option<SeriesReadProgressCounts>,
    status: ReadStatus,
) -> bool {
    match status {
        ReadStatus::Unread => read_progress.is_none(),
        ReadStatus::Read => read_progress
            .map(|counts| counts.read_count.max(0) as u64 == books_count)
            .unwrap_or(false),
        ReadStatus::InProgress => read_progress
            .map(|counts| counts.read_count.max(0) as u64 != books_count)
            .unwrap_or(false),
    }
}

pub(super) fn poster_matches(
    poster: &BookPosterRow,
    poster_type: Option<ThumbnailType>,
    poster_selected: Option<bool>,
) -> bool {
    let type_matches = poster_type
        .map(|thumbnail_type| poster.thumbnail_type == thumbnail_type)
        .unwrap_or(true);
    let selected_matches = poster_selected
        .map(|value| poster.selected == value)
        .unwrap_or(true);
    type_matches && selected_matches
}

pub(super) fn normalized_author_filter_value(name: &str, role: &str) -> String {
    if role.is_empty() {
        name.to_ascii_lowercase()
    } else {
        format!("{name}::{role}").to_ascii_lowercase()
    }
}

pub(super) fn author_matches_filter_value(author: &str, expected: &[String]) -> bool {
    let normalized = author.to_ascii_lowercase();
    expected
        .iter()
        .any(|value| author_value_matches(&normalized, value))
}

pub(super) fn author_matches_filter(name: &str, role: &str, expected: &[String]) -> bool {
    author_matches_filter_value(&normalized_author_filter_value(name, role), expected)
}

pub(super) fn author_contains_filter_value(author: &str, expected: &[String]) -> bool {
    let normalized = author.to_ascii_lowercase();
    expected
        .iter()
        .any(|value| normalized.contains(value.as_str()))
}

pub(super) fn author_contains_filter(name: &str, role: &str, expected: &[String]) -> bool {
    author_contains_filter_value(&normalized_author_filter_value(name, role), expected)
}

struct AuthorComponents<'a> {
    name: &'a str,
    role: Option<&'a str>,
}

fn split_author_components(value: &str) -> AuthorComponents<'_> {
    if let Some(role) = value.strip_prefix("::") {
        return AuthorComponents {
            name: "",
            role: Some(role),
        };
    }

    if let Some((name, role)) = value.split_once("::").or_else(|| value.split_once(',')) {
        return AuthorComponents {
            name,
            role: Some(role),
        };
    }

    AuthorComponents {
        name: value,
        role: None,
    }
}

pub(super) fn author_value_matches(author: &str, expected: &str) -> bool {
    if expected.contains("::") || expected.contains(',') {
        let expected = split_author_components(expected);
        let author = split_author_components(author);
        return author.name.eq_ignore_ascii_case(expected.name)
            && author
                .role
                .unwrap_or_default()
                .eq_ignore_ascii_case(expected.role.unwrap_or_default());
    }

    split_author_components(author)
        .name
        .eq_ignore_ascii_case(expected)
}
