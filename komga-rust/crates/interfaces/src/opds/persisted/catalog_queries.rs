use std::collections::HashSet;

use axum::http::HeaderMap;
use serde_json::{Value, json};

use crate::request_urls::app_absolute_url;
use crate::state::{BrowseSeriesNavigationEntry, OpdsCatalogPort, OpdsSeriesEntry};

use super::PersistedSeries;

fn persisted_series(entry: OpdsSeriesEntry) -> PersistedSeries {
    PersistedSeries {
        id: entry.id,
        title: entry.title,
        last_modified: entry.last_modified,
    }
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
    let (entries, total) = backend
        .load_browse_series_navigation_entries(
            allowed_library_ids.as_ref(),
            library_id,
            publishers,
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
    backend: &dyn OpdsCatalogPort,
    headers: &HeaderMap,
    allowed_library_ids: &Option<HashSet<String>>,
    library_id: Option<&str>,
) -> Result<Vec<Value>, String> {
    let entries = backend
        .load_browse_publisher_entries(allowed_library_ids.as_ref(), library_id)
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
    backend: &dyn OpdsCatalogPort,
    allowed_library_ids: &Option<HashSet<String>>,
    search: Option<&str>,
    publishers: &[String],
    offset: i64,
    limit: i64,
) -> Result<Vec<PersistedSeries>, String> {
    backend
        .load_series_page(
            allowed_library_ids.as_ref(),
            search,
            publishers,
            offset,
            limit,
        )
        .await
        .map(|entries| entries.into_iter().map(persisted_series).collect())
}
