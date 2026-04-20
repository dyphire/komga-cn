use crate::state::PersistedDiscoveryService;

use super::*;

pub async fn load_persisted_alphabetical_groups(
    backend: &dyn PersistedDiscoveryService,
    database_file: &FsPath,
    context: &DiscoveryQueryContext,
    filters: RuntimeSeriesFilters,
    full_text_search: Option<String>,
) -> Result<Vec<Value>, String> {
    let page = load_persisted_series_page(
        backend,
        database_file,
        context,
        PersistedSeriesBrowseQuery::from_filters(
            filters.into_criteria(),
            full_text_search,
            0,
            usize::MAX,
            true,
            vec![PersistedSeriesSortMode::TitleAsc],
        ),
    )
    .await?;

    let mut counts = BTreeMap::<String, i64>::new();
    for series in page.content {
        let group = first_group_key(&series.title_sort);
        *counts.entry(group).or_insert(0) += 1;
    }

    Ok(counts
        .into_iter()
        .map(|(group, count)| json!({ "group": group, "count": count }))
        .collect())
}
