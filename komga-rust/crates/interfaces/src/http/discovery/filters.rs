use super::*;

#[path = "filters/books.rs"]
mod books;
#[path = "filters/dispatch.rs"]
mod dispatch;
#[path = "request_mapping.rs"]
mod request_mapping;
#[path = "filters/series.rs"]
mod series;
#[path = "filters/shared.rs"]
mod shared;

use request_mapping::*;
use shared::*;
pub(super) use shared::{normalize_release_date_date_time, parse_iso8601_duration_to_days};

pub(super) fn parse_runtime_series_filters_with_mode(
    condition: Option<&Value>,
    mode: OperatorValidationMode,
) -> Result<RuntimeSeriesFilters, DiscoveryError> {
    dispatch::parse_runtime_series_filters_with_mode_impl(condition, mode)
}

pub(super) fn parse_runtime_books_filters(
    condition: Option<&Value>,
) -> Result<RuntimeBooksFilters, DiscoveryError> {
    dispatch::parse_runtime_books_filters_impl(condition)
}

pub(super) fn parse_runtime_books_filters_with_mode(
    condition: Option<&Value>,
    mode: OperatorValidationMode,
) -> Result<RuntimeBooksFilters, DiscoveryError> {
    dispatch::parse_runtime_books_filters_with_mode_impl(condition, mode)
}

#[derive(Clone, Copy)]
pub(super) enum OperatorValidationMode {
    Lenient,
    Strict,
}

impl OperatorValidationMode {
    pub(super) fn is_strict(self) -> bool {
        matches!(self, Self::Strict)
    }
}

impl From<DiscoveryRequestValidation> for OperatorValidationMode {
    fn from(value: DiscoveryRequestValidation) -> Self {
        match value {
            DiscoveryRequestValidation::Lenient => Self::Lenient,
            DiscoveryRequestValidation::Strict => Self::Strict,
        }
    }
}

pub(super) fn exact_oneshot_bootstrap_series_id(payload: Option<&Value>) -> Option<String> {
    dispatch::exact_oneshot_bootstrap_series_id_impl(payload)
}

pub(super) fn webui_bridge_series_filters_from_payload(
    payload: Option<&Value>,
) -> RuntimeSeriesFilters {
    request_mapping::webui_bridge_series_filters_from_payload(payload)
}

pub(super) fn webui_bridge_books_filters_from_payload(
    payload: Option<&Value>,
) -> RuntimeBooksFilters {
    request_mapping::webui_bridge_books_filters_from_payload(payload)
}

pub(super) fn restrict_books_filters_to_persisted_shape(filters: &mut RuntimeBooksFilters) {
    request_mapping::restrict_books_filters_to_persisted_shape(filters)
}

pub(super) fn parse_books_library_id_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeBooksFilters, DiscoveryError> {
    books::parse_books_library_id_filter(condition, mode)
}

pub(super) fn parse_books_series_id_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeBooksFilters, DiscoveryError> {
    books::parse_books_series_id_filter(condition, mode)
}

pub(super) fn parse_books_read_list_id_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeBooksFilters, DiscoveryError> {
    books::parse_books_read_list_id_filter(condition, mode)
}

pub(super) fn parse_books_title_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeBooksFilters, DiscoveryError> {
    books::parse_books_title_filter(condition, mode)
}

pub(super) fn parse_books_deleted_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeBooksFilters, DiscoveryError> {
    books::parse_books_deleted_filter(condition, mode)
}

pub(super) fn parse_books_oneshot_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeBooksFilters, DiscoveryError> {
    books::parse_books_oneshot_filter(condition, mode)
}

pub(super) fn parse_books_genre_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeBooksFilters, DiscoveryError> {
    books::parse_books_genre_filter(condition, mode)
}

pub(super) fn parse_books_tag_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeBooksFilters, DiscoveryError> {
    books::parse_books_tag_filter(condition, mode)
}

pub(super) fn parse_books_language_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeBooksFilters, DiscoveryError> {
    books::parse_books_language_filter(condition, mode)
}

pub(super) fn parse_books_publisher_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeBooksFilters, DiscoveryError> {
    books::parse_books_publisher_filter(condition, mode)
}

pub(super) fn parse_books_age_rating_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeBooksFilters, DiscoveryError> {
    books::parse_books_age_rating_filter(condition, mode)
}

pub(super) fn parse_books_read_status_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeBooksFilters, DiscoveryError> {
    books::parse_books_read_status_filter(condition, mode)
}

pub(super) fn parse_books_media_profile_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeBooksFilters, DiscoveryError> {
    books::parse_books_media_profile_filter(condition, mode)
}

pub(super) fn parse_books_media_status_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeBooksFilters, DiscoveryError> {
    books::parse_books_media_status_filter(condition, mode)
}

pub(super) fn parse_books_author_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeBooksFilters, DiscoveryError> {
    books::parse_books_author_filter(condition, mode)
}

pub(super) fn parse_books_poster_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeBooksFilters, DiscoveryError> {
    books::parse_books_poster_filter(condition, mode)
}

pub(super) fn parse_books_number_sort_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeBooksFilters, DiscoveryError> {
    books::parse_books_number_sort_filter(condition, mode)
}

pub(super) fn parse_books_release_date_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeBooksFilters, DiscoveryError> {
    books::parse_books_release_date_filter(condition, mode)
}

pub(super) fn parse_books_composite_filters(
    condition: &Value,
    all_of: bool,
    mode: OperatorValidationMode,
) -> Result<RuntimeBooksFilters, DiscoveryError> {
    let Some(children) = condition.get("conditions").and_then(Value::as_array) else {
        return Err(DiscoveryError::InvalidSemantics(
            "books composite filter missing conditions".to_string(),
        ));
    };

    let mut aggregate = RuntimeBooksFilters::default();
    let mut child_count = 0usize;
    let mut series_leaf_count = 0usize;
    let mut library_groups: Vec<Vec<String>> = vec![];
    let mut series_groups: Vec<Vec<String>> = vec![];
    let mut series_excluded_groups: Vec<Vec<String>> = vec![];
    let mut read_list_groups: Vec<Vec<String>> = vec![];
    let mut read_list_excluded_groups: Vec<Vec<String>> = vec![];
    let mut title_groups: Vec<Vec<String>> = vec![];
    let mut title_excluded_groups: Vec<Vec<String>> = vec![];
    let mut title_contains_groups: Vec<Vec<String>> = vec![];
    let mut title_contains_excluded_groups: Vec<Vec<String>> = vec![];
    let mut title_begins_with_groups: Vec<Vec<String>> = vec![];
    let mut title_begins_with_excluded_groups: Vec<Vec<String>> = vec![];
    let mut title_ends_with_groups: Vec<Vec<String>> = vec![];
    let mut title_ends_with_excluded_groups: Vec<Vec<String>> = vec![];
    let mut tag_groups: Vec<Vec<String>> = vec![];
    let mut tag_excluded_groups: Vec<Vec<String>> = vec![];
    let mut read_status_groups: Vec<Vec<String>> = vec![];
    let mut read_status_excluded_groups: Vec<Vec<String>> = vec![];
    let mut media_profile_groups: Vec<Vec<String>> = vec![];
    let mut media_profile_excluded_groups: Vec<Vec<String>> = vec![];
    let mut media_status_groups: Vec<Vec<String>> = vec![];
    let mut media_status_excluded_groups: Vec<Vec<String>> = vec![];
    let mut author_groups: Vec<Vec<String>> = vec![];
    let mut author_excluded_groups: Vec<Vec<String>> = vec![];
    let mut poster_type_groups: Vec<Vec<String>> = vec![];
    let mut poster_type_excluded_groups: Vec<Vec<String>> = vec![];
    let mut release_date_groups: Vec<Vec<String>> = vec![];
    let mut release_date_excluded_groups: Vec<Vec<String>> = vec![];
    let mut release_date_gt_bounds: Vec<String> = vec![];
    let mut release_date_lt_bounds: Vec<String> = vec![];
    let mut release_date_begins_with_groups: Vec<Vec<String>> = vec![];
    let mut release_date_ends_with_groups: Vec<Vec<String>> = vec![];
    let mut release_date_contains_excluded_groups: Vec<Vec<String>> = vec![];
    let mut release_date_begins_with_excluded_groups: Vec<Vec<String>> = vec![];
    let mut release_date_ends_with_excluded_groups: Vec<Vec<String>> = vec![];
    let mut release_date_in_last_days_bounds: Vec<i64> = vec![];
    let mut release_date_not_in_last_days_bounds: Vec<i64> = vec![];
    let mut number_sort_groups: Vec<Vec<f64>> = vec![];
    let mut number_sort_excluded_groups: Vec<Vec<f64>> = vec![];
    let mut number_sort_gt_bounds: Vec<f64> = vec![];
    let mut number_sort_lt_bounds: Vec<f64> = vec![];

    for child in children {
        child_count += 1;
        let parsed = parse_runtime_books_filters_with_mode(Some(child), mode)?;
        let parsed = parsed.criteria;
        let is_series_leaf = parsed.series_ids.is_some()
            && parsed.series_ids_excluded.is_none()
            && parsed.read_list_ids.is_none()
            && parsed.read_list_ids_excluded.is_none()
            && parsed.titles.is_none()
            && parsed.titles_excluded.is_none()
            && parsed.titles_contains.is_none()
            && parsed.titles_contains_excluded.is_none()
            && parsed.titles_begins_with.is_none()
            && parsed.titles_begins_with_excluded.is_none()
            && parsed.titles_ends_with.is_none()
            && parsed.titles_ends_with_excluded.is_none()
            && parsed.library_ids.is_none()
            && parsed.deleted.is_none()
            && parsed.oneshot.is_none()
            && parsed.tags.is_none()
            && parsed.tags_excluded.is_none()
            && parsed.tags_null.is_none()
            && parsed.read_statuses.is_none()
            && parsed.read_statuses_excluded.is_none()
            && parsed.media_profiles.is_none()
            && parsed.media_profiles_excluded.is_none()
            && parsed.media_statuses.is_none()
            && parsed.media_statuses_excluded.is_none()
            && parsed.authors.is_none()
            && parsed.authors_excluded.is_none()
            && parsed.poster_types.is_none()
            && parsed.poster_types_excluded.is_none()
            && parsed.poster_selected.is_none()
            && parsed.poster_selected_excluded.is_none()
            && parsed.release_dates.is_none()
            && parsed.release_dates_excluded.is_none()
            && parsed.release_dates_null.is_none()
            && parsed.release_date_gt.is_none()
            && parsed.release_date_lt.is_none()
            && parsed.release_date_begins_with.is_none()
            && parsed.release_date_ends_with.is_none()
            && parsed.release_date_contains_excluded.is_none()
            && parsed.release_date_begins_with_excluded.is_none()
            && parsed.release_date_ends_with_excluded.is_none()
            && parsed.release_date_in_last_days.is_none()
            && parsed.release_date_not_in_last_days.is_none()
            && parsed.number_sorts.is_none()
            && parsed.number_sorts_excluded.is_none()
            && parsed.number_sort_gt.is_none()
            && parsed.number_sort_lt.is_none();
        if is_series_leaf {
            series_leaf_count += 1;
        }

        if let Some(ids) = parsed.library_ids {
            library_groups.push(ids);
        }
        if let Some(ids) = parsed.series_ids {
            series_groups.push(ids);
        }
        if let Some(ids) = parsed.series_ids_excluded {
            series_excluded_groups.push(ids);
        }
        if let Some(ids) = parsed.read_list_ids {
            read_list_groups.push(ids);
        }
        if let Some(ids) = parsed.read_list_ids_excluded {
            read_list_excluded_groups.push(ids);
        }
        if let Some(titles) = parsed.titles {
            title_groups.push(titles);
        }
        if let Some(titles_excluded) = parsed.titles_excluded {
            title_excluded_groups.push(titles_excluded);
        }
        if let Some(titles_contains) = parsed.titles_contains {
            title_contains_groups.push(titles_contains);
        }
        if let Some(titles_contains_excluded) = parsed.titles_contains_excluded {
            title_contains_excluded_groups.push(titles_contains_excluded);
        }
        if let Some(titles_begins_with) = parsed.titles_begins_with {
            title_begins_with_groups.push(titles_begins_with);
        }
        if let Some(titles_begins_with_excluded) = parsed.titles_begins_with_excluded {
            title_begins_with_excluded_groups.push(titles_begins_with_excluded);
        }
        if let Some(titles_ends_with) = parsed.titles_ends_with {
            title_ends_with_groups.push(titles_ends_with);
        }
        if let Some(titles_ends_with_excluded) = parsed.titles_ends_with_excluded {
            title_ends_with_excluded_groups.push(titles_ends_with_excluded);
        }
        if let Some(tags) = parsed.tags {
            tag_groups.push(tags);
        }
        if let Some(tags_excluded) = parsed.tags_excluded {
            tag_excluded_groups.push(tags_excluded);
        }
        if let Some(read_statuses) = parsed.read_statuses {
            read_status_groups.push(read_statuses);
        }
        if let Some(read_statuses_excluded) = parsed.read_statuses_excluded {
            read_status_excluded_groups.push(read_statuses_excluded);
        }
        if let Some(media_profiles) = parsed.media_profiles {
            media_profile_groups.push(media_profiles);
        }
        if let Some(media_profiles_excluded) = parsed.media_profiles_excluded {
            media_profile_excluded_groups.push(media_profiles_excluded);
        }
        if let Some(media_statuses) = parsed.media_statuses {
            media_status_groups.push(media_statuses);
        }
        if let Some(media_statuses_excluded) = parsed.media_statuses_excluded {
            media_status_excluded_groups.push(media_statuses_excluded);
        }
        if let Some(authors) = parsed.authors {
            author_groups.push(authors);
        }
        if let Some(authors_excluded) = parsed.authors_excluded {
            author_excluded_groups.push(authors_excluded);
        }
        if let Some(poster_types) = parsed.poster_types {
            poster_type_groups.push(poster_types);
        }
        if let Some(poster_types_excluded) = parsed.poster_types_excluded {
            poster_type_excluded_groups.push(poster_types_excluded);
        }
        if let Some(release_dates) = parsed.release_dates {
            release_date_groups.push(release_dates);
        }
        if let Some(release_dates_excluded) = parsed.release_dates_excluded {
            release_date_excluded_groups.push(release_dates_excluded);
        }
        if let Some(release_date_gt) = parsed.release_date_gt {
            release_date_gt_bounds.push(release_date_gt);
        }
        if let Some(release_date_lt) = parsed.release_date_lt {
            release_date_lt_bounds.push(release_date_lt);
        }
        if let Some(release_date_begins_with) = parsed.release_date_begins_with {
            release_date_begins_with_groups.push(release_date_begins_with);
        }
        if let Some(release_date_ends_with) = parsed.release_date_ends_with {
            release_date_ends_with_groups.push(release_date_ends_with);
        }
        if let Some(release_date_contains_excluded) = parsed.release_date_contains_excluded {
            release_date_contains_excluded_groups.push(release_date_contains_excluded);
        }
        if let Some(release_date_begins_with_excluded) = parsed.release_date_begins_with_excluded {
            release_date_begins_with_excluded_groups.push(release_date_begins_with_excluded);
        }
        if let Some(release_date_ends_with_excluded) = parsed.release_date_ends_with_excluded {
            release_date_ends_with_excluded_groups.push(release_date_ends_with_excluded);
        }
        if let Some(release_date_in_last_days) = parsed.release_date_in_last_days {
            release_date_in_last_days_bounds.push(release_date_in_last_days);
        }
        if let Some(release_date_not_in_last_days) = parsed.release_date_not_in_last_days {
            release_date_not_in_last_days_bounds.push(release_date_not_in_last_days);
        }
        if let Some(number_sorts) = parsed.number_sorts {
            number_sort_groups.push(number_sorts);
        }
        if let Some(number_sorts_excluded) = parsed.number_sorts_excluded {
            number_sort_excluded_groups.push(number_sorts_excluded);
        }
        if let Some(number_sort_gt) = parsed.number_sort_gt {
            number_sort_gt_bounds.push(number_sort_gt);
        }
        if let Some(number_sort_lt) = parsed.number_sort_lt {
            number_sort_lt_bounds.push(number_sort_lt);
        }

        aggregate.deleted = merge_boolean_filter(aggregate.deleted, parsed.deleted)?;
        aggregate.oneshot = merge_boolean_filter(aggregate.oneshot, parsed.oneshot)?;
        aggregate.tags_null = merge_boolean_filter(aggregate.tags_null, parsed.tags_null)?;
        aggregate.poster_selected =
            merge_boolean_filter(aggregate.poster_selected, parsed.poster_selected)?;
        aggregate.poster_selected_excluded = merge_boolean_filter(
            aggregate.poster_selected_excluded,
            parsed.poster_selected_excluded,
        )?;
        aggregate.release_dates_null =
            merge_boolean_filter(aggregate.release_dates_null, parsed.release_dates_null)?;
    }

    aggregate.library_ids = merge_string_groups(library_groups, all_of);
    aggregate.series_ids = merge_string_groups(series_groups, all_of);
    aggregate.series_ids_excluded = merge_string_groups(series_excluded_groups, all_of);
    aggregate.read_list_ids = merge_string_groups(read_list_groups, all_of);
    aggregate.read_list_ids_excluded = merge_string_groups(read_list_excluded_groups, all_of);
    aggregate.titles = merge_string_groups(title_groups, all_of);
    aggregate.titles_excluded = merge_string_groups(title_excluded_groups, all_of);
    aggregate.titles_contains = merge_string_groups(title_contains_groups, all_of);
    aggregate.titles_contains_excluded =
        merge_string_groups(title_contains_excluded_groups, all_of);
    aggregate.titles_begins_with = merge_string_groups(title_begins_with_groups, all_of);
    aggregate.titles_begins_with_excluded =
        merge_string_groups(title_begins_with_excluded_groups, all_of);
    aggregate.titles_ends_with = merge_string_groups(title_ends_with_groups, all_of);
    aggregate.titles_ends_with_excluded =
        merge_string_groups(title_ends_with_excluded_groups, all_of);
    aggregate.tags = merge_string_groups(tag_groups, all_of);
    aggregate.tags_excluded = merge_string_groups(tag_excluded_groups, all_of);
    aggregate.read_statuses = merge_string_groups(read_status_groups, all_of);
    aggregate.read_statuses_excluded = merge_string_groups(read_status_excluded_groups, all_of);
    aggregate.media_profiles = merge_string_groups(media_profile_groups, all_of);
    aggregate.media_profiles_excluded = merge_string_groups(media_profile_excluded_groups, all_of);
    aggregate.media_statuses = merge_string_groups(media_status_groups, all_of);
    aggregate.media_statuses_excluded = merge_string_groups(media_status_excluded_groups, all_of);
    aggregate.authors = merge_string_groups(author_groups, all_of);
    aggregate.authors_excluded = merge_string_groups(author_excluded_groups, all_of);
    aggregate.poster_types = merge_string_groups(poster_type_groups, all_of);
    aggregate.poster_types_excluded = merge_string_groups(poster_type_excluded_groups, all_of);
    aggregate.release_dates = merge_string_groups(release_date_groups, all_of);
    aggregate.release_dates_excluded = merge_string_groups(release_date_excluded_groups, all_of);
    aggregate.release_date_gt = merge_release_date_lower_bound(release_date_gt_bounds, all_of);
    aggregate.release_date_lt = merge_release_date_upper_bound(release_date_lt_bounds, all_of);
    aggregate.release_date_begins_with =
        merge_string_groups(release_date_begins_with_groups, all_of);
    aggregate.release_date_ends_with = merge_string_groups(release_date_ends_with_groups, all_of);
    aggregate.release_date_contains_excluded =
        merge_string_groups(release_date_contains_excluded_groups, all_of);
    aggregate.release_date_begins_with_excluded =
        merge_string_groups(release_date_begins_with_excluded_groups, all_of);
    aggregate.release_date_ends_with_excluded =
        merge_string_groups(release_date_ends_with_excluded_groups, all_of);
    aggregate.release_date_in_last_days =
        merge_release_date_in_last_days_bound(release_date_in_last_days_bounds, all_of);
    aggregate.release_date_not_in_last_days =
        merge_release_date_not_in_last_days_bound(release_date_not_in_last_days_bounds, all_of);
    aggregate.number_sorts = merge_f64_groups(number_sort_groups, all_of);
    aggregate.number_sorts_excluded = merge_f64_groups(number_sort_excluded_groups, all_of);
    aggregate.number_sort_gt = merge_numeric_lower_bound_f64(number_sort_gt_bounds, all_of);
    aggregate.number_sort_lt = merge_numeric_upper_bound_f64(number_sort_lt_bounds, all_of);
    aggregate.direct_browse_family = if all_of && child_count == 1 && series_leaf_count == 1 {
        Some(DirectBrowseBooksListFamily::BrowseSeriesPaged)
    } else {
        None
    };

    Ok(aggregate)
}

pub(super) fn parse_library_id_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeSeriesFilters, DiscoveryError> {
    series::parse_library_id_filter(condition, mode)
}

pub(super) fn parse_collection_id_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeSeriesFilters, DiscoveryError> {
    series::parse_collection_id_filter(condition, mode)
}

pub(super) fn parse_series_title_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeSeriesFilters, DiscoveryError> {
    series::parse_series_title_filter(condition, mode)
}

pub(super) fn parse_series_title_sort_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeSeriesFilters, DiscoveryError> {
    series::parse_series_title_sort_filter(condition, mode)
}

pub(super) fn parse_deleted_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeSeriesFilters, DiscoveryError> {
    series::parse_deleted_filter(condition, mode)
}

pub(super) fn parse_oneshot_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeSeriesFilters, DiscoveryError> {
    series::parse_oneshot_filter(condition, mode)
}

pub(super) fn parse_series_read_status_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeSeriesFilters, DiscoveryError> {
    series::parse_series_read_status_filter(condition, mode)
}

pub(super) fn parse_series_genre_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeSeriesFilters, DiscoveryError> {
    series::parse_series_genre_filter(condition, mode)
}

pub(super) fn parse_series_tag_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeSeriesFilters, DiscoveryError> {
    series::parse_series_tag_filter(condition, mode)
}

pub(super) fn parse_series_language_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeSeriesFilters, DiscoveryError> {
    series::parse_series_language_filter(condition, mode)
}

pub(super) fn parse_series_publisher_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeSeriesFilters, DiscoveryError> {
    series::parse_series_publisher_filter(condition, mode)
}

pub(super) fn parse_series_age_rating_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeSeriesFilters, DiscoveryError> {
    series::parse_series_age_rating_filter(condition, mode)
}

pub(super) fn parse_series_release_date_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeSeriesFilters, DiscoveryError> {
    series::parse_series_release_date_filter(condition, mode)
}

pub(super) fn parse_series_sharing_label_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeSeriesFilters, DiscoveryError> {
    series::parse_series_sharing_label_filter(condition, mode)
}

pub(super) fn parse_series_status_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeSeriesFilters, DiscoveryError> {
    series::parse_series_status_filter(condition, mode)
}

pub(super) fn parse_series_complete_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeSeriesFilters, DiscoveryError> {
    series::parse_series_complete_filter(condition, mode)
}

pub(super) fn parse_series_author_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeSeriesFilters, DiscoveryError> {
    series::parse_series_author_filter(condition, mode)
}

pub(super) fn parse_composite_filters(
    condition: &Value,
    all_of: bool,
    mode: OperatorValidationMode,
) -> Result<RuntimeSeriesFilters, DiscoveryError> {
    let Some(children) = condition.get("conditions").and_then(Value::as_array) else {
        return Err(DiscoveryError::InvalidSemantics(
            "series composite filter missing conditions".to_string(),
        ));
    };

    let mut aggregate = RuntimeSeriesFilters::default();
    let mut library_groups: Vec<Vec<String>> = vec![];
    let mut title_groups: Vec<Vec<String>> = vec![];
    let mut title_excluded_groups: Vec<Vec<String>> = vec![];
    let mut title_contains_groups: Vec<Vec<String>> = vec![];
    let mut title_contains_excluded_groups: Vec<Vec<String>> = vec![];
    let mut title_begins_with_groups: Vec<Vec<String>> = vec![];
    let mut title_begins_with_excluded_groups: Vec<Vec<String>> = vec![];
    let mut title_ends_with_groups: Vec<Vec<String>> = vec![];
    let mut title_ends_with_excluded_groups: Vec<Vec<String>> = vec![];
    let mut title_sort_groups: Vec<Vec<String>> = vec![];
    let mut title_sort_excluded_groups: Vec<Vec<String>> = vec![];
    let mut title_sort_contains_groups: Vec<Vec<String>> = vec![];
    let mut title_sort_contains_excluded_groups: Vec<Vec<String>> = vec![];
    let mut title_sort_begins_with_groups: Vec<Vec<String>> = vec![];
    let mut title_sort_begins_with_excluded_groups: Vec<Vec<String>> = vec![];
    let mut title_sort_ends_with_groups: Vec<Vec<String>> = vec![];
    let mut title_sort_ends_with_excluded_groups: Vec<Vec<String>> = vec![];
    let mut read_status_groups: Vec<Vec<String>> = vec![];
    let mut read_status_excluded_groups: Vec<Vec<String>> = vec![];
    let mut genre_groups: Vec<Vec<String>> = vec![];
    let mut genre_excluded_groups: Vec<Vec<String>> = vec![];
    let mut tag_groups: Vec<Vec<String>> = vec![];
    let mut tag_excluded_groups: Vec<Vec<String>> = vec![];
    let mut language_groups: Vec<Vec<String>> = vec![];
    let mut language_excluded_groups: Vec<Vec<String>> = vec![];
    let mut publisher_groups: Vec<Vec<String>> = vec![];
    let mut publisher_excluded_groups: Vec<Vec<String>> = vec![];
    let mut release_date_groups: Vec<Vec<String>> = vec![];
    let mut release_date_excluded_groups: Vec<Vec<String>> = vec![];
    let mut release_date_gt_bounds: Vec<String> = vec![];
    let mut release_date_lt_bounds: Vec<String> = vec![];
    let mut release_date_begins_with_groups: Vec<Vec<String>> = vec![];
    let mut release_date_ends_with_groups: Vec<Vec<String>> = vec![];
    let mut release_date_contains_excluded_groups: Vec<Vec<String>> = vec![];
    let mut release_date_begins_with_excluded_groups: Vec<Vec<String>> = vec![];
    let mut release_date_ends_with_excluded_groups: Vec<Vec<String>> = vec![];
    let mut release_date_in_last_days_bounds: Vec<i64> = vec![];
    let mut release_date_not_in_last_days_bounds: Vec<i64> = vec![];
    let mut sharing_label_groups: Vec<Vec<String>> = vec![];
    let mut sharing_label_excluded_groups: Vec<Vec<String>> = vec![];
    let mut series_status_groups: Vec<Vec<String>> = vec![];
    let mut series_status_excluded_groups: Vec<Vec<String>> = vec![];
    let mut author_groups: Vec<Vec<String>> = vec![];
    let mut author_excluded_groups: Vec<Vec<String>> = vec![];
    let mut age_rating_groups: Vec<Vec<u16>> = vec![];
    let mut age_rating_excluded_groups: Vec<Vec<u16>> = vec![];
    let mut age_rating_gt_bounds: Vec<u16> = vec![];
    let mut age_rating_lt_bounds: Vec<u16> = vec![];

    for child in children {
        let parsed = parse_runtime_series_filters_with_mode(Some(child), mode)?;
        let parsed = parsed.criteria;
        if let Some(ids) = parsed.library_ids {
            library_groups.push(ids);
        }
        if let Some(titles) = parsed.titles {
            title_groups.push(titles);
        }
        if let Some(titles_excluded) = parsed.titles_excluded {
            title_excluded_groups.push(titles_excluded);
        }
        if let Some(titles_contains) = parsed.titles_contains {
            title_contains_groups.push(titles_contains);
        }
        if let Some(titles_contains_excluded) = parsed.titles_contains_excluded {
            title_contains_excluded_groups.push(titles_contains_excluded);
        }
        if let Some(titles_begins_with) = parsed.titles_begins_with {
            title_begins_with_groups.push(titles_begins_with);
        }
        if let Some(titles_begins_with_excluded) = parsed.titles_begins_with_excluded {
            title_begins_with_excluded_groups.push(titles_begins_with_excluded);
        }
        if let Some(titles_ends_with) = parsed.titles_ends_with {
            title_ends_with_groups.push(titles_ends_with);
        }
        if let Some(titles_ends_with_excluded) = parsed.titles_ends_with_excluded {
            title_ends_with_excluded_groups.push(titles_ends_with_excluded);
        }
        if let Some(title_sorts) = parsed.title_sorts {
            title_sort_groups.push(title_sorts);
        }
        if let Some(title_sorts_excluded) = parsed.title_sorts_excluded {
            title_sort_excluded_groups.push(title_sorts_excluded);
        }
        if let Some(title_sorts_contains) = parsed.title_sorts_contains {
            title_sort_contains_groups.push(title_sorts_contains);
        }
        if let Some(title_sorts_contains_excluded) = parsed.title_sorts_contains_excluded {
            title_sort_contains_excluded_groups.push(title_sorts_contains_excluded);
        }
        if let Some(title_sorts_begins_with) = parsed.title_sorts_begins_with {
            title_sort_begins_with_groups.push(title_sorts_begins_with);
        }
        if let Some(title_sorts_begins_with_excluded) = parsed.title_sorts_begins_with_excluded {
            title_sort_begins_with_excluded_groups.push(title_sorts_begins_with_excluded);
        }
        if let Some(title_sorts_ends_with) = parsed.title_sorts_ends_with {
            title_sort_ends_with_groups.push(title_sorts_ends_with);
        }
        if let Some(title_sorts_ends_with_excluded) = parsed.title_sorts_ends_with_excluded {
            title_sort_ends_with_excluded_groups.push(title_sorts_ends_with_excluded);
        }
        if let Some(read_statuses) = parsed.read_statuses {
            read_status_groups.push(read_statuses);
        }
        if let Some(read_statuses_excluded) = parsed.read_statuses_excluded {
            read_status_excluded_groups.push(read_statuses_excluded);
        }
        if let Some(genres) = parsed.genres {
            genre_groups.push(genres);
        }
        if let Some(genres_excluded) = parsed.genres_excluded {
            genre_excluded_groups.push(genres_excluded);
        }
        if let Some(tags) = parsed.tags {
            tag_groups.push(tags);
        }
        if let Some(tags_excluded) = parsed.tags_excluded {
            tag_excluded_groups.push(tags_excluded);
        }
        if let Some(languages) = parsed.languages {
            language_groups.push(languages);
        }
        if let Some(languages_excluded) = parsed.languages_excluded {
            language_excluded_groups.push(languages_excluded);
        }
        if let Some(publishers) = parsed.publishers {
            publisher_groups.push(publishers);
        }
        if let Some(publishers_excluded) = parsed.publishers_excluded {
            publisher_excluded_groups.push(publishers_excluded);
        }
        if let Some(age_ratings) = parsed.age_ratings {
            age_rating_groups.push(age_ratings);
        }
        if let Some(age_ratings_excluded) = parsed.age_ratings_excluded {
            age_rating_excluded_groups.push(age_ratings_excluded);
        }
        if let Some(age_rating_gt) = parsed.age_rating_gt {
            age_rating_gt_bounds.push(age_rating_gt);
        }
        if let Some(age_rating_lt) = parsed.age_rating_lt {
            age_rating_lt_bounds.push(age_rating_lt);
        }
        if let Some(release_dates) = parsed.release_dates {
            release_date_groups.push(release_dates);
        }
        if let Some(release_dates_excluded) = parsed.release_dates_excluded {
            release_date_excluded_groups.push(release_dates_excluded);
        }
        if let Some(release_date_gt) = parsed.release_date_gt {
            release_date_gt_bounds.push(release_date_gt);
        }
        if let Some(release_date_lt) = parsed.release_date_lt {
            release_date_lt_bounds.push(release_date_lt);
        }
        if let Some(release_date_begins_with) = parsed.release_date_begins_with {
            release_date_begins_with_groups.push(release_date_begins_with);
        }
        if let Some(release_date_ends_with) = parsed.release_date_ends_with {
            release_date_ends_with_groups.push(release_date_ends_with);
        }
        if let Some(release_date_contains_excluded) = parsed.release_date_contains_excluded {
            release_date_contains_excluded_groups.push(release_date_contains_excluded);
        }
        if let Some(release_date_begins_with_excluded) = parsed.release_date_begins_with_excluded {
            release_date_begins_with_excluded_groups.push(release_date_begins_with_excluded);
        }
        if let Some(release_date_ends_with_excluded) = parsed.release_date_ends_with_excluded {
            release_date_ends_with_excluded_groups.push(release_date_ends_with_excluded);
        }
        if let Some(release_date_in_last_days) = parsed.release_date_in_last_days {
            release_date_in_last_days_bounds.push(release_date_in_last_days);
        }
        if let Some(release_date_not_in_last_days) = parsed.release_date_not_in_last_days {
            release_date_not_in_last_days_bounds.push(release_date_not_in_last_days);
        }
        if let Some(sharing_labels) = parsed.sharing_labels {
            sharing_label_groups.push(sharing_labels);
        }
        if let Some(sharing_labels_excluded) = parsed.sharing_labels_excluded {
            sharing_label_excluded_groups.push(sharing_labels_excluded);
        }
        if let Some(series_statuses) = parsed.series_statuses {
            series_status_groups.push(series_statuses);
        }
        if let Some(series_statuses_excluded) = parsed.series_statuses_excluded {
            series_status_excluded_groups.push(series_statuses_excluded);
        }
        if let Some(authors) = parsed.authors {
            author_groups.push(authors);
        }
        if let Some(authors_excluded) = parsed.authors_excluded {
            author_excluded_groups.push(authors_excluded);
        }

        aggregate.deleted = merge_boolean_filter(aggregate.deleted, parsed.deleted)?;
        aggregate.oneshot = merge_boolean_filter(aggregate.oneshot, parsed.oneshot)?;
        aggregate.genres_null = merge_boolean_filter(aggregate.genres_null, parsed.genres_null)?;
        aggregate.tags_null = merge_boolean_filter(aggregate.tags_null, parsed.tags_null)?;
        aggregate.age_ratings_null =
            merge_boolean_filter(aggregate.age_ratings_null, parsed.age_ratings_null)?;
        aggregate.sharing_labels_null =
            merge_boolean_filter(aggregate.sharing_labels_null, parsed.sharing_labels_null)?;
        aggregate.release_dates_null =
            merge_boolean_filter(aggregate.release_dates_null, parsed.release_dates_null)?;
        aggregate.complete = merge_boolean_filter(aggregate.complete, parsed.complete)?;
    }

    aggregate.library_ids = if library_groups.is_empty() {
        None
    } else if all_of {
        let mut intersection = library_groups[0].clone();
        for group in library_groups.iter().skip(1) {
            intersection.retain(|candidate| group.contains(candidate));
        }
        Some(intersection)
    } else {
        let mut union = vec![];
        for group in library_groups {
            for candidate in group {
                if !union.contains(&candidate) {
                    union.push(candidate);
                }
            }
        }
        Some(union)
    };

    aggregate.read_statuses = merge_string_groups(read_status_groups, all_of);
    aggregate.read_statuses_excluded = merge_string_groups(read_status_excluded_groups, all_of);
    aggregate.titles = merge_string_groups(title_groups, all_of);
    aggregate.titles_excluded = merge_string_groups(title_excluded_groups, all_of);
    aggregate.titles_contains = merge_string_groups(title_contains_groups, all_of);
    aggregate.titles_contains_excluded =
        merge_string_groups(title_contains_excluded_groups, all_of);
    aggregate.titles_begins_with = merge_string_groups(title_begins_with_groups, all_of);
    aggregate.titles_begins_with_excluded =
        merge_string_groups(title_begins_with_excluded_groups, all_of);
    aggregate.titles_ends_with = merge_string_groups(title_ends_with_groups, all_of);
    aggregate.titles_ends_with_excluded =
        merge_string_groups(title_ends_with_excluded_groups, all_of);
    aggregate.title_sorts = merge_string_groups(title_sort_groups, all_of);
    aggregate.title_sorts_excluded = merge_string_groups(title_sort_excluded_groups, all_of);
    aggregate.title_sorts_contains = merge_string_groups(title_sort_contains_groups, all_of);
    aggregate.title_sorts_contains_excluded =
        merge_string_groups(title_sort_contains_excluded_groups, all_of);
    aggregate.title_sorts_begins_with = merge_string_groups(title_sort_begins_with_groups, all_of);
    aggregate.title_sorts_begins_with_excluded =
        merge_string_groups(title_sort_begins_with_excluded_groups, all_of);
    aggregate.title_sorts_ends_with = merge_string_groups(title_sort_ends_with_groups, all_of);
    aggregate.title_sorts_ends_with_excluded =
        merge_string_groups(title_sort_ends_with_excluded_groups, all_of);
    aggregate.genres = merge_string_groups(genre_groups, all_of);
    aggregate.genres_excluded = merge_string_groups(genre_excluded_groups, all_of);
    aggregate.tags = merge_string_groups(tag_groups, all_of);
    aggregate.tags_excluded = merge_string_groups(tag_excluded_groups, all_of);
    aggregate.languages = merge_string_groups(language_groups, all_of);
    aggregate.languages_excluded = merge_string_groups(language_excluded_groups, all_of);
    aggregate.publishers = merge_string_groups(publisher_groups, all_of);
    aggregate.publishers_excluded = merge_string_groups(publisher_excluded_groups, all_of);
    aggregate.age_ratings = merge_u16_groups(age_rating_groups, all_of);
    aggregate.age_ratings_excluded = merge_u16_groups(age_rating_excluded_groups, all_of);
    aggregate.age_rating_gt = merge_u16_lower_bound(age_rating_gt_bounds, all_of);
    aggregate.age_rating_lt = merge_u16_upper_bound(age_rating_lt_bounds, all_of);
    aggregate.release_dates = merge_string_groups(release_date_groups, all_of);
    aggregate.release_dates_excluded = merge_string_groups(release_date_excluded_groups, all_of);
    aggregate.release_date_gt = merge_release_date_lower_bound(release_date_gt_bounds, all_of);
    aggregate.release_date_lt = merge_release_date_upper_bound(release_date_lt_bounds, all_of);
    aggregate.release_date_begins_with =
        merge_string_groups(release_date_begins_with_groups, all_of);
    aggregate.release_date_ends_with = merge_string_groups(release_date_ends_with_groups, all_of);
    aggregate.release_date_contains_excluded =
        merge_string_groups(release_date_contains_excluded_groups, all_of);
    aggregate.release_date_begins_with_excluded =
        merge_string_groups(release_date_begins_with_excluded_groups, all_of);
    aggregate.release_date_ends_with_excluded =
        merge_string_groups(release_date_ends_with_excluded_groups, all_of);
    aggregate.release_date_in_last_days =
        merge_release_date_in_last_days_bound(release_date_in_last_days_bounds, all_of);
    aggregate.release_date_not_in_last_days =
        merge_release_date_not_in_last_days_bound(release_date_not_in_last_days_bounds, all_of);
    aggregate.sharing_labels = merge_string_groups(sharing_label_groups, all_of);
    aggregate.sharing_labels_excluded = merge_string_groups(sharing_label_excluded_groups, all_of);
    aggregate.series_statuses = merge_string_groups(series_status_groups, all_of);
    aggregate.series_statuses_excluded = merge_string_groups(series_status_excluded_groups, all_of);
    aggregate.authors = merge_string_groups(author_groups, all_of);
    aggregate.authors_excluded = merge_string_groups(author_excluded_groups, all_of);

    Ok(aggregate)
}

pub(super) fn merge_boolean_filter(
    left: Option<bool>,
    right: Option<bool>,
) -> Result<Option<bool>, DiscoveryError> {
    match (left, right) {
        (Some(a), Some(b)) if a == b => Ok(Some(a)),
        (Some(_), Some(_)) => Ok(None),
        (Some(value), None) | (None, Some(value)) => Ok(Some(value)),
        (None, None) => Ok(None),
    }
}
