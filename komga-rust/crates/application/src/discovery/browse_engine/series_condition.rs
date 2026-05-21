use komga_domain::discovery::{
    AgeRatingCondition, DateCondition, FilterOperator, InclusionCondition, ReadStatusCondition,
    SeriesCondition, SeriesStatusCondition, SeriesValueCondition, StringCondition,
};

use super::helpers::{author_contains_filter_value, author_matches_filter_value, series_matches_read_status};
use super::models::{SeriesEvaluationContext, SeriesRow};
use super::text_matching::{
    any_ignore_ascii_case, any_normalized_text_matches, normalized_text_matches, TextMatchMode,
};

pub fn evaluate(row: &SeriesRow, condition: &SeriesCondition, ctx: &SeriesEvaluationContext) -> bool {
    match condition {
        SeriesCondition::Value(value) => evaluate_value(row, value, ctx),
        SeriesCondition::Composite(composite) => match composite.operator {
            FilterOperator::All => composite
                .conditions
                .iter()
                .all(|c| evaluate(row, c, ctx)),
            FilterOperator::Any => {
                composite.conditions.is_empty()
                    || composite.conditions.iter().any(|c| evaluate(row, c, ctx))
            }
        },
    }
}

fn evaluate_value(
    row: &SeriesRow,
    condition: &SeriesValueCondition,
    ctx: &SeriesEvaluationContext,
) -> bool {
    match condition {
        SeriesValueCondition::LibraryId(inc) => {
            matches_string_inclusion(row.library_id.as_str(), inc, |id| id.as_str())
        }
        SeriesValueCondition::CollectionId(inc) => matches_collection_condition(row, inc, ctx),
        SeriesValueCondition::Title(condition) => matches_string_condition(&row.title, condition),
        SeriesValueCondition::TitleSort(condition) => {
            matches_string_condition(&row.title_sort, condition)
        }
        SeriesValueCondition::Deleted(value) => row.deleted == *value,
        SeriesValueCondition::OneShot(value) => row.oneshot == *value,
        SeriesValueCondition::ReadStatus(condition) => {
            matches_series_read_status_condition(row, condition, ctx)
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
            matches_date_condition(row.books_metadata_release_date.as_deref(), condition, ctx)
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
        SeriesValueCondition::Complete(value) => ctx
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
    row: &SeriesRow,
    condition: &InclusionCondition<komga_domain::common_ids::CollectionId>,
    ctx: &SeriesEvaluationContext,
) -> bool {
    let memberships = ctx
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
    row: &SeriesRow,
    condition: &ReadStatusCondition,
    ctx: &SeriesEvaluationContext,
) -> bool {
    if !ctx.user_id_present {
        return false;
    }
    let read_progress = ctx
        .read_progress
        .as_ref()
        .and_then(|progress| progress.get(&row.id).copied());
    match condition {
        ReadStatusCondition::Include(values) => values
            .iter()
            .any(|status| series_matches_read_status(row.books_count, read_progress, status)),
        ReadStatusCondition::Exclude(values) => !values
            .iter()
            .any(|status| series_matches_read_status(row.books_count, read_progress, status)),
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
        StringCondition::Regex(patterns) => {
            let normalized = actual.to_ascii_lowercase();
            patterns.iter().any(|pattern| {
                regex::RegexBuilder::new(pattern)
                    .case_insensitive(true)
                    .build()
                    .map(|re| re.is_match(&normalized))
                    .unwrap_or(false)
            })
        }
        StringCondition::IsEmpty => actual.is_empty(),
        StringCondition::IsNotEmpty => !actual.is_empty(),
    }
}

fn matches_string_values_condition<'a>(
    values: impl IntoIterator<Item = &'a str>,
    condition: &StringCondition,
) -> bool {
    let values: Vec<&str> = values.into_iter().collect();
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
        StringCondition::Regex(patterns) => values.iter().any(|value| {
            let normalized = value.to_ascii_lowercase();
            patterns.iter().any(|pattern| {
                regex::RegexBuilder::new(pattern)
                    .case_insensitive(true)
                    .build()
                    .map(|re| re.is_match(&normalized))
                    .unwrap_or(false)
            })
        }),
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

fn matches_author_condition(row: &SeriesRow, condition: &StringCondition) -> bool {
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
        StringCondition::StartsWith(_) | StringCondition::EndsWith(_) | StringCondition::Regex(_) => false,
    }
}

fn matches_date_condition(
    actual: Option<&str>,
    condition: &DateCondition,
    ctx: &SeriesEvaluationContext,
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
        DateCondition::WithinLastDays(days) => ctx
            .release_date_cutoffs
            .get(days)
            .and_then(Option::as_deref)
            .map(|cutoff| actual.is_some_and(|actual| actual > cutoff))
            .unwrap_or(true),
        DateCondition::OutsideLastDays(days) => ctx
            .release_date_cutoffs
            .get(days)
            .and_then(Option::as_deref)
            .map(|cutoff| actual.is_some_and(|actual| actual < cutoff))
            .unwrap_or(true),
        DateCondition::IsEmpty => actual.is_none(),
        DateCondition::IsNotEmpty => actual.is_some(),
    }
}
