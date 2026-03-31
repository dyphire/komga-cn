use std::path::Path as FsPath;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Extension, Path};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use komga_application::discovery::normalize_readlists_search;
use komga_domain::discovery::{DiscoveryError, PageEnvelope};
use serde_json::{Map, Value, json};

use crate::discovery_detail_access::{
    books as detail_access_books, collections as detail_access_collections,
    readlists as detail_access_readlists, series as detail_access_series,
};

use crate::http::discovery_auth::{
    AgeRestrictionKind, DetailContentContext, DetailResourceContext, DiscoveryAuthState,
    DiscoveryQueryContext, QueryRestrictions,
};
use crate::http::helpers::{
    apply_persisted_diagnostics, detail_access_denial_response, mark_persisted_owned,
    mark_runtime_owned, query_bool, query_value, query_values, restricted_book_url,
};
use crate::http::identity_access::auth::{require_admin, require_auth};
use crate::http::state::AuthDatabaseState;
use crate::http::state::RuntimeProfile;

#[path = "detail/books_detail.rs"]
mod books_detail;
#[path = "detail/books_persistence.rs"]
mod books_persistence;
#[path = "detail/collections.rs"]
mod collections;
#[path = "detail/collections_support.rs"]
mod collections_support;
#[path = "detail/detail_utils.rs"]
mod detail_utils;
#[path = "detail/readlists.rs"]
mod readlists;
#[path = "detail/readlists_support.rs"]
mod readlists_support;
#[path = "detail/series_detail.rs"]
mod series_detail;
#[path = "detail/series_persistence.rs"]
mod series_persistence;

pub struct DiscoveryDetailAccessBackends {
    pub books: detail_access_books::DiscoveryDetailBooksAccessBackend,
    pub collections: detail_access_collections::DiscoveryDetailCollectionsAccessBackend,
    pub readlists: detail_access_readlists::DiscoveryDetailReadlistsAccessBackend,
    pub series: detail_access_series::DiscoveryDetailSeriesAccessBackend,
}

pub fn install_discovery_detail_access_backends(backends: DiscoveryDetailAccessBackends) {
    detail_access_books::install_backend(backends.books);
    detail_access_collections::install_backend(backends.collections);
    detail_access_readlists::install_backend(backends.readlists);
    detail_access_series::install_backend(backends.series);
}

pub use detail_access_books::DiscoveryDetailBooksAccessBackend;
pub use detail_access_books::{
    PersistedBookDetailRecord, PersistedBookResourceRecord, PersistedBookSiblingDirectionRecord,
    PersistedReadProgressRecord,
};
pub use detail_access_collections::DiscoveryDetailCollectionsAccessBackend;
pub use detail_access_collections::{
    PersistedCollectionRecord as PersistedCollectionAccessRecord,
    PersistedCollectionSeriesRecord as PersistedCollectionSeriesAccessRecord,
    PersistedSeriesRestrictionRecord,
};
pub use detail_access_readlists::DiscoveryDetailReadlistsAccessBackend;
pub use detail_access_readlists::{PersistedReadlistBookRecord, PersistedReadlistRecord};
pub use detail_access_series::DiscoveryDetailSeriesAccessBackend;
pub use detail_access_series::{
    ExistingSeriesMetadataRecord, PersistedCollectionRecord as PersistedSeriesCollectionRecord,
    PersistedSeriesDetailRecord, PersistedSeriesResourceRecord, SeriesSummaryRecord,
};

pub use books_detail::{book_detail, book_readlists, book_sibling_next, book_sibling_previous};
pub use books_persistence::{
    PersistedBookSiblingDirection, load_persisted_book_detail, load_persisted_book_resource,
    load_persisted_book_sibling_detail, resolve_book_id_for_persisted,
};
pub use collections::{
    collection_create, collection_delete, collection_detail, collection_series, collection_update,
    collections,
};
pub use collections_support::{
    collection_payload, collection_series_page_payload, collection_write_input,
    collections_page_payload, delete_persisted_collection, load_persisted_collection_detail,
    load_persisted_collection_series, load_persisted_collections, persist_collection_create,
    persist_collection_update, persisted_collections_exist, series_visible_to_context,
};
pub use detail_utils::{
    coerce_library_id_for_id_bridge, format_size_bytes, internal_error_response,
    media_profile_for_media_type, parse_csv_values, random_hex_token, uses_id_bridge,
};
pub use readlists::{
    readlist_book_sibling_next, readlist_book_sibling_previous, readlist_books, readlist_create,
    readlist_delete, readlist_detail, readlist_match_comicrack, readlist_update, readlists,
};
pub use readlists_support::{
    ReadListsSort, decode_query_component, delete_persisted_readlist,
    load_persisted_readlist_detail, load_persisted_readlists, parse_optional_query_bool,
    parse_readlists_sort, persist_readlist_create, persist_readlist_update,
    persisted_readlists_exist, readlist_author_query_values, readlist_payload,
    readlist_query_values, readlist_search_score, readlist_write_input, readlists_page_payload,
};
pub use series_detail::{series_collections, series_detail, series_metadata_update};
pub use series_persistence::{
    load_existing_series_metadata, load_persisted_series_collections, load_persisted_series_detail,
    load_persisted_series_resource, persist_series_metadata_update, refresh_series_search_document,
    resolve_series_id_for_persisted,
};

#[derive(Clone)]
struct PersistedReadProgress {
    page: i32,
    completed: bool,
    read_date: Option<String>,
    created: String,
    last_modified: String,
    device_id: Option<String>,
    device_name: Option<String>,
}

#[derive(Clone)]
pub(super) struct BookDetailReadModel {
    id: String,
    series_id: String,
    series_title: String,
    library_id: String,
    name: String,
    url: String,
    number: i32,
    created: String,
    last_modified: String,
    file_last_modified: String,
    size_bytes: u64,
    media_status: String,
    media_type: String,
    media_pages_count: u32,
    media_comment: String,
    metadata_title: String,
    metadata_summary: String,
    metadata_number: String,
    metadata_number_sort: f64,
    metadata_release_date: Option<String>,
    metadata_authors: Vec<String>,
    metadata_tags: Vec<String>,
    metadata_isbn: String,
    metadata_created: String,
    metadata_last_modified: String,
    read_progress: Option<PersistedReadProgress>,
    deleted: bool,
    file_hash: String,
    oneshot: bool,
}

#[derive(Clone)]
pub(super) struct ReadListReadModel {
    id: String,
    name: String,
    summary: String,
    ordered: bool,
    book_ids: Vec<String>,
    created_date: String,
    last_modified_date: String,
    filtered: bool,
}

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
    summary: String,
    reading_direction: String,
    publisher: String,
    age_rating: Option<u16>,
    language: String,
    genres: Vec<String>,
    tags: Vec<String>,
    total_book_count: Option<u32>,
    sharing_labels: Vec<String>,
    alternate_titles: Vec<String>,
    metadata_created: String,
    metadata_last_modified: String,
    books_metadata_tags: Vec<String>,
    books_metadata_release_date: Option<String>,
    books_metadata_summary: String,
    books_metadata_summary_number: String,
    books_metadata_created: String,
    books_metadata_last_modified: String,
    deleted: bool,
    oneshot: bool,
}

fn book_detail_payload(book: &BookDetailReadModel, is_admin: bool) -> Value {
    let url = restricted_book_url(&book.url, is_admin);
    let media_profile = media_profile_for_media_type(&book.media_type);

    json!({
        "id": book.id,
        "seriesId": book.series_id,
        "seriesTitle": book.series_title,
        "libraryId": book.library_id,
        "name": book.name,
        "url": url,
        "number": book.number,
        "created": book.created,
        "lastModified": book.last_modified,
        "fileLastModified": book.file_last_modified,
        "sizeBytes": book.size_bytes,
        "size": format_size_bytes(book.size_bytes),
        "media": {
            "status": book.media_status,
            "mediaType": book.media_type,
            "pagesCount": book.media_pages_count,
            "comment": book.media_comment,
            "epubDivinaCompatible": false,
            "epubIsKepub": false,
            "mediaProfile": media_profile
        },
        "metadata": {
            "title": book.metadata_title,
            "titleLock": false,
            "summary": book.metadata_summary,
            "summaryLock": false,
            "number": book.metadata_number,
            "numberLock": false,
            "numberSort": book.metadata_number_sort,
            "numberSortLock": false,
            "releaseDate": book.metadata_release_date,
            "releaseDateLock": false,
            "authors": book.metadata_authors.iter().map(|name| json!({ "name": name, "role": "writer" })).collect::<Vec<_>>(),
            "authorsLock": false,
            "tags": book.metadata_tags,
            "tagsLock": false,
            "isbn": book.metadata_isbn,
            "isbnLock": false,
            "links": [],
            "linksLock": false,
            "created": book.metadata_created,
            "lastModified": book.metadata_last_modified
        },
        "readProgress": book.read_progress.as_ref().map_or(Value::Null, |progress| json!({
            "page": progress.page,
            "completed": progress.completed,
            "readDate": progress.read_date,
            "created": progress.created,
            "lastModified": progress.last_modified,
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
        series.url.clone()
    } else {
        String::new()
    };

    let mut metadata = Map::new();
    metadata.insert("status".to_string(), Value::String(series.status.clone()));
    metadata.insert("statusLock".to_string(), Value::Bool(false));
    metadata.insert("title".to_string(), Value::String(series.title.clone()));
    metadata.insert("titleLock".to_string(), Value::Bool(false));
    metadata.insert(
        "titleSort".to_string(),
        Value::String(series.title_sort.clone()),
    );
    metadata.insert("titleSortLock".to_string(), Value::Bool(false));
    metadata.insert("summary".to_string(), Value::String(series.summary.clone()));
    metadata.insert("summaryLock".to_string(), Value::Bool(false));
    metadata.insert(
        "readingDirection".to_string(),
        Value::String(series.reading_direction.clone()),
    );
    metadata.insert("readingDirectionLock".to_string(), Value::Bool(false));
    metadata.insert(
        "publisher".to_string(),
        Value::String(series.publisher.clone()),
    );
    metadata.insert("publisherLock".to_string(), Value::Bool(false));
    metadata.insert(
        "ageRating".to_string(),
        series
            .age_rating
            .map_or(Value::Null, |it| Value::Number(it.into())),
    );
    metadata.insert("ageRatingLock".to_string(), Value::Bool(false));
    metadata.insert(
        "language".to_string(),
        Value::String(series.language.clone()),
    );
    metadata.insert("languageLock".to_string(), Value::Bool(false));
    metadata.insert(
        "genres".to_string(),
        Value::Array(series.genres.iter().cloned().map(Value::String).collect()),
    );
    metadata.insert("genresLock".to_string(), Value::Bool(false));
    metadata.insert(
        "tags".to_string(),
        Value::Array(series.tags.iter().cloned().map(Value::String).collect()),
    );
    metadata.insert("tagsLock".to_string(), Value::Bool(false));
    metadata.insert(
        "totalBookCount".to_string(),
        series
            .total_book_count
            .map_or(Value::Null, |it| Value::Number(it.into())),
    );
    metadata.insert("totalBookCountLock".to_string(), Value::Bool(false));
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
    metadata.insert("sharingLabelsLock".to_string(), Value::Bool(false));
    metadata.insert("links".to_string(), Value::Array(vec![]));
    metadata.insert("linksLock".to_string(), Value::Bool(false));
    metadata.insert(
        "alternateTitles".to_string(),
        Value::Array(
            series
                .alternate_titles
                .iter()
                .cloned()
                .map(Value::String)
                .collect(),
        ),
    );
    metadata.insert("alternateTitlesLock".to_string(), Value::Bool(false));
    metadata.insert(
        "created".to_string(),
        Value::String(series.metadata_created.clone()),
    );
    metadata.insert(
        "lastModified".to_string(),
        Value::String(series.metadata_last_modified.clone()),
    );

    let mut books_metadata = Map::new();
    books_metadata.insert("authors".to_string(), Value::Array(vec![]));
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
        Value::String(series.books_metadata_created.clone()),
    );
    books_metadata.insert(
        "lastModified".to_string(),
        Value::String(series.books_metadata_last_modified.clone()),
    );

    let mut payload = Map::new();
    payload.insert("id".to_string(), Value::String(series.id.clone()));
    payload.insert(
        "libraryId".to_string(),
        Value::String(series.library_id.clone()),
    );
    payload.insert("name".to_string(), Value::String(series.title.clone()));
    payload.insert("url".to_string(), Value::String(url));
    payload.insert("created".to_string(), Value::String(series.created.clone()));
    payload.insert(
        "lastModified".to_string(),
        Value::String(series.last_modified.clone()),
    );
    payload.insert(
        "fileLastModified".to_string(),
        Value::String(series.file_last_modified.clone()),
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
