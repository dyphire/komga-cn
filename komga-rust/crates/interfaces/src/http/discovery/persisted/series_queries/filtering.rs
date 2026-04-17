use super::common_helpers::{
    TextMatchMode, any_ignore_ascii_case, any_normalized_text_matches, matches_optional_value,
    normalized_text_matches,
};
use super::*;

pub async fn load_persisted_series_summaries(
    database_file: &FsPath,
) -> Result<Vec<PersistedSeriesSummary>, String> {
    persisted_backend_load_persisted_series_summaries(database_file).await
}

pub async fn load_persisted_series_page(
    database_file: &FsPath,
    context: &DiscoveryQueryContext,
    query: PersistedSeriesBrowseQuery,
) -> Result<PageEnvelope<PersistedSeriesSummary>, String> {
    let mut series = Vec::new();
    let filters = &query.filters;
    let mut relevance_scores: HashMap<String, f32> = HashMap::new();
    if let Some(search) = query.search.as_ref().map(|value| value.trim())
        && !search.is_empty()
    {
        let total_count = persisted_backend_load_persisted_series_count(database_file).await?;
        let ranked_candidates =
            persisted_backend_search_series_scored_ids(database_file, search, total_count.max(1))
                .await?;
        let candidate_ids = ranked_candidates
            .iter()
            .map(|(_, id)| id.clone())
            .collect::<Vec<_>>();
        if candidate_ids.is_empty() {
            series.clear();
        } else {
            relevance_scores = ranked_candidates
                .iter()
                .map(|(score, id)| (id.clone(), *score))
                .collect();
            series = persisted_backend_load_persisted_series_summaries_by_ids(
                database_file,
                &candidate_ids,
            )
            .await?;
        }
    } else {
        series = load_persisted_series_summaries(database_file).await?;
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

        let read_progress = load_series_read_progress_counts(database_file, user_id).await?;

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
        let total_book_counts = load_series_total_book_counts(database_file).await?;
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
            any_normalized_text_matches(
                row.labels.iter().map(String::as_str),
                sharing_labels,
                TextMatchMode::Contains,
            )
        });
    }

    if let Some(sharing_labels_excluded) = filters.sharing_labels_excluded.as_ref() {
        series = filter_rows(series, |row| {
            !any_normalized_text_matches(
                row.labels.iter().map(String::as_str),
                sharing_labels_excluded,
                TextMatchMode::Contains,
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
            persisted_utc_date_minus_days(database_file, release_date_in_last_days).await?
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
            persisted_utc_date_minus_days(database_file, release_date_not_in_last_days).await?
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
        let memberships = load_collection_memberships(database_file).await?;
        series = filter_rows(series, |row| {
            memberships
                .get(&row.id)
                .into_iter()
                .flatten()
                .any(|collection_id| collection_ids.iter().any(|id| id == collection_id))
        });
    }

    if !query.sort_modes.is_empty() {
        series.sort_by(|left, right| {
            for sort_mode in &query.sort_modes {
                let ordering = match sort_mode {
                    PersistedSeriesSortMode::TitleAsc => left
                        .title_sort
                        .to_ascii_lowercase()
                        .cmp(&right.title_sort.to_ascii_lowercase()),
                    PersistedSeriesSortMode::CreatedDesc => right.created.cmp(&left.created),
                    PersistedSeriesSortMode::LastModifiedDesc => {
                        right.last_modified.cmp(&left.last_modified)
                    }
                    PersistedSeriesSortMode::ReleaseDateDesc => right
                        .books_metadata_release_date
                        .cmp(&left.books_metadata_release_date),
                    PersistedSeriesSortMode::BooksCountDesc => {
                        right.books_count.cmp(&left.books_count)
                    }
                    PersistedSeriesSortMode::RelevanceAsc => {
                        compare_relevance_scores(&relevance_scores, &left.id, &right.id, false)
                    }
                    PersistedSeriesSortMode::RelevanceDesc => {
                        compare_relevance_scores(&relevance_scores, &left.id, &right.id, true)
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

fn compare_relevance_scores(
    relevance_scores: &HashMap<String, f32>,
    left_id: &str,
    right_id: &str,
    descending: bool,
) -> std::cmp::Ordering {
    let left_score = relevance_scores.get(left_id).copied();
    let right_score = relevance_scores.get(right_id).copied();
    match (left_score, right_score) {
        (Some(left), Some(right)) if descending => {
            right.total_cmp(&left).then_with(|| left_id.cmp(right_id))
        }
        (Some(left), Some(right)) => left.total_cmp(&right).then_with(|| left_id.cmp(right_id)),
        _ => std::cmp::Ordering::Equal,
    }
}
