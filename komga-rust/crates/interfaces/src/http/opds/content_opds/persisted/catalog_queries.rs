use std::collections::HashSet;
use std::path::Path;

use axum::http::HeaderMap;
use serde_json::{Value, json};

use crate::http::request_urls::app_absolute_url;
use crate::http::state::OpdsCatalogService;
use crate::opds_catalog_access::BrowseSeriesNavigationEntry;

use super::{PersistedReadlist, PersistedSeries};

fn persisted_series(entry: crate::opds_catalog_access::OpdsSeriesEntry) -> PersistedSeries {
    PersistedSeries {
        id: entry.id,
        library_id: entry.library_id,
        title: entry.title,
        summary: String::new(),
        age_rating: entry.age_rating,
        sharing_labels: entry.sharing_labels,
        last_modified: entry.last_modified,
    }
}

fn persisted_readlist(entry: crate::opds_catalog_access::OpdsReadlistEntry) -> PersistedReadlist {
    PersistedReadlist {
        id: entry.id,
        name: entry.name,
        last_modified: entry.last_modified,
        ordered: false,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn load_browse_series_navigation(
    backend: &dyn OpdsCatalogService,
    headers: &HeaderMap,
    database_file: &Path,
    allowed_library_ids: &Option<HashSet<String>>,
    library_id: Option<&str>,
    publishers: &[String],
    page: usize,
    size: usize,
) -> Result<(Vec<Value>, usize), String> {
    let (entries, total) = backend
        .load_browse_series_navigation_entries(
            database_file.to_path_buf(),
            allowed_library_ids.clone(),
            library_id.map(str::to_string),
            publishers.to_vec(),
            page,
            size,
        )
        .await?;

    Ok(browse_series_navigation_values(headers, entries, total))
}

pub(super) fn browse_series_navigation_values(
    headers: &HeaderMap,
    entries: Vec<BrowseSeriesNavigationEntry>,
    total: usize,
) -> (Vec<Value>, usize) {
    (
        entries
            .into_iter()
            .map(|entry| {
                json!({
                    "title": entry.title,
                    "href": app_absolute_url(headers, format!("/opds/v2/series/{}", entry.id).as_str()),
                    "type": "application/opds+json",
                })
            })
            .collect(),
        total,
    )
}

pub(super) async fn load_browse_publisher_navigation(
    backend: &dyn OpdsCatalogService,
    headers: &HeaderMap,
    database_file: &Path,
    allowed_library_ids: &Option<HashSet<String>>,
    library_id: Option<&str>,
) -> Result<Vec<Value>, String> {
    let entries = backend
        .load_browse_publisher_entries(
            database_file.to_path_buf(),
            allowed_library_ids.clone(),
            library_id.map(str::to_string),
        )
        .await?;
    let library_segment = library_id.map(|id| format!("/{id}")).unwrap_or_default();
    Ok(entries
        .into_iter()
        .map(|entry| {
            let href = format!(
                "/opds/v2/libraries{library_segment}/browse?publisher={}",
                super::super::feeds::query_escape(entry.publisher.as_str()),
            );
            json!({
                "title": entry.publisher,
                "href": app_absolute_url(headers, href.as_str()),
                "type": "application/opds+json",
            })
        })
        .collect())
}

pub(super) async fn load_series_page(
    backend: &dyn OpdsCatalogService,
    database_file: &Path,
    allowed_library_ids: &Option<HashSet<String>>,
    search: Option<&str>,
    publishers: &[String],
    offset: i64,
    limit: i64,
) -> Result<Vec<PersistedSeries>, String> {
    backend
        .load_series_page(
            database_file.to_path_buf(),
            allowed_library_ids.clone(),
            search.map(str::to_string),
            publishers.to_vec(),
            offset,
            limit,
        )
        .await
        .map(|entries| entries.into_iter().map(persisted_series).collect())
}

pub(super) async fn load_all_readlists(
    backend: &dyn OpdsCatalogService,
    database_file: &Path,
) -> Result<Vec<PersistedReadlist>, String> {
    backend
        .load_all_readlists(database_file.to_path_buf())
        .await
        .map(|entries| entries.into_iter().map(persisted_readlist).collect())
}
