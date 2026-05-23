use std::collections::{HashMap, HashSet};

use axum::Json;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use komga_domain::discovery::{
    AgeRestrictionKind as DomainAgeRestrictionKind, QueryRestrictions as DomainRestrictions,
    content_allowed_by_restrictions as domain_content_allowed,
};

use crate::discovery_auth::principal::AgeRestrictionKind;
use crate::identity_access::auth::{
    AuthUser, user_payload_json, user_shared_all_libraries, user_shared_library_ids,
};
use crate::state::{
    OpdsBookAuthorEntry, OpdsCatalogPort, OpdsPersistedPort, PersistedLibraryRecord,
    PersistedSeriesBookRecord, PersistedSeriesRecord,
};

use super::types::{
    OpdsRestrictions, PersistedLibrary, PersistedSeries, PersistedSeriesBook,
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

pub(super) fn content_allowed_by_restrictions(
    restrictions: Option<&OpdsRestrictions>,
    age_rating: Option<u16>,
    sharing_labels: &[String],
) -> bool {
    let Some(restrictions) = restrictions else {
        return true;
    };
    let domain_restrictions = DomainRestrictions {
        age: restrictions.age,
        age_restriction: restrictions.age_restriction.map(|kind| match kind {
            AgeRestrictionKind::AllowOnly => DomainAgeRestrictionKind::AllowOnly,
            AgeRestrictionKind::Exclude => DomainAgeRestrictionKind::Exclude,
        }),
        labels_allow: restrictions.labels_allow.clone(),
        labels_exclude: restrictions.labels_exclude.clone(),
    };
    domain_content_allowed(&domain_restrictions, age_rating, sharing_labels)
}

pub(super) async fn load_libraries(
    backend: &dyn OpdsPersistedPort,
) -> Result<Vec<PersistedLibrary>, String> {
    let records = backend.load_libraries().await?;
    Ok(records.into_iter().map(map_library_record).collect())
}

pub(super) async fn load_library(
    backend: &dyn OpdsPersistedPort,
    library_id: &str,
) -> Result<Option<PersistedLibrary>, String> {
    let record = backend.load_library(library_id).await?;
    Ok(record.map(map_library_record))
}

pub(super) async fn load_series(
    backend: &dyn OpdsPersistedPort,
    series_id: &str,
) -> Result<Option<PersistedSeries>, String> {
    let record = backend.load_series(series_id).await?;
    Ok(record.map(map_series_record))
}

pub(super) async fn load_series_books_paged(
    backend: &dyn OpdsPersistedPort,
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

pub(super) async fn load_series_tags(
    backend: &dyn OpdsPersistedPort,
    series_id: &str,
) -> Result<Vec<String>, String> {
    backend.load_series_tags(series_id).await
}

pub(super) async fn load_opds_v1_series_search_results(
    persisted_backend: &dyn OpdsPersistedPort,
    catalog_backend: &dyn OpdsCatalogPort,
    allowed_library_ids: &Option<HashSet<String>>,
    search: &str,
    publishers: &[String],
) -> Result<Vec<PersistedSeriesSearchResult>, String> {
    let (series_rows, _, _, _) = persisted_backend
        .load_unified_search_results(search)
        .await?;
    let series = series_rows
        .into_iter()
        .map(|row| PersistedSeriesSearchResult {
            id: row.id,
            title: row.title,
            library_id: row.library_id,
            age_rating: row.age_rating,
            sharing_labels: row.sharing_labels,
            last_modified: row.last_modified,
        })
        .collect::<Vec<_>>();

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

#[allow(clippy::too_many_arguments)]
pub(super) async fn load_browse_series_navigation(
    backend: &dyn OpdsCatalogPort,
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
    backend: &dyn OpdsCatalogPort,
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

pub(super) async fn load_series_page(
    backend: &dyn OpdsCatalogPort,
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

pub(super) async fn validate_library_scope(
    backend: &dyn OpdsPersistedPort,
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
