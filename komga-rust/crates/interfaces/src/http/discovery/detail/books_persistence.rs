use super::*;

use crate::discovery_detail_access::books as books_access;

#[derive(Clone)]
pub struct PersistedBookResource {
    pub library_id: String,
    pub age_rating: Option<u16>,
    pub sharing_labels: Vec<String>,
}

#[derive(Clone, Copy)]
pub enum PersistedBookSiblingDirection {
    Previous,
    Next,
}

pub async fn resolve_book_id_for_persisted(
    database_file: &FsPath,
    requested_book_id: &str,
) -> String {
    let Some(index) = requested_book_id
        .strip_prefix("book-")
        .and_then(|value| value.parse::<usize>().ok())
    else {
        return requested_book_id.to_string();
    };

    if index == 0 {
        return requested_book_id.to_string();
    }

    if matches!(
        load_persisted_book_resource(database_file, requested_book_id).await,
        Ok(Some(_))
    ) {
        return requested_book_id.to_string();
    }

    match books_access::load_book_id_by_sorted_position(database_file, index).await {
        Ok(Some(book_id)) => book_id,
        _ => requested_book_id.to_string(),
    }
}

pub async fn load_persisted_book_resource(
    database_file: &FsPath,
    book_id: &str,
) -> Result<Option<PersistedBookResource>, String> {
    let resource = books_access::load_persisted_book_resource(database_file, book_id)
        .await?
        .map(|row| PersistedBookResource {
            library_id: row.library_id,
            age_rating: row.age_rating,
            sharing_labels: parse_csv_values(&row.sharing_labels),
        });
    Ok(resource)
}

pub async fn load_persisted_book_detail(
    database_file: &FsPath,
    book_id: &str,
    user_id: Option<&str>,
) -> Result<Option<BookDetailReadModel>, String> {
    let model = books_access::load_persisted_book_detail(database_file, book_id, user_id)
        .await?
        .map(|row| BookDetailReadModel {
            id: row.id,
            series_id: row.series_id,
            series_title: row.series_title,
            library_id: row.library_id,
            name: row.name,
            url: row.url,
            number: row.number,
            created: row.created,
            last_modified: row.last_modified,
            file_last_modified: row.file_last_modified,
            size_bytes: row.size_bytes,
            media_status: row.media_status,
            media_type: row.media_type,
            media_pages_count: row.media_pages_count,
            media_comment: row.media_comment,
            metadata_title: row.metadata_title,
            metadata_summary: row.metadata_summary,
            metadata_number: row.metadata_number,
            metadata_number_sort: row.metadata_number_sort,
            metadata_release_date: row.metadata_release_date,
            metadata_title_lock: row.metadata_title_lock,
            metadata_summary_lock: row.metadata_summary_lock,
            metadata_number_lock: row.metadata_number_lock,
            metadata_number_sort_lock: row.metadata_number_sort_lock,
            metadata_release_date_lock: row.metadata_release_date_lock,
            metadata_authors: parse_metadata_authors(&row.metadata_authors),
            metadata_authors_lock: row.metadata_authors_lock,
            metadata_tags: parse_csv_values(&row.metadata_tags),
            metadata_tags_lock: row.metadata_tags_lock,
            metadata_isbn: row.metadata_isbn,
            metadata_isbn_lock: row.metadata_isbn_lock,
            metadata_links: parse_metadata_links(&row.metadata_links),
            metadata_links_lock: row.metadata_links_lock,
            metadata_created: row.metadata_created,
            metadata_last_modified: row.metadata_last_modified,
            media_epub_divina_compatible: row.media_epub_divina_compatible,
            media_epub_is_kepub: row.media_epub_is_kepub,
            read_progress: row.read_progress.map(|progress| PersistedReadProgress {
                page: progress.page,
                completed: progress.completed,
                read_date: progress.read_date,
                created: progress.created,
                last_modified: progress.last_modified,
                device_id: progress.device_id,
                device_name: progress.device_name,
            }),
            deleted: row.deleted,
            file_hash: row.file_hash,
            oneshot: row.oneshot,
        });
    Ok(model)
}

fn parse_metadata_authors(raw: &str) -> Vec<BookMetadataAuthorReadModel> {
    raw.split('\u{001F}')
        .filter(|entry| !entry.is_empty())
        .map(|author| match author.split_once('\u{001E}') {
            Some((name, role)) => BookMetadataAuthorReadModel {
                name: name.to_string(),
                role: role.to_string(),
            },
            None => BookMetadataAuthorReadModel {
                name: author.to_string(),
                role: String::new(),
            },
        })
        .collect()
}

fn parse_metadata_links(raw: &str) -> Vec<BookMetadataLinkReadModel> {
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

#[cfg(test)]
mod tests {
    use super::parse_metadata_links;

    #[test]
    fn parse_metadata_links_decodes_separator_encoded_rows() {
        let links = parse_metadata_links(
            "Wiki\u{001E}https://example.com\u{001F}Store\u{001E}https://shop.example.com",
        );

        assert_eq!(links.len(), 2);
        assert_eq!(links[0].label, "Wiki");
        assert_eq!(links[0].url, "https://example.com");
        assert_eq!(links[1].label, "Store");
        assert_eq!(links[1].url, "https://shop.example.com");
    }
}

pub async fn load_persisted_book_sibling_detail(
    database_file: &FsPath,
    book_id: &str,
    direction: PersistedBookSiblingDirection,
    user_id: Option<&str>,
) -> Result<Option<BookDetailReadModel>, String> {
    let direction = match direction {
        PersistedBookSiblingDirection::Previous => {
            books_access::PersistedBookSiblingDirectionRecord::Previous
        }
        PersistedBookSiblingDirection::Next => {
            books_access::PersistedBookSiblingDirectionRecord::Next
        }
    };

    let Some(sibling_id) =
        books_access::load_persisted_book_sibling_id(database_file, book_id, direction).await?
    else {
        return Ok(None);
    };

    load_persisted_book_detail(database_file, &sibling_id, user_id).await
}
