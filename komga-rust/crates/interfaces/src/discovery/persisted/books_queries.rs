#![allow(clippy::too_many_arguments)]

use crate::state::PersistedDiscoveryService;
use std::path::Path;

use super::common_helpers::{
    TextMatchMode, any_ignore_ascii_case, any_normalized_text_matches, matches_optional_value,
    normalize_unpaged_page_size, normalized_text_matches, runtime_list_request,
};
use super::*;
use crate::discovery::filters::{
    OperatorValidationMode, exact_oneshot_bootstrap_series_id, parse_runtime_books_filters,
    parse_runtime_books_filters_with_mode, restrict_books_filters_to_persisted_shape,
    webui_bridge_books_filters_from_payload,
};
use crate::discovery_auth::state::DiscoveryAuthState;
use komga_application::discovery::{
    BookMetadataAuthorReadModel, BookMetadataLinkReadModel, BookReadProgressReadModel,
};

pub async fn load_book_poster_summaries(
    backend: &dyn PersistedDiscoveryService,
) -> Result<HashMap<String, Vec<PersistedBookPosterSummary>>, String> {
    backend.load_book_poster_summaries().await
}

pub async fn load_persisted_books_page(
    backend: &dyn PersistedDiscoveryService,
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
        let candidate_ids = backend
            .search_book_ids(search.to_string(), total_count.max(1))
            .await?;
        if candidate_ids.is_empty() {
            books.clear();
        } else {
            relevance_ranks = candidate_ids
                .iter()
                .enumerate()
                .map(|(index, id)| (id.clone(), index))
                .collect();
            books = backend
                .load_persisted_book_summaries_by_ids(
                    context.user_id.as_deref().map(str::to_string),
                    candidate_ids,
                )
                .await?;
        }
    } else {
        books = backend
            .load_persisted_book_summaries(context.user_id.as_deref().map(str::to_string))
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
                    PersistedBooksSortMode::ReleaseDateDesc => {
                        right.metadata_release_date.cmp(&left.metadata_release_date)
                    }
                    PersistedBooksSortMode::NumberSortAsc => left
                        .metadata_number_sort
                        .partial_cmp(&right.metadata_number_sort)
                        .unwrap_or(std::cmp::Ordering::Equal),
                    PersistedBooksSortMode::SeriesIdAsc => left.series_id.cmp(&right.series_id),
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
                name: row.title.clone(),
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
        (Some(left), Some(right)) if descending => left.cmp(&right),
        (Some(left), Some(right)) => right.cmp(&left),
        _ => std::cmp::Ordering::Equal,
    }
}

pub async fn runtime_owned_persisted_books_page(
    backend: &dyn PersistedDiscoveryService,
    context: &DiscoveryQueryContext,
    filters: &RuntimeBooksFilters,
    sorts: &[String],
    full_text_search: Option<String>,
    page: usize,
    size: usize,
    unpaged: bool,
) -> Option<Result<PageEnvelope<BookReadModel>, String>> {
    let sort_modes = parse_persisted_books_sort_modes(sorts, full_text_search.as_deref());
    Some(
        load_persisted_books_page(
            backend,
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

pub fn parse_persisted_books_sort_modes(
    sorts: &[String],
    full_text_search: Option<&str>,
) -> Vec<PersistedBooksSortMode> {
    let has_full_text_search = full_text_search
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some();
    let mut modes = sorts
        .iter()
        .filter_map(|sort| match sort.as_str() {
            "series,metadata.numberSort,asc" => Some(PersistedBooksSortMode::NumberSortAsc),
            "metadata.title,asc" | "title,asc" => Some(PersistedBooksSortMode::TitleAsc),
            "createdDate,desc" | "created,desc" => Some(PersistedBooksSortMode::CreatedDateDesc),
            "lastModifiedDate,desc" | "lastModified,desc" => {
                Some(PersistedBooksSortMode::LastModifiedDateDesc)
            }
            "readProgress.lastModified,asc" => {
                Some(PersistedBooksSortMode::ReadProgressLastModifiedDateAsc)
            }
            "readProgress.lastModified,desc" | "readProgress.lastModified" => {
                Some(PersistedBooksSortMode::ReadProgressLastModifiedDateDesc)
            }
            "readProgress.readDate,asc" => Some(PersistedBooksSortMode::ReadProgressReadDateAsc),
            "readProgress.readDate,desc" | "readProgress.readDate" => {
                Some(PersistedBooksSortMode::ReadProgressReadDateDesc)
            }
            "metadata.releaseDate,desc" => Some(PersistedBooksSortMode::ReleaseDateDesc),
            "metadata.numberSort,asc" | "number,asc" => Some(PersistedBooksSortMode::NumberSortAsc),
            "seriesId,asc" => Some(PersistedBooksSortMode::SeriesIdAsc),
            "relevance,asc" if has_full_text_search => Some(PersistedBooksSortMode::RelevanceAsc),
            "relevance,desc" if has_full_text_search => Some(PersistedBooksSortMode::RelevanceDesc),
            _ => None,
        })
        .collect::<Vec<_>>();
    modes.dedup();
    if modes.is_empty() && sorts.is_empty() && has_full_text_search {
        modes.push(PersistedBooksSortMode::RelevanceDesc);
    }
    modes
}

pub async fn runtime_owned_books_list_response(
    backend: &dyn PersistedDiscoveryService,
    headers: &HeaderMap,
    uri: &Uri,
    payload: Option<&Value>,
    full_text_search: Option<String>,
    auth_state: &DiscoveryAuthState,
    database_file: &Path,
    strict_runtime_shape: bool,
) -> Option<Response> {
    let query_string = uri.query().unwrap_or_default();
    let request = runtime_list_request(query_string);
    let sorts = request.sorts;
    let persisted_sort_modes =
        parse_persisted_books_sort_modes(&sorts, full_text_search.as_deref());
    let page = request.page;
    let size = request.size;
    let unpaged = request.unpaged;
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
            backend,
            filters.criteria.library_ids.as_ref(),
        )
        .await;
    }

    let requested_library_ids = requested_library_ids_for_runtime_shape(
        strict_runtime_shape,
        filters.criteria.library_ids.clone(),
    );
    let context = match auth_state
        .resolve_query_context_with_persistence(
            headers,
            requested_library_ids.as_deref(),
            database_file,
        )
        .await
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
        backend,
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
                let sorted = !persisted_sort_modes.is_empty();
                let mut response =
                    Json(books_page_payload(page, is_admin, !unpaged, sorted)).into_response();
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
    backend: &dyn PersistedDiscoveryService,
    headers: &HeaderMap,
    uri: &Uri,
    auth_state: &DiscoveryAuthState,
) -> Option<Response> {
    let query = uri.query().unwrap_or_default();
    let request = runtime_list_request(query);

    let requested_library_ids = requested_query_values(query, "library_id");
    let library_ids =
        remap_requested_library_ids_for_persisted(backend, requested_library_ids.as_ref())
            .await
            .or(requested_library_ids);

    let page = request.page;
    let size = request.size;
    let unpaged = request.unpaged;

    let context = match auth_state.resolve_query_context(headers, library_ids.as_deref()) {
        Some(context) => context,
        None => return Some(StatusCode::UNAUTHORIZED.into_response()),
    };

    match load_persisted_books_page(
        backend,
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
                (normalize_unpaged_page_size(page, 20), true)
            } else {
                (page, true)
            };
            let mut response =
                Json(books_page_payload(page, context.is_admin, paged, true)).into_response();
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
