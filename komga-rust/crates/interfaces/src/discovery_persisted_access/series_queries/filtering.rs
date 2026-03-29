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
    let mut series = load_persisted_series_summaries(database_file).await?;

    if let Some(allowed_ids) = context.authorized_library_ids.as_ref() {
        series.retain(|row| allowed_ids.iter().any(|id| id == row.library_id.as_str()));
    }
    if let Some(library_ids) = query.library_ids.as_ref() {
        series.retain(|row| library_ids.iter().any(|id| id == row.library_id.as_str()));
    }

    if let Some(titles) = query.titles.as_ref() {
        series.retain(|row| {
            let normalized = row.title.to_ascii_lowercase();
            titles.iter().any(|value| normalized == *value)
        });
    }

    if let Some(titles_excluded) = query.titles_excluded.as_ref() {
        series.retain(|row| {
            let normalized = row.title.to_ascii_lowercase();
            !titles_excluded.iter().any(|value| normalized == *value)
        });
    }

    if let Some(titles_contains) = query.titles_contains.as_ref() {
        series.retain(|row| {
            let normalized = row.title.to_ascii_lowercase();
            titles_contains
                .iter()
                .any(|value| normalized.contains(value))
        });
    }

    if let Some(titles_contains_excluded) = query.titles_contains_excluded.as_ref() {
        series.retain(|row| {
            let normalized = row.title.to_ascii_lowercase();
            !titles_contains_excluded
                .iter()
                .any(|value| normalized.contains(value))
        });
    }

    if let Some(titles_begins_with) = query.titles_begins_with.as_ref() {
        series.retain(|row| {
            let normalized = row.title.to_ascii_lowercase();
            titles_begins_with
                .iter()
                .any(|value| normalized.starts_with(value))
        });
    }

    if let Some(titles_begins_with_excluded) = query.titles_begins_with_excluded.as_ref() {
        series.retain(|row| {
            let normalized = row.title.to_ascii_lowercase();
            !titles_begins_with_excluded
                .iter()
                .any(|value| normalized.starts_with(value))
        });
    }

    if let Some(titles_ends_with) = query.titles_ends_with.as_ref() {
        series.retain(|row| {
            let normalized = row.title.to_ascii_lowercase();
            titles_ends_with
                .iter()
                .any(|value| normalized.ends_with(value))
        });
    }

    if let Some(titles_ends_with_excluded) = query.titles_ends_with_excluded.as_ref() {
        series.retain(|row| {
            let normalized = row.title.to_ascii_lowercase();
            !titles_ends_with_excluded
                .iter()
                .any(|value| normalized.ends_with(value))
        });
    }

    if let Some(title_sorts) = query.title_sorts.as_ref() {
        series.retain(|row| {
            let normalized = row.title_sort.to_ascii_lowercase();
            title_sorts.iter().any(|value| normalized == *value)
        });
    }

    if let Some(title_sorts_excluded) = query.title_sorts_excluded.as_ref() {
        series.retain(|row| {
            let normalized = row.title_sort.to_ascii_lowercase();
            !title_sorts_excluded
                .iter()
                .any(|value| normalized == *value)
        });
    }

    if let Some(title_sorts_contains) = query.title_sorts_contains.as_ref() {
        series.retain(|row| {
            let normalized = row.title_sort.to_ascii_lowercase();
            title_sorts_contains
                .iter()
                .any(|value| normalized.contains(value))
        });
    }

    if let Some(title_sorts_contains_excluded) = query.title_sorts_contains_excluded.as_ref() {
        series.retain(|row| {
            let normalized = row.title_sort.to_ascii_lowercase();
            !title_sorts_contains_excluded
                .iter()
                .any(|value| normalized.contains(value))
        });
    }

    if let Some(title_sorts_begins_with) = query.title_sorts_begins_with.as_ref() {
        series.retain(|row| {
            let normalized = row.title_sort.to_ascii_lowercase();
            title_sorts_begins_with
                .iter()
                .any(|value| normalized.starts_with(value))
        });
    }

    if let Some(title_sorts_begins_with_excluded) = query.title_sorts_begins_with_excluded.as_ref()
    {
        series.retain(|row| {
            let normalized = row.title_sort.to_ascii_lowercase();
            !title_sorts_begins_with_excluded
                .iter()
                .any(|value| normalized.starts_with(value))
        });
    }

    if let Some(title_sorts_ends_with) = query.title_sorts_ends_with.as_ref() {
        series.retain(|row| {
            let normalized = row.title_sort.to_ascii_lowercase();
            title_sorts_ends_with
                .iter()
                .any(|value| normalized.ends_with(value))
        });
    }

    if let Some(title_sorts_ends_with_excluded) = query.title_sorts_ends_with_excluded.as_ref() {
        series.retain(|row| {
            let normalized = row.title_sort.to_ascii_lowercase();
            !title_sorts_ends_with_excluded
                .iter()
                .any(|value| normalized.ends_with(value))
        });
    }

    if let Some(deleted) = query.deleted {
        series.retain(|row| row.deleted == deleted);
    }

    if let Some(oneshot) = query.oneshot {
        series.retain(|row| row.oneshot == oneshot);
    }

    if query.read_statuses.is_some() || query.read_statuses_excluded.is_some() {
        let Some(user_id) = context.user_id.as_deref() else {
            series.clear();
            let page = PageEnvelope::from_slice(vec![], query.page, query.size, 0);
            return Ok(page);
        };

        let read_progress = load_series_read_progress_counts(database_file, user_id).await?;

        if let Some(read_statuses) = query.read_statuses.as_ref() {
            series.retain(|row| {
                read_statuses.iter().any(|status| {
                    series_matches_read_status(
                        row,
                        read_progress.get(&row.id).copied(),
                        status.as_str(),
                    )
                })
            });
        }

        if let Some(read_statuses_excluded) = query.read_statuses_excluded.as_ref() {
            series.retain(|row| {
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

    if let Some(complete) = query.complete {
        let total_book_counts = load_series_total_book_counts(database_file).await?;
        series.retain(|row| {
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

    if let Some(genres) = query.genres.as_ref() {
        series.retain(|row| {
            row.genres.iter().any(|genre| {
                let normalized = genre.to_ascii_lowercase();
                genres.iter().any(|value| normalized.contains(value))
            })
        });
    }

    if let Some(genres_excluded) = query.genres_excluded.as_ref() {
        series.retain(|row| {
            !row.genres.iter().any(|genre| {
                let normalized = genre.to_ascii_lowercase();
                genres_excluded
                    .iter()
                    .any(|value| normalized.contains(value))
            })
        });
    }

    if let Some(genres_null) = query.genres_null {
        series.retain(|row| row.genres.is_empty() == genres_null);
    }

    if let Some(tags) = query.tags.as_ref() {
        series.retain(|row| {
            row.tags.iter().any(|tag| {
                let normalized = tag.to_ascii_lowercase();
                tags.iter().any(|value| normalized.contains(value))
            })
        });
    }

    if let Some(tags_excluded) = query.tags_excluded.as_ref() {
        series.retain(|row| {
            !row.tags.iter().any(|tag| {
                let normalized = tag.to_ascii_lowercase();
                tags_excluded.iter().any(|value| normalized.contains(value))
            })
        });
    }

    if let Some(tags_null) = query.tags_null {
        series.retain(|row| row.tags.is_empty() == tags_null);
    }

    if let Some(languages) = query.languages.as_ref() {
        series.retain(|row| {
            languages
                .iter()
                .any(|value| value.eq_ignore_ascii_case(&row.language))
        });
    }

    if let Some(languages_excluded) = query.languages_excluded.as_ref() {
        series.retain(|row| {
            !languages_excluded
                .iter()
                .any(|value| value.eq_ignore_ascii_case(&row.language))
        });
    }

    if let Some(publishers) = query.publishers.as_ref() {
        series.retain(|row| {
            publishers
                .iter()
                .any(|value| value.eq_ignore_ascii_case(&row.publisher))
        });
    }

    if let Some(publishers_excluded) = query.publishers_excluded.as_ref() {
        series.retain(|row| {
            !publishers_excluded
                .iter()
                .any(|value| value.eq_ignore_ascii_case(&row.publisher))
        });
    }

    if let Some(age_ratings) = query.age_ratings.as_ref() {
        series.retain(|row| {
            row.age_rating
                .map(|rating| age_ratings.iter().any(|value| *value == rating))
                .unwrap_or(false)
        });
    }

    if let Some(age_ratings_excluded) = query.age_ratings_excluded.as_ref() {
        series.retain(|row| {
            row.age_rating
                .map(|rating| !age_ratings_excluded.iter().any(|value| *value == rating))
                .unwrap_or(true)
        });
    }

    if let Some(age_ratings_null) = query.age_ratings_null {
        series.retain(|row| row.age_rating.is_none() == age_ratings_null);
    }

    if let Some(age_rating_gt) = query.age_rating_gt {
        series.retain(|row| {
            row.age_rating
                .map(|rating| rating > age_rating_gt)
                .unwrap_or(false)
        });
    }

    if let Some(age_rating_lt) = query.age_rating_lt {
        series.retain(|row| {
            row.age_rating
                .map(|rating| rating < age_rating_lt)
                .unwrap_or(false)
        });
    }

    if let Some(sharing_labels) = query.sharing_labels.as_ref() {
        series.retain(|row| {
            row.labels.iter().any(|label| {
                let normalized = label.to_ascii_lowercase();
                sharing_labels
                    .iter()
                    .any(|value| normalized.contains(value))
            })
        });
    }

    if let Some(sharing_labels_excluded) = query.sharing_labels_excluded.as_ref() {
        series.retain(|row| {
            !row.labels.iter().any(|label| {
                let normalized = label.to_ascii_lowercase();
                sharing_labels_excluded
                    .iter()
                    .any(|value| normalized.contains(value))
            })
        });
    }

    if let Some(sharing_labels_null) = query.sharing_labels_null {
        series.retain(|row| row.labels.is_empty() == sharing_labels_null);
    }

    if let Some(authors) = query.authors.as_ref() {
        series.retain(|row| {
            row.books_metadata_authors.iter().any(|author| {
                let normalized = author.to_ascii_lowercase();
                authors
                    .iter()
                    .any(|value| author_value_matches(&normalized, value))
            })
        });
    }

    if let Some(authors_excluded) = query.authors_excluded.as_ref() {
        series.retain(|row| {
            !row.books_metadata_authors.iter().any(|author| {
                let normalized = author.to_ascii_lowercase();
                authors_excluded
                    .iter()
                    .any(|value| author_value_matches(&normalized, value))
            })
        });
    }

    if let Some(series_statuses) = query.series_statuses.as_ref() {
        series.retain(|row| {
            series_statuses
                .iter()
                .any(|value| value.eq_ignore_ascii_case(&row.status))
        });
    }

    if let Some(series_statuses_excluded) = query.series_statuses_excluded.as_ref() {
        series.retain(|row| {
            !series_statuses_excluded
                .iter()
                .any(|value| value.eq_ignore_ascii_case(&row.status))
        });
    }

    if let Some(release_dates) = query.release_dates.as_ref() {
        series.retain(|row| {
            let Some(release_date) = row.books_metadata_release_date.as_ref() else {
                return false;
            };
            release_dates.iter().any(|value| value == release_date)
        });
    }

    if let Some(release_dates_excluded) = query.release_dates_excluded.as_ref() {
        series.retain(|row| {
            let Some(release_date) = row.books_metadata_release_date.as_ref() else {
                return true;
            };
            !release_dates_excluded
                .iter()
                .any(|value| value == release_date)
        });
    }

    if let Some(release_dates_null) = query.release_dates_null {
        series.retain(|row| row.books_metadata_release_date.is_none() == release_dates_null);
    }

    if let Some(release_date_gt) = query.release_date_gt.as_ref() {
        series.retain(|row| {
            let Some(release_date) = row.books_metadata_release_date.as_ref() else {
                return false;
            };
            release_date > release_date_gt
        });
    }

    if let Some(release_date_lt) = query.release_date_lt.as_ref() {
        series.retain(|row| {
            let Some(release_date) = row.books_metadata_release_date.as_ref() else {
                return false;
            };
            release_date < release_date_lt
        });
    }

    if let Some(release_date_in_last_days) = query.release_date_in_last_days
        && let Some(cutoff) =
            persisted_utc_date_minus_days(database_file, release_date_in_last_days).await?
    {
        series.retain(|row| {
            let Some(release_date) = row.books_metadata_release_date.as_ref() else {
                return false;
            };
            release_date > &cutoff
        });
    }

    if let Some(release_date_not_in_last_days) = query.release_date_not_in_last_days
        && let Some(cutoff) =
            persisted_utc_date_minus_days(database_file, release_date_not_in_last_days).await?
    {
        series.retain(|row| {
            let Some(release_date) = row.books_metadata_release_date.as_ref() else {
                return false;
            };
            release_date < &cutoff
        });
    }

    if let Some(release_date_begins_with) = query.release_date_begins_with.as_ref() {
        series.retain(|row| {
            let Some(release_date) = row.books_metadata_release_date.as_ref() else {
                return false;
            };
            let normalized = release_date.to_ascii_lowercase();
            release_date_begins_with
                .iter()
                .any(|value| normalized.starts_with(value))
        });
    }

    if let Some(release_date_ends_with) = query.release_date_ends_with.as_ref() {
        series.retain(|row| {
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
        query.release_date_begins_with_excluded.as_ref()
    {
        series.retain(|row| {
            let Some(release_date) = row.books_metadata_release_date.as_ref() else {
                return true;
            };
            let normalized = release_date.to_ascii_lowercase();
            !release_date_begins_with_excluded
                .iter()
                .any(|value| normalized.starts_with(value))
        });
    }

    if let Some(release_date_ends_with_excluded) = query.release_date_ends_with_excluded.as_ref() {
        series.retain(|row| {
            let Some(release_date) = row.books_metadata_release_date.as_ref() else {
                return true;
            };
            let normalized = release_date.to_ascii_lowercase();
            !release_date_ends_with_excluded
                .iter()
                .any(|value| normalized.ends_with(value))
        });
    }

    if let Some(release_date_contains_excluded) = query.release_date_contains_excluded.as_ref() {
        series.retain(|row| {
            let Some(release_date) = row.books_metadata_release_date.as_ref() else {
                return true;
            };
            let normalized = release_date.to_ascii_lowercase();
            !release_date_contains_excluded
                .iter()
                .any(|value| normalized.contains(value))
        });
    }

    if let Some(collection_ids) = query.collection_ids.as_ref() {
        let memberships = load_collection_memberships(database_file).await?;
        series.retain(|row| {
            memberships
                .get(&row.id)
                .into_iter()
                .flatten()
                .any(|collection_id| collection_ids.iter().any(|id| id == collection_id))
        });
    }

    if let Some(search) = query.search.as_ref() {
        let normalized = search.trim().to_ascii_lowercase();
        if !normalized.is_empty() {
            series.retain(|row| {
                row.title.to_ascii_lowercase().contains(&normalized)
                    || row.title_sort.to_ascii_lowercase().contains(&normalized)
            });
        }
    }

    if let Some((pattern, field)) = query.search_regex.as_ref() {
        series.retain(|row| {
            let candidate = if field == "title_sort" {
                row.title_sort.as_str()
            } else {
                row.title.as_str()
            };
            matches_search_pattern(candidate, pattern)
        });
    }

    series.sort_by(|left, right| {
        for sort_mode in &query.sort_modes {
            let ordering = match sort_mode {
                PersistedSeriesSortMode::TitleAsc => left
                    .title_sort
                    .to_ascii_lowercase()
                    .cmp(&right.title_sort.to_ascii_lowercase()),
                PersistedSeriesSortMode::Latest => right.last_modified.cmp(&left.last_modified),
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
