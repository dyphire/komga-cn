#![allow(clippy::too_many_arguments)]

use crate::state::PersistedDiscoveryListDataSource;

use super::common_helpers::{
    TextMatchMode, any_ignore_ascii_case, any_normalized_text_matches, matches_optional_value,
    normalized_text_matches,
};
use super::models::PersistedBookSummary;
use super::*;
use komga_application::discovery::{
    BookMetadataAuthorReadModel, BookMetadataLinkReadModel, BookReadProgressReadModel,
};
use komga_domain::discovery::{
    BookCondition, BookPosterCondition, BookValueCondition, DateCondition, FilterOperator,
    InclusionCondition, NumberCondition, ReadStatusCondition, StringCondition,
};

pub async fn load_book_poster_summaries(
    backend: &dyn PersistedDiscoveryListDataSource,
) -> Result<HashMap<String, Vec<PersistedBookPosterSummary>>, String> {
    backend.load_book_poster_summaries().await
}

struct BookConditionEvaluationData {
    user_id_present: bool,
    readlist_memberships: Option<BTreeMap<String, BTreeSet<String>>>,
    posters: Option<HashMap<String, Vec<PersistedBookPosterSummary>>>,
    release_date_cutoffs: HashMap<i64, Option<String>>,
}

impl BookConditionEvaluationData {
    async fn load(
        backend: &dyn PersistedDiscoveryListDataSource,
        context: &DiscoveryQueryContext,
        condition: &BookCondition,
    ) -> Result<Self, String> {
        let readlist_memberships = if condition_needs_readlist_memberships(condition) {
            Some(backend.load_readlist_memberships().await?)
        } else {
            None
        };
        let posters = if condition_needs_posters(condition) {
            Some(load_book_poster_summaries(backend).await?)
        } else {
            None
        };
        let mut release_date_offsets = BTreeSet::new();
        collect_release_date_offsets(condition, &mut release_date_offsets);
        let mut release_date_cutoffs = HashMap::new();
        for days in release_date_offsets {
            release_date_cutoffs.insert(days, backend.persisted_utc_date_minus_days(days).await?);
        }

        Ok(Self {
            user_id_present: context.user_id.is_some(),
            readlist_memberships,
            posters,
            release_date_cutoffs,
        })
    }
}

fn condition_needs_readlist_memberships(condition: &BookCondition) -> bool {
    match condition {
        BookCondition::Value(BookValueCondition::ReadListId(_)) => true,
        BookCondition::Composite(composite) => composite
            .conditions
            .iter()
            .any(condition_needs_readlist_memberships),
        _ => false,
    }
}

fn condition_needs_posters(condition: &BookCondition) -> bool {
    match condition {
        BookCondition::Value(BookValueCondition::Poster(_)) => true,
        BookCondition::Composite(composite) => {
            composite.conditions.iter().any(condition_needs_posters)
        }
        _ => false,
    }
}

fn collect_release_date_offsets(condition: &BookCondition, offsets: &mut BTreeSet<i64>) {
    match condition {
        BookCondition::Value(BookValueCondition::ReleaseDate(
            DateCondition::WithinLastDays(days) | DateCondition::OutsideLastDays(days),
        )) => {
            offsets.insert(*days);
        }
        BookCondition::Composite(composite) => {
            for child in &composite.conditions {
                collect_release_date_offsets(child, offsets);
            }
        }
        _ => {}
    }
}

fn first_readlist_sort_id(condition: Option<&BookCondition>) -> Option<&str> {
    fn visit(condition: &BookCondition) -> Option<&str> {
        match condition {
            BookCondition::Value(BookValueCondition::ReadListId(InclusionCondition::Include(
                values,
            ))) => values.first().map(|value| value.as_str()),
            BookCondition::Composite(composite) => composite.conditions.iter().find_map(visit),
            _ => None,
        }
    }

    condition.and_then(visit)
}

fn row_matches_book_condition(
    row: &PersistedBookSummary,
    condition: &BookCondition,
    data: &BookConditionEvaluationData,
) -> bool {
    match condition {
        BookCondition::Value(value) => row_matches_book_value_condition(row, value, data),
        BookCondition::Composite(composite) => match composite.operator {
            FilterOperator::All => composite
                .conditions
                .iter()
                .all(|condition| row_matches_book_condition(row, condition, data)),
            FilterOperator::Any => {
                composite.conditions.is_empty()
                    || composite
                        .conditions
                        .iter()
                        .any(|condition| row_matches_book_condition(row, condition, data))
            }
        },
    }
}

fn row_matches_book_value_condition(
    row: &PersistedBookSummary,
    condition: &BookValueCondition,
    data: &BookConditionEvaluationData,
) -> bool {
    match condition {
        BookValueCondition::LibraryId(inc) => {
            matches_string_inclusion(row.library_id.as_str(), inc, |id| id.as_str())
        }
        BookValueCondition::SeriesId(inc) => {
            matches_string_inclusion(row.series_id.as_str(), inc, |id| id.as_str())
        }
        BookValueCondition::ReadListId(inc) => matches_readlist_condition(row, inc, data),
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
            data.user_id_present && any_ignore_ascii_case([row.read_status.as_str()], values)
        }
        BookValueCondition::ReadStatus(ReadStatusCondition::Exclude(values)) => {
            data.user_id_present && !any_ignore_ascii_case([row.read_status.as_str()], values)
        }
        BookValueCondition::MediaProfile(inc) => {
            let profile = media_profile_for_media_type(&row.media_type);
            matches_string_inclusion(profile, inc, String::as_str)
        }
        BookValueCondition::MediaStatus(inc) => {
            matches_string_inclusion(row.media_status.as_str(), inc, String::as_str)
        }
        BookValueCondition::Author(condition) => matches_author_condition(row, condition),
        BookValueCondition::Poster(inc) => matches_poster_condition(row, inc, data),
        BookValueCondition::NumberSort(condition) => {
            matches_number_condition(row.metadata_number_sort, condition)
        }
        BookValueCondition::ReleaseDate(condition) => {
            matches_date_condition(row.metadata_release_date.as_deref(), condition, data)
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
    row: &PersistedBookSummary,
    condition: &InclusionCondition<komga_domain::common_ids::ReadListId>,
    data: &BookConditionEvaluationData,
) -> bool {
    let memberships = data
        .readlist_memberships
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

fn matches_string_list_condition(values: &[String], condition: &StringCondition) -> bool {
    match condition {
        StringCondition::Exact(InclusionCondition::Include(expected)) => {
            any_normalized_text_matches(
                values.iter().map(String::as_str),
                expected,
                TextMatchMode::Exact,
            )
        }
        StringCondition::Exact(InclusionCondition::Exclude(expected)) => {
            !any_normalized_text_matches(
                values.iter().map(String::as_str),
                expected,
                TextMatchMode::Exact,
            )
        }
        StringCondition::Contains(InclusionCondition::Include(expected)) => {
            any_normalized_text_matches(
                values.iter().map(String::as_str),
                expected,
                TextMatchMode::Contains,
            )
        }
        StringCondition::Contains(InclusionCondition::Exclude(expected)) => {
            !any_normalized_text_matches(
                values.iter().map(String::as_str),
                expected,
                TextMatchMode::Contains,
            )
        }
        StringCondition::StartsWith(InclusionCondition::Include(expected)) => {
            any_normalized_text_matches(
                values.iter().map(String::as_str),
                expected,
                TextMatchMode::StartsWith,
            )
        }
        StringCondition::StartsWith(InclusionCondition::Exclude(expected)) => {
            !any_normalized_text_matches(
                values.iter().map(String::as_str),
                expected,
                TextMatchMode::StartsWith,
            )
        }
        StringCondition::EndsWith(InclusionCondition::Include(expected)) => {
            any_normalized_text_matches(
                values.iter().map(String::as_str),
                expected,
                TextMatchMode::EndsWith,
            )
        }
        StringCondition::EndsWith(InclusionCondition::Exclude(expected)) => {
            !any_normalized_text_matches(
                values.iter().map(String::as_str),
                expected,
                TextMatchMode::EndsWith,
            )
        }
        StringCondition::IsEmpty => values.is_empty(),
        StringCondition::IsNotEmpty => !values.is_empty(),
    }
}

fn matches_author_condition(row: &PersistedBookSummary, condition: &StringCondition) -> bool {
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
        StringCondition::StartsWith(_) | StringCondition::EndsWith(_) => false,
    }
}

fn matches_poster_condition(
    row: &PersistedBookSummary,
    condition: &InclusionCondition<BookPosterCondition>,
    data: &BookConditionEvaluationData,
) -> bool {
    let posters = data
        .posters
        .as_ref()
        .and_then(|posters| posters.get(&row.id));
    match condition {
        InclusionCondition::Include(conditions) => posters.is_some_and(|posters| {
            posters.iter().any(|poster| {
                conditions.iter().any(|condition| {
                    poster_matches(
                        poster,
                        condition
                            .thumbnail_type
                            .as_ref()
                            .map(|value| vec![value.clone()])
                            .as_ref(),
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
                            condition
                                .thumbnail_type
                                .as_ref()
                                .map(|value| vec![value.clone()])
                                .as_ref(),
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
            .map(|expected| actual > expected)
            .unwrap_or(false),
        NumberCondition::LessThan(value) => value
            .parse::<f64>()
            .map(|expected| actual < expected)
            .unwrap_or(false),
    }
}

fn matches_date_condition(
    actual: Option<&str>,
    condition: &DateCondition,
    data: &BookConditionEvaluationData,
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

pub(crate) async fn load_persisted_books_page(
    backend: &dyn PersistedDiscoveryListDataSource,
    context: &DiscoveryQueryContext,
    query: PersistedBooksBrowseQuery,
) -> Result<PageEnvelope<BookReadModel>, String> {
    let mut books = Vec::new();
    let filters = &query.filters;
    let mut relevance_ranks: HashMap<String, usize> = HashMap::new();
    if let Some(search) = query.search.as_ref().map(|value| value.trim())
        && !search.is_empty()
    {
        let total_count = backend.load_persisted_book_count().await?;
        let candidate_ids = backend.search_book_ids(search, total_count.max(1)).await?;
        if candidate_ids.is_empty() {
            books.clear();
        } else {
            relevance_ranks = candidate_ids
                .iter()
                .enumerate()
                .map(|(index, id)| (id.clone(), index))
                .collect();
            books = backend
                .load_persisted_book_summaries_by_ids(context.user_id.as_deref(), &candidate_ids)
                .await?;
        }
    } else {
        books = backend
            .load_persisted_book_summaries(context.user_id.as_deref())
            .await?;
    }

    if let Some(allowed_ids) = context.authorized_library_ids.as_ref() {
        books = filter_rows(books, |row| {
            allowed_ids.iter().any(|id| id == row.library_id.as_str())
        });
    }
    if let Some(library_ids) = filters.library_ids.as_ref() {
        books = filter_rows(books, |row| {
            library_ids.iter().any(|id| id == row.library_id.as_str())
        });
    }

    if let Some(restrictions) = context.restrictions.as_ref()
        && let (Some(age), Some(crate::discovery_auth::principal::AgeRestrictionKind::Exclude)) =
            (restrictions.age, restrictions.age_restriction)
    {
        books = filter_rows(books, |row| {
            row.age_rating
                .map(|age_rating| age_rating < age)
                .unwrap_or(true)
        });
    }

    if let Some(condition) = query.condition.as_ref() {
        let evaluation_data =
            BookConditionEvaluationData::load(backend, context, condition).await?;
        books = filter_rows(books, |row| {
            row_matches_book_condition(row, condition, &evaluation_data)
        });
    }

    if let Some(series_ids) = filters.series_ids.as_ref() {
        books = filter_rows(books, |row| {
            series_ids.iter().any(|id| id == row.series_id.as_str())
        });
    }

    if let Some(series_ids_excluded) = filters.series_ids_excluded.as_ref() {
        books = filter_rows(books, |row| {
            !series_ids_excluded
                .iter()
                .any(|id| id == row.series_id.as_str())
        });
    }

    let readlist_memberships =
        if filters.read_list_ids.is_some() || filters.read_list_ids_excluded.is_some() {
            Some(load_readlist_memberships(backend).await?)
        } else {
            None
        };

    if let Some(read_list_ids) = filters.read_list_ids.as_ref() {
        let memberships = readlist_memberships
            .as_ref()
            .expect("readlist memberships should load when include filter is present");
        books = filter_rows(books, |row| {
            memberships
                .get(&row.id)
                .into_iter()
                .flatten()
                .any(|read_list_id| read_list_ids.iter().any(|id| id == read_list_id))
        });
    }

    if let Some(read_list_ids_excluded) = filters.read_list_ids_excluded.as_ref() {
        let memberships = readlist_memberships
            .as_ref()
            .expect("readlist memberships should load when exclude filter is present");
        books = filter_rows(books, |row| {
            !memberships
                .get(&row.id)
                .into_iter()
                .flatten()
                .any(|read_list_id| read_list_ids_excluded.iter().any(|id| id == read_list_id))
        });
    }

    if let Some(titles) = filters.titles.as_ref() {
        books = filter_rows(books, |row| {
            normalized_text_matches(&row.title, titles, TextMatchMode::Exact)
        });
    }

    if let Some(titles_excluded) = filters.titles_excluded.as_ref() {
        books = filter_rows(books, |row| {
            !normalized_text_matches(&row.title, titles_excluded, TextMatchMode::Exact)
        });
    }

    if let Some(titles_contains) = filters.titles_contains.as_ref() {
        books = filter_rows(books, |row| {
            normalized_text_matches(&row.title, titles_contains, TextMatchMode::Contains)
        });
    }

    if let Some(titles_contains_excluded) = filters.titles_contains_excluded.as_ref() {
        books = filter_rows(books, |row| {
            !normalized_text_matches(
                &row.title,
                titles_contains_excluded,
                TextMatchMode::Contains,
            )
        });
    }

    if let Some(titles_begins_with) = filters.titles_begins_with.as_ref() {
        books = filter_rows(books, |row| {
            normalized_text_matches(&row.title, titles_begins_with, TextMatchMode::StartsWith)
        });
    }

    if let Some(titles_begins_with_excluded) = filters.titles_begins_with_excluded.as_ref() {
        books = filter_rows(books, |row| {
            !normalized_text_matches(
                &row.title,
                titles_begins_with_excluded,
                TextMatchMode::StartsWith,
            )
        });
    }

    if let Some(titles_ends_with) = filters.titles_ends_with.as_ref() {
        books = filter_rows(books, |row| {
            normalized_text_matches(&row.title, titles_ends_with, TextMatchMode::EndsWith)
        });
    }

    if let Some(titles_ends_with_excluded) = filters.titles_ends_with_excluded.as_ref() {
        books = filter_rows(books, |row| {
            !normalized_text_matches(
                &row.title,
                titles_ends_with_excluded,
                TextMatchMode::EndsWith,
            )
        });
    }

    if let Some(tags) = filters.tags.as_ref() {
        books = filter_rows(books, |row| {
            any_normalized_text_matches(
                row.metadata_tags.iter().map(String::as_str),
                tags,
                TextMatchMode::Exact,
            )
        });
    }

    if let Some(tags_excluded) = filters.tags_excluded.as_ref() {
        books = filter_rows(books, |row| {
            !any_normalized_text_matches(
                row.metadata_tags.iter().map(String::as_str),
                tags_excluded,
                TextMatchMode::Exact,
            )
        });
    }

    if let Some(tags_null) = filters.tags_null {
        books = filter_rows(books, |row| row.metadata_tags.is_empty() == tags_null);
    }

    if let Some(genres) = filters.genres.as_ref() {
        books = filter_rows(books, |row| {
            any_normalized_text_matches(
                row.genres.iter().map(String::as_str),
                genres,
                TextMatchMode::Exact,
            )
        });
    }

    if let Some(genres_excluded) = filters.genres_excluded.as_ref() {
        books = filter_rows(books, |row| {
            !any_normalized_text_matches(
                row.genres.iter().map(String::as_str),
                genres_excluded,
                TextMatchMode::Exact,
            )
        });
    }

    if let Some(genres_null) = filters.genres_null {
        books = filter_rows(books, |row| row.genres.is_empty() == genres_null);
    }

    if let Some(languages) = filters.languages.as_ref() {
        books = filter_rows(books, |row| {
            row.language
                .as_deref()
                .is_some_and(|language| any_ignore_ascii_case([language], languages))
        });
    }

    if let Some(languages_excluded) = filters.languages_excluded.as_ref() {
        books = filter_rows(books, |row| {
            !row.language
                .as_deref()
                .is_some_and(|language| any_ignore_ascii_case([language], languages_excluded))
        });
    }

    if let Some(publishers) = filters.publishers.as_ref() {
        books = filter_rows(books, |row| {
            row.publisher
                .as_deref()
                .is_some_and(|publisher| any_ignore_ascii_case([publisher], publishers))
        });
    }

    if let Some(publishers_excluded) = filters.publishers_excluded.as_ref() {
        books = filter_rows(books, |row| {
            !row.publisher
                .as_deref()
                .is_some_and(|publisher| any_ignore_ascii_case([publisher], publishers_excluded))
        });
    }

    if let Some(age_ratings) = filters.age_ratings.as_ref() {
        books = filter_rows(books, |row| {
            row.age_rating
                .is_some_and(|age_rating| age_ratings.contains(&age_rating))
        });
    }

    if let Some(age_ratings_excluded) = filters.age_ratings_excluded.as_ref() {
        books = filter_rows(books, |row| {
            !row.age_rating
                .is_some_and(|age_rating| age_ratings_excluded.contains(&age_rating))
        });
    }

    if let Some(age_ratings_null) = filters.age_ratings_null {
        books = filter_rows(books, |row| row.age_rating.is_none() == age_ratings_null);
    }

    if let Some(age_rating_gt) = filters.age_rating_gt {
        books = filter_rows(books, |row| {
            row.age_rating
                .is_some_and(|age_rating| age_rating > age_rating_gt)
        });
    }

    if let Some(age_rating_lt) = filters.age_rating_lt {
        books = filter_rows(books, |row| {
            row.age_rating
                .is_some_and(|age_rating| age_rating < age_rating_lt)
        });
    }

    if let Some(authors) = filters.authors.as_ref() {
        books = filter_rows(books, |row| {
            row.metadata_authors
                .iter()
                .any(|author| author_matches_filter(&author.name, &author.role, authors))
        });
    }

    if let Some(authors_contains) = filters.authors_contains.as_ref() {
        books = filter_rows(books, |row| {
            row.metadata_authors
                .iter()
                .any(|author| author_contains_filter(&author.name, &author.role, authors_contains))
        });
    }

    if let Some(authors_excluded) = filters.authors_excluded.as_ref() {
        books = filter_rows(books, |row| {
            !row.metadata_authors
                .iter()
                .any(|author| author_matches_filter(&author.name, &author.role, authors_excluded))
        });
    }

    if filters.poster_types.is_some()
        || filters.poster_types_excluded.is_some()
        || filters.poster_selected.is_some()
        || filters.poster_selected_excluded.is_some()
    {
        let posters = load_book_poster_summaries(backend).await?;

        if filters.poster_types.is_some() || filters.poster_selected.is_some() {
            books = filter_rows(books, |row| {
                posters.get(&row.id).into_iter().flatten().any(|poster| {
                    poster_matches(
                        poster,
                        filters.poster_types.as_ref(),
                        filters.poster_selected,
                    )
                })
            });
        }

        if filters.poster_types_excluded.is_some() || filters.poster_selected_excluded.is_some() {
            books = filter_rows(books, |row| {
                !posters.get(&row.id).into_iter().flatten().any(|poster| {
                    poster_matches(
                        poster,
                        filters.poster_types_excluded.as_ref(),
                        filters.poster_selected_excluded,
                    )
                })
            });
        }
    }

    if let Some(media_profiles) = filters.media_profiles.as_ref() {
        books = filter_rows(books, |row| {
            let profile = media_profile_for_media_type(&row.media_type);
            any_ignore_ascii_case([profile], media_profiles)
        });
    }

    if let Some(media_profiles_excluded) = filters.media_profiles_excluded.as_ref() {
        books = filter_rows(books, |row| {
            let profile = media_profile_for_media_type(&row.media_type);
            !any_ignore_ascii_case([profile], media_profiles_excluded)
        });
    }

    if let Some(deleted) = filters.deleted {
        books = filter_rows(books, |row| row.deleted == deleted);
    }

    if let Some(oneshot) = filters.oneshot {
        books = filter_rows(books, |row| row.oneshot == oneshot);
    }

    if let Some(number_sorts) = filters.number_sorts.as_ref() {
        books = filter_rows(books, |row| {
            number_sorts
                .iter()
                .any(|value| (row.metadata_number_sort - *value).abs() <= f64::EPSILON)
        });
    }

    if let Some(number_sorts_excluded) = filters.number_sorts_excluded.as_ref() {
        books = filter_rows(books, |row| {
            !number_sorts_excluded
                .iter()
                .any(|value| (row.metadata_number_sort - *value).abs() <= f64::EPSILON)
        });
    }

    if let Some(number_sort_gt) = filters.number_sort_gt {
        books = filter_rows(books, |row| row.metadata_number_sort > number_sort_gt);
    }

    if let Some(number_sort_lt) = filters.number_sort_lt {
        books = filter_rows(books, |row| row.metadata_number_sort < number_sort_lt);
    }

    if let Some(media_statuses) = filters.media_statuses.as_ref() {
        books = filter_rows(books, |row| {
            normalized_text_matches(&row.media_status, media_statuses, TextMatchMode::StartsWith)
        });
    }

    if let Some(media_statuses_excluded) = filters.media_statuses_excluded.as_ref() {
        books = filter_rows(books, |row| {
            !any_ignore_ascii_case([row.media_status.as_str()], media_statuses_excluded)
        });
    }

    if let Some(read_statuses) = filters.read_statuses.as_ref() {
        if context.user_id.is_none() {
            books.clear();
        } else {
            books = filter_rows(books, |row| {
                any_ignore_ascii_case([row.read_status.as_str()], read_statuses)
            });
        }
    }

    if let Some(read_statuses_excluded) = filters.read_statuses_excluded.as_ref() {
        if context.user_id.is_none() {
            books.clear();
        } else {
            books = filter_rows(books, |row| {
                !any_ignore_ascii_case([row.read_status.as_str()], read_statuses_excluded)
            });
        }
    }

    if let Some(release_dates) = filters.release_dates.as_ref() {
        books = filter_rows(books, |row| {
            matches_optional_value(
                row.metadata_release_date.as_deref(),
                false,
                |release_date| release_dates.iter().any(|value| value == release_date),
            )
        });
    }

    if let Some(release_dates_excluded) = filters.release_dates_excluded.as_ref() {
        books = filter_rows(books, |row| {
            matches_optional_value(row.metadata_release_date.as_deref(), true, |release_date| {
                !release_dates_excluded
                    .iter()
                    .any(|value| value == release_date)
            })
        });
    }

    if let Some(release_dates_null) = filters.release_dates_null {
        books = filter_rows(books, |row| {
            row.metadata_release_date.is_none() == release_dates_null
        });
    }

    if let Some(release_date_gt) = filters.release_date_gt.as_ref() {
        books = filter_rows(books, |row| {
            matches_optional_value(
                row.metadata_release_date.as_deref(),
                false,
                |release_date| release_date > release_date_gt.as_str(),
            )
        });
    }

    if let Some(release_date_lt) = filters.release_date_lt.as_ref() {
        books = filter_rows(books, |row| {
            matches_optional_value(
                row.metadata_release_date.as_deref(),
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
        books = filter_rows(books, |row| {
            matches_optional_value(
                row.metadata_release_date.as_deref(),
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
        books = filter_rows(books, |row| {
            matches_optional_value(
                row.metadata_release_date.as_deref(),
                false,
                |release_date| release_date < cutoff.as_str(),
            )
        });
    }

    if let Some(release_date_begins_with) = filters.release_date_begins_with.as_ref() {
        books = filter_rows(books, |row| {
            matches_optional_value(
                row.metadata_release_date.as_deref(),
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
        books = filter_rows(books, |row| {
            matches_optional_value(
                row.metadata_release_date.as_deref(),
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
        books = filter_rows(books, |row| {
            matches_optional_value(row.metadata_release_date.as_deref(), true, |release_date| {
                !normalized_text_matches(
                    release_date,
                    release_date_begins_with_excluded,
                    TextMatchMode::StartsWith,
                )
            })
        });
    }

    if let Some(release_date_ends_with_excluded) = filters.release_date_ends_with_excluded.as_ref()
    {
        books = filter_rows(books, |row| {
            matches_optional_value(row.metadata_release_date.as_deref(), true, |release_date| {
                !normalized_text_matches(
                    release_date,
                    release_date_ends_with_excluded,
                    TextMatchMode::EndsWith,
                )
            })
        });
    }

    if let Some(release_date_contains_excluded) = filters.release_date_contains_excluded.as_ref() {
        books = filter_rows(books, |row| {
            matches_optional_value(row.metadata_release_date.as_deref(), true, |release_date| {
                !normalized_text_matches(
                    release_date,
                    release_date_contains_excluded,
                    TextMatchMode::Contains,
                )
            })
        });
    }

    if !query.sort_modes.is_empty() {
        let readlist_ordering = if query.sort_modes.iter().any(|sort_mode| {
            matches!(
                sort_mode,
                PersistedBooksSortMode::ReadListNumberAsc
                    | PersistedBooksSortMode::ReadListNumberDesc
            )
        }) {
            if let Some(readlist_id) = first_readlist_sort_id(query.condition.as_ref()) {
                Some(backend.load_readlist_ordering(readlist_id).await?)
            } else {
                None
            }
        } else {
            None
        };

        books.sort_by(|left, right| {
            for sort_mode in &query.sort_modes {
                let ordering = match sort_mode {
                    PersistedBooksSortMode::TitleAsc => left
                        .title
                        .to_ascii_lowercase()
                        .cmp(&right.title.to_ascii_lowercase()),
                    PersistedBooksSortMode::TitleDesc => right
                        .title
                        .to_ascii_lowercase()
                        .cmp(&left.title.to_ascii_lowercase()),
                    PersistedBooksSortMode::NameAsc => left
                        .name
                        .to_ascii_lowercase()
                        .cmp(&right.name.to_ascii_lowercase()),
                    PersistedBooksSortMode::NameDesc => right
                        .name
                        .to_ascii_lowercase()
                        .cmp(&left.name.to_ascii_lowercase()),
                    PersistedBooksSortMode::SeriesTitleAsc => left
                        .series_title_sort
                        .to_ascii_lowercase()
                        .cmp(&right.series_title_sort.to_ascii_lowercase()),
                    PersistedBooksSortMode::SeriesTitleDesc => right
                        .series_title_sort
                        .to_ascii_lowercase()
                        .cmp(&left.series_title_sort.to_ascii_lowercase()),
                    PersistedBooksSortMode::CreatedDateAsc => left.created.cmp(&right.created),
                    PersistedBooksSortMode::CreatedDateDesc => right.created.cmp(&left.created),
                    PersistedBooksSortMode::LastModifiedDateAsc => {
                        left.last_modified.cmp(&right.last_modified)
                    }
                    PersistedBooksSortMode::LastModifiedDateDesc => {
                        right.last_modified.cmp(&left.last_modified)
                    }
                    PersistedBooksSortMode::FileSizeAsc => left.size_bytes.cmp(&right.size_bytes),
                    PersistedBooksSortMode::FileSizeDesc => right.size_bytes.cmp(&left.size_bytes),
                    PersistedBooksSortMode::FileHashAsc => left.file_hash.cmp(&right.file_hash),
                    PersistedBooksSortMode::FileHashDesc => right.file_hash.cmp(&left.file_hash),
                    PersistedBooksSortMode::UrlAsc => left
                        .url
                        .to_ascii_lowercase()
                        .cmp(&right.url.to_ascii_lowercase()),
                    PersistedBooksSortMode::UrlDesc => right
                        .url
                        .to_ascii_lowercase()
                        .cmp(&left.url.to_ascii_lowercase()),
                    PersistedBooksSortMode::MediaStatusAsc => left
                        .media_status
                        .to_ascii_lowercase()
                        .cmp(&right.media_status.to_ascii_lowercase()),
                    PersistedBooksSortMode::MediaStatusDesc => right
                        .media_status
                        .to_ascii_lowercase()
                        .cmp(&left.media_status.to_ascii_lowercase()),
                    PersistedBooksSortMode::MediaCommentAsc => left
                        .media_comment
                        .to_ascii_lowercase()
                        .cmp(&right.media_comment.to_ascii_lowercase()),
                    PersistedBooksSortMode::MediaCommentDesc => right
                        .media_comment
                        .to_ascii_lowercase()
                        .cmp(&left.media_comment.to_ascii_lowercase()),
                    PersistedBooksSortMode::MediaTypeAsc => left
                        .media_type
                        .to_ascii_lowercase()
                        .cmp(&right.media_type.to_ascii_lowercase()),
                    PersistedBooksSortMode::MediaTypeDesc => right
                        .media_type
                        .to_ascii_lowercase()
                        .cmp(&left.media_type.to_ascii_lowercase()),
                    PersistedBooksSortMode::MediaPagesCountAsc => {
                        left.media_pages_count.cmp(&right.media_pages_count)
                    }
                    PersistedBooksSortMode::MediaPagesCountDesc => {
                        right.media_pages_count.cmp(&left.media_pages_count)
                    }
                    PersistedBooksSortMode::ReadProgressLastModifiedDateAsc => left
                        .read_progress
                        .as_ref()
                        .map(|progress| progress.last_modified.as_str())
                        .cmp(
                            &right
                                .read_progress
                                .as_ref()
                                .map(|progress| progress.last_modified.as_str()),
                        ),
                    PersistedBooksSortMode::ReadProgressLastModifiedDateDesc => right
                        .read_progress
                        .as_ref()
                        .map(|progress| progress.last_modified.as_str())
                        .cmp(
                            &left
                                .read_progress
                                .as_ref()
                                .map(|progress| progress.last_modified.as_str()),
                        ),
                    PersistedBooksSortMode::ReadProgressReadDateAsc => left
                        .read_progress
                        .as_ref()
                        .and_then(|progress| progress.read_date.as_deref())
                        .cmp(
                            &right
                                .read_progress
                                .as_ref()
                                .and_then(|progress| progress.read_date.as_deref()),
                        ),
                    PersistedBooksSortMode::ReadProgressReadDateDesc => right
                        .read_progress
                        .as_ref()
                        .and_then(|progress| progress.read_date.as_deref())
                        .cmp(
                            &left
                                .read_progress
                                .as_ref()
                                .and_then(|progress| progress.read_date.as_deref()),
                        ),
                    PersistedBooksSortMode::ReleaseDateAsc => {
                        left.metadata_release_date.cmp(&right.metadata_release_date)
                    }
                    PersistedBooksSortMode::ReleaseDateDesc => {
                        right.metadata_release_date.cmp(&left.metadata_release_date)
                    }
                    PersistedBooksSortMode::NumberSortAsc => left
                        .metadata_number_sort
                        .partial_cmp(&right.metadata_number_sort)
                        .unwrap_or(std::cmp::Ordering::Equal),
                    PersistedBooksSortMode::NumberSortDesc => right
                        .metadata_number_sort
                        .partial_cmp(&left.metadata_number_sort)
                        .unwrap_or(std::cmp::Ordering::Equal),
                    PersistedBooksSortMode::SeriesIdAsc => left.series_id.cmp(&right.series_id),
                    PersistedBooksSortMode::ReadListNumberAsc => readlist_ordering
                        .as_ref()
                        .and_then(|ordering| ordering.get(&left.id))
                        .cmp(
                            &readlist_ordering
                                .as_ref()
                                .and_then(|ordering| ordering.get(&right.id)),
                        ),
                    PersistedBooksSortMode::ReadListNumberDesc => readlist_ordering
                        .as_ref()
                        .and_then(|ordering| ordering.get(&right.id))
                        .cmp(
                            &readlist_ordering
                                .as_ref()
                                .and_then(|ordering| ordering.get(&left.id)),
                        ),
                    PersistedBooksSortMode::RelevanceAsc => {
                        compare_relevance_ranks(&relevance_ranks, &left.id, &right.id, false)
                    }
                    PersistedBooksSortMode::RelevanceDesc => {
                        compare_relevance_ranks(&relevance_ranks, &left.id, &right.id, true)
                    }
                };
                if ordering != std::cmp::Ordering::Equal {
                    return ordering;
                }
            }
            left.id.cmp(&right.id)
        });
    }

    let total_elements = books.len();
    let content = if query.unpaged {
        books
    } else {
        let offset = query.page.saturating_mul(query.size);
        if offset >= total_elements {
            vec![]
        } else {
            books.into_iter().skip(offset).take(query.size).collect()
        }
    };
    let page = if query.unpaged { 0 } else { query.page };
    let page_size = if query.unpaged {
        total_elements.max(1)
    } else {
        query.size.max(1)
    };

    Ok(PageEnvelope::from_slice(
        content
            .into_iter()
            .map(|row| BookReadModel {
                id: row.id,
                series_id: row.series_id,
                series_title: row.series_title,
                library_id: row.library_id,
                name: row.name,
                url: row.url,
                number: row.number,
                created: row.created,
                last_modified: row.last_modified,
                file_last_modified: row.file_last_modified,
                size_bytes: row.size_bytes,
                media_status: row.media_status,
                media_type: row.media_type,
                media_pages_count: row.media_pages_count,
                media_comment: row.media_comment,
                media_epub_divina_compatible: row.media_epub_divina_compatible,
                media_epub_is_kepub: row.media_epub_is_kepub,
                metadata_title: row.title,
                metadata_title_lock: row.metadata_title_lock,
                metadata_summary: row.metadata_summary,
                metadata_summary_lock: row.metadata_summary_lock,
                metadata_number: row.metadata_number,
                metadata_number_lock: row.metadata_number_lock,
                metadata_number_sort: row.metadata_number_sort,
                metadata_number_sort_lock: row.metadata_number_sort_lock,
                metadata_release_date: row.metadata_release_date,
                metadata_release_date_lock: row.metadata_release_date_lock,
                metadata_authors: row
                    .metadata_authors
                    .into_iter()
                    .map(|author| BookMetadataAuthorReadModel {
                        name: author.name,
                        role: author.role,
                    })
                    .collect(),
                metadata_authors_lock: row.metadata_authors_lock,
                metadata_tags: row.metadata_tags,
                metadata_tags_lock: row.metadata_tags_lock,
                metadata_isbn: row.metadata_isbn,
                metadata_isbn_lock: row.metadata_isbn_lock,
                metadata_links: row
                    .metadata_links
                    .into_iter()
                    .map(|link| BookMetadataLinkReadModel {
                        label: link.label,
                        url: link.url,
                    })
                    .collect(),
                metadata_links_lock: row.metadata_links_lock,
                metadata_created: row.metadata_created,
                metadata_last_modified: row.metadata_last_modified,
                read_progress: row.read_progress.map(|progress| BookReadProgressReadModel {
                    page: progress.page,
                    completed: progress.completed,
                    read_date: progress.read_date,
                    created: progress.created,
                    last_modified: progress.last_modified,
                    device_id: progress.device_id,
                    device_name: progress.device_name,
                }),
                deleted: row.deleted,
                file_hash: row.file_hash,
                oneshot: row.oneshot,
            })
            .collect(),
        page,
        page_size,
        total_elements,
    ))
}

fn compare_relevance_ranks(
    relevance_ranks: &HashMap<String, usize>,
    left_id: &str,
    right_id: &str,
    descending: bool,
) -> std::cmp::Ordering {
    let left_rank = relevance_ranks.get(left_id).copied();
    let right_rank = relevance_ranks.get(right_id).copied();
    match (left_rank, right_rank) {
        (Some(left), Some(right)) if descending => right.cmp(&left),
        (Some(left), Some(right)) => left.cmp(&right),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        _ => std::cmp::Ordering::Equal,
    }
}
