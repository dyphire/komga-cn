use super::*;

pub async fn load_persisted_alphabetical_groups(
    database_file: &FsPath,
    context: &DiscoveryQueryContext,
    filters: RuntimeSeriesFilters,
    full_text_search: Option<String>,
    search_regex: Option<(String, String)>,
) -> Result<Vec<Value>, String> {
    let page = load_persisted_series_page(
        database_file,
        context,
        PersistedSeriesBrowseQuery {
            library_ids: filters.library_ids,
            collection_ids: filters.collection_ids,
            titles: filters.titles,
            titles_excluded: filters.titles_excluded,
            titles_contains: filters.titles_contains,
            titles_contains_excluded: filters.titles_contains_excluded,
            titles_begins_with: filters.titles_begins_with,
            titles_begins_with_excluded: filters.titles_begins_with_excluded,
            titles_ends_with: filters.titles_ends_with,
            titles_ends_with_excluded: filters.titles_ends_with_excluded,
            title_sorts: filters.title_sorts,
            title_sorts_excluded: filters.title_sorts_excluded,
            title_sorts_contains: filters.title_sorts_contains,
            title_sorts_contains_excluded: filters.title_sorts_contains_excluded,
            title_sorts_begins_with: filters.title_sorts_begins_with,
            title_sorts_begins_with_excluded: filters.title_sorts_begins_with_excluded,
            title_sorts_ends_with: filters.title_sorts_ends_with,
            title_sorts_ends_with_excluded: filters.title_sorts_ends_with_excluded,
            deleted: filters.deleted,
            oneshot: filters.oneshot,
            read_statuses: filters.read_statuses,
            read_statuses_excluded: filters.read_statuses_excluded,
            complete: filters.complete,
            genres: filters.genres,
            genres_excluded: filters.genres_excluded,
            genres_null: filters.genres_null,
            tags: filters.tags,
            tags_excluded: filters.tags_excluded,
            tags_null: filters.tags_null,
            languages: filters.languages,
            languages_excluded: filters.languages_excluded,
            publishers: filters.publishers,
            publishers_excluded: filters.publishers_excluded,
            age_ratings: filters.age_ratings,
            age_ratings_excluded: filters.age_ratings_excluded,
            age_ratings_null: filters.age_ratings_null,
            age_rating_gt: filters.age_rating_gt,
            age_rating_lt: filters.age_rating_lt,
            sharing_labels: filters.sharing_labels,
            sharing_labels_excluded: filters.sharing_labels_excluded,
            sharing_labels_null: filters.sharing_labels_null,
            authors: filters.authors,
            authors_excluded: filters.authors_excluded,
            release_dates: filters.release_dates,
            release_dates_excluded: filters.release_dates_excluded,
            release_dates_null: filters.release_dates_null,
            release_date_gt: filters.release_date_gt,
            release_date_lt: filters.release_date_lt,
            release_date_begins_with: filters.release_date_begins_with,
            release_date_ends_with: filters.release_date_ends_with,
            release_date_contains_excluded: filters.release_date_contains_excluded,
            release_date_begins_with_excluded: filters.release_date_begins_with_excluded,
            release_date_ends_with_excluded: filters.release_date_ends_with_excluded,
            release_date_in_last_days: filters.release_date_in_last_days,
            release_date_not_in_last_days: filters.release_date_not_in_last_days,
            series_statuses: filters.series_statuses,
            series_statuses_excluded: filters.series_statuses_excluded,
            search: full_text_search,
            search_regex,
            page: 0,
            size: usize::MAX,
            unpaged: true,
            sort_modes: vec![PersistedSeriesSortMode::TitleAsc],
        },
    )
    .await?;

    let mut counts = BTreeMap::<String, i64>::new();
    for series in page.content {
        let group = first_group_key(&series.title);
        *counts.entry(group).or_insert(0) += 1;
    }

    Ok(counts
        .into_iter()
        .map(|(group, count)| json!({ "group": group, "count": count }))
        .collect())
}
