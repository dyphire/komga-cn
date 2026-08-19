use komga_application::discovery::{
    BookMetadataAuthorReadModel, BookReadModel, SeriesAlternateTitleRecord,
    SeriesMetadataLinkRecord, SeriesReadingDirection,
};
use komga_domain::discovery::SeriesStatus;

use crate::state::DiscoveryState;

mod books_detail;
mod books_persistence;
mod collections;
mod collections_support;
mod detail_utils;
mod readlists;
mod readlists_support;
mod series_detail;
mod series_persistence;

pub(crate) use books_detail::{
    book_detail, book_readlists, book_sibling_next, book_sibling_previous,
};
pub(in crate::discovery) use books_persistence::load_persisted_book_resource;
pub(crate) use collections::{
    collection_create, collection_delete, collection_detail, collection_series, collection_update,
    collections,
};
pub(crate) use readlists::{
    readlist_book_sibling_next, readlist_book_sibling_previous, readlist_books, readlist_create,
    readlist_delete, readlist_detail, readlist_match_comicrack, readlist_update, readlists,
};
pub(crate) use series_detail::{series_collections, series_detail, series_metadata_update};
pub(in crate::discovery) use series_persistence::load_persisted_series_resource;

pub(super) type BookDetailReadModel = BookReadModel;

#[derive(Clone)]
pub(crate) struct SeriesDetailReadModel {
    pub(crate) id: String,
    pub(crate) library_id: String,
    pub(crate) name: String,
    pub(crate) title: String,
    pub(crate) title_sort: String,
    pub(crate) url: String,
    pub(crate) created: String,
    pub(crate) last_modified: String,
    pub(crate) file_last_modified: String,
    pub(crate) books_count: u32,
    pub(crate) books_read_count: u32,
    pub(crate) books_unread_count: u32,
    pub(crate) books_in_progress_count: u32,
    pub(crate) status: SeriesStatus,
    pub(crate) status_lock: bool,
    pub(crate) summary: String,
    pub(crate) summary_lock: bool,
    pub(crate) reading_direction: Option<SeriesReadingDirection>,
    pub(crate) reading_direction_lock: bool,
    pub(crate) publisher: String,
    pub(crate) publisher_lock: bool,
    pub(crate) age_rating: Option<u32>,
    pub(crate) age_rating_lock: bool,
    pub(crate) language: String,
    pub(crate) language_lock: bool,
    pub(crate) genres: Vec<String>,
    pub(crate) genres_lock: bool,
    pub(crate) tags: Vec<String>,
    pub(crate) tags_lock: bool,
    pub(crate) total_book_count: Option<u32>,
    pub(crate) total_book_count_lock: bool,
    pub(crate) sharing_labels: Vec<String>,
    pub(crate) sharing_labels_lock: bool,
    pub(crate) links: Vec<SeriesMetadataLinkRecord>,
    pub(crate) links_lock: bool,
    pub(crate) alternate_titles: Vec<SeriesAlternateTitleRecord>,
    pub(crate) alternate_titles_lock: bool,
    pub(crate) title_lock: bool,
    pub(crate) title_sort_lock: bool,
    pub(crate) metadata_created: String,
    pub(crate) metadata_last_modified: String,
    pub(crate) books_metadata_tags: Vec<String>,
    pub(crate) books_metadata_authors: Vec<BookMetadataAuthorReadModel>,
    pub(crate) books_metadata_release_date: Option<String>,
    pub(crate) books_metadata_summary: String,
    pub(crate) books_metadata_summary_number: String,
    pub(crate) books_metadata_created: String,
    pub(crate) books_metadata_last_modified: String,
    pub(crate) deleted: bool,
    pub(crate) oneshot: bool,
}

pub(super) async fn load_persisted_book_detail(
    app: &DiscoveryState,
    book_id: &str,
    user_id: Option<&str>,
) -> anyhow::Result<Option<BookReadModel>> {
    books_persistence::load_persisted_book_detail(app, book_id, user_id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::discovery::{BookDto, SeriesDto};
    use komga_application::discovery::BookMetadataLinkReadModel;
    use komga_domain::discovery::MediaStatus;
    use serde_json::{Value, json};

    #[test]
    fn book_dto_uses_persisted_lock_link_and_media_flags() {
        let payload = serde_json::to_value(
            BookDto::from_read_model(
                &BookDetailReadModel {
                    id: "book-1".to_string(),
                    series_id: "series-1".to_string(),
                    series_title: "Series".to_string(),
                    series_title_sort: "Series".to_string(),
                    library_id: "lib-1".to_string(),
                    name: "Book".to_string(),
                    url: "/data/books/book.cbz".to_string(),
                    number: 1,
                    created: "2024-01-01T00:00:00Z".to_string(),
                    last_modified: "2024-01-02T00:00:00Z".to_string(),
                    file_last_modified: "2024-01-03T00:00:00Z".to_string(),
                    size_bytes: 123,
                    media_status: MediaStatus::Ready,
                    media_type: "application/epub+zip".to_string(),
                    media_pages_count: 5,
                    media_comment: "ok".to_string(),
                    metadata_title: "Meta".to_string(),
                    metadata_summary: "Summary".to_string(),
                    metadata_number: "1".to_string(),
                    metadata_number_sort: 1.0,
                    metadata_release_date: Some("2024-01-04".to_string()),
                    metadata_title_lock: true,
                    metadata_summary_lock: true,
                    metadata_number_lock: true,
                    metadata_number_sort_lock: true,
                    metadata_release_date_lock: true,
                    metadata_authors: vec![BookMetadataAuthorReadModel {
                        name: "Author".to_string(),
                        role: "Writer".to_string(),
                    }],
                    metadata_authors_lock: true,
                    metadata_tags: vec!["tag".to_string()],
                    metadata_tags_lock: true,
                    metadata_isbn: "isbn".to_string(),
                    metadata_isbn_lock: true,
                    metadata_links: vec![BookMetadataLinkReadModel {
                        label: "Wiki".to_string(),
                        url: "https://example.com".to_string(),
                    }],
                    metadata_links_lock: true,
                    metadata_created: "2024-01-01T00:00:00Z".to_string(),
                    metadata_last_modified: "2024-01-02T00:00:00Z".to_string(),
                    media_epub_divina_compatible: true,
                    media_epub_is_kepub: true,
                    read_progress: None,
                    deleted: false,
                    file_hash: "hash".to_string(),
                    oneshot: false,
                },
                true,
            )
            .expect("book detail should map"),
        )
        .expect("book detail should serialize");

        assert_eq!(
            payload
                .get("media")
                .and_then(|value| value.get("epubDivinaCompatible"))
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            payload
                .get("metadata")
                .and_then(|value| value.get("linksLock"))
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            payload
                .get("metadata")
                .and_then(|value| value.get("links"))
                .and_then(Value::as_array)
                .map(|links| links.len()),
            Some(1)
        );
    }

    #[test]
    fn book_dto_decodes_legacy_admin_file_urls() {
        let payload = serde_json::to_value(
            BookDto::from_read_model(
                &BookDetailReadModel {
                    id: "book-1".to_string(),
                    series_id: "series-1".to_string(),
                    series_title: "Series".to_string(),
                    series_title_sort: "Series".to_string(),
                    library_id: "lib-1".to_string(),
                    name: "Book".to_string(),
                    url: "file:/library%20root/books/book%201.cbz".to_string(),
                    number: 1,
                    created: "2024-01-01T00:00:00Z".to_string(),
                    last_modified: "2024-01-02T00:00:00Z".to_string(),
                    file_last_modified: "2024-01-03T00:00:00Z".to_string(),
                    size_bytes: 123,
                    media_status: MediaStatus::Ready,
                    media_type: "application/vnd.comicbook+zip".to_string(),
                    media_pages_count: 5,
                    media_comment: "ok".to_string(),
                    metadata_title: "Meta".to_string(),
                    metadata_summary: "Summary".to_string(),
                    metadata_number: "1".to_string(),
                    metadata_number_sort: 1.0,
                    metadata_release_date: Some("2024-01-04".to_string()),
                    metadata_title_lock: false,
                    metadata_summary_lock: false,
                    metadata_number_lock: false,
                    metadata_number_sort_lock: false,
                    metadata_release_date_lock: false,
                    metadata_authors: Vec::new(),
                    metadata_authors_lock: false,
                    metadata_tags: Vec::new(),
                    metadata_tags_lock: false,
                    metadata_isbn: String::new(),
                    metadata_isbn_lock: false,
                    metadata_links: Vec::new(),
                    metadata_links_lock: false,
                    metadata_created: "2024-01-01T00:00:00Z".to_string(),
                    metadata_last_modified: "2024-01-02T00:00:00Z".to_string(),
                    media_epub_divina_compatible: false,
                    media_epub_is_kepub: false,
                    read_progress: None,
                    deleted: false,
                    file_hash: "hash".to_string(),
                    oneshot: false,
                },
                true,
            )
            .expect("book detail should map"),
        )
        .expect("book detail should serialize");

        assert_eq!(
            payload.get("url"),
            Some(&json!("/library root/books/book 1.cbz"))
        );
    }

    #[test]
    fn series_dto_formats_datetime_fields() {
        let payload = serde_json::to_value(
            SeriesDto::from_detail(
                &SeriesDetailReadModel {
                    id: "series-1".to_string(),
                    library_id: "library-1".to_string(),
                    name: "Series Shelf Name".to_string(),
                    title: "Series Metadata Title".to_string(),
                    title_sort: "Series Sort".to_string(),
                    url: "file:///data/series".to_string(),
                    created: "2024-01-01 00:00:00".to_string(),
                    last_modified: "2024-01-02 00:00:00".to_string(),
                    file_last_modified: "1704240000".to_string(),
                    books_count: 2,
                    books_read_count: 1,
                    books_unread_count: 1,
                    books_in_progress_count: 0,
                    status: SeriesStatus::Ongoing,
                    status_lock: false,
                    summary: "Summary".to_string(),
                    summary_lock: false,
                    reading_direction: Some(SeriesReadingDirection::LeftToRight),
                    reading_direction_lock: false,
                    publisher: "Publisher".to_string(),
                    publisher_lock: false,
                    age_rating: Some(13),
                    age_rating_lock: false,
                    language: "en".to_string(),
                    language_lock: false,
                    genres: vec!["Drama".to_string()],
                    genres_lock: false,
                    tags: vec!["Favorite".to_string()],
                    tags_lock: false,
                    total_book_count: Some(2),
                    total_book_count_lock: false,
                    sharing_labels: vec!["Team".to_string()],
                    sharing_labels_lock: false,
                    links: vec![SeriesMetadataLinkRecord {
                        label: "Wiki".to_string(),
                        url: "https://example.com".to_string(),
                    }],
                    links_lock: false,
                    alternate_titles: vec![SeriesAlternateTitleRecord {
                        label: "en".to_string(),
                        title: "Alt Title".to_string(),
                    }],
                    alternate_titles_lock: false,
                    title_lock: false,
                    title_sort_lock: false,
                    metadata_created: "2024-01-03 00:00:00".to_string(),
                    metadata_last_modified: "2024-01-04 00:00:00".to_string(),
                    books_metadata_tags: vec!["tag".to_string()],
                    books_metadata_authors: vec![BookMetadataAuthorReadModel {
                        name: "Author".to_string(),
                        role: "Writer".to_string(),
                    }],
                    books_metadata_release_date: Some("2024-01-15".to_string()),
                    books_metadata_summary: "Books summary".to_string(),
                    books_metadata_summary_number: "2".to_string(),
                    books_metadata_created: "2024-01-05 00:00:00".to_string(),
                    books_metadata_last_modified: "2024-01-06 00:00:00".to_string(),
                    deleted: false,
                    oneshot: true,
                },
                false,
            )
            .expect("series detail should map"),
        )
        .expect("series detail should serialize");

        assert_eq!(payload.get("created"), Some(&json!("2024-01-01T00:00:00Z")));
        assert_eq!(
            payload.get("lastModified"),
            Some(&json!("2024-01-02T00:00:00Z"))
        );
        assert_eq!(
            payload.get("fileLastModified"),
            Some(&json!("2024-01-03T00:00:00Z"))
        );
        assert_eq!(
            payload
                .get("metadata")
                .and_then(|value| value.get("created")),
            Some(&json!("2024-01-03T00:00:00Z"))
        );
        assert_eq!(
            payload
                .get("metadata")
                .and_then(|value| value.get("lastModified")),
            Some(&json!("2024-01-04T00:00:00Z"))
        );
        assert_eq!(
            payload
                .get("booksMetadata")
                .and_then(|value| value.get("created")),
            Some(&json!("2024-01-05T00:00:00Z"))
        );
        assert_eq!(
            payload
                .get("booksMetadata")
                .and_then(|value| value.get("lastModified")),
            Some(&json!("2024-01-06T00:00:00Z"))
        );
        assert_eq!(
            payload
                .get("booksMetadata")
                .and_then(|value| value.get("releaseDate")),
            Some(&json!("2024-01-15"))
        );
        assert_eq!(payload.get("url"), Some(&json!("")));
        assert_eq!(
            payload
                .get("metadata")
                .and_then(|value| value.get("totalBookCount")),
            Some(&json!(2))
        );
        assert_eq!(
            payload.get("metadata").and_then(|value| value.get("links")),
            Some(&json!([{ "label": "Wiki", "url": "https://example.com" }]))
        );
        assert_eq!(
            payload
                .get("metadata")
                .and_then(|value| value.get("alternateTitles")),
            Some(&json!([{ "label": "en", "title": "Alt Title" }]))
        );
    }
}
