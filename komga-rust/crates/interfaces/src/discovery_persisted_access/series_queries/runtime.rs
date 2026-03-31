use super::super::common_helpers::should_ignore_runtime_filter_error;
use super::*;

pub async fn runtime_owned_persisted_series_page(
    database_file: &FsPath,
    context: &DiscoveryQueryContext,
    filters: &RuntimeSeriesFilters,
    sorts: &[String],
    full_text_search: Option<String>,
    page: usize,
    size: usize,
    unpaged: bool,
) -> Option<Result<PageEnvelope<PersistedSeriesSummary>, String>> {
    if !database_file.exists() {
        return None;
    }

    let sort_modes = parse_persisted_series_sort_modes(sorts);
    let has_persisted_rows = match persisted_series_exist(database_file).await {
        Ok(has_rows) => has_rows,
        Err(error) => return Some(Err(error)),
    };
    if !has_persisted_rows {
        return None;
    }

    Some(
        load_persisted_series_page(
            database_file,
            context,
            PersistedSeriesBrowseQuery {
                library_ids: filters.library_ids.clone(),
                collection_ids: filters.collection_ids.clone(),
                titles: filters.titles.clone(),
                titles_excluded: filters.titles_excluded.clone(),
                titles_contains: filters.titles_contains.clone(),
                titles_contains_excluded: filters.titles_contains_excluded.clone(),
                titles_begins_with: filters.titles_begins_with.clone(),
                titles_begins_with_excluded: filters.titles_begins_with_excluded.clone(),
                titles_ends_with: filters.titles_ends_with.clone(),
                titles_ends_with_excluded: filters.titles_ends_with_excluded.clone(),
                title_sorts: filters.title_sorts.clone(),
                title_sorts_excluded: filters.title_sorts_excluded.clone(),
                title_sorts_contains: filters.title_sorts_contains.clone(),
                title_sorts_contains_excluded: filters.title_sorts_contains_excluded.clone(),
                title_sorts_begins_with: filters.title_sorts_begins_with.clone(),
                title_sorts_begins_with_excluded: filters.title_sorts_begins_with_excluded.clone(),
                title_sorts_ends_with: filters.title_sorts_ends_with.clone(),
                title_sorts_ends_with_excluded: filters.title_sorts_ends_with_excluded.clone(),
                deleted: filters.deleted,
                oneshot: filters.oneshot,
                read_statuses: filters.read_statuses.clone(),
                read_statuses_excluded: filters.read_statuses_excluded.clone(),
                complete: filters.complete,
                genres: filters.genres.clone(),
                genres_excluded: filters.genres_excluded.clone(),
                genres_null: filters.genres_null,
                tags: filters.tags.clone(),
                tags_excluded: filters.tags_excluded.clone(),
                tags_null: filters.tags_null,
                languages: filters.languages.clone(),
                languages_excluded: filters.languages_excluded.clone(),
                publishers: filters.publishers.clone(),
                publishers_excluded: filters.publishers_excluded.clone(),
                age_ratings: filters.age_ratings.clone(),
                age_ratings_excluded: filters.age_ratings_excluded.clone(),
                age_ratings_null: filters.age_ratings_null,
                age_rating_gt: filters.age_rating_gt,
                age_rating_lt: filters.age_rating_lt,
                sharing_labels: filters.sharing_labels.clone(),
                sharing_labels_excluded: filters.sharing_labels_excluded.clone(),
                sharing_labels_null: filters.sharing_labels_null,
                authors: filters.authors.clone(),
                authors_excluded: filters.authors_excluded.clone(),
                release_dates: filters.release_dates.clone(),
                release_dates_excluded: filters.release_dates_excluded.clone(),
                release_dates_null: filters.release_dates_null,
                release_date_gt: filters.release_date_gt.clone(),
                release_date_lt: filters.release_date_lt.clone(),
                release_date_begins_with: filters.release_date_begins_with.clone(),
                release_date_ends_with: filters.release_date_ends_with.clone(),
                release_date_contains_excluded: filters.release_date_contains_excluded.clone(),
                release_date_begins_with_excluded: filters
                    .release_date_begins_with_excluded
                    .clone(),
                release_date_ends_with_excluded: filters.release_date_ends_with_excluded.clone(),
                release_date_in_last_days: filters.release_date_in_last_days,
                release_date_not_in_last_days: filters.release_date_not_in_last_days,
                series_statuses: filters.series_statuses.clone(),
                series_statuses_excluded: filters.series_statuses_excluded.clone(),
                search: full_text_search,
                search_regex: None,
                page,
                size,
                unpaged,
                sort_modes,
            },
        )
        .await,
    )
}

pub fn parse_persisted_series_sort_modes(sorts: &[String]) -> Vec<PersistedSeriesSortMode> {
    let mut modes = sorts
        .iter()
        .filter_map(|sort| match sort.as_str() {
            "metadata.titleSort,asc" => Some(PersistedSeriesSortMode::TitleAsc),
            "createdDate,desc" | "lastModifiedDate,desc" | "booksMetadata.releaseDate,desc" => {
                Some(PersistedSeriesSortMode::Latest)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    modes.dedup();
    if modes.is_empty() {
        modes.push(PersistedSeriesSortMode::TitleAsc);
    }
    modes
}

async fn persisted_series_exist(database_file: &FsPath) -> Result<bool, String> {
    persisted_backend_persisted_series_exist(database_file).await
}

pub async fn runtime_owned_series_list_response(
    headers: &HeaderMap,
    uri: &Uri,
    payload: Option<&Value>,
    full_text_search: Option<String>,
    auth_state: &DiscoveryAuthState,
    database_file: &FsPath,
    strict_runtime_shape: bool,
) -> Option<Response> {
    let sorts = query_values(uri.query().unwrap_or_default(), "sort")
        .into_iter()
        .map(decode_query_component)
        .collect::<Vec<_>>();
    let page = query_value(uri.query().unwrap_or_default(), "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(uri.query().unwrap_or_default(), "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);
    let unpaged = query_bool(uri.query().unwrap_or_default(), "unpaged");

    let parse_mode = OperatorValidationMode::from(query_validation_mode(strict_runtime_shape));
    let mut filters = match parse_runtime_series_filters_with_mode(
        payload.and_then(|value| value.get("condition")),
        parse_mode,
    ) {
        Ok(filters) => filters,
        Err(error) => {
            if strict_runtime_shape && should_ignore_runtime_filter_error(&error) {
                RuntimeSeriesFilters::default()
            } else if strict_runtime_shape {
                return Some(invalid_runtime_series_list_response(error));
            } else {
                webui_bridge_series_filters_from_payload(payload)
            }
        }
    };

    if !strict_runtime_shape {
        restrict_series_filters_to_persisted_shape(&mut filters);
        filters.library_ids =
            remap_requested_library_ids_for_persisted(database_file, filters.library_ids.as_ref())
                .await;
    }

    let requested_library_ids =
        requested_library_ids_for_runtime_shape(strict_runtime_shape, filters.library_ids.clone());
    let context = match auth_state.resolve_query_context(headers, requested_library_ids.as_deref())
    {
        Some(context) => context,
        None => return Some(StatusCode::UNAUTHORIZED.into_response()),
    };

    if let Some(persisted_page) = runtime_owned_persisted_series_page(
        database_file,
        &context,
        &filters,
        &sorts,
        full_text_search.clone(),
        page,
        size,
        unpaged,
    )
    .await
    {
        match persisted_page {
            Ok(page) => {
                let mut response = Json(series_page_payload(page, !unpaged)).into_response();
                mark_runtime_owned(&mut response);
                return Some(response);
            }
            Err(error) => {
                return Some(
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "error": format!("runtime series list failed: {error}") })),
                    )
                        .into_response(),
                );
            }
        }
    }

    None
}
