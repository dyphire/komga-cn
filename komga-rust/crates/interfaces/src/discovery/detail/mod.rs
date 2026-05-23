use axum::Json;
use axum::body::Bytes;
use axum::extract::Path;
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use komga_domain::discovery::PageEnvelope;
use serde_json::{Map, Value, json};

use crate::discovery_auth::context::{
    DetailContentContext, DetailResourceContext, DiscoveryQueryContext, QueryRestrictions,
};
use crate::discovery_auth::principal::AgeRestrictionKind;
use crate::helpers::{
    api_file_path, detail_access_denial_response, mark_runtime_owned, normalized_date_time,
    normalized_file_last_modified, normalized_optional_read_progress_date, query_bool, query_value,
    query_values, restricted_book_url, to_domain_query_context,
};
use crate::state::DiscoveryState;
use crate::state::{
    PersistedBookSiblingDirectionRecord, PersistedComicrackMatchCandidateRecord,
    PersistedSeriesDetailRecord, SeriesAlternateTitleRecord, SeriesMetadataLinkRecord,
    SeriesMetadataUpdateRecord,
};
use komga_application::discovery::{
    BookMetadataAuthorReadModel, BookReadModel, ReadListReadModel, ReadlistVisibilityService,
};

mod books_detail;
mod books_persistence;
mod collections;
mod collections_support;
mod detail_utils;
mod readlists;
mod readlists_support;
mod series_detail;
mod series_persistence;

pub use books_detail::{book_detail, book_readlists, book_sibling_next, book_sibling_previous};
use books_persistence::load_persisted_book_sibling_detail;
pub use books_persistence::{
    PersistedBookSiblingDirection, load_persisted_book_resource, load_persisted_book_series_id,
    resolve_book_id_for_persisted,
};
pub use collections::{
    collection_create, collection_delete, collection_detail, collection_series, collection_update,
    collections,
};
use collections_support::{
    collection_payload, collections_page_payload, collections_unpaged_payload,
    load_persisted_collection_detail, load_persisted_collections,
};
pub use collections_support::{
    delete_collection_search_document, delete_persisted_collection, load_series_library_id,
    persist_collection_create, persist_collection_update, persisted_collections_exist,
    series_visible_to_context, upsert_collection_search_document,
};
pub use detail_utils::{
    format_size_bytes, internal_error_response, media_profile_for_media_type, parse_csv_values,
    random_hex_token,
};
pub use readlists::{
    readlist_book_sibling_next, readlist_book_sibling_previous, readlist_books, readlist_create,
    readlist_delete, readlist_detail, readlist_match_comicrack, readlist_update, readlists,
};
use readlists_support::{load_persisted_readlist_detail, readlist_payload, readlists_page_payload};
pub use readlists_support::{match_comicrack_readlist, parse_comicrack_readlist};
pub use series_detail::{series_collections, series_detail, series_metadata_update};
pub use series_persistence::{
    load_existing_series_metadata, load_persisted_series_resource, persist_series_metadata_update,
    resolve_series_id_for_persisted, sync_series_search_documents_after_metadata_update,
};
use series_persistence::{load_persisted_series_collections, load_persisted_series_detail};

pub(super) type BookDetailReadModel = BookReadModel;

#[derive(Clone)]
pub(super) struct CollectionReadModel {
    id: String,
    name: String,
    ordered: bool,
    series_ids: Vec<String>,
    created_date: String,
    last_modified_date: String,
    filtered: bool,
}

#[derive(Clone)]
pub(super) struct SeriesDetailReadModel {
    id: String,
    library_id: String,
    name: String,
    title: String,
    title_sort: String,
    url: String,
    created: String,
    last_modified: String,
    file_last_modified: String,
    books_count: u32,
    books_read_count: u32,
    books_unread_count: u32,
    books_in_progress_count: u32,
    status: String,
    status_lock: bool,
    summary: String,
    summary_lock: bool,
    reading_direction: String,
    reading_direction_lock: bool,
    publisher: String,
    publisher_lock: bool,
    age_rating: Option<u32>,
    age_rating_lock: bool,
    language: String,
    language_lock: bool,
    genres: Vec<String>,
    genres_lock: bool,
    tags: Vec<String>,
    tags_lock: bool,
    total_book_count: Option<u32>,
    total_book_count_lock: bool,
    sharing_labels: Vec<String>,
    sharing_labels_lock: bool,
    links: Vec<SeriesMetadataLinkRecord>,
    links_lock: bool,
    alternate_titles: Vec<SeriesAlternateTitleRecord>,
    alternate_titles_lock: bool,
    title_lock: bool,
    title_sort_lock: bool,
    metadata_created: String,
    metadata_last_modified: String,
    books_metadata_tags: Vec<String>,
    books_metadata_authors: Vec<BookMetadataAuthorReadModel>,
    books_metadata_release_date: Option<String>,
    books_metadata_summary: String,
    books_metadata_summary_number: String,
    books_metadata_created: String,
    books_metadata_last_modified: String,
    deleted: bool,
    oneshot: bool,
}

pub(super) async fn load_persisted_book_detail(
    app: &DiscoveryState,
    book_id: &str,
    user_id: Option<&str>,
) -> Result<Option<BookReadModel>, String> {
    books_persistence::load_persisted_book_detail(app, book_id, user_id).await
}

pub(super) fn book_detail_payload(book: &BookReadModel, is_admin: bool) -> Value {
    let admin_url = api_file_path(&book.url);
    let url = if is_admin {
        admin_url
    } else {
        restricted_book_url(&admin_url, false)
    };
    let media_profile = media_profile_for_media_type(&book.media_type);

    json!({
        "id": book.id,
        "seriesId": book.series_id,
        "seriesTitle": book.series_title,
        "libraryId": book.library_id,
        "name": book.name,
        "url": url,
        "number": book.number,
        "created": normalized_date_time(&book.created),
        "lastModified": normalized_date_time(&book.last_modified),
        "fileLastModified": normalized_file_last_modified(&book.file_last_modified),
        "sizeBytes": book.size_bytes,
        "size": format_size_bytes(book.size_bytes),
        "media": {
            "status": book.media_status,
            "mediaType": book.media_type,
            "pagesCount": book.media_pages_count,
            "comment": book.media_comment,
            "epubDivinaCompatible": book.media_epub_divina_compatible,
            "epubIsKepub": book.media_epub_is_kepub,
            "mediaProfile": media_profile
        },
        "metadata": {
            "title": book.metadata_title,
            "titleLock": book.metadata_title_lock,
            "summary": book.metadata_summary,
            "summaryLock": book.metadata_summary_lock,
            "number": book.metadata_number,
            "numberLock": book.metadata_number_lock,
            "numberSort": book.metadata_number_sort,
            "numberSortLock": book.metadata_number_sort_lock,
            "releaseDate": book.metadata_release_date,
            "releaseDateLock": book.metadata_release_date_lock,
            "authors": book.metadata_authors.iter().map(|author| json!({ "name": author.name, "role": author.role })).collect::<Vec<_>>(),
            "authorsLock": book.metadata_authors_lock,
            "tags": book.metadata_tags,
            "tagsLock": book.metadata_tags_lock,
            "isbn": book.metadata_isbn,
            "isbnLock": book.metadata_isbn_lock,
            "links": book.metadata_links.iter().map(|link| json!({ "label": link.label, "url": link.url })).collect::<Vec<_>>(),
            "linksLock": book.metadata_links_lock,
            "created": normalized_date_time(&book.metadata_created),
            "lastModified": normalized_date_time(&book.metadata_last_modified)
        },
        "readProgress": book.read_progress.as_ref().map_or(Value::Null, |progress| json!({
            "page": progress.page,
            "completed": progress.completed,
            "readDate": normalized_optional_read_progress_date(progress.read_date.as_deref(), &progress.last_modified, &progress.created),
            "created": normalized_date_time(&progress.created),
            "lastModified": normalized_date_time(&progress.last_modified),
            "deviceId": progress.device_id,
            "deviceName": progress.device_name,
        })),
        "deleted": book.deleted,
        "fileHash": book.file_hash,
        "oneshot": book.oneshot
    })
}

fn series_detail_payload(series: &SeriesDetailReadModel, is_admin: bool) -> Value {
    let url = if is_admin {
        api_file_path(&series.url)
    } else {
        String::new()
    };

    let mut metadata = Map::new();
    metadata.insert("status".to_string(), Value::String(series.status.clone()));
    metadata.insert("statusLock".to_string(), Value::Bool(series.status_lock));
    metadata.insert("title".to_string(), Value::String(series.title.clone()));
    metadata.insert("titleLock".to_string(), Value::Bool(series.title_lock));
    metadata.insert(
        "titleSort".to_string(),
        Value::String(series.title_sort.clone()),
    );
    metadata.insert(
        "titleSortLock".to_string(),
        Value::Bool(series.title_sort_lock),
    );
    metadata.insert("summary".to_string(), Value::String(series.summary.clone()));
    metadata.insert("summaryLock".to_string(), Value::Bool(series.summary_lock));
    metadata.insert(
        "readingDirection".to_string(),
        Value::String(series.reading_direction.clone()),
    );
    metadata.insert(
        "readingDirectionLock".to_string(),
        Value::Bool(series.reading_direction_lock),
    );
    metadata.insert(
        "publisher".to_string(),
        Value::String(series.publisher.clone()),
    );
    metadata.insert(
        "publisherLock".to_string(),
        Value::Bool(series.publisher_lock),
    );
    metadata.insert(
        "ageRating".to_string(),
        series
            .age_rating
            .map_or(Value::Null, |it| Value::Number(it.into())),
    );
    metadata.insert(
        "ageRatingLock".to_string(),
        Value::Bool(series.age_rating_lock),
    );
    metadata.insert(
        "language".to_string(),
        Value::String(series.language.clone()),
    );
    metadata.insert(
        "languageLock".to_string(),
        Value::Bool(series.language_lock),
    );
    metadata.insert(
        "genres".to_string(),
        Value::Array(series.genres.iter().cloned().map(Value::String).collect()),
    );
    metadata.insert("genresLock".to_string(), Value::Bool(series.genres_lock));
    metadata.insert(
        "tags".to_string(),
        Value::Array(series.tags.iter().cloned().map(Value::String).collect()),
    );
    metadata.insert("tagsLock".to_string(), Value::Bool(series.tags_lock));
    metadata.insert(
        "totalBookCount".to_string(),
        series
            .total_book_count
            .map_or(Value::Null, |it| Value::Number(it.into())),
    );
    metadata.insert(
        "totalBookCountLock".to_string(),
        Value::Bool(series.total_book_count_lock),
    );
    metadata.insert(
        "sharingLabels".to_string(),
        Value::Array(
            series
                .sharing_labels
                .iter()
                .cloned()
                .map(Value::String)
                .collect(),
        ),
    );
    metadata.insert(
        "sharingLabelsLock".to_string(),
        Value::Bool(series.sharing_labels_lock),
    );
    metadata.insert(
        "links".to_string(),
        Value::Array(
            series
                .links
                .iter()
                .map(|link| json!({ "label": link.label, "url": link.url }))
                .collect(),
        ),
    );
    metadata.insert("linksLock".to_string(), Value::Bool(series.links_lock));
    metadata.insert(
        "alternateTitles".to_string(),
        Value::Array(
            series
                .alternate_titles
                .iter()
                .map(|title| json!({ "label": title.label, "title": title.title }))
                .collect(),
        ),
    );
    metadata.insert(
        "alternateTitlesLock".to_string(),
        Value::Bool(series.alternate_titles_lock),
    );
    metadata.insert(
        "created".to_string(),
        Value::String(normalized_date_time(&series.metadata_created)),
    );
    metadata.insert(
        "lastModified".to_string(),
        Value::String(normalized_date_time(&series.metadata_last_modified)),
    );

    let mut books_metadata = Map::new();
    books_metadata.insert(
        "authors".to_string(),
        Value::Array(
            series
                .books_metadata_authors
                .iter()
                .map(|author| json!({ "name": author.name, "role": author.role }))
                .collect(),
        ),
    );
    books_metadata.insert(
        "tags".to_string(),
        Value::Array(
            series
                .books_metadata_tags
                .iter()
                .cloned()
                .map(Value::String)
                .collect(),
        ),
    );
    books_metadata.insert(
        "releaseDate".to_string(),
        series
            .books_metadata_release_date
            .clone()
            .map_or(Value::Null, Value::String),
    );
    books_metadata.insert(
        "summary".to_string(),
        Value::String(series.books_metadata_summary.clone()),
    );
    books_metadata.insert(
        "summaryNumber".to_string(),
        Value::String(series.books_metadata_summary_number.clone()),
    );
    books_metadata.insert(
        "created".to_string(),
        Value::String(normalized_date_time(&series.books_metadata_created)),
    );
    books_metadata.insert(
        "lastModified".to_string(),
        Value::String(normalized_date_time(&series.books_metadata_last_modified)),
    );

    let mut payload = Map::new();
    payload.insert("id".to_string(), Value::String(series.id.clone()));
    payload.insert(
        "libraryId".to_string(),
        Value::String(series.library_id.clone()),
    );
    payload.insert("name".to_string(), Value::String(series.name.clone()));
    payload.insert("url".to_string(), Value::String(url));
    payload.insert(
        "created".to_string(),
        Value::String(normalized_date_time(&series.created)),
    );
    payload.insert(
        "lastModified".to_string(),
        Value::String(normalized_date_time(&series.last_modified)),
    );
    payload.insert(
        "fileLastModified".to_string(),
        Value::String(normalized_file_last_modified(&series.file_last_modified)),
    );
    payload.insert(
        "booksCount".to_string(),
        Value::Number(series.books_count.into()),
    );
    payload.insert(
        "booksReadCount".to_string(),
        Value::Number(series.books_read_count.into()),
    );
    payload.insert(
        "booksUnreadCount".to_string(),
        Value::Number(series.books_unread_count.into()),
    );
    payload.insert(
        "booksInProgressCount".to_string(),
        Value::Number(series.books_in_progress_count.into()),
    );
    payload.insert("metadata".to_string(), Value::Object(metadata));
    payload.insert("booksMetadata".to_string(), Value::Object(books_metadata));
    payload.insert("deleted".to_string(), Value::Bool(series.deleted));
    payload.insert("oneshot".to_string(), Value::Bool(series.oneshot));

    Value::Object(payload)
}

fn series_collections_payload(collections: &[CollectionReadModel]) -> Value {
    Value::Array(collections.iter().map(collection_payload).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use komga_application::discovery::BookMetadataLinkReadModel;

    #[test]
    fn book_detail_payload_uses_persisted_lock_link_and_media_flags() {
        let payload = book_detail_payload(
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
                media_status: "READY".to_string(),
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
        );

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
    fn book_detail_payload_decodes_legacy_admin_file_urls() {
        let payload = book_detail_payload(
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
                media_status: "READY".to_string(),
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
        );

        assert_eq!(
            payload.get("url"),
            Some(&json!("/library root/books/book 1.cbz"))
        );
    }

    #[test]
    fn series_detail_payload_normalizes_datetime_fields() {
        let payload = series_detail_payload(
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
                status: "ONGOING".to_string(),
                status_lock: false,
                summary: "Summary".to_string(),
                summary_lock: false,
                reading_direction: "LEFT_TO_RIGHT".to_string(),
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
        );

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
    }
}
