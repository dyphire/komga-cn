use std::collections::BTreeMap;

use komga_application::discovery::SeriesAlphabeticalGroup;
use komga_domain::discovery::SeriesCondition;

use super::super::grouping::first_group_key;
use super::super::models::{
    PersistedSeriesBrowseQuery, PersistedSeriesSortMode, SeriesFilterCriteria,
};
use super::super::{DiscoveryQueryContext, SqliteDiscoveryBrowseService};
use super::filtering::load_persisted_series_page;

pub(in crate::persisted::browse) async fn load_persisted_alphabetical_groups(
    backend: &SqliteDiscoveryBrowseService,
    context: &DiscoveryQueryContext,
    condition: Option<SeriesCondition>,
    full_text_search: Option<String>,
) -> anyhow::Result<Vec<SeriesAlphabeticalGroup>> {
    let page = load_persisted_series_page(
        backend,
        context,
        PersistedSeriesBrowseQuery::from_filters(
            SeriesFilterCriteria::default(),
            full_text_search,
            0,
            usize::MAX,
            true,
            vec![PersistedSeriesSortMode::TitleAsc],
        )
        .with_condition(condition),
    )
    .await?;

    let mut counts = BTreeMap::<String, i64>::new();
    for series in page.content {
        let group = first_group_key(&series.title_sort);
        *counts.entry(group).or_insert(0) += 1;
    }

    Ok(counts
        .into_iter()
        .map(|(group, count)| SeriesAlphabeticalGroup { group, count })
        .collect())
}
