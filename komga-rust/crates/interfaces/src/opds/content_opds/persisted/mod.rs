use std::collections::{HashMap, HashSet};

use axum::Json;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use crate::discovery_auth::principal::AgeRestrictionKind;
use crate::identity_access::auth::{
    AuthUser, user_payload_json, user_shared_all_libraries, user_shared_library_ids,
};
use crate::state::{
    OpdsBookAuthorEntry, OpdsCatalogAccess, OpdsPersistedAccess, PersistedLibraryRecord,
    PersistedReadlistBookRecord, PersistedReadlistRecord, PersistedSeriesBookRecord,
    PersistedSeriesRecord,
};

use super::types::{
    OpdsRestrictions, PersistedBookSearchResult, PersistedCollection,
    PersistedCollectionSearchResult, PersistedLibrary, PersistedReadlist, PersistedReadlistBook,
    PersistedReadlistSearchResult, PersistedSeries, PersistedSeriesBook,
    PersistedSeriesSearchResult,
};

mod catalog_queries;

pub(super) fn allowed_library_ids_for_user(user: &AuthUser) -> Option<HashSet<String>> {
    if user_shared_all_libraries(user) {
        return None;
    }

    let ids = user_shared_library_ids(user)
        .iter()
        .cloned()
        .collect::<HashSet<_>>();
    Some(ids)
}

pub(super) fn library_visible(allowed: &Option<HashSet<String>>, library_id: &str) -> bool {
    match allowed {
        None => true,
        Some(ids) => ids.contains(library_id),
    }
}

pub(super) fn opds_restrictions_for_user(user: &AuthUser) -> Option<OpdsRestrictions> {
    let payload = user_payload_json(user);

    let age = payload
        .get("ageRestriction")
        .and_then(|value| value.get("age"))
        .and_then(Value::as_u64)
        .and_then(|value| u16::try_from(value).ok());
    let age_restriction = payload
        .get("ageRestriction")
        .and_then(|value| value.get("restriction"))
        .and_then(Value::as_str)
        .and_then(|value| match value.trim().to_ascii_uppercase().as_str() {
            "ALLOW_ONLY" => Some(AgeRestrictionKind::AllowOnly),
            "EXCLUDE" => Some(AgeRestrictionKind::Exclude),
            _ => None,
        });
    let labels_allow = payload
        .get("labelsAllow")
        .and_then(Value::as_array)
        .map(|labels| {
            labels
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| value.to_ascii_lowercase())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let labels_exclude = payload
        .get("labelsExclude")
        .and_then(Value::as_array)
        .map(|labels| {
            labels
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| value.to_ascii_lowercase())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if age.is_none()
        && age_restriction.is_none()
        && labels_allow.is_empty()
        && labels_exclude.is_empty()
    {
        None
    } else {
        Some(OpdsRestrictions {
            age,
            age_restriction,
            labels_allow,
            labels_exclude,
        })
    }
}

fn normalized_sharing_labels(labels: &[String]) -> Vec<String> {
    labels
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect()
}

pub(super) fn content_allowed_by_restrictions(
    restrictions: Option<&OpdsRestrictions>,
    age_rating: Option<u16>,
    sharing_labels: &[String],
) -> bool {
    let Some(restrictions) = restrictions else {
        return true;
    };

    let labels = normalized_sharing_labels(sharing_labels);

    let age_allowed = if restrictions.age_restriction == Some(AgeRestrictionKind::AllowOnly) {
        restrictions
            .age
            .map(|age_limit| age_rating.is_some_and(|age| age <= age_limit))
    } else {
        None
    };
    let label_allowed = if restrictions.labels_allow.is_empty() {
        None
    } else {
        Some(
            restrictions
                .labels_allow
                .iter()
                .any(|candidate| labels.contains(candidate)),
        )
    };

    let allowed = match (age_allowed, label_allowed) {
        (None, label_allowed) => label_allowed != Some(false),
        (age_allowed, None) => age_allowed != Some(false),
        (age_allowed, label_allowed) => age_allowed != Some(false) || label_allowed != Some(false),
    };
    if !allowed {
        return false;
    }

    let age_denied = if restrictions.age_restriction == Some(AgeRestrictionKind::Exclude) {
        restrictions
            .age
            .is_some_and(|age_limit| age_rating.is_some_and(|age| age >= age_limit))
    } else {
        false
    };
    let label_denied = if restrictions.labels_exclude.is_empty() {
        false
    } else {
        restrictions
            .labels_exclude
            .iter()
            .any(|candidate| labels.contains(candidate))
    };

    !age_denied && !label_denied
}

pub(super) async fn load_libraries(
    backend: &OpdsPersistedAccess,
) -> Result<Vec<PersistedLibrary>, String> {
    let records = backend.load_libraries().await?;
    Ok(records.into_iter().map(map_library_record).collect())
}

pub(super) async fn load_library(
    backend: &OpdsPersistedAccess,
    library_id: &str,
) -> Result<Option<PersistedLibrary>, String> {
    let record = backend.load_library(library_id).await?;
    Ok(record.map(map_library_record))
}

pub(super) async fn load_readlists_for_library(
    backend: &OpdsPersistedAccess,
    library_id: &str,
) -> Result<Vec<PersistedReadlist>, String> {
    let records = backend.load_readlists_for_library(library_id).await?;
    Ok(records.into_iter().map(map_readlist_record).collect())
}

pub(super) async fn load_series(
    backend: &OpdsPersistedAccess,
    series_id: &str,
) -> Result<Option<PersistedSeries>, String> {
    let record = backend.load_series(series_id).await?;
    Ok(record.map(map_series_record))
}

pub(super) async fn load_series_books_paged(
    backend: &OpdsPersistedAccess,
    series_id: &str,
    user_id: &str,
    offset: i64,
    limit: i64,
) -> Result<Vec<PersistedSeriesBook>, String> {
    let records = backend
        .load_series_books_paged(series_id, user_id, offset, limit)
        .await?;
    Ok(records.into_iter().map(map_series_book_record).collect())
}

pub(super) async fn load_readlist(
    backend: &OpdsPersistedAccess,
    readlist_id: &str,
) -> Result<Option<PersistedReadlist>, String> {
    let record = backend.load_readlist(readlist_id).await?;
    Ok(record.map(map_readlist_record))
}

pub(super) async fn load_readlist_books(
    backend: &OpdsPersistedAccess,
    readlist_id: &str,
) -> Result<Vec<PersistedReadlistBook>, String> {
    let records = backend.load_readlist_books(readlist_id).await?;
    Ok(records.into_iter().map(map_readlist_book_record).collect())
}

pub(super) async fn load_unified_search_results(
    backend: &OpdsPersistedAccess,
    query: &str,
) -> Result<
    (
        Vec<PersistedSeriesSearchResult>,
        Vec<PersistedBookSearchResult>,
        Vec<PersistedCollectionSearchResult>,
        Vec<PersistedReadlistSearchResult>,
    ),
    String,
> {
    let (series_rows, book_rows, collection_rows, readlist_rows) =
        backend.load_unified_search_results(query).await?;

    Ok((
        series_rows
            .into_iter()
            .map(|row| PersistedSeriesSearchResult {
                id: row.id,
                title: row.title,
                library_id: row.library_id,
                age_rating: row.age_rating,
                sharing_labels: row.sharing_labels,
                last_modified: row.last_modified,
            })
            .collect(),
        book_rows
            .into_iter()
            .map(|row| PersistedBookSearchResult {
                id: row.id,
                series_id: row.series_id,
                title: row.title,
                series_title: row.series_title,
                number: row.number,
                number_sort: row.number_sort,
                summary: row.summary,
                isbn: row.isbn,
                authors: row
                    .authors
                    .into_iter()
                    .map(|author| OpdsBookAuthorEntry {
                        name: author.name,
                        role: author.role,
                    })
                    .collect(),
                tags: row.tags,
                file_name: row.file_name,
                file_size: row.file_size,
                media_type: row.media_type,
                page_count: row.page_count,
                epub_divina_compatible: row.epub_divina_compatible,
                library_id: row.library_id,
                age_rating: row.age_rating,
                sharing_labels: row.sharing_labels,
                last_modified: row.last_modified,
                release_date: row.release_date,
            })
            .collect(),
        collection_rows
            .into_iter()
            .map(|row| PersistedCollectionSearchResult {
                id: row.id,
                name: row.name,
            })
            .collect(),
        readlist_rows
            .into_iter()
            .map(|row| PersistedReadlistSearchResult {
                id: row.id,
                name: row.name,
            })
            .collect(),
    ))
}

pub(super) async fn load_opds_v1_series_search_results(
    persisted_backend: &OpdsPersistedAccess,
    catalog_backend: &OpdsCatalogAccess,
    allowed_library_ids: &Option<HashSet<String>>,
    search: &str,
    publishers: &[String],
) -> Result<Vec<PersistedSeriesSearchResult>, String> {
    let (series, _, _, _) = load_unified_search_results(persisted_backend, search).await?;

    if publishers.is_empty() {
        return Ok(series
            .into_iter()
            .filter(|row| library_visible(allowed_library_ids, &row.library_id))
            .collect());
    }

    let visible_publisher_rows = load_series_page(
        catalog_backend,
        allowed_library_ids,
        None,
        publishers,
        0,
        i64::MAX,
    )
    .await?;
    let visible_by_id = visible_publisher_rows
        .into_iter()
        .map(|row| (row.id.clone(), row))
        .collect::<HashMap<_, _>>();

    Ok(series
        .into_iter()
        .filter(|row| library_visible(allowed_library_ids, &row.library_id))
        .filter_map(|row| {
            visible_by_id
                .contains_key(&row.id)
                .then_some(PersistedSeriesSearchResult {
                    id: row.id,
                    title: row.title,
                    library_id: row.library_id,
                    age_rating: row.age_rating,
                    sharing_labels: row.sharing_labels,
                    last_modified: row.last_modified,
                })
        })
        .collect())
}

pub(super) async fn load_publishers(
    backend: &OpdsPersistedAccess,
    allowed_library_ids: &Option<HashSet<String>>,
) -> Result<Vec<String>, String> {
    backend.load_publishers(allowed_library_ids.as_ref()).await
}

pub(super) async fn has_visible_collections_for_scope(
    backend: &OpdsPersistedAccess,
    allowed_library_ids: &Option<HashSet<String>>,
    restrictions: Option<&OpdsRestrictions>,
    library_id: Option<&str>,
) -> bool {
    let collections = match load_collections(backend, library_id).await {
        Ok(collections) => collections,
        Err(_) => return false,
    };
    for collection in collections {
        let series = match load_collection_series(backend, &collection.id, collection.ordered).await
        {
            Ok(series) => series,
            Err(_) => continue,
        };
        if series.iter().any(|series| {
            library_visible(allowed_library_ids, &series.library_id)
                && content_allowed_by_restrictions(
                    restrictions,
                    series.age_rating,
                    &series.sharing_labels,
                )
        }) {
            return true;
        }
    }
    false
}

pub(super) async fn has_visible_readlists_for_scope(
    catalog_backend: &OpdsCatalogAccess,
    persisted_backend: &OpdsPersistedAccess,
    allowed_library_ids: &Option<HashSet<String>>,
    restrictions: Option<&OpdsRestrictions>,
    library_id: Option<&str>,
) -> bool {
    if let Some(id) = library_id {
        for readlist in load_readlists_for_library(persisted_backend, id)
            .await
            .unwrap_or_default()
        {
            let books = match load_readlist_books(persisted_backend, &readlist.id).await {
                Ok(books) => books,
                Err(_) => continue,
            };
            if books.iter().any(|book| {
                library_visible(allowed_library_ids, &book.library_id)
                    && content_allowed_by_restrictions(
                        restrictions,
                        book.age_rating,
                        &book.sharing_labels,
                    )
            }) {
                return true;
            }
        }
        return false;
    }

    for readlist in load_all_readlists(catalog_backend)
        .await
        .unwrap_or_default()
    {
        let books = match load_readlist_books(persisted_backend, &readlist.id).await {
            Ok(books) => books,
            Err(_) => continue,
        };
        if books.iter().any(|book| {
            library_visible(allowed_library_ids, &book.library_id)
                && content_allowed_by_restrictions(
                    restrictions,
                    book.age_rating,
                    &book.sharing_labels,
                )
        }) {
            return true;
        }
    }
    false
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn load_browse_series_navigation(
    backend: &OpdsCatalogAccess,
    headers: &HeaderMap,
    allowed_library_ids: &Option<HashSet<String>>,
    library_id: Option<&str>,
    publishers: &[String],
    page: usize,
    size: usize,
) -> Result<(Vec<Value>, usize), String> {
    catalog_queries::load_browse_series_navigation(
        backend,
        headers,
        allowed_library_ids,
        library_id,
        publishers,
        page,
        size,
    )
    .await
}

pub(super) async fn load_browse_publisher_navigation(
    backend: &OpdsCatalogAccess,
    headers: &HeaderMap,
    allowed_library_ids: &Option<HashSet<String>>,
    library_id: Option<&str>,
) -> Result<Vec<Value>, String> {
    catalog_queries::load_browse_publisher_navigation(
        backend,
        headers,
        allowed_library_ids,
        library_id,
    )
    .await
}

pub(super) async fn load_collections(
    backend: &OpdsPersistedAccess,
    library_id: Option<&str>,
) -> Result<Vec<PersistedCollection>, String> {
    let rows = backend.load_collections(library_id).await?;
    Ok(rows
        .into_iter()
        .map(|row| PersistedCollection {
            id: row.id,
            name: row.name,
            last_modified: row.last_modified,
            ordered: row.ordered,
        })
        .collect())
}

pub(super) async fn load_collection(
    backend: &OpdsPersistedAccess,
    collection_id: &str,
) -> Result<Option<PersistedCollection>, String> {
    let row = backend.load_collection(collection_id).await?;
    Ok(row.map(|row| PersistedCollection {
        id: row.id,
        name: row.name,
        last_modified: row.last_modified,
        ordered: row.ordered,
    }))
}

pub(super) async fn load_collection_series(
    backend: &OpdsPersistedAccess,
    collection_id: &str,
    ordered: bool,
) -> Result<Vec<PersistedSeries>, String> {
    let rows = backend
        .load_collection_series(collection_id, ordered)
        .await?;
    Ok(rows.into_iter().map(map_series_record).collect())
}

fn map_library_record(row: PersistedLibraryRecord) -> PersistedLibrary {
    PersistedLibrary {
        id: row.id,
        name: row.name,
        last_modified: row.last_modified,
    }
}

fn map_series_record(row: PersistedSeriesRecord) -> PersistedSeries {
    PersistedSeries {
        id: row.id,
        library_id: row.library_id,
        title: row.title,
        summary: row.summary,
        age_rating: row.age_rating,
        sharing_labels: row.sharing_labels,
        last_modified: row.last_modified,
    }
}

fn map_series_book_record(row: PersistedSeriesBookRecord) -> PersistedSeriesBook {
    PersistedSeriesBook {
        id: row.id,
        series_id: row.series_id,
        title: row.title,
        series_title: row.series_title,
        number: row.number,
        number_sort: row.number_sort,
        summary: row.summary,
        isbn: row.isbn,
        authors: row
            .authors
            .into_iter()
            .map(|author| OpdsBookAuthorEntry {
                name: author.name,
                role: author.role,
            })
            .collect(),
        tags: row.tags,
        file_name: row.file_name,
        file_size: row.file_size,
        media_type: row.media_type,
        page_count: row.page_count,
        epub_divina_compatible: row.epub_divina_compatible,
        last_read: row.last_read,
        last_read_date: row.last_read_date,
        library_id: row.library_id,
        age_rating: row.age_rating,
        sharing_labels: row.sharing_labels,
        last_modified: row.last_modified,
        release_date: row.release_date,
    }
}

fn map_readlist_record(row: PersistedReadlistRecord) -> PersistedReadlist {
    PersistedReadlist {
        id: row.id,
        name: row.name,
        last_modified: row.last_modified,
        ordered: row.ordered,
    }
}

fn map_readlist_book_record(row: PersistedReadlistBookRecord) -> PersistedReadlistBook {
    PersistedReadlistBook {
        id: row.id,
        series_id: row.series_id,
        title: row.title,
        series_title: row.series_title,
        number: row.number,
        number_sort: row.number_sort,
        summary: row.summary,
        isbn: row.isbn,
        authors: row
            .authors
            .into_iter()
            .map(|author| OpdsBookAuthorEntry {
                name: author.name,
                role: author.role,
            })
            .collect(),
        tags: row.tags,
        file_name: row.file_name,
        file_size: row.file_size,
        media_type: row.media_type,
        media_status: row.media_status,
        page_count: row.page_count,
        epub_divina_compatible: row.epub_divina_compatible,
        library_id: row.library_id,
        age_rating: row.age_rating,
        sharing_labels: row.sharing_labels,
        last_modified: row.last_modified,
        release_date: row.release_date,
    }
}

pub(super) async fn load_series_page(
    backend: &OpdsCatalogAccess,
    allowed_library_ids: &Option<HashSet<String>>,
    search: Option<&str>,
    publishers: &[String],
    offset: i64,
    limit: i64,
) -> Result<Vec<PersistedSeries>, String> {
    catalog_queries::load_series_page(
        backend,
        allowed_library_ids,
        search,
        publishers,
        offset,
        limit,
    )
    .await
}

pub(super) async fn load_all_readlists(
    backend: &OpdsCatalogAccess,
) -> Result<Vec<PersistedReadlist>, String> {
    catalog_queries::load_all_readlists(backend).await
}

pub(super) async fn validate_library_scope(
    backend: &OpdsPersistedAccess,
    allowed_library_ids: &Option<HashSet<String>>,
    library_id: Option<&str>,
) -> Option<Response> {
    let library_id = library_id?;

    let library = match load_library(backend, library_id).await {
        Ok(library) => library,
        Err(error) => {
            return Some(
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": format!("load OPDS library scope: {error}") })),
                )
                    .into_response(),
            );
        }
    };

    if library.is_none() {
        return Some(StatusCode::NOT_FOUND.into_response());
    }
    if !library_visible(allowed_library_ids, library_id) {
        return Some(StatusCode::FORBIDDEN.into_response());
    }

    None
}
