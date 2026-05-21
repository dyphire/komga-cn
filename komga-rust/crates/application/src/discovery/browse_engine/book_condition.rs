use komga_domain::common_ids::ReadListId;
use komga_domain::discovery::{
    BookCondition, BookPosterCondition, BookValueCondition, DateCondition, FilterOperator,
    InclusionCondition, NumberCondition, ReadStatusCondition, StringCondition,
};

use super::helpers::{
    author_contains_filter, author_matches_filter, media_profile_for_media_type, poster_matches,
};
use super::models::{BookEvaluationContext, BookRow};
use super::text_matching::{
    any_ignore_ascii_case, any_normalized_text_matches, matches_optional_value,
    normalized_text_matches, TextMatchMode,
};

pub fn evaluate(row: &BookRow, condition: &BookCondition, ctx: &BookEvaluationContext) -> bool {
    match condition {
        BookCondition::Value(value) => evaluate_value(row, value, ctx),
        BookCondition::Composite(composite) => match composite.operator {
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

fn evaluate_value(row: &BookRow, condition: &BookValueCondition, ctx: &BookEvaluationContext) -> bool {
    match condition {
        BookValueCondition::LibraryId(inc) => {
            matches_string_inclusion(row.library_id.as_str(), inc, |id| id.as_str())
        }
        BookValueCondition::SeriesId(inc) => {
            matches_string_inclusion(row.series_id.as_str(), inc, |id| id.as_str())
        }
        BookValueCondition::ReadListId(inc) => matches_readlist_condition(row, inc, ctx),
        BookValueCondition::Title(condition) => matches_string_condition(&row.title, condition),
        BookValueCondition::Deleted(value) => row.deleted == *value,
        BookValueCondition::OneShot(value) => row.oneshot == *value,
        BookValueCondition::Tag(condition) => {
            matches_string_list_condition(&row.metadata_tags, condition)
        }
        BookValueCondition::Genre(condition) => {
            matches_string_list_condition(&row.genres, condition)
        }
        BookValueCondition::Language(inc) => {
            matches_optional_string_inclusion(row.language.as_deref(), inc)
        }
        BookValueCondition::Publisher(inc) => {
            matches_optional_string_inclusion(row.publisher.as_deref(), inc)
        }
        BookValueCondition::AgeRating(inc) => matches_optional_copy_inclusion(row.age_rating, inc),
        BookValueCondition::ReadStatus(ReadStatusCondition::Include(values)) => {
            ctx.user_id_present && any_ignore_ascii_case([row.read_status.as_str()], values)
        }
        BookValueCondition::ReadStatus(ReadStatusCondition::Exclude(values)) => {
            ctx.user_id_present && !any_ignore_ascii_case([row.read_status.as_str()], values)
        }
        BookValueCondition::MediaProfile(inc) => {
            let profile = media_profile_for_media_type(&row.media_type);
            matches_string_inclusion(profile, inc, String::as_str)
        }
        BookValueCondition::MediaStatus(inc) => {
            matches_string_inclusion(row.media_status.as_str(), inc, String::as_str)
        }
        BookValueCondition::Author(condition) => matches_author_condition(row, condition),
        BookValueCondition::Poster(inc) => matches_poster_condition(row, inc, ctx),
        BookValueCondition::NumberSort(condition) => {
            matches_number_condition(row.metadata_number_sort, condition)
        }
        BookValueCondition::ReleaseDate(condition) => {
            matches_date_condition(row.metadata_release_date.as_deref(), condition, &ctx.release_date_cutoffs)
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

fn matches_optional_string_inclusion(
    actual: Option<&str>,
    condition: &InclusionCondition<String>,
) -> bool {
    match condition {
        InclusionCondition::Include(values) => {
            actual.is_some_and(|actual| any_ignore_ascii_case([actual], values))
        }
        InclusionCondition::Exclude(values) => actual
            .map(|actual| !any_ignore_ascii_case([actual], values))
            .unwrap_or(true),
    }
}

fn matches_optional_copy_inclusion<T: Copy + PartialEq>(
    actual: Option<T>,
    condition: &InclusionCondition<T>,
) -> bool {
    match condition {
        InclusionCondition::Include(values) => {
            actual.is_some_and(|actual| values.contains(&actual))
        }
        InclusionCondition::Exclude(values) => actual
            .map(|actual| !values.contains(&actual))
            .unwrap_or(true),
    }
}

fn matches_readlist_condition(
    row: &BookRow,
    condition: &InclusionCondition<ReadListId>,
    ctx: &BookEvaluationContext,
) -> bool {
    let memberships = match ctx.readlist_memberships.as_ref() {
        Some(m) => m,
        None => return false,
    };
    let book_readlists = memberships.get(&row.id);
    match condition {
        InclusionCondition::Include(ids) => book_readlists
            .map(|rl| ids.iter().any(|id| rl.contains(id.as_str())))
            .unwrap_or(false),
        InclusionCondition::Exclude(ids) => book_readlists
            .map(|rl| !ids.iter().any(|id| rl.contains(id.as_str())))
            .unwrap_or(true),
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

fn matches_string_list_condition(values: &[String], condition: &StringCondition) -> bool {
    match condition {
        StringCondition::Exact(InclusionCondition::Include(expected)) => {
            any_normalized_text_matches(values.iter().map(String::as_str), expected, TextMatchMode::Exact)
        }
        StringCondition::Exact(InclusionCondition::Exclude(expected)) => {
            !any_normalized_text_matches(values.iter().map(String::as_str), expected, TextMatchMode::Exact)
        }
        StringCondition::Contains(InclusionCondition::Include(expected)) => {
            any_normalized_text_matches(values.iter().map(String::as_str), expected, TextMatchMode::Contains)
        }
        StringCondition::Contains(InclusionCondition::Exclude(expected)) => {
            !any_normalized_text_matches(values.iter().map(String::as_str), expected, TextMatchMode::Contains)
        }
        StringCondition::StartsWith(InclusionCondition::Include(expected)) => {
            any_normalized_text_matches(values.iter().map(String::as_str), expected, TextMatchMode::StartsWith)
        }
        StringCondition::StartsWith(InclusionCondition::Exclude(expected)) => {
            !any_normalized_text_matches(values.iter().map(String::as_str), expected, TextMatchMode::StartsWith)
        }
        StringCondition::EndsWith(InclusionCondition::Include(expected)) => {
            any_normalized_text_matches(values.iter().map(String::as_str), expected, TextMatchMode::EndsWith)
        }
        StringCondition::EndsWith(InclusionCondition::Exclude(expected)) => {
            !any_normalized_text_matches(values.iter().map(String::as_str), expected, TextMatchMode::EndsWith)
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

fn matches_author_condition(row: &BookRow, condition: &StringCondition) -> bool {
    match condition {
        StringCondition::Contains(InclusionCondition::Include(values)) => row
            .metadata_authors
            .iter()
            .any(|author| author_contains_filter(&author.name, &author.role, values)),
        StringCondition::Contains(InclusionCondition::Exclude(values)) => !row
            .metadata_authors
            .iter()
            .any(|author| author_contains_filter(&author.name, &author.role, values)),
        StringCondition::Exact(InclusionCondition::Include(values)) => row
            .metadata_authors
            .iter()
            .any(|author| author_matches_filter(&author.name, &author.role, values)),
        StringCondition::Exact(InclusionCondition::Exclude(values)) => !row
            .metadata_authors
            .iter()
            .any(|author| author_matches_filter(&author.name, &author.role, values)),
        StringCondition::IsEmpty => row.metadata_authors.is_empty(),
        StringCondition::IsNotEmpty => !row.metadata_authors.is_empty(),
        StringCondition::StartsWith(_) | StringCondition::EndsWith(_) | StringCondition::Regex(_) => false,
    }
}

fn matches_poster_condition(
    row: &BookRow,
    condition: &InclusionCondition<BookPosterCondition>,
    ctx: &BookEvaluationContext,
) -> bool {
    let posters = ctx
        .posters
        .as_ref()
        .and_then(|posters| posters.get(&row.id));
    match condition {
        InclusionCondition::Include(conditions) => posters.is_some_and(|posters| {
            posters.iter().any(|poster| {
                conditions.iter().any(|condition| {
                    poster_matches(
                        poster,
                        condition.thumbnail_type.as_ref().map(|v| vec![v.clone()]).as_ref(),
                        condition.selected,
                    )
                })
            })
        }),
        InclusionCondition::Exclude(conditions) => posters
            .map(|posters| {
                !posters.iter().any(|poster| {
                    conditions.iter().any(|condition| {
                        poster_matches(
                            poster,
                            condition.thumbnail_type.as_ref().map(|v| vec![v.clone()]).as_ref(),
                            condition.selected,
                        )
                    })
                })
            })
            .unwrap_or(true),
    }
}

fn matches_number_condition(actual: f64, condition: &NumberCondition) -> bool {
    match condition {
        NumberCondition::Exact(InclusionCondition::Include(values)) => values.iter().any(|value| {
            value
                .parse::<f64>()
                .map(|expected| (actual - expected).abs() <= f64::EPSILON)
                .unwrap_or(false)
        }),
        NumberCondition::Exact(InclusionCondition::Exclude(values)) => {
            !values.iter().any(|value| {
                value
                    .parse::<f64>()
                    .map(|expected| (actual - expected).abs() <= f64::EPSILON)
                    .unwrap_or(false)
            })
        }
        NumberCondition::GreaterThan(value) => value
            .parse::<f64>()
            .map(|threshold| actual > threshold)
            .unwrap_or(false),
        NumberCondition::LessThan(value) => value
            .parse::<f64>()
            .map(|threshold| actual < threshold)
            .unwrap_or(false),
    }
}

fn matches_date_condition(
    actual: Option<&str>,
    condition: &DateCondition,
    cutoffs: &std::collections::HashMap<i64, Option<String>>,
) -> bool {
    match condition {
        DateCondition::Exact(InclusionCondition::Include(values)) => {
            matches_optional_value(actual, false, |date| values.iter().any(|v| v == date))
        }
        DateCondition::Exact(InclusionCondition::Exclude(values)) => {
            matches_optional_value(actual, true, |date| !values.iter().any(|v| v == date))
        }
        DateCondition::Before(value) => {
            matches_optional_value(actual, false, |date| date < value.as_str())
        }
        DateCondition::After(value) => {
            matches_optional_value(actual, false, |date| date > value.as_str())
        }
        DateCondition::Contains(InclusionCondition::Include(values)) => {
            matches_optional_value(actual, false, |date| {
                normalized_text_matches(date, values, TextMatchMode::Contains)
            })
        }
        DateCondition::Contains(InclusionCondition::Exclude(values)) => {
            matches_optional_value(actual, true, |date| {
                !normalized_text_matches(date, values, TextMatchMode::Contains)
            })
        }
        DateCondition::StartsWith(InclusionCondition::Include(values)) => {
            matches_optional_value(actual, false, |date| {
                normalized_text_matches(date, values, TextMatchMode::StartsWith)
            })
        }
        DateCondition::StartsWith(InclusionCondition::Exclude(values)) => {
            matches_optional_value(actual, true, |date| {
                !normalized_text_matches(date, values, TextMatchMode::StartsWith)
            })
        }
        DateCondition::EndsWith(InclusionCondition::Include(values)) => {
            matches_optional_value(actual, false, |date| {
                normalized_text_matches(date, values, TextMatchMode::EndsWith)
            })
        }
        DateCondition::EndsWith(InclusionCondition::Exclude(values)) => {
            matches_optional_value(actual, true, |date| {
                !normalized_text_matches(date, values, TextMatchMode::EndsWith)
            })
        }
        DateCondition::WithinLastDays(days) => {
            let cutoff = cutoffs.get(days).and_then(|c| c.as_deref());
            match cutoff {
                Some(cutoff) => matches_optional_value(actual, false, |date| date > cutoff),
                None => false,
            }
        }
        DateCondition::OutsideLastDays(days) => {
            let cutoff = cutoffs.get(days).and_then(|c| c.as_deref());
            match cutoff {
                Some(cutoff) => matches_optional_value(actual, false, |date| date < cutoff),
                None => false,
            }
        }
        DateCondition::IsEmpty => actual.is_none(),
        DateCondition::IsNotEmpty => actual.is_some(),
    }
}