use komga_application::discovery::{BookMetadataAuthorReadModel, BookMetadataLinkReadModel};

pub(crate) fn parse_csv_values(raw: &str) -> Vec<String> {
    raw.split(',')
        .filter(|entry| !entry.is_empty())
        .map(|entry| entry.to_string())
        .collect()
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
