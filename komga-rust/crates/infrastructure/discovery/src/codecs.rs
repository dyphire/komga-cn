use komga_application::discovery::{BookMetadataAuthorReadModel, BookMetadataLinkReadModel};
use komga_domain::media_assets::ThumbnailType;

pub fn parse_thumbnail_type(value: &str) -> ThumbnailType {
    ThumbnailType::parse(value).expect("persisted thumbnail type should use a known value")
}

pub fn parse_metadata_authors(raw: &str) -> Vec<BookMetadataAuthorReadModel> {
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

pub fn parse_metadata_links(raw: &str) -> Vec<BookMetadataLinkReadModel> {
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
