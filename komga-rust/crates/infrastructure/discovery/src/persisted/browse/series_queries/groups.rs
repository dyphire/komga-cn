use std::collections::BTreeMap;

use komga_application::discovery::SeriesAlphabeticalGroup;
use komga_domain::discovery::SeriesCondition;

use super::super::grouping::first_group_key;
use super::super::models::{
    PersistedSeriesBrowseQuery, PersistedSeriesSortMode, SeriesFilterCriteria,
};
use super::super::sql_pushdown;
use super::super::{DiscoveryQueryContext, SqliteDiscoveryBrowseService};
use super::filtering::load_persisted_series_page;

pub(in crate::persisted::browse) async fn load_persisted_alphabetical_groups(
    backend: &SqliteDiscoveryBrowseService,
    context: &DiscoveryQueryContext,
    condition: Option<SeriesCondition>,
    full_text_search: Option<String>,
) -> anyhow::Result<Vec<SeriesAlphabeticalGroup>> {
    let query = PersistedSeriesBrowseQuery::from_filters(
        SeriesFilterCriteria::default(),
        full_text_search,
        0,
        usize::MAX,
        true,
        vec![PersistedSeriesSortMode::TitleAsc],
    )
    .with_condition(condition);

    let mut counts = BTreeMap::<String, i64>::new();
    // Lightweight path: SQL pushdown returns the visible series IDs (already
    // filtered by library scope, restrictions and conditions); only the title
    // sort column is fetched for grouping instead of the full summaries.
    // Falls back to the full summaries load when the translator cannot push
    // down (unsupported conditions, e.g. regex).
    if let Some(sql_page) = sql_pushdown::try_load_series_page(backend, context, &query).await? {
        let mut ids = Vec::with_capacity(sql_page.ids.len());
        for chunk in sql_page.ids.chunks(500) {
            ids.extend_from_slice(chunk);
        }
        let names = backend.load_series_names(&ids).await?;
        for name in names.values() {
            let group = first_group_key(name);
            *counts.entry(group).or_insert(0) += 1;
        }
    } else {
        let page = load_persisted_series_page(backend, context, query).await?;
        for series in page.content {
            let group = first_group_key(&series.title_sort);
            *counts.entry(group).or_insert(0) += 1;
        }
    }

    Ok(counts
        .into_iter()
        .map(|(group, count)| SeriesAlphabeticalGroup { group, count })
        .collect())
}
