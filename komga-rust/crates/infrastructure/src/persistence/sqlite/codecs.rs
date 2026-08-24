use komga_application::discovery::{BookMetadataAuthorReadModel, BookMetadataLinkReadModel};
use komga_domain::media_assets::ThumbnailType;

pub(crate) fn parse_sqlite_group_concat_values(raw: &str) -> Vec<String> {
    const SEPARATOR: char = '\u{1e}';

    if raw.trim().is_empty() {
        return vec![];
    }

    raw.split(SEPARATOR)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

pub(crate) fn clamp_kotlin_int_u32(value: i64) -> u32 {
    value.clamp(0, i64::from(i32::MAX)) as u32
}

pub(crate) fn parse_thumbnail_type(value: &str) -> ThumbnailType {
    ThumbnailType::parse(value).expect("persisted thumbnail type should use a known value")
}

pub(crate) fn parse_metadata_authors(raw: &str) -> Vec<BookMetadataAuthorReadModel> {
    raw.split('\u{001F}')
        .filter(|entry| !entry.is_empty())
        .map(|entry| match entry.split_once('\u{001E}') {
            Some((name, role)) => BookMetadataAuthorReadModel {
                name: name.to_string(),
                role: role.to_string(),
            },
            None => BookMetadataAuthorReadModel {
                name: entry.to_string(),
                role: String::new(),
            },
        })
        .collect()
}

pub(crate) fn parse_metadata_links(raw: &str) -> Vec<BookMetadataLinkReadModel> {
    raw.split('\u{001F}')
        .filter(|entry| !entry.is_empty())
        .filter_map(|entry| {
            entry
                .split_once('\u{001E}')
                .map(|(label, url)| BookMetadataLinkReadModel {
                    label: label.to_string(),
                    url: url.to_string(),
                })
        })
        .collect()
}
