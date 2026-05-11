use super::common_helpers::{
    TextMatchMode, any_ignore_ascii_case, any_normalized_text_matches, matches_optional_value,
    normalized_text_matches,
};
use super::*;
use komga_domain::discovery::{
    AgeRatingCondition, DateCondition, FilterOperator, InclusionCondition, ReadStatusCondition,
    SeriesCondition, SeriesStatusCondition, SeriesValueCondition, StringCondition,
};
use regex::{Regex, RegexBuilder};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::{SystemTime, UNIX_EPOCH};

fn compile_case_insensitive_regexes(
    patterns: Option<&Vec<String>>,
    field: &str,
) -> Result<Option<Vec<Regex>>, String> {
    patterns
        .map(|patterns| {
            patterns
                .iter()
                .map(|pattern| {
                    RegexBuilder::new(pattern)
                        .case_insensitive(true)
                        .build()
                        .map_err(|error| format!("invalid {field} regex `{pattern}`: {error}"))
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()
}

fn matches_any_regex(value: &str, regexes: &[Regex]) -> bool {
    regexes.iter().any(|regex| regex.is_match(value))
}

fn compare_rank_order(
    order: &HashMap<String, usize>,
    left_id: &str,
    right_id: &str,
    descending: bool,
) -> std::cmp::Ordering {
    let left_rank = order.get(left_id).copied();
    let right_rank = order.get(right_id).copied();
    match (left_rank, right_rank) {
        (Some(left), Some(right)) if descending => {
            right.cmp(&left).then_with(|| left_id.cmp(right_id))
        }
        (Some(left), Some(right)) => left.cmp(&right).then_with(|| left_id.cmp(right_id)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => left_id.cmp(right_id),
    }
}

fn random_sort_keys(series: &[PersistedSeriesSummary]) -> HashMap<String, u64> {
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0);

    series
        .iter()
        .map(|row| {
            let mut hasher = DefaultHasher::new();
            seed.hash(&mut hasher);
            row.id.hash(&mut hasher);
            (row.id.clone(), hasher.finish())
        })
        .collect()
}

struct SeriesConditionEvaluationData {
    user_id_present: bool,
    collection_memberships: Option<BTreeMap<String, BTreeSet<String>>>,
    read_progress: Option<HashMap<String, (i64, i64)>>,
    total_book_counts: Option<HashMap<String, i64>>,
    release_date_cutoffs: HashMap<i64, Option<String>>,
}

impl SeriesConditionEvaluationData {
    async fn load(
        backend: &dyn PersistedDiscoveryBrowseDataSource,
        context: &DiscoveryQueryContext,
        condition: &SeriesCondition,
    ) -> Result<Self, String> {
        let collection_memberships = if condition_needs_collection_memberships(condition) {
            Some(backend.load_collection_memberships().await?)
        } else {
            None
        };
        let read_progress = if condition_needs_read_progress(condition) {
            if let Some(user_id) = context.user_id.as_deref() {
                Some(backend.load_series_read_progress_counts(user_id).await?)
            } else {
                None
            }
        } else {
            None
        };
        let total_book_counts = if condition_needs_total_book_counts(condition) {
            Some(backend.load_series_total_book_counts().await?)
        } else {
            None
        };
        let mut release_date_offsets = BTreeSet::new();
        collect_series_release_date_offsets(condition, &mut release_date_offsets);
        let mut release_date_cutoffs = HashMap::new();
        for days in release_date_offsets {
            release_date_cutoffs.insert(days, backend.persisted_utc_date_minus_days(days).await?);
        }

        Ok(Self {
            user_id_present: context.user_id.is_some(),
            collection_memberships,
            read_progress,
            total_book_counts,
            release_date_cutoffs,
        })
    }
}

fn condition_needs_collection_memberships(condition: &SeriesCondition) -> bool {
    match condition {
        SeriesCondition::Value(SeriesValueCondition::CollectionId(_)) => true,
        SeriesCondition::Composite(composite) => composite
            .conditions
            .iter()
            .any(condition_needs_collection_memberships),
        _ => false,
    }
}

fn condition_needs_read_progress(condition: &SeriesCondition) -> bool {
    match condition {
        SeriesCondition::Value(SeriesValueCondition::ReadStatus(_)) => true,
        SeriesCondition::Composite(composite) => composite
            .conditions
            .iter()
            .any(condition_needs_read_progress),
        _ => false,
    }
}

fn condition_needs_total_book_counts(condition: &SeriesCondition) -> bool {
    match condition {
        SeriesCondition::Value(SeriesValueCondition::Complete(_)) => true,
        SeriesCondition::Composite(composite) => composite
            .conditions
            .iter()
            .any(condition_needs_total_book_counts),
        _ => false,
    }
}

fn collect_series_release_date_offsets(condition: &SeriesCondition, offsets: &mut BTreeSet<i64>) {
    match condition {
        SeriesCondition::Value(SeriesValueCondition::ReleaseDate(
            DateCondition::WithinLastDays(days) | DateCondition::OutsideLastDays(days),
        )) => {
            offsets.insert(*days);
        }
        SeriesCondition::Composite(composite) => {
            for child in &composite.conditions {
                collect_series_release_date_offsets(child, offsets);
            }
        }
        _ => {}
    }
}

fn condition_contains_deleted(condition: Option<&SeriesCondition>) -> bool {
    fn visit(condition: &SeriesCondition) -> bool {
        match condition {
            SeriesCondition::Value(SeriesValueCondition::Deleted(_)) => true,
            SeriesCondition::Composite(composite) => composite.conditions.iter().any(visit),
            _ => false,
        }
    }

    condition.is_some_and(visit)
}

fn first_collection_sort_id(condition: Option<&SeriesCondition>) -> Option<&str> {
    fn visit(condition: &SeriesCondition) -> Option<&str> {
        match condition {
            SeriesCondition::Value(SeriesValueCondition::CollectionId(
                InclusionCondition::Include(values),
            )) => values.first().map(|value| value.as_str()),
            SeriesCondition::Composite(composite) => composite.conditions.iter().find_map(visit),
            _ => None,
        }
    }

    condition.and_then(visit)
}

fn row_matches_series_condition(
    row: &PersistedSeriesSummary,
    condition: &SeriesCondition,
    data: &SeriesConditionEvaluationData,
) -> bool {
    match condition {
        SeriesCondition::Value(value) => row_matches_series_value_condition(row, value, data),
        SeriesCondition::Composite(composite) => match composite.operator {
            FilterOperator::All => composite
                .conditions
                .iter()
                .all(|condition| row_matches_series_condition(row, condition, data)),
            FilterOperator::Any => {
                composite.conditions.is_empty()
                    || composite
                        .conditions
                        .iter()
                        .any(|condition| row_matches_series_condition(row, condition, data))
            }
        },
    }
}

fn row_matches_series_value_condition(
    row: &PersistedSeriesSummary,
    condition: &SeriesValueCondition,
    data: &SeriesConditionEvaluationData,
) -> bool {
    match condition {
        SeriesValueCondition::LibraryId(inc) => {
            matches_string_inclusion(row.library_id.as_str(), inc, |id| id.as_str())
        }
        SeriesValueCondition::CollectionId(inc) => matches_collection_condition(row, inc, data),
        SeriesValueCondition::Title(condition) => matches_string_condition(&row.title, condition),
        SeriesValueCondition::TitleSort(condition) => {
            matches_string_condition(&row.title_sort, condition)
        }
        SeriesValueCondition::Deleted(value) => row.deleted == *value,
        SeriesValueCondition::OneShot(value) => row.oneshot == *value,
        SeriesValueCondition::ReadStatus(condition) => {
            matches_series_read_status_condition(row, condition, data)
        }
        SeriesValueCondition::Genre(condition) => {
            matches_string_values_condition(row.genres.iter().map(String::as_str), condition)
        }
        SeriesValueCondition::Tag(condition) => matches_string_values_condition(
            row.tags
                .iter()
                .chain(row.books_metadata_tags.iter())
                .map(String::as_str),
            condition,
        ),
        SeriesValueCondition::Language(inc) => {
            matches_non_empty_string_inclusion(&row.language, inc)
        }
        SeriesValueCondition::Publisher(inc) => {
            matches_non_empty_string_inclusion(&row.publisher, inc)
        }
        SeriesValueCondition::AgeRating(condition) => {
            matches_age_rating_condition(row.age_rating, condition)
        }
        SeriesValueCondition::ReleaseDate(condition) => {
            matches_date_condition(row.books_metadata_release_date.as_deref(), condition, data)
        }
        SeriesValueCondition::SharingLabel(condition) => {
            matches_string_values_condition(row.labels.iter().map(String::as_str), condition)
        }
        SeriesValueCondition::SeriesStatus(SeriesStatusCondition::Include(values)) => {
            any_ignore_ascii_case([row.status.as_str()], values)
        }
        SeriesValueCondition::SeriesStatus(SeriesStatusCondition::Exclude(values)) => {
            !any_ignore_ascii_case([row.status.as_str()], values)
        }
        SeriesValueCondition::Complete(value) => data
            .total_book_counts
            .as_ref()
            .and_then(|counts| counts.get(&row.id))
            .map(|count| (*count).max(0) as u64 == row.books_count)
            .map(|complete| complete == *value)
            .unwrap_or(false),
        SeriesValueCondition::Author(condition) => matches_author_condition(row, condition),
        SeriesValueCondition::ExcludeNewlyAdded(value) => {
            !*value || row.created != row.last_modified
        }
    }
}

fn matches_string_inclusion<T>(
    actual: &str,
    condition: &InclusionCondition<T>,
    value: impl Fn(&T) -> &str,
) -> bool {
    match condition {
        InclusionCondition::Include(values) => values
            .iter()
            .any(|expected| actual.eq_ignore_ascii_case(value(expected))),
        InclusionCondition::Exclude(values) => !values
            .iter()
            .any(|expected| actual.eq_ignore_ascii_case(value(expected))),
    }
}

fn matches_non_empty_string_inclusion(
    actual: &str,
    condition: &InclusionCondition<String>,
) -> bool {
    let actual = (!actual.is_empty()).then_some(actual);
    match condition {
        InclusionCondition::Include(values) => {
            actual.is_some_and(|actual| any_ignore_ascii_case([actual], values))
        }
        InclusionCondition::Exclude(values) => actual
            .map(|actual| !any_ignore_ascii_case([actual], values))
            .unwrap_or(false),
    }
}

fn matches_collection_condition(
    row: &PersistedSeriesSummary,
    condition: &InclusionCondition<komga_domain::common_ids::CollectionId>,
    data: &SeriesConditionEvaluationData,
) -> bool {
    let memberships = data
        .collection_memberships
        .as_ref()
        .and_then(|memberships| memberships.get(&row.id));
    match condition {
        InclusionCondition::Include(values) => memberships.is_some_and(|memberships| {
            values
                .iter()
                .any(|value| memberships.contains(value.as_str()))
        }),
        InclusionCondition::Exclude(values) => memberships
            .map(|memberships| {
                !values
                    .iter()
                    .any(|value| memberships.contains(value.as_str()))
            })
            .unwrap_or(true),
    }
}

fn matches_series_read_status_condition(
    row: &PersistedSeriesSummary,
    condition: &ReadStatusCondition,
    data: &SeriesConditionEvaluationData,
) -> bool {
    if !data.user_id_present {
        return false;
    }
    let read_progress = data
        .read_progress
        .as_ref()
        .and_then(|progress| progress.get(&row.id).copied());
    match condition {
        ReadStatusCondition::Include(values) => values
            .iter()
            .any(|status| series_matches_read_status(row, read_progress, status)),
        ReadStatusCondition::Exclude(values) => !values
            .iter()
            .any(|status| series_matches_read_status(row, read_progress, status)),
    }
}

fn matches_string_condition(actual: &str, condition: &StringCondition) -> bool {
    match condition {
        StringCondition::Exact(InclusionCondition::Include(values)) => {
            normalized_text_matches(actual, values, TextMatchMode::Exact)
        }
        StringCondition::Exact(InclusionCondition::Exclude(values)) => {
            !normalized_text_matches(actual, values, TextMatchMode::Exact)
        }
        StringCondition::Contains(InclusionCondition::Include(values)) => {
            normalized_text_matches(actual, values, TextMatchMode::Contains)
        }
        StringCondition::Contains(InclusionCondition::Exclude(values)) => {
            !normalized_text_matches(actual, values, TextMatchMode::Contains)
        }
        StringCondition::StartsWith(InclusionCondition::Include(values)) => {
            normalized_text_matches(actual, values, TextMatchMode::StartsWith)
        }
        StringCondition::StartsWith(InclusionCondition::Exclude(values)) => {
            !normalized_text_matches(actual, values, TextMatchMode::StartsWith)
        }
        StringCondition::EndsWith(InclusionCondition::Include(values)) => {
            normalized_text_matches(actual, values, TextMatchMode::EndsWith)
        }
        StringCondition::EndsWith(InclusionCondition::Exclude(values)) => {
            !normalized_text_matches(actual, values, TextMatchMode::EndsWith)
        }
        StringCondition::IsEmpty => actual.is_empty(),
        StringCondition::IsNotEmpty => !actual.is_empty(),
    }
}

fn matches_string_values_condition<'a>(
    values: impl IntoIterator<Item = &'a str>,
    condition: &StringCondition,
) -> bool {
    let values = values.into_iter().collect::<Vec<_>>();
    match condition {
        StringCondition::Exact(InclusionCondition::Include(expected)) => {
            any_normalized_text_matches(values.iter().copied(), expected, TextMatchMode::Exact)
        }
        StringCondition::Exact(InclusionCondition::Exclude(expected)) => {
            !any_normalized_text_matches(values.iter().copied(), expected, TextMatchMode::Exact)
        }
        StringCondition::Contains(InclusionCondition::Include(expected)) => {
            any_normalized_text_matches(values.iter().copied(), expected, TextMatchMode::Contains)
        }
        StringCondition::Contains(InclusionCondition::Exclude(expected)) => {
            !any_normalized_text_matches(values.iter().copied(), expected, TextMatchMode::Contains)
        }
        StringCondition::StartsWith(InclusionCondition::Include(expected)) => {
            any_normalized_text_matches(values.iter().copied(), expected, TextMatchMode::StartsWith)
        }
        StringCondition::StartsWith(InclusionCondition::Exclude(expected)) => {
            !any_normalized_text_matches(
                values.iter().copied(),
                expected,
                TextMatchMode::StartsWith,
            )
        }
        StringCondition::EndsWith(InclusionCondition::Include(expected)) => {
            any_normalized_text_matches(values.iter().copied(), expected, TextMatchMode::EndsWith)
        }
        StringCondition::EndsWith(InclusionCondition::Exclude(expected)) => {
            !any_normalized_text_matches(values.iter().copied(), expected, TextMatchMode::EndsWith)
        }
        StringCondition::IsEmpty => values.is_empty(),
        StringCondition::IsNotEmpty => !values.is_empty(),
    }
}

fn matches_age_rating_condition(actual: Option<u16>, condition: &AgeRatingCondition) -> bool {
    match condition {
        AgeRatingCondition::Exact(InclusionCondition::Include(values)) => {
            actual.is_some_and(|actual| values.contains(&actual))
        }
        AgeRatingCondition::Exact(InclusionCondition::Exclude(values)) => actual
            .map(|actual| !values.contains(&actual))
            .unwrap_or(true),
        AgeRatingCondition::ExactOrEmpty(values) => actual
            .map(|actual| values.contains(&actual))
            .unwrap_or(true),
        AgeRatingCondition::GreaterThan(value) => {
            actual.map(|actual| actual > *value).unwrap_or(false)
        }
        AgeRatingCondition::LessThan(value) => {
            actual.map(|actual| actual < *value).unwrap_or(false)
        }
        AgeRatingCondition::IsEmpty => actual.is_none(),
        AgeRatingCondition::IsNotEmpty => actual.is_some(),
    }
}

fn matches_author_condition(row: &PersistedSeriesSummary, condition: &StringCondition) -> bool {
    match condition {
        StringCondition::Contains(InclusionCondition::Include(values)) => row
            .books_metadata_authors
            .iter()
            .any(|author| author_contains_filter_value(author, values)),
        StringCondition::Contains(InclusionCondition::Exclude(values)) => !row
            .books_metadata_authors
            .iter()
            .any(|author| author_contains_filter_value(author, values)),
        StringCondition::Exact(InclusionCondition::Include(values)) => row
            .books_metadata_authors
            .iter()
            .any(|author| author_matches_filter_value(author, values)),
        StringCondition::Exact(InclusionCondition::Exclude(values)) => !row
            .books_metadata_authors
            .iter()
            .any(|author| author_matches_filter_value(author, values)),
        StringCondition::IsEmpty => row.books_metadata_authors.is_empty(),
        StringCondition::IsNotEmpty => !row.books_metadata_authors.is_empty(),
        StringCondition::StartsWith(_) | StringCondition::EndsWith(_) => false,
    }
}

fn matches_date_condition(
    actual: Option<&str>,
    condition: &DateCondition,
    data: &SeriesConditionEvaluationData,
) -> bool {
    match condition {
        DateCondition::Exact(InclusionCondition::Include(values)) => {
            actual.is_some_and(|actual| values.iter().any(|value| value == actual))
        }
        DateCondition::Exact(InclusionCondition::Exclude(values)) => actual
            .map(|actual| !values.iter().any(|value| value == actual))
            .unwrap_or(true),
        DateCondition::Before(value) => actual.is_some_and(|actual| actual < value.as_str()),
        DateCondition::After(value) => actual.is_some_and(|actual| actual > value.as_str()),
        DateCondition::Contains(InclusionCondition::Include(values)) => actual
            .is_some_and(|actual| normalized_text_matches(actual, values, TextMatchMode::Contains)),
        DateCondition::Contains(InclusionCondition::Exclude(values)) => actual
            .map(|actual| !normalized_text_matches(actual, values, TextMatchMode::Contains))
            .unwrap_or(true),
        DateCondition::StartsWith(InclusionCondition::Include(values)) => {
            actual.is_some_and(|actual| {
                normalized_text_matches(actual, values, TextMatchMode::StartsWith)
            })
        }
        DateCondition::StartsWith(InclusionCondition::Exclude(values)) => actual
            .map(|actual| !normalized_text_matches(actual, values, TextMatchMode::StartsWith))
            .unwrap_or(true),
        DateCondition::EndsWith(InclusionCondition::Include(values)) => actual
            .is_some_and(|actual| normalized_text_matches(actual, values, TextMatchMode::EndsWith)),
        DateCondition::EndsWith(InclusionCondition::Exclude(values)) => actual
            .map(|actual| !normalized_text_matches(actual, values, TextMatchMode::EndsWith))
            .unwrap_or(true),
        DateCondition::WithinLastDays(days) => data
            .release_date_cutoffs
            .get(days)
            .and_then(Option::as_deref)
            .map(|cutoff| actual.is_some_and(|actual| actual > cutoff))
            .unwrap_or(true),
        DateCondition::OutsideLastDays(days) => data
            .release_date_cutoffs
            .get(days)
            .and_then(Option::as_deref)
            .map(|cutoff| actual.is_some_and(|actual| actual < cutoff))
            .unwrap_or(true),
        DateCondition::IsEmpty => actual.is_none(),
        DateCondition::IsNotEmpty => actual.is_some(),
    }
}

pub(crate) async fn load_persisted_series_page(
    backend: &dyn PersistedDiscoveryBrowseDataSource,
    context: &DiscoveryQueryContext,
    query: PersistedSeriesBrowseQuery,
) -> Result<PageEnvelope<PersistedSeriesSummary>, String> {
    let mut series = Vec::new();
    let filters = &query.filters;
    let title_regexes = compile_case_insensitive_regexes(filters.titles_regex.as_ref(), "title")?;
    let title_sort_regexes =
        compile_case_insensitive_regexes(filters.title_sorts_regex.as_ref(), "titleSort")?;
    let mut search_order: HashMap<String, usize> = HashMap::new();
    if let Some(search) = query.search.as_ref().map(|value| value.trim())
        && !search.is_empty()
    {
        let total_count = backend.load_persisted_series_count().await?;
        let ranked_candidates = backend
            .search_series_scored_ids(search, total_count.max(1))
            .await?;
        let candidate_ids = ranked_candidates
            .iter()
            .map(|(_, id)| id.clone())
            .collect::<Vec<_>>();
        if candidate_ids.is_empty() {
            series.clear();
        } else {
            search_order = ranked_candidates
                .iter()
                .enumerate()
                .map(|(index, (_, id))| (id.clone(), index))
                .collect();
            series = backend
                .load_persisted_series_summaries_by_ids(&candidate_ids)
                .await?;
        }
    } else {
        series = backend.load_persisted_series_summaries().await?;
    }

    if let Some(allowed_ids) = context.authorized_library_ids.as_ref() {
        series = filter_rows(series, |row| {
            allowed_ids.iter().any(|id| id == row.library_id.as_str())
        });
    }
    if let Some(library_ids) = filters.library_ids.as_ref() {
        series = filter_rows(series, |row| {
            library_ids.iter().any(|id| id == row.library_id.as_str())
        });
    }

    if let Some(restrictions) = context.restrictions.as_ref() {
        if let (Some(age), Some(AgeRestrictionKind::Exclude)) =
            (restrictions.age, restrictions.age_restriction)
        {
            series = filter_rows(series, |row| {
                row.age_rating
                    .map(|age_rating| age_rating < age)
                    .unwrap_or(true)
            });
        }

        if !restrictions.labels_exclude.is_empty() {
            series = filter_rows(series, |row| {
                !any_ignore_ascii_case(
                    row.labels.iter().map(String::as_str),
                    &restrictions.labels_exclude,
                )
            });
        }

        if !restrictions.labels_allow.is_empty() {
            series = filter_rows(series, |row| {
                any_ignore_ascii_case(
                    row.labels.iter().map(String::as_str),
                    &restrictions.labels_allow,
                )
            });
        }
    }

    if let Some(condition) = query.condition.as_ref() {
        let evaluation_data =
            SeriesConditionEvaluationData::load(backend, context, condition).await?;
        series = filter_rows(series, |row| {
            row_matches_series_condition(row, condition, &evaluation_data)
        });
    }

    if let Some(titles) = filters.titles.as_ref() {
        series = filter_rows(series, |row| {
            normalized_text_matches(&row.title, titles, TextMatchMode::Exact)
        });
    }

    if let Some(titles_excluded) = filters.titles_excluded.as_ref() {
        series = filter_rows(series, |row| {
            !normalized_text_matches(&row.title, titles_excluded, TextMatchMode::Exact)
        });
    }

    if let Some(titles_contains) = filters.titles_contains.as_ref() {
        series = filter_rows(series, |row| {
            normalized_text_matches(&row.title, titles_contains, TextMatchMode::Contains)
        });
    }

    if let Some(titles_contains_excluded) = filters.titles_contains_excluded.as_ref() {
        series = filter_rows(series, |row| {
            !normalized_text_matches(
                &row.title,
                titles_contains_excluded,
                TextMatchMode::Contains,
            )
        });
    }

    if let Some(titles_begins_with) = filters.titles_begins_with.as_ref() {
        series = filter_rows(series, |row| {
            normalized_text_matches(&row.title, titles_begins_with, TextMatchMode::StartsWith)
        });
    }

    if let Some(titles_begins_with_excluded) = filters.titles_begins_with_excluded.as_ref() {
        series = filter_rows(series, |row| {
            !normalized_text_matches(
                &row.title,
                titles_begins_with_excluded,
                TextMatchMode::StartsWith,
            )
        });
    }

    if let Some(titles_ends_with) = filters.titles_ends_with.as_ref() {
        series = filter_rows(series, |row| {
            normalized_text_matches(&row.title, titles_ends_with, TextMatchMode::EndsWith)
        });
    }

    if let Some(titles_ends_with_excluded) = filters.titles_ends_with_excluded.as_ref() {
        series = filter_rows(series, |row| {
            !normalized_text_matches(
                &row.title,
                titles_ends_with_excluded,
                TextMatchMode::EndsWith,
            )
        });
    }

    if let Some(title_regexes) = title_regexes.as_ref() {
        series = filter_rows(series, |row| matches_any_regex(&row.title, title_regexes));
    }

    if let Some(title_sorts) = filters.title_sorts.as_ref() {
        series = filter_rows(series, |row| {
            normalized_text_matches(&row.title_sort, title_sorts, TextMatchMode::Exact)
        });
    }

    if let Some(title_sorts_excluded) = filters.title_sorts_excluded.as_ref() {
        series = filter_rows(series, |row| {
            !normalized_text_matches(&row.title_sort, title_sorts_excluded, TextMatchMode::Exact)
        });
    }

    if let Some(title_sorts_contains) = filters.title_sorts_contains.as_ref() {
        series = filter_rows(series, |row| {
            normalized_text_matches(
                &row.title_sort,
                title_sorts_contains,
                TextMatchMode::Contains,
            )
        });
    }

    if let Some(title_sorts_contains_excluded) = filters.title_sorts_contains_excluded.as_ref() {
        series = filter_rows(series, |row| {
            !normalized_text_matches(
                &row.title_sort,
                title_sorts_contains_excluded,
                TextMatchMode::Contains,
            )
        });
    }

    if let Some(title_sorts_begins_with) = filters.title_sorts_begins_with.as_ref() {
        series = filter_rows(series, |row| {
            normalized_text_matches(
                &row.title_sort,
                title_sorts_begins_with,
                TextMatchMode::StartsWith,
            )
        });
    }

    if let Some(title_sorts_begins_with_excluded) =
        filters.title_sorts_begins_with_excluded.as_ref()
    {
        series = filter_rows(series, |row| {
            !normalized_text_matches(
                &row.title_sort,
                title_sorts_begins_with_excluded,
                TextMatchMode::StartsWith,
            )
        });
    }

    if let Some(title_sorts_ends_with) = filters.title_sorts_ends_with.as_ref() {
        series = filter_rows(series, |row| {
            normalized_text_matches(
                &row.title_sort,
                title_sorts_ends_with,
                TextMatchMode::EndsWith,
            )
        });
    }

    if let Some(title_sorts_ends_with_excluded) = filters.title_sorts_ends_with_excluded.as_ref() {
        series = filter_rows(series, |row| {
            !normalized_text_matches(
                &row.title_sort,
                title_sorts_ends_with_excluded,
                TextMatchMode::EndsWith,
            )
        });
    }

    if let Some(title_sort_regexes) = title_sort_regexes.as_ref() {
        series = filter_rows(series, |row| {
            matches_any_regex(&row.title_sort, title_sort_regexes)
        });
    }

    if !condition_contains_deleted(query.condition.as_ref()) {
        let deleted = filters.deleted.unwrap_or_default();
        series = filter_rows(series, |row| row.deleted == deleted);
    }

    if let Some(oneshot) = filters.oneshot {
        series = filter_rows(series, |row| row.oneshot == oneshot);
    }

    if filters.exclude_newly_added {
        series = filter_rows(series, |row| row.created != row.last_modified);
    }

    if filters.read_statuses.is_some() || filters.read_statuses_excluded.is_some() {
        let Some(user_id) = context.user_id.as_deref() else {
            series.clear();
            let page = PageEnvelope::from_slice(vec![], query.page, query.size, 0);
            return Ok(page);
        };

        let read_progress = backend.load_series_read_progress_counts(user_id).await?;

        if let Some(read_statuses) = filters.read_statuses.as_ref() {
            series = filter_rows(series, |row| {
                read_statuses.iter().any(|status| {
                    series_matches_read_status(
                        row,
                        read_progress.get(&row.id).copied(),
                        status.as_str(),
                    )
                })
            });
        }

        if let Some(read_statuses_excluded) = filters.read_statuses_excluded.as_ref() {
            series = filter_rows(series, |row| {
                !read_statuses_excluded.iter().any(|status| {
                    series_matches_read_status(
                        row,
                        read_progress.get(&row.id).copied(),
                        status.as_str(),
                    )
                })
            });
        }
    }

    if let Some(complete) = filters.complete {
        let total_book_counts = backend.load_series_total_book_counts().await?;
        series = filter_rows(series, |row| {
            let Some(total_book_count) = total_book_counts.get(&row.id).copied() else {
                return false;
            };
            let total_book_count = total_book_count.max(0) as u64;
            if complete {
                total_book_count == row.books_count
            } else {
                total_book_count != row.books_count
            }
        });
    }

    if let Some(genres) = filters.genres.as_ref() {
        series = filter_rows(series, |row| {
            any_normalized_text_matches(
                row.genres.iter().map(String::as_str),
                genres,
                TextMatchMode::Contains,
            )
        });
    }

    if let Some(genres_excluded) = filters.genres_excluded.as_ref() {
        series = filter_rows(series, |row| {
            !any_normalized_text_matches(
                row.genres.iter().map(String::as_str),
                genres_excluded,
                TextMatchMode::Contains,
            )
        });
    }

    if let Some(genres_null) = filters.genres_null {
        series = filter_rows(series, |row| row.genres.is_empty() == genres_null);
    }

    if let Some(tags) = filters.tags.as_ref() {
        series = filter_rows(series, |row| {
            any_normalized_text_matches(
                row.tags.iter().map(String::as_str),
                tags,
                TextMatchMode::Contains,
            )
        });
    }

    if let Some(tags_excluded) = filters.tags_excluded.as_ref() {
        series = filter_rows(series, |row| {
            !any_normalized_text_matches(
                row.tags.iter().map(String::as_str),
                tags_excluded,
                TextMatchMode::Contains,
            )
        });
    }

    if let Some(tags_null) = filters.tags_null {
        series = filter_rows(series, |row| row.tags.is_empty() == tags_null);
    }

    if let Some(languages) = filters.languages.as_ref() {
        series = filter_rows(series, |row| {
            any_ignore_ascii_case([row.language.as_str()], languages)
        });
    }

    if let Some(languages_excluded) = filters.languages_excluded.as_ref() {
        series = filter_rows(series, |row| {
            !any_ignore_ascii_case([row.language.as_str()], languages_excluded)
        });
    }

    if let Some(publishers) = filters.publishers.as_ref() {
        series = filter_rows(series, |row| {
            any_ignore_ascii_case([row.publisher.as_str()], publishers)
        });
    }

    if let Some(publishers_excluded) = filters.publishers_excluded.as_ref() {
        series = filter_rows(series, |row| {
            !any_ignore_ascii_case([row.publisher.as_str()], publishers_excluded)
        });
    }

    if let Some(age_ratings) = filters.age_ratings.as_ref() {
        series = filter_rows(series, |row| {
            row.age_rating
                .map(|rating| age_ratings.contains(&rating))
                .unwrap_or(false)
        });
    }

    if let Some(age_ratings_or_empty) = filters.age_ratings_or_empty.as_ref() {
        series = filter_rows(series, |row| {
            row.age_rating
                .map(|rating| age_ratings_or_empty.contains(&rating))
                .unwrap_or(true)
        });
    }

    if let Some(age_ratings_excluded) = filters.age_ratings_excluded.as_ref() {
        series = filter_rows(series, |row| {
            row.age_rating
                .map(|rating| !age_ratings_excluded.contains(&rating))
                .unwrap_or(true)
        });
    }

    if let Some(age_ratings_null) = filters.age_ratings_null {
        series = filter_rows(series, |row| row.age_rating.is_none() == age_ratings_null);
    }

    if let Some(age_rating_gt) = filters.age_rating_gt {
        series = filter_rows(series, |row| {
            row.age_rating
                .map(|rating| rating > age_rating_gt)
                .unwrap_or(false)
        });
    }

    if let Some(age_rating_lt) = filters.age_rating_lt {
        series = filter_rows(series, |row| {
            row.age_rating
                .map(|rating| rating < age_rating_lt)
                .unwrap_or(false)
        });
    }

    if let Some(sharing_labels) = filters.sharing_labels.as_ref() {
        series = filter_rows(series, |row| {
            any_ignore_ascii_case(row.labels.iter().map(String::as_str), sharing_labels)
        });
    }

    if !query.sharing_labels_contains_groups.is_empty() {
        for sharing_labels_contains in &query.sharing_labels_contains_groups {
            series = filter_rows(series, |row| {
                any_normalized_text_matches(
                    row.labels.iter().map(String::as_str),
                    sharing_labels_contains,
                    TextMatchMode::Contains,
                )
            });
        }
    } else if let Some(sharing_labels_contains) = filters.sharing_labels_contains.as_ref() {
        series = filter_rows(series, |row| {
            any_normalized_text_matches(
                row.labels.iter().map(String::as_str),
                sharing_labels_contains,
                TextMatchMode::Contains,
            )
        });
    }

    if let Some(sharing_labels_excluded) = filters.sharing_labels_excluded.as_ref() {
        series = filter_rows(series, |row| {
            !any_ignore_ascii_case(
                row.labels.iter().map(String::as_str),
                sharing_labels_excluded,
            )
        });
    }

    if let Some(sharing_labels_null) = filters.sharing_labels_null {
        series = filter_rows(series, |row| row.labels.is_empty() == sharing_labels_null);
    }

    if let Some(authors) = filters.authors.as_ref() {
        series = filter_rows(series, |row| {
            row.books_metadata_authors
                .iter()
                .any(|author| author_matches_filter_value(author, authors))
        });
    }

    if let Some(authors_contains) = filters.authors_contains.as_ref() {
        series = filter_rows(series, |row| {
            row.books_metadata_authors
                .iter()
                .any(|author| author_contains_filter_value(author, authors_contains))
        });
    }

    if let Some(authors_excluded) = filters.authors_excluded.as_ref() {
        series = filter_rows(series, |row| {
            !row.books_metadata_authors
                .iter()
                .any(|author| author_matches_filter_value(author, authors_excluded))
        });
    }

    if let Some(series_statuses) = filters.series_statuses.as_ref() {
        series = filter_rows(series, |row| {
            any_ignore_ascii_case([row.status.as_str()], series_statuses)
        });
    }

    if let Some(series_statuses_excluded) = filters.series_statuses_excluded.as_ref() {
        series = filter_rows(series, |row| {
            !any_ignore_ascii_case([row.status.as_str()], series_statuses_excluded)
        });
    }

    if let Some(release_dates) = filters.release_dates.as_ref() {
        series = filter_rows(series, |row| {
            matches_optional_value(
                row.books_metadata_release_date.as_deref(),
                false,
                |release_date| release_dates.iter().any(|value| value == release_date),
            )
        });
    }

    if let Some(release_dates_excluded) = filters.release_dates_excluded.as_ref() {
        series = filter_rows(series, |row| {
            matches_optional_value(
                row.books_metadata_release_date.as_deref(),
                true,
                |release_date| {
                    !release_dates_excluded
                        .iter()
                        .any(|value| value == release_date)
                },
            )
        });
    }

    if let Some(release_dates_null) = filters.release_dates_null {
        series = filter_rows(series, |row| {
            row.books_metadata_release_date.is_none() == release_dates_null
        });
    }

    if let Some(release_date_gt) = filters.release_date_gt.as_ref() {
        series = filter_rows(series, |row| {
            matches_optional_value(
                row.books_metadata_release_date.as_deref(),
                false,
                |release_date| release_date > release_date_gt.as_str(),
            )
        });
    }

    if let Some(release_date_lt) = filters.release_date_lt.as_ref() {
        series = filter_rows(series, |row| {
            matches_optional_value(
                row.books_metadata_release_date.as_deref(),
                false,
                |release_date| release_date < release_date_lt.as_str(),
            )
        });
    }

    if let Some(release_date_in_last_days) = filters.release_date_in_last_days
        && let Some(cutoff) = backend
            .persisted_utc_date_minus_days(release_date_in_last_days)
            .await?
    {
        series = filter_rows(series, |row| {
            matches_optional_value(
                row.books_metadata_release_date.as_deref(),
                false,
                |release_date| release_date > cutoff.as_str(),
            )
        });
    }

    if let Some(release_date_not_in_last_days) = filters.release_date_not_in_last_days
        && let Some(cutoff) = backend
            .persisted_utc_date_minus_days(release_date_not_in_last_days)
            .await?
    {
        series = filter_rows(series, |row| {
            matches_optional_value(
                row.books_metadata_release_date.as_deref(),
                false,
                |release_date| release_date < cutoff.as_str(),
            )
        });
    }

    if let Some(release_date_begins_with) = filters.release_date_begins_with.as_ref() {
        series = filter_rows(series, |row| {
            matches_optional_value(
                row.books_metadata_release_date.as_deref(),
                false,
                |release_date| {
                    normalized_text_matches(
                        release_date,
                        release_date_begins_with,
                        TextMatchMode::StartsWith,
                    )
                },
            )
        });
    }

    if let Some(release_date_ends_with) = filters.release_date_ends_with.as_ref() {
        series = filter_rows(series, |row| {
            matches_optional_value(
                row.books_metadata_release_date.as_deref(),
                false,
                |release_date| {
                    normalized_text_matches(
                        release_date,
                        release_date_ends_with,
                        TextMatchMode::EndsWith,
                    )
                },
            )
        });
    }

    if let Some(release_date_begins_with_excluded) =
        filters.release_date_begins_with_excluded.as_ref()
    {
        series = filter_rows(series, |row| {
            matches_optional_value(
                row.books_metadata_release_date.as_deref(),
                true,
                |release_date| {
                    !normalized_text_matches(
                        release_date,
                        release_date_begins_with_excluded,
                        TextMatchMode::StartsWith,
                    )
                },
            )
        });
    }

    if let Some(release_date_ends_with_excluded) = filters.release_date_ends_with_excluded.as_ref()
    {
        series = filter_rows(series, |row| {
            matches_optional_value(
                row.books_metadata_release_date.as_deref(),
                true,
                |release_date| {
                    !normalized_text_matches(
                        release_date,
                        release_date_ends_with_excluded,
                        TextMatchMode::EndsWith,
                    )
                },
            )
        });
    }

    if let Some(release_date_contains_excluded) = filters.release_date_contains_excluded.as_ref() {
        series = filter_rows(series, |row| {
            matches_optional_value(
                row.books_metadata_release_date.as_deref(),
                true,
                |release_date| {
                    !normalized_text_matches(
                        release_date,
                        release_date_contains_excluded,
                        TextMatchMode::Contains,
                    )
                },
            )
        });
    }

    if let Some(collection_ids) = filters.collection_ids.as_ref() {
        let memberships = load_collection_memberships(backend).await?;
        series = filter_rows(series, |row| {
            memberships
                .get(&row.id)
                .into_iter()
                .flatten()
                .any(|collection_id| collection_ids.iter().any(|id| id == collection_id))
        });
    }

    let collection_ordering = if query.sort_modes.iter().any(|mode| {
        matches!(
            mode,
            PersistedSeriesSortMode::CollectionNumberAsc
                | PersistedSeriesSortMode::CollectionNumberDesc
        )
    }) {
        if let Some(collection_id) = filters
            .collection_ids
            .as_ref()
            .and_then(|ids| ids.first().map(String::as_str))
            .or_else(|| first_collection_sort_id(query.condition.as_ref()))
        {
            load_collection_ordering(backend, collection_id).await?
        } else {
            HashMap::new()
        }
    } else {
        HashMap::new()
    };

    let read_dates = if query.sort_modes.iter().any(|mode| {
        matches!(
            mode,
            PersistedSeriesSortMode::ReadDateAsc | PersistedSeriesSortMode::ReadDateDesc
        )
    }) {
        if let Some(user_id) = context.user_id.as_deref() {
            backend.load_series_read_dates(user_id).await?
        } else {
            HashMap::new()
        }
    } else {
        HashMap::new()
    };

    let random_keys = if query
        .sort_modes
        .iter()
        .any(|mode| matches!(mode, PersistedSeriesSortMode::Random))
    {
        random_sort_keys(&series)
    } else {
        HashMap::new()
    };

    if !query.sort_modes.is_empty() {
        series.sort_by(|left, right| {
            for sort_mode in &query.sort_modes {
                let ordering = match sort_mode {
                    PersistedSeriesSortMode::TitleAsc => left
                        .title_sort
                        .to_ascii_lowercase()
                        .cmp(&right.title_sort.to_ascii_lowercase()),
                    PersistedSeriesSortMode::TitleDesc => right
                        .title_sort
                        .to_ascii_lowercase()
                        .cmp(&left.title_sort.to_ascii_lowercase()),
                    PersistedSeriesSortMode::NameAsc => left
                        .name
                        .to_ascii_lowercase()
                        .cmp(&right.name.to_ascii_lowercase()),
                    PersistedSeriesSortMode::NameDesc => right
                        .name
                        .to_ascii_lowercase()
                        .cmp(&left.name.to_ascii_lowercase()),
                    PersistedSeriesSortMode::ReadDateAsc => {
                        read_dates.get(&left.id).cmp(&read_dates.get(&right.id))
                    }
                    PersistedSeriesSortMode::ReadDateDesc => {
                        read_dates.get(&right.id).cmp(&read_dates.get(&left.id))
                    }
                    PersistedSeriesSortMode::CollectionNumberAsc => collection_ordering
                        .get(&left.id)
                        .cmp(&collection_ordering.get(&right.id)),
                    PersistedSeriesSortMode::CollectionNumberDesc => collection_ordering
                        .get(&right.id)
                        .cmp(&collection_ordering.get(&left.id)),
                    PersistedSeriesSortMode::Random => {
                        random_keys.get(&left.id).cmp(&random_keys.get(&right.id))
                    }
                    PersistedSeriesSortMode::CreatedAsc => left.created.cmp(&right.created),
                    PersistedSeriesSortMode::CreatedDesc => right.created.cmp(&left.created),
                    PersistedSeriesSortMode::LastModifiedAsc => {
                        left.last_modified.cmp(&right.last_modified)
                    }
                    PersistedSeriesSortMode::LastModifiedDesc => {
                        right.last_modified.cmp(&left.last_modified)
                    }
                    PersistedSeriesSortMode::ReleaseDateAsc => left
                        .books_metadata_release_date
                        .cmp(&right.books_metadata_release_date),
                    PersistedSeriesSortMode::ReleaseDateDesc => right
                        .books_metadata_release_date
                        .cmp(&left.books_metadata_release_date),
                    PersistedSeriesSortMode::BooksCountAsc => {
                        left.books_count.cmp(&right.books_count)
                    }
                    PersistedSeriesSortMode::BooksCountDesc => {
                        right.books_count.cmp(&left.books_count)
                    }
                    PersistedSeriesSortMode::RelevanceAsc => {
                        compare_rank_order(&search_order, &left.id, &right.id, false)
                    }
                    PersistedSeriesSortMode::RelevanceDesc => {
                        compare_rank_order(&search_order, &left.id, &right.id, true)
                    }
                };
                if ordering != std::cmp::Ordering::Equal {
                    return ordering;
                }
            }
            left.id.cmp(&right.id)
        });
    }

    let total_elements = series.len();
    let mut content = if query.unpaged {
        series
    } else {
        let offset = query.page.saturating_mul(query.size);
        if offset >= total_elements {
            vec![]
        } else {
            series.into_iter().skip(offset).take(query.size).collect()
        }
    };
    let page = if query.unpaged { 0 } else { query.page };
    let page_size = if query.unpaged {
        total_elements.max(1)
    } else {
        query.size.max(1)
    };

    if let Some(user_id) = context.user_id.as_deref() {
        let read_progress = backend.load_series_read_progress_counts(user_id).await?;
        for row in &mut content {
            let (read_count, in_progress_count) =
                read_progress.get(&row.id).copied().unwrap_or_default();
            row.books_read_count = read_count.max(0) as u64;
            row.books_in_progress_count = in_progress_count.max(0) as u64;
            row.books_unread_count = row
                .books_count
                .saturating_sub(row.books_read_count + row.books_in_progress_count);
        }
    }

    Ok(PageEnvelope::from_slice(
        content,
        page,
        page_size,
        total_elements,
    ))
}
