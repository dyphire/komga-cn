#![allow(clippy::too_many_arguments)]

use super::*;

fn normalize_books_latest_unpaged_page_shape<T>(mut page: PageEnvelope<T>) -> PageEnvelope<T> {
    const KOTLIN_PAGE_SIZE: usize = 20;

    page.page = 0;
    page.size = KOTLIN_PAGE_SIZE;
    page.total_pages = if page.total_elements == 0 {
        0
    } else {
        ((page.total_elements - 1) / KOTLIN_PAGE_SIZE) + 1
    };
    page
}

pub async fn load_book_poster_summaries(
    database_file: &FsPath,
) -> Result<HashMap<String, Vec<PersistedBookPosterSummary>>, String> {
    persisted_backend_load_book_poster_summaries(database_file).await
}

pub async fn load_persisted_books_page(
    database_file: &FsPath,
    context: &DiscoveryQueryContext,
    query: PersistedBooksBrowseQuery,
) -> Result<PageEnvelope<BookReadModel>, String> {
    let mut books = Vec::new();
    let filters = &query.filters;
    let mut relevance_ranks: HashMap<String, usize> = HashMap::new();
    if let Some(search) = query.search.as_ref().map(|value| value.trim())
        && !search.is_empty()
    {
        let total_count = persisted_backend_load_persisted_book_count(database_file).await?;
        let candidate_ids =
            persisted_backend_search_book_ids(database_file, search, total_count.max(1)).await?;
        if candidate_ids.is_empty() {
            books.clear();
        } else {
            relevance_ranks = candidate_ids
                .iter()
                .enumerate()
                .map(|(index, id)| (id.clone(), index))
                .collect();
            books = persisted_backend_load_persisted_book_summaries_by_ids(
                database_file,
                context.user_id.as_deref(),
                &candidate_ids,
            )
            .await?;
        }
    } else {
        books = load_persisted_book_summaries(database_file, context.user_id.as_deref()).await?;
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
        && let (Some(age), Some(crate::http::discovery_auth::AgeRestrictionKind::Exclude)) =
            (restrictions.age, restrictions.age_restriction)
    {
        books = filter_rows(books, |row| {
            row.age_rating
                .map(|age_rating| age_rating < age)
                .unwrap_or(true)
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
            Some(load_readlist_memberships(database_file).await?)
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
            let normalized = row.title.to_ascii_lowercase();
            titles.contains(&normalized)
        });
    }

    if let Some(titles_excluded) = filters.titles_excluded.as_ref() {
        books = filter_rows(books, |row| {
            let normalized = row.title.to_ascii_lowercase();
            !titles_excluded.contains(&normalized)
        });
    }

    if let Some(titles_contains) = filters.titles_contains.as_ref() {
        books = filter_rows(books, |row| {
            let normalized = row.title.to_ascii_lowercase();
            titles_contains
                .iter()
                .any(|value| normalized.contains(value))
        });
    }

    if let Some(titles_contains_excluded) = filters.titles_contains_excluded.as_ref() {
        books = filter_rows(books, |row| {
            let normalized = row.title.to_ascii_lowercase();
            !titles_contains_excluded
                .iter()
                .any(|value| normalized.contains(value))
        });
    }

    if let Some(titles_begins_with) = filters.titles_begins_with.as_ref() {
        books = filter_rows(books, |row| {
            let normalized = row.title.to_ascii_lowercase();
            titles_begins_with
                .iter()
                .any(|value| normalized.starts_with(value))
        });
    }

    if let Some(titles_begins_with_excluded) = filters.titles_begins_with_excluded.as_ref() {
        books = filter_rows(books, |row| {
            let normalized = row.title.to_ascii_lowercase();
            !titles_begins_with_excluded
                .iter()
                .any(|value| normalized.starts_with(value))
        });
    }

    if let Some(titles_ends_with) = filters.titles_ends_with.as_ref() {
        books = filter_rows(books, |row| {
            let normalized = row.title.to_ascii_lowercase();
            titles_ends_with
                .iter()
                .any(|value| normalized.ends_with(value))
        });
    }

    if let Some(titles_ends_with_excluded) = filters.titles_ends_with_excluded.as_ref() {
        books = filter_rows(books, |row| {
            let normalized = row.title.to_ascii_lowercase();
            !titles_ends_with_excluded
                .iter()
                .any(|value| normalized.ends_with(value))
        });
    }

    if let Some(tags) = filters.tags.as_ref() {
        books = filter_rows(books, |row| {
            row.metadata_tags.iter().any(|tag| {
                let normalized = tag.to_ascii_lowercase();
                tags.contains(&normalized)
            })
        });
    }

    if let Some(tags_excluded) = filters.tags_excluded.as_ref() {
        books = filter_rows(books, |row| {
            !row.metadata_tags.iter().any(|tag| {
                let normalized = tag.to_ascii_lowercase();
                tags_excluded.contains(&normalized)
            })
        });
    }

    if let Some(tags_null) = filters.tags_null {
        books = filter_rows(books, |row| row.metadata_tags.is_empty() == tags_null);
    }

    if let Some(genres) = filters.genres.as_ref() {
        books = filter_rows(books, |row| {
            row.genres.iter().any(|genre| {
                let normalized = genre.to_ascii_lowercase();
                genres.contains(&normalized)
            })
        });
    }

    if let Some(genres_excluded) = filters.genres_excluded.as_ref() {
        books = filter_rows(books, |row| {
            !row.genres.iter().any(|genre| {
                let normalized = genre.to_ascii_lowercase();
                genres_excluded.contains(&normalized)
            })
        });
    }

    if let Some(genres_null) = filters.genres_null {
        books = filter_rows(books, |row| row.genres.is_empty() == genres_null);
    }

    if let Some(languages) = filters.languages.as_ref() {
        books = filter_rows(books, |row| {
            row.language.as_ref().is_some_and(|language| {
                languages
                    .iter()
                    .any(|value| language.eq_ignore_ascii_case(value))
            })
        });
    }

    if let Some(languages_excluded) = filters.languages_excluded.as_ref() {
        books = filter_rows(books, |row| {
            !row.language.as_ref().is_some_and(|language| {
                languages_excluded
                    .iter()
                    .any(|value| language.eq_ignore_ascii_case(value))
            })
        });
    }

    if let Some(publishers) = filters.publishers.as_ref() {
        books = filter_rows(books, |row| {
            row.publisher.as_ref().is_some_and(|publisher| {
                publishers
                    .iter()
                    .any(|value| publisher.eq_ignore_ascii_case(value))
            })
        });
    }

    if let Some(publishers_excluded) = filters.publishers_excluded.as_ref() {
        books = filter_rows(books, |row| {
            !row.publisher.as_ref().is_some_and(|publisher| {
                publishers_excluded
                    .iter()
                    .any(|value| publisher.eq_ignore_ascii_case(value))
            })
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
            row.metadata_authors.iter().any(|author| {
                let normalized = author.to_ascii_lowercase();
                authors
                    .iter()
                    .any(|value| author_value_matches(&normalized, value))
            })
        });
    }

    if let Some(authors_excluded) = filters.authors_excluded.as_ref() {
        books = filter_rows(books, |row| {
            !row.metadata_authors.iter().any(|author| {
                let normalized = author.to_ascii_lowercase();
                authors_excluded
                    .iter()
                    .any(|value| author_value_matches(&normalized, value))
            })
        });
    }

    if filters.poster_types.is_some()
        || filters.poster_types_excluded.is_some()
        || filters.poster_selected.is_some()
        || filters.poster_selected_excluded.is_some()
    {
        let posters = load_book_poster_summaries(database_file).await?;

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
            media_profiles
                .iter()
                .any(|value| value.eq_ignore_ascii_case(profile))
        });
    }

    if let Some(media_profiles_excluded) = filters.media_profiles_excluded.as_ref() {
        books = filter_rows(books, |row| {
            let profile = media_profile_for_media_type(&row.media_type);
            !media_profiles_excluded
                .iter()
                .any(|value| value.eq_ignore_ascii_case(profile))
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
            row.metadata_number_sort
                .map(|number_sort| {
                    number_sorts
                        .iter()
                        .any(|value| (number_sort - *value).abs() <= f64::EPSILON)
                })
                .unwrap_or(false)
        });
    }

    if let Some(number_sorts_excluded) = filters.number_sorts_excluded.as_ref() {
        books = filter_rows(books, |row| {
            row.metadata_number_sort
                .map(|number_sort| {
                    !number_sorts_excluded
                        .iter()
                        .any(|value| (number_sort - *value).abs() <= f64::EPSILON)
                })
                .unwrap_or(false)
        });
    }

    if let Some(number_sort_gt) = filters.number_sort_gt {
        books = filter_rows(books, |row| {
            row.metadata_number_sort
                .map(|number_sort| number_sort > number_sort_gt)
                .unwrap_or(false)
        });
    }

    if let Some(number_sort_lt) = filters.number_sort_lt {
        books = filter_rows(books, |row| {
            row.metadata_number_sort
                .map(|number_sort| number_sort < number_sort_lt)
                .unwrap_or(false)
        });
    }

    if let Some(media_statuses) = filters.media_statuses.as_ref() {
        books = filter_rows(books, |row| {
            media_statuses
                .iter()
                .any(|value| row.media_status.to_ascii_lowercase().starts_with(value))
        });
    }

    if let Some(media_statuses_excluded) = filters.media_statuses_excluded.as_ref() {
        books = filter_rows(books, |row| {
            !media_statuses_excluded
                .iter()
                .any(|value| value.eq_ignore_ascii_case(&row.media_status))
        });
    }

    if let Some(read_statuses) = filters.read_statuses.as_ref() {
        if context.user_id.is_none() {
            books.clear();
        } else {
            books = filter_rows(books, |row| {
                read_statuses
                    .iter()
                    .any(|value| value.eq_ignore_ascii_case(&row.read_status))
            });
        }
    }

    if let Some(read_statuses_excluded) = filters.read_statuses_excluded.as_ref() {
        if context.user_id.is_none() {
            books.clear();
        } else {
            books = filter_rows(books, |row| {
                !read_statuses_excluded
                    .iter()
                    .any(|value| value.eq_ignore_ascii_case(&row.read_status))
            });
        }
    }

    if let Some(release_dates) = filters.release_dates.as_ref() {
        books = filter_rows(books, |row| {
            let Some(release_date) = row.metadata_release_date.as_ref() else {
                return false;
            };
            release_dates.iter().any(|value| value == release_date)
        });
    }

    if let Some(release_dates_excluded) = filters.release_dates_excluded.as_ref() {
        books = filter_rows(books, |row| {
            let Some(release_date) = row.metadata_release_date.as_ref() else {
                return true;
            };
            !release_dates_excluded
                .iter()
                .any(|value| value == release_date)
        });
    }

    if let Some(release_dates_null) = filters.release_dates_null {
        books = filter_rows(books, |row| {
            row.metadata_release_date.is_none() == release_dates_null
        });
    }

    if let Some(release_date_gt) = filters.release_date_gt.as_ref() {
        books = filter_rows(books, |row| {
            let Some(release_date) = row.metadata_release_date.as_ref() else {
                return false;
            };
            release_date > release_date_gt
        });
    }

    if let Some(release_date_lt) = filters.release_date_lt.as_ref() {
        books = filter_rows(books, |row| {
            let Some(release_date) = row.metadata_release_date.as_ref() else {
                return false;
            };
            release_date < release_date_lt
        });
    }

    if let Some(release_date_in_last_days) = filters.release_date_in_last_days
        && let Some(cutoff) =
            persisted_utc_date_minus_days(database_file, release_date_in_last_days).await?
    {
        books = filter_rows(books, |row| {
            let Some(release_date) = row.metadata_release_date.as_ref() else {
                return false;
            };
            release_date > &cutoff
        });
    }

    if let Some(release_date_not_in_last_days) = filters.release_date_not_in_last_days
        && let Some(cutoff) =
            persisted_utc_date_minus_days(database_file, release_date_not_in_last_days).await?
    {
        books = filter_rows(books, |row| {
            let Some(release_date) = row.metadata_release_date.as_ref() else {
                return false;
            };
            release_date < &cutoff
        });
    }

    if let Some(release_date_begins_with) = filters.release_date_begins_with.as_ref() {
        books = filter_rows(books, |row| {
            let Some(release_date) = row.metadata_release_date.as_ref() else {
                return false;
            };
            let normalized = release_date.to_ascii_lowercase();
            release_date_begins_with
                .iter()
                .any(|value| normalized.starts_with(value))
        });
    }

    if let Some(release_date_ends_with) = filters.release_date_ends_with.as_ref() {
        books = filter_rows(books, |row| {
            let Some(release_date) = row.metadata_release_date.as_ref() else {
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
        books = filter_rows(books, |row| {
            let Some(release_date) = row.metadata_release_date.as_ref() else {
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
        books = filter_rows(books, |row| {
            let Some(release_date) = row.metadata_release_date.as_ref() else {
                return true;
            };
            let normalized = release_date.to_ascii_lowercase();
            !release_date_ends_with_excluded
                .iter()
                .any(|value| normalized.ends_with(value))
        });
    }

    if let Some(release_date_contains_excluded) = filters.release_date_contains_excluded.as_ref() {
        books = filter_rows(books, |row| {
            let Some(release_date) = row.metadata_release_date.as_ref() else {
                return true;
            };
            let normalized = release_date.to_ascii_lowercase();
            !release_date_contains_excluded
                .iter()
                .any(|value| normalized.contains(value))
        });
    }

    books.sort_by(|left, right| {
        for sort_mode in &query.sort_modes {
            let ordering = match sort_mode {
                PersistedBooksSortMode::TitleAsc => left
                    .title
                    .to_ascii_lowercase()
                    .cmp(&right.title.to_ascii_lowercase()),
                PersistedBooksSortMode::CreatedDateDesc => right.created.cmp(&left.created),
                PersistedBooksSortMode::LastModifiedDateDesc => {
                    right.last_modified.cmp(&left.last_modified)
                }
                PersistedBooksSortMode::ReleaseDateDesc => {
                    right.metadata_release_date.cmp(&left.metadata_release_date)
                }
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
                name: row.title,
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
        (Some(left), Some(right)) if descending => left.cmp(&right),
        (Some(left), Some(right)) => right.cmp(&left),
        _ => std::cmp::Ordering::Equal,
    }
}

pub async fn load_persisted_book_summaries(
    database_file: &FsPath,
    user_id: Option<&str>,
) -> Result<Vec<PersistedBookSummary>, String> {
    persisted_backend_load_persisted_book_summaries(database_file, user_id).await
}

pub async fn runtime_owned_persisted_books_page(
    database_file: &FsPath,
    context: &DiscoveryQueryContext,
    filters: &RuntimeBooksFilters,
    sorts: &[String],
    full_text_search: Option<String>,
    page: usize,
    size: usize,
    unpaged: bool,
) -> Option<Result<PageEnvelope<BookReadModel>, String>> {
    if !database_file.exists() {
        return None;
    }

    let sort_modes = parse_persisted_books_sort_modes(sorts);
    let has_persisted_rows = match persisted_books_exist(database_file).await {
        Ok(has_rows) => has_rows,
        Err(error) => return Some(Err(error)),
    };
    if !has_persisted_rows {
        return None;
    }

    Some(
        load_persisted_books_page(
            database_file,
            context,
            PersistedBooksBrowseQuery::from_runtime_filters(
                filters,
                full_text_search,
                page,
                size,
                unpaged,
                sort_modes,
            ),
        )
        .await,
    )
}

pub fn parse_persisted_books_sort_modes(sorts: &[String]) -> Vec<PersistedBooksSortMode> {
    let mut modes = sorts
        .iter()
        .filter_map(|sort| match sort.as_str() {
            "metadata.title,asc" | "series,metadata.numberSort,asc" => {
                Some(PersistedBooksSortMode::TitleAsc)
            }
            "createdDate,desc" => Some(PersistedBooksSortMode::CreatedDateDesc),
            "lastModifiedDate,desc" => Some(PersistedBooksSortMode::LastModifiedDateDesc),
            "metadata.releaseDate,desc" => Some(PersistedBooksSortMode::ReleaseDateDesc),
            "relevance,asc" => Some(PersistedBooksSortMode::RelevanceAsc),
            "relevance,desc" => Some(PersistedBooksSortMode::RelevanceDesc),
            _ => None,
        })
        .collect::<Vec<_>>();
    modes.dedup();
    if modes.is_empty() {
        modes.push(PersistedBooksSortMode::TitleAsc);
    }
    modes
}

async fn persisted_books_exist(database_file: &FsPath) -> Result<bool, String> {
    persisted_backend_persisted_books_exist(database_file).await
}

pub async fn runtime_owned_books_list_response(
    headers: &HeaderMap,
    uri: &Uri,
    payload: Option<&Value>,
    full_text_search: Option<String>,
    auth_state: &DiscoveryAuthState,
    database_file: &FsPath,
    strict_runtime_shape: bool,
) -> Option<Response> {
    let query_string = uri.query().unwrap_or_default();
    let sorts = query_values(query_string, "sort")
        .into_iter()
        .map(decode_query_component)
        .collect::<Vec<_>>();
    let page = query_value(query_string, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query_string, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);
    let unpaged = query_bool(query_string, "unpaged");
    let mut oneshot_bootstrap_series_id = exact_oneshot_bootstrap_series_id(payload);

    if reject_bootstrap_shape_mismatch(
        strict_runtime_shape,
        oneshot_bootstrap_series_id.is_some(),
        !query_string.trim().is_empty(),
    ) {
        return None;
    }

    oneshot_bootstrap_series_id =
        bootstrap_series_id_for_runtime_shape(strict_runtime_shape, oneshot_bootstrap_series_id);

    let validation_mode = OperatorValidationMode::from(query_validation_mode(strict_runtime_shape));

    let mut filters = match if validation_mode.is_strict() {
        parse_runtime_books_filters_with_mode(
            payload.and_then(|value| value.get("condition")),
            validation_mode,
        )
    } else {
        parse_runtime_books_filters(payload.and_then(|value| value.get("condition")))
    } {
        Ok(filters) => filters,
        Err(error) => {
            if strict_runtime_shape {
                return Some(invalid_runtime_books_list_response(error));
            } else {
                webui_bridge_books_filters_from_payload(payload)
            }
        }
    };

    if !strict_runtime_shape {
        restrict_books_filters_to_persisted_shape(&mut filters);
        filters.criteria.library_ids = remap_requested_library_ids_for_persisted(
            database_file,
            filters.criteria.library_ids.as_ref(),
        )
        .await;
    }

    let requested_library_ids = requested_library_ids_for_runtime_shape(
        strict_runtime_shape,
        filters.criteria.library_ids.clone(),
    );
    let context = match auth_state.resolve_query_context(headers, requested_library_ids.as_deref())
    {
        Some(context) => context,
        None => return Some(StatusCode::UNAUTHORIZED.into_response()),
    };

    if let Some(series_id) = oneshot_bootstrap_series_id.clone() {
        filters.direct_browse_family = Some(DirectBrowseBooksListFamily::BrowseOneshotBootstrap);
        filters.criteria.series_ids = Some(vec![series_id]);
    }

    let is_admin = context.is_admin;

    if let Some(persisted_page) = runtime_owned_persisted_books_page(
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
                let mut response =
                    Json(books_page_payload(page, is_admin, !unpaged)).into_response();
                mark_runtime_owned(&mut response);
                return Some(response);
            }
            Err(error) => {
                return Some(
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "error": format!("runtime books list failed: {error}") })),
                    )
                        .into_response(),
                );
            }
        }
    }

    None
}

pub async fn runtime_owned_books_latest_response(
    headers: &HeaderMap,
    uri: &Uri,
    auth_state: &DiscoveryAuthState,
    database_file: &FsPath,
) -> Option<Response> {
    let query = uri.query().unwrap_or_default();

    if !database_file.exists() {
        return None;
    }

    let requested_library_ids = requested_query_values(query, "library_id");
    let library_ids =
        remap_requested_library_ids_for_persisted(database_file, requested_library_ids.as_ref())
            .await
            .or(requested_library_ids);

    let page = query_value(query, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);
    let unpaged = query_bool(query, "unpaged");

    let context = match auth_state.resolve_query_context(headers, library_ids.as_deref()) {
        Some(context) => context,
        None => return Some(StatusCode::UNAUTHORIZED.into_response()),
    };

    match load_persisted_books_page(
        database_file,
        &context,
        PersistedBooksBrowseQuery::from_filters(
            BooksFilterCriteria {
                library_ids,
                ..BooksFilterCriteria::default()
            },
            None,
            page,
            size,
            unpaged,
            vec![PersistedBooksSortMode::LastModifiedDateDesc],
        ),
    )
    .await
    {
        Ok(page) => {
            let (page, paged) = if unpaged {
                (normalize_books_latest_unpaged_page_shape(page), true)
            } else {
                (page, true)
            };
            let mut response =
                Json(books_page_payload(page, context.is_admin, paged)).into_response();
            mark_runtime_owned(&mut response);
            Some(response)
        }
        Err(error) => Some(
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("runtime books latest failed: {error}") })),
            )
                .into_response(),
        ),
    }
}
