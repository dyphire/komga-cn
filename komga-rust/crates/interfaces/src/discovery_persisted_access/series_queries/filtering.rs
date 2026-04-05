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
    let mut relevance_ranks: HashMap<String, usize> = HashMap::new();
    if let Some(search) = query.search.as_ref().map(|value| value.trim())
        && !search.is_empty()
    {
        let total_count = persisted_backend_load_persisted_series_count(database_file).await?;
        let candidate_ids =
            persisted_backend_search_series_ids(database_file, search, total_count.max(1)).await?;
        if candidate_ids.is_empty() {
            series.clear();
        } else {
            relevance_ranks = candidate_ids
                .iter()
                .enumerate()
                .map(|(index, id)| (id.clone(), index))
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
        if let (Some(age), Some(crate::http::discovery_auth::AgeRestrictionKind::Exclude)) =
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
                !row.labels.iter().any(|label| {
                    restrictions
                        .labels_exclude
                        .iter()
                        .any(|excluded| label.eq_ignore_ascii_case(excluded))
                })
            });
        }

        if !restrictions.labels_allow.is_empty() {
            series = filter_rows(series, |row| {
                row.labels.iter().any(|label| {
                    restrictions
                        .labels_allow
                        .iter()
                        .any(|allowed| label.eq_ignore_ascii_case(allowed))
                })
            });
        }
    }

    if let Some(titles) = filters.titles.as_ref() {
        series = filter_rows(series, |row| {
            let normalized = row.title.to_ascii_lowercase();
            titles.contains(&normalized)
        });
    }

    if let Some(titles_excluded) = filters.titles_excluded.as_ref() {
        series = filter_rows(series, |row| {
            let normalized = row.title.to_ascii_lowercase();
            !titles_excluded.contains(&normalized)
        });
    }

    if let Some(titles_contains) = filters.titles_contains.as_ref() {
        series = filter_rows(series, |row| {
            let normalized = row.title.to_ascii_lowercase();
            titles_contains
                .iter()
                .any(|value| normalized.contains(value))
        });
    }

    if let Some(titles_contains_excluded) = filters.titles_contains_excluded.as_ref() {
        series = filter_rows(series, |row| {
            let normalized = row.title.to_ascii_lowercase();
            !titles_contains_excluded
                .iter()
                .any(|value| normalized.contains(value))
        });
    }

    if let Some(titles_begins_with) = filters.titles_begins_with.as_ref() {
        series = filter_rows(series, |row| {
            let normalized = row.title.to_ascii_lowercase();
            titles_begins_with
                .iter()
                .any(|value| normalized.starts_with(value))
        });
    }

    if let Some(titles_begins_with_excluded) = filters.titles_begins_with_excluded.as_ref() {
        series = filter_rows(series, |row| {
            let normalized = row.title.to_ascii_lowercase();
            !titles_begins_with_excluded
                .iter()
                .any(|value| normalized.starts_with(value))
        });
    }

    if let Some(titles_ends_with) = filters.titles_ends_with.as_ref() {
        series = filter_rows(series, |row| {
            let normalized = row.title.to_ascii_lowercase();
            titles_ends_with
                .iter()
                .any(|value| normalized.ends_with(value))
        });
    }

    if let Some(titles_ends_with_excluded) = filters.titles_ends_with_excluded.as_ref() {
        series = filter_rows(series, |row| {
            let normalized = row.title.to_ascii_lowercase();
            !titles_ends_with_excluded
                .iter()
                .any(|value| normalized.ends_with(value))
        });
    }

    if let Some(title_sorts) = filters.title_sorts.as_ref() {
        series = filter_rows(series, |row| {
            let normalized = row.title_sort.to_ascii_lowercase();
            title_sorts.contains(&normalized)
        });
    }

    if let Some(title_sorts_excluded) = filters.title_sorts_excluded.as_ref() {
        series = filter_rows(series, |row| {
            let normalized = row.title_sort.to_ascii_lowercase();
            !title_sorts_excluded.contains(&normalized)
        });
    }

    if let Some(title_sorts_contains) = filters.title_sorts_contains.as_ref() {
        series = filter_rows(series, |row| {
            let normalized = row.title_sort.to_ascii_lowercase();
            title_sorts_contains
                .iter()
                .any(|value| normalized.contains(value))
        });
    }

    if let Some(title_sorts_contains_excluded) = filters.title_sorts_contains_excluded.as_ref() {
        series = filter_rows(series, |row| {
            let normalized = row.title_sort.to_ascii_lowercase();
            !title_sorts_contains_excluded
                .iter()
                .any(|value| normalized.contains(value))
        });
    }

    if let Some(title_sorts_begins_with) = filters.title_sorts_begins_with.as_ref() {
        series = filter_rows(series, |row| {
            let normalized = row.title_sort.to_ascii_lowercase();
            title_sorts_begins_with
                .iter()
                .any(|value| normalized.starts_with(value))
        });
    }

    if let Some(title_sorts_begins_with_excluded) =
        filters.title_sorts_begins_with_excluded.as_ref()
    {
        series = filter_rows(series, |row| {
            let normalized = row.title_sort.to_ascii_lowercase();
            !title_sorts_begins_with_excluded
                .iter()
                .any(|value| normalized.starts_with(value))
        });
    }

    if let Some(title_sorts_ends_with) = filters.title_sorts_ends_with.as_ref() {
        series = filter_rows(series, |row| {
            let normalized = row.title_sort.to_ascii_lowercase();
            title_sorts_ends_with
                .iter()
                .any(|value| normalized.ends_with(value))
        });
    }

    if let Some(title_sorts_ends_with_excluded) = filters.title_sorts_ends_with_excluded.as_ref() {
        series = filter_rows(series, |row| {
            let normalized = row.title_sort.to_ascii_lowercase();
            !title_sorts_ends_with_excluded
                .iter()
                .any(|value| normalized.ends_with(value))
        });
    }

    if let Some(deleted) = filters.deleted {
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
            row.genres.iter().any(|genre| {
                let normalized = genre.to_ascii_lowercase();
                genres.iter().any(|value| normalized.contains(value))
            })
        });
    }

    if let Some(genres_excluded) = filters.genres_excluded.as_ref() {
        series = filter_rows(series, |row| {
            !row.genres.iter().any(|genre| {
                let normalized = genre.to_ascii_lowercase();
                genres_excluded
                    .iter()
                    .any(|value| normalized.contains(value))
            })
        });
    }

    if let Some(genres_null) = filters.genres_null {
        series = filter_rows(series, |row| row.genres.is_empty() == genres_null);
    }

    if let Some(tags) = filters.tags.as_ref() {
        series = filter_rows(series, |row| {
            row.tags.iter().any(|tag| {
                let normalized = tag.to_ascii_lowercase();
                tags.iter().any(|value| normalized.contains(value))
            })
        });
    }

    if let Some(tags_excluded) = filters.tags_excluded.as_ref() {
        series = filter_rows(series, |row| {
            !row.tags.iter().any(|tag| {
                let normalized = tag.to_ascii_lowercase();
                tags_excluded.iter().any(|value| normalized.contains(value))
            })
        });
    }

    if let Some(tags_null) = filters.tags_null {
        series = filter_rows(series, |row| row.tags.is_empty() == tags_null);
    }

    if let Some(languages) = filters.languages.as_ref() {
        series = filter_rows(series, |row| {
            languages
                .iter()
                .any(|value| value.eq_ignore_ascii_case(&row.language))
        });
    }

    if let Some(languages_excluded) = filters.languages_excluded.as_ref() {
        series = filter_rows(series, |row| {
            !languages_excluded
                .iter()
                .any(|value| value.eq_ignore_ascii_case(&row.language))
        });
    }

    if let Some(publishers) = filters.publishers.as_ref() {
        series = filter_rows(series, |row| {
            publishers
                .iter()
                .any(|value| value.eq_ignore_ascii_case(&row.publisher))
        });
    }

    if let Some(publishers_excluded) = filters.publishers_excluded.as_ref() {
        series = filter_rows(series, |row| {
            !publishers_excluded
                .iter()
                .any(|value| value.eq_ignore_ascii_case(&row.publisher))
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
            row.labels.iter().any(|label| {
                let normalized = label.to_ascii_lowercase();
                sharing_labels
                    .iter()
                    .any(|value| normalized.contains(value))
            })
        });
    }

    if let Some(sharing_labels_excluded) = filters.sharing_labels_excluded.as_ref() {
        series = filter_rows(series, |row| {
            !row.labels.iter().any(|label| {
                let normalized = label.to_ascii_lowercase();
                sharing_labels_excluded
                    .iter()
                    .any(|value| normalized.contains(value))
            })
        });
    }

    if let Some(sharing_labels_null) = filters.sharing_labels_null {
        series = filter_rows(series, |row| row.labels.is_empty() == sharing_labels_null);
    }

    if let Some(authors) = filters.authors.as_ref() {
        series = filter_rows(series, |row| {
            row.books_metadata_authors.iter().any(|author| {
                let normalized = author.to_ascii_lowercase();
                authors
                    .iter()
                    .any(|value| author_value_matches(&normalized, value))
            })
        });
    }

    if let Some(authors_excluded) = filters.authors_excluded.as_ref() {
        series = filter_rows(series, |row| {
            !row.books_metadata_authors.iter().any(|author| {
                let normalized = author.to_ascii_lowercase();
                authors_excluded
                    .iter()
                    .any(|value| author_value_matches(&normalized, value))
            })
        });
    }

    if let Some(series_statuses) = filters.series_statuses.as_ref() {
        series = filter_rows(series, |row| {
            series_statuses
                .iter()
                .any(|value| value.eq_ignore_ascii_case(&row.status))
        });
    }

    if let Some(series_statuses_excluded) = filters.series_statuses_excluded.as_ref() {
        series = filter_rows(series, |row| {
            !series_statuses_excluded
                .iter()
                .any(|value| value.eq_ignore_ascii_case(&row.status))
        });
    }

    if let Some(release_dates) = filters.release_dates.as_ref() {
        series = filter_rows(series, |row| {
            let Some(release_date) = row.books_metadata_release_date.as_ref() else {
                return false;
            };
            release_dates.iter().any(|value| value == release_date)
        });
    }

    if let Some(release_dates_excluded) = filters.release_dates_excluded.as_ref() {
        series = filter_rows(series, |row| {
            let Some(release_date) = row.books_metadata_release_date.as_ref() else {
                return true;
            };
            !release_dates_excluded
                .iter()
                .any(|value| value == release_date)
        });
    }

    if let Some(release_dates_null) = filters.release_dates_null {
        series = filter_rows(series, |row| {
            row.books_metadata_release_date.is_none() == release_dates_null
        });
    }

    if let Some(release_date_gt) = filters.release_date_gt.as_ref() {
        series = filter_rows(series, |row| {
            let Some(release_date) = row.books_metadata_release_date.as_ref() else {
                return false;
            };
            release_date > release_date_gt
        });
    }

    if let Some(release_date_lt) = filters.release_date_lt.as_ref() {
        series = filter_rows(series, |row| {
            let Some(release_date) = row.books_metadata_release_date.as_ref() else {
                return false;
            };
            release_date < release_date_lt
        });
    }

    if let Some(release_date_in_last_days) = filters.release_date_in_last_days
        && let Some(cutoff) =
            persisted_utc_date_minus_days(database_file, release_date_in_last_days).await?
    {
        series = filter_rows(series, |row| {
            let Some(release_date) = row.books_metadata_release_date.as_ref() else {
                return false;
            };
            release_date > &cutoff
        });
    }

    if let Some(release_date_not_in_last_days) = filters.release_date_not_in_last_days
        && let Some(cutoff) =
            persisted_utc_date_minus_days(database_file, release_date_not_in_last_days).await?
    {
        series = filter_rows(series, |row| {
            let Some(release_date) = row.books_metadata_release_date.as_ref() else {
                return false;
            };
            release_date < &cutoff
        });
    }

    if let Some(release_date_begins_with) = filters.release_date_begins_with.as_ref() {
        series = filter_rows(series, |row| {
            let Some(release_date) = row.books_metadata_release_date.as_ref() else {
                return false;
            };
            let normalized = release_date.to_ascii_lowercase();
            release_date_begins_with
                .iter()
                .any(|value| normalized.starts_with(value))
        });
    }

    if let Some(release_date_ends_with) = filters.release_date_ends_with.as_ref() {
        series = filter_rows(series, |row| {
            let Some(release_date) = row.books_metadata_release_date.as_ref() else {
                return false;
            };
            let normalized = release_date.to_ascii_lowercase();
            release_date_ends_with
                .iter()
                .any(|value| normalized.ends_with(value))
        });
    }

    if let Some(release_date_begins_with_excluded) =
        filters.release_date_begins_with_excluded.as_ref()
    {
        series = filter_rows(series, |row| {
            let Some(release_date) = row.books_metadata_release_date.as_ref() else {
                return true;
            };
            let normalized = release_date.to_ascii_lowercase();
            !release_date_begins_with_excluded
                .iter()
                .any(|value| normalized.starts_with(value))
        });
    }

    if let Some(release_date_ends_with_excluded) = filters.release_date_ends_with_excluded.as_ref()
    {
        series = filter_rows(series, |row| {
            let Some(release_date) = row.books_metadata_release_date.as_ref() else {
                return true;
            };
            let normalized = release_date.to_ascii_lowercase();
            !release_date_ends_with_excluded
                .iter()
                .any(|value| normalized.ends_with(value))
        });
    }

    if let Some(release_date_contains_excluded) = filters.release_date_contains_excluded.as_ref() {
        series = filter_rows(series, |row| {
            let Some(release_date) = row.books_metadata_release_date.as_ref() else {
                return true;
            };
            let normalized = release_date.to_ascii_lowercase();
            !release_date_contains_excluded
                .iter()
                .any(|value| normalized.contains(value))
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

    series.sort_by(|left, right| {
        for sort_mode in &query.sort_modes {
            let ordering = match sort_mode {
                PersistedSeriesSortMode::TitleAsc => left
                    .title_sort
                    .to_ascii_lowercase()
                    .cmp(&right.title_sort.to_ascii_lowercase()),
                PersistedSeriesSortMode::CreatedDesc => right.created.cmp(&left.created),
                PersistedSeriesSortMode::Latest => right.last_modified.cmp(&left.last_modified),
                PersistedSeriesSortMode::RelevanceAsc => {
                    compare_relevance_ranks(&relevance_ranks, &left.id, &right.id, false)
                }
                PersistedSeriesSortMode::RelevanceDesc => {
                    compare_relevance_ranks(&relevance_ranks, &left.id, &right.id, true)
                }
            };
            if ordering != std::cmp::Ordering::Equal {
                return ordering;
            }
        }
        left.id.cmp(&right.id)
    });

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

fn compare_relevance_ranks(
    relevance_ranks: &HashMap<String, usize>,
    left_id: &str,
    right_id: &str,
    descending: bool,
) -> std::cmp::Ordering {
    let left_rank = relevance_ranks.get(left_id).copied();
    let right_rank = relevance_ranks.get(right_id).copied();
    match (left_rank, right_rank) {
        (Some(left), Some(right)) if descending => left.cmp(&right),
        (Some(left), Some(right)) => right.cmp(&left),
        _ => std::cmp::Ordering::Equal,
    }
}
