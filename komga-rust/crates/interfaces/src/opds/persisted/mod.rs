use std::collections::{HashMap, HashSet};

use axum::Json;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use crate::identity_access::auth::{AuthUser, user_shared_all_libraries, user_shared_library_ids};
use crate::state::{OpdsCatalogPort, OpdsPersistedPort, PersistedLibraryRecord};

use super::types::{PersistedLibrary, PersistedSeries, PersistedSeriesSearchResult};

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
