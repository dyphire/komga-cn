use std::path::Path as FsPath;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Extension, Path};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use komga_application::discovery::normalize_readlists_search;
use komga_domain::discovery::PageEnvelope;
use reqwest::Url;
use serde_json::{Map, Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::discovery_detail_access::{
    books as detail_access_books, collections as detail_access_collections,
    readlists as detail_access_readlists, series as detail_access_series,
};

use crate::http::discovery_auth::{
    AgeRestrictionKind, DetailContentContext, DetailResourceContext, DiscoveryAuthState,
    DiscoveryQueryContext, QueryRestrictions,
};
use crate::http::helpers::{
    detail_access_denial_response, mark_runtime_owned, query_bool, query_value, query_values,
    restricted_book_url,
};
use crate::http::identity_access::auth::{require_admin, require_auth};
use crate::http::state::AuthDatabaseState;
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
    PersistedCollectionRecord as PersistedCollectionAccessRecord, PersistedSeriesRestrictionRecord,
};
pub use detail_access_readlists::DiscoveryDetailReadlistsAccessBackend;
pub use detail_access_readlists::{
    PersistedBookAuthorRecord, PersistedComicrackMatchCandidateRecord, PersistedReadlistBookRecord,
    PersistedReadlistRecord,
};
pub use detail_access_series::DiscoveryDetailSeriesAccessBackend;
pub use detail_access_series::{
    ExistingSeriesMetadataRecord, PersistedCollectionRecord as PersistedSeriesCollectionRecord,
    PersistedSeriesDetailRecord, PersistedSeriesResourceRecord, SeriesAlternateTitleRecord,
    SeriesMetadataLinkRecord, SeriesMetadataUpdateRecord, SeriesSummaryRecord,
};

pub use books_detail::{book_detail, book_readlists, book_sibling_next, book_sibling_previous};
pub use books_persistence::{
    PersistedBookSiblingDirection, load_persisted_book_detail, load_persisted_book_resource,
    load_persisted_book_series_id, load_persisted_book_sibling_detail,
    resolve_book_id_for_persisted,
};
pub use collections::{
    collection_create, collection_delete, collection_detail, collection_series, collection_update,
    collections,
};
pub use collections_support::{
    collection_payload, collections_page_payload, collections_unpaged_payload,
    delete_collection_search_document, delete_persisted_collection,
    load_persisted_collection_detail, load_persisted_collections, load_series_library_id,
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
pub use readlists_support::{
    PersistedReadlistBooksQuery, ReadListsSort, decode_query_component, delete_persisted_readlist,
    delete_readlist_search_document, load_persisted_readlist_detail, load_persisted_readlists,
    load_visible_persisted_readlist_books, match_comicrack_readlist,
    paginate_persisted_readlist_books, parse_comicrack_readlist,
    parse_persisted_readlist_books_query, parse_readlists_sort, persist_readlist_create,
    persist_readlist_update, readlist_payload, readlists_page_payload,
    sort_visible_persisted_readlist_books, upsert_readlist_search_document,
};
pub use series_detail::{series_collections, series_detail, series_metadata_update};
pub use series_persistence::{
    load_existing_series_metadata, load_persisted_series_collections, load_persisted_series_detail,
    load_persisted_series_resource, persist_series_metadata_update,
    resolve_series_id_for_persisted, sync_series_search_documents_after_metadata_update,
};

#[derive(Clone)]
struct PersistedReadProgress {
    page: i32,
    completed: bool,
    read_date: Option<String>,
    created: String,
    last_modified: String,
    device_id: String,
    device_name: String,
}

#[derive(Clone)]
struct BookMetadataAuthorReadModel {
    name: String,
    role: String,
}

#[derive(Clone)]
struct BookMetadataLinkReadModel {
    label: String,
    url: String,
}

#[derive(Clone)]
pub(super) struct BookDetailReadModel {
    id: String,
    series_id: String,
    series_title: String,
    pub(super) series_title_sort: String,
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
    metadata_title_lock: bool,
    metadata_summary_lock: bool,
    metadata_number_lock: bool,
    metadata_number_sort_lock: bool,
    metadata_release_date_lock: bool,
    metadata_authors: Vec<BookMetadataAuthorReadModel>,
    metadata_authors_lock: bool,
    metadata_tags: Vec<String>,
    metadata_tags_lock: bool,
    metadata_isbn: String,
    metadata_isbn_lock: bool,
    metadata_links: Vec<BookMetadataLinkReadModel>,
    metadata_links_lock: bool,
    metadata_created: String,
    metadata_last_modified: String,
    media_epub_divina_compatible: bool,
    media_epub_is_kepub: bool,
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

pub(crate) async fn load_persisted_webpub_metadata_additions(
    database_file: &FsPath,
    book_id: &str,
) -> Result<Option<(Map<String, Value>, bool)>, String> {
    let Some(book) = load_persisted_book_detail(database_file, book_id, None).await? else {
        return Ok(None);
    };

    let series = load_persisted_series_detail(database_file, &book.series_id, None).await?;

    let mut metadata = Map::new();
    if !book.metadata_summary.is_empty() {
        metadata.insert(
            "description".to_string(),
            Value::String(book.metadata_summary.clone()),
        );
    }
    if !book.metadata_isbn.is_empty() {
        metadata.insert(
            "identifier".to_string(),
            Value::String(format!("urn:isbn:{}", book.metadata_isbn)),
        );
    }
    if book.media_pages_count > 0 {
        metadata.insert(
            "numberOfPages".to_string(),
            Value::Number(book.media_pages_count.into()),
        );
    }
    if let Some(release_date) = book
        .metadata_release_date
        .as_ref()
        .filter(|value| !value.is_empty())
    {
        metadata.insert("published".to_string(), Value::String(release_date.clone()));
    }
    if !book.last_modified.is_empty() {
        metadata.insert(
            "modified".to_string(),
            Value::String(normalize_webpub_modified(&book.last_modified)),
        );
    }
    if !book.metadata_tags.is_empty() {
        metadata.insert(
            "subject".to_string(),
            Value::Array(
                book.metadata_tags
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect(),
            ),
        );
    }
    extend_webpub_metadata_with_role_authors(&mut metadata, &book.metadata_authors);
    if !book.series_title.is_empty() {
        let mut series_entry = Map::new();
        series_entry.insert("name".to_string(), Value::String(book.series_title.clone()));
        if let Some(position) = serde_json::Number::from_f64(book.metadata_number_sort) {
            series_entry.insert("position".to_string(), Value::Number(position));
        }
        metadata.insert(
            "belongsTo".to_string(),
            Value::Object(Map::from_iter([(
                "series".to_string(),
                Value::Array(vec![Value::Object(series_entry)]),
            )])),
        );
    }

    if let Some(series) = series {
        if !series.language.is_empty() {
            metadata.insert("language".to_string(), Value::String(series.language));
        }
        if let Some(reading_progression) =
            webpub_reading_progression(series.reading_direction.as_str())
        {
            metadata.insert(
                "readingProgression".to_string(),
                Value::String(reading_progression.to_string()),
            );
        }
    }

    Ok(Some((metadata, book.media_epub_divina_compatible)))
}

fn normalize_webpub_modified(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return trimmed.to_string();
    }
    if OffsetDateTime::parse(trimmed, &Rfc3339).is_ok() {
        return trimmed.to_string();
    }
    if let Some((date, time)) = trimmed.split_once(' ') {
        return format!("{date}T{time}Z");
    }
    if trimmed.contains('T') {
        return format!("{trimmed}Z");
    }
    trimmed.to_string()
}

fn webpub_reading_progression(reading_direction: &str) -> Option<&'static str> {
    match reading_direction.trim().to_ascii_uppercase().as_str() {
        "LEFT_TO_RIGHT" => Some("ltr"),
        "RIGHT_TO_LEFT" => Some("rtl"),
        "VERTICAL" | "WEBTOON" => Some("ttb"),
        _ => None,
    }
}

fn extend_webpub_metadata_with_role_authors(
    metadata: &mut Map<String, Value>,
    authors: &[BookMetadataAuthorReadModel],
) {
    let mut author = Vec::new();
    let mut translator = Vec::new();
    let mut editor = Vec::new();
    let mut artist = Vec::new();
    let mut illustrator = Vec::new();
    let mut letterer = Vec::new();
    let mut penciler = Vec::new();
    let mut colorist = Vec::new();
    let mut inker = Vec::new();
    let mut contributor = Vec::new();

    for entry in authors {
        let target = match entry.role.trim().to_ascii_lowercase().as_str() {
            "author" => &mut author,
            "translator" => &mut translator,
            "editor" => &mut editor,
            "artist" => &mut artist,
            "illustrator" => &mut illustrator,
            "letterer" => &mut letterer,
            "penciler" | "penciller" => &mut penciler,
            "colorist" => &mut colorist,
            "inker" => &mut inker,
            _ => &mut contributor,
        };
        target.push(Value::String(entry.name.clone()));
    }

    for (key, values) in [
        ("author", author),
        ("translator", translator),
        ("editor", editor),
        ("artist", artist),
        ("illustrator", illustrator),
        ("letterer", letterer),
        ("penciler", penciler),
        ("colorist", colorist),
        ("inker", inker),
        ("contributor", contributor),
    ] {
        if !values.is_empty() {
            metadata.insert(key.to_string(), Value::Array(values));
        }
    }
}

pub(super) fn book_detail_payload(book: &BookDetailReadModel, is_admin: bool) -> Value {
    let admin_url = admin_file_url(&book.url);
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
        "created": book.created,
        "lastModified": book.last_modified,
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

fn admin_file_url(url: &str) -> String {
    match Url::parse(url) {
        Ok(parsed) if parsed.scheme() == "file" => parsed
            .to_file_path()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|_| url.to_string()),
        _ => url.to_string(),
    }
}

fn normalized_file_last_modified(value: &str) -> String {
    if let Ok(epoch_seconds) = value.parse::<i64>()
        && let Ok(datetime) = OffsetDateTime::from_unix_timestamp(epoch_seconds)
        && let Ok(formatted) = datetime.format(&Rfc3339)
    {
        return formatted;
    }

    if let Ok(datetime) = OffsetDateTime::parse(value, &Rfc3339)
        && let Ok(formatted) = datetime.format(&Rfc3339)
    {
        return formatted;
    }

    value.to_string()
}

fn series_detail_payload(series: &SeriesDetailReadModel, is_admin: bool) -> Value {
    let url = if is_admin {
        admin_file_url(&series.url)
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
        Value::String(series.metadata_created.clone()),
    );
    metadata.insert(
        "lastModified".to_string(),
        Value::String(series.metadata_last_modified.clone()),
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
    payload.insert("name".to_string(), Value::String(series.name.clone()));
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

#[cfg(test)]
mod tests {
    use super::*;

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
}

fn series_collections_payload(collections: &[CollectionReadModel]) -> Value {
    Value::Array(collections.iter().map(collection_payload).collect())
}
