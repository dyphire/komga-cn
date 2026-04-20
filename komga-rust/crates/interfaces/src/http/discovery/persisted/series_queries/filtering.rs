use super::common_helpers::{
    TextMatchMode, any_ignore_ascii_case, any_normalized_text_matches, matches_optional_value,
    normalized_text_matches,
};
use super::*;
use crate::discovery_persisted_access::PersistedDiscoveryService;
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
            left.cmp(&right).then_with(|| left_id.cmp(right_id))
        }
        (Some(left), Some(right)) => right.cmp(&left).then_with(|| left_id.cmp(right_id)),
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

pub async fn load_persisted_series_page(
    backend: &dyn PersistedDiscoveryService,
    database_file: &FsPath,
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
        let total_count = backend
            .load_persisted_series_count(database_file.to_path_buf())
            .await?;
        let ranked_candidates = backend
            .search_series_scored_ids(
                database_file.to_path_buf(),
                search.to_string(),
                total_count.max(1),
            )
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
                .load_persisted_series_summaries_by_ids(database_file.to_path_buf(), candidate_ids)
                .await?;
        }
    } else {
        series = backend
            .load_persisted_series_summaries(database_file.to_path_buf())
            .await?;
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
        if let (
            Some(age),
            Some(crate::http::discovery_auth::principal::AgeRestrictionKind::Exclude),
        ) = (restrictions.age, restrictions.age_restriction)
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

    let deleted = filters.deleted.unwrap_or_default();
    series = filter_rows(series, |row| row.deleted == deleted);

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

        let read_progress =
            load_series_read_progress_counts(backend, database_file, user_id).await?;

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
        let total_book_counts = load_series_total_book_counts(backend, database_file).await?;
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
        && let Some(cutoff) =
            persisted_utc_date_minus_days(backend, database_file, release_date_in_last_days).await?
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
        && let Some(cutoff) =
            persisted_utc_date_minus_days(backend, database_file, release_date_not_in_last_days)
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
        let memberships = load_collection_memberships(backend, database_file).await?;
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
        if let Some(collection_id) = filters.collection_ids.as_ref().and_then(|ids| ids.first()) {
            load_collection_ordering(backend, database_file, collection_id).await?
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
            load_series_read_dates(backend, database_file, user_id).await?
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
    let content = if query.unpaged {
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

    Ok(PageEnvelope::from_slice(
        content,
        page,
        page_size,
        total_elements,
    ))
}
