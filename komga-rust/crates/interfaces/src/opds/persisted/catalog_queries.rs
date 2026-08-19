use std::collections::HashSet;

use axum::http::HeaderMap;

use komga_application::opds::{
    BrowseSeriesNavigationEntry, OpdsBrowseCatalogPort, OpdsSeriesEntry,
};

use crate::contracts::opds::OpdsV2LinkDto;
use crate::request_urls::app_absolute_url;

use super::{OpdsJsonNavigationPage, PersistedSeries};

fn persisted_series(entry: OpdsSeriesEntry) -> PersistedSeries {
    PersistedSeries {
        id: entry.id,
        title: entry.title,
        last_modified: entry.last_modified,
    }
}

pub(super) async fn load_browse_series_navigation(
    backend: &dyn OpdsBrowseCatalogPort,
    headers: &HeaderMap,
    allowed_library_ids: &Option<HashSet<String>>,
    library_id: Option<&str>,
    publishers: &[String],
    page: usize,
    size: usize,
) -> anyhow::Result<OpdsJsonNavigationPage> {
    let page_result = backend
        .load_browse_series_navigation_entries(
            allowed_library_ids.as_ref(),
            library_id,
            publishers,
            page,
            size,
        )
        .await?;

    Ok(browse_series_navigation_values(
        headers,
        page_result.entries,
        page_result.total_count,
    ))
}

pub(super) fn browse_series_navigation_values(
    headers: &HeaderMap,
    entries: Vec<BrowseSeriesNavigationEntry>,
    total: usize,
) -> OpdsJsonNavigationPage {
    OpdsJsonNavigationPage {
        entries: entries
            .into_iter()
            .map(|entry| OpdsV2LinkDto {
                title: Some(entry.title),
                rel: None,
                href: app_absolute_url(headers, format!("/opds/v2/series/{}", entry.id).as_str()),
                media_type: Some("application/opds+json".to_string()),
                templated: None,
                properties: None,
            })
            .collect(),
        total_count: total,
    }
}

pub(super) async fn load_browse_publisher_navigation(
    backend: &dyn OpdsBrowseCatalogPort,
    headers: &HeaderMap,
    allowed_library_ids: &Option<HashSet<String>>,
    library_id: Option<&str>,
) -> anyhow::Result<Vec<OpdsV2LinkDto>> {
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
            OpdsV2LinkDto {
                title: Some(entry.publisher),
                rel: None,
                href: app_absolute_url(headers, href.as_str()),
                media_type: Some("application/opds+json".to_string()),
                templated: None,
                properties: None,
            }
        })
        .collect())
}

pub(super) async fn load_series_page(
    backend: &dyn OpdsBrowseCatalogPort,
    allowed_library_ids: &Option<HashSet<String>>,
    search: Option<&str>,
    publishers: &[String],
    offset: i64,
    limit: i64,
) -> anyhow::Result<Vec<PersistedSeries>> {
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
