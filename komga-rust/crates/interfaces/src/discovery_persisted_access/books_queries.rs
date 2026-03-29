use super::*;

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
    let mut books =
        load_persisted_book_summaries(database_file, context.user_id.as_deref()).await?;

    if let Some(allowed_ids) = context.authorized_library_ids.as_ref() {
        books.retain(|row| allowed_ids.iter().any(|id| id == row.library_id.as_str()));
    }
    if let Some(library_ids) = query.library_ids.as_ref() {
        books.retain(|row| library_ids.iter().any(|id| id == row.library_id.as_str()));
    }

    if let Some(series_ids) = query.series_ids.as_ref() {
        books.retain(|row| series_ids.iter().any(|id| id == row.series_id.as_str()));
    }

    if let Some(series_ids_excluded) = query.series_ids_excluded.as_ref() {
        books.retain(|row| {
            !series_ids_excluded
                .iter()
                .any(|id| id == row.series_id.as_str())
        });
    }

    if let Some(read_list_ids) = query.read_list_ids.as_ref() {
        let memberships = load_readlist_memberships(database_file).await?;
        books.retain(|row| {
            memberships
                .get(&row.id)
                .into_iter()
                .flatten()
                .any(|read_list_id| read_list_ids.iter().any(|id| id == read_list_id))
        });
    }

    if let Some(read_list_ids_excluded) = query.read_list_ids_excluded.as_ref() {
        let memberships = load_readlist_memberships(database_file).await?;
        books.retain(|row| {
            !memberships
                .get(&row.id)
                .into_iter()
                .flatten()
                .any(|read_list_id| read_list_ids_excluded.iter().any(|id| id == read_list_id))
        });
    }

    if let Some(titles) = query.titles.as_ref() {
        books.retain(|row| {
            let normalized = row.title.to_ascii_lowercase();
            titles.iter().any(|value| normalized == *value)
        });
    }

    if let Some(titles_excluded) = query.titles_excluded.as_ref() {
        books.retain(|row| {
            let normalized = row.title.to_ascii_lowercase();
            !titles_excluded.iter().any(|value| normalized == *value)
        });
    }

    if let Some(titles_contains) = query.titles_contains.as_ref() {
        books.retain(|row| {
            let normalized = row.title.to_ascii_lowercase();
            titles_contains
                .iter()
                .any(|value| normalized.contains(value))
        });
    }

    if let Some(titles_contains_excluded) = query.titles_contains_excluded.as_ref() {
        books.retain(|row| {
            let normalized = row.title.to_ascii_lowercase();
            !titles_contains_excluded
                .iter()
                .any(|value| normalized.contains(value))
        });
    }

    if let Some(titles_begins_with) = query.titles_begins_with.as_ref() {
        books.retain(|row| {
            let normalized = row.title.to_ascii_lowercase();
            titles_begins_with
                .iter()
                .any(|value| normalized.starts_with(value))
        });
    }

    if let Some(titles_begins_with_excluded) = query.titles_begins_with_excluded.as_ref() {
        books.retain(|row| {
            let normalized = row.title.to_ascii_lowercase();
            !titles_begins_with_excluded
                .iter()
                .any(|value| normalized.starts_with(value))
        });
    }

    if let Some(titles_ends_with) = query.titles_ends_with.as_ref() {
        books.retain(|row| {
            let normalized = row.title.to_ascii_lowercase();
            titles_ends_with
                .iter()
                .any(|value| normalized.ends_with(value))
        });
    }

    if let Some(titles_ends_with_excluded) = query.titles_ends_with_excluded.as_ref() {
        books.retain(|row| {
            let normalized = row.title.to_ascii_lowercase();
            !titles_ends_with_excluded
                .iter()
                .any(|value| normalized.ends_with(value))
        });
    }

    if let Some(tags) = query.tags.as_ref() {
        books.retain(|row| {
            row.metadata_tags.iter().any(|tag| {
                let normalized = tag.to_ascii_lowercase();
                tags.iter().any(|value| normalized == *value)
            })
        });
    }

    if let Some(tags_excluded) = query.tags_excluded.as_ref() {
        books.retain(|row| {
            !row.metadata_tags.iter().any(|tag| {
                let normalized = tag.to_ascii_lowercase();
                tags_excluded.iter().any(|value| normalized == *value)
            })
        });
    }

    if let Some(tags_null) = query.tags_null {
        books.retain(|row| row.metadata_tags.is_empty() == tags_null);
    }

    if let Some(authors) = query.authors.as_ref() {
        books.retain(|row| {
            row.metadata_authors.iter().any(|author| {
                let normalized = author.to_ascii_lowercase();
                authors
                    .iter()
                    .any(|value| author_value_matches(&normalized, value))
            })
        });
    }

    if let Some(authors_excluded) = query.authors_excluded.as_ref() {
        books.retain(|row| {
            !row.metadata_authors.iter().any(|author| {
                let normalized = author.to_ascii_lowercase();
                authors_excluded
                    .iter()
                    .any(|value| author_value_matches(&normalized, value))
            })
        });
    }

    if query.poster_types.is_some()
        || query.poster_types_excluded.is_some()
        || query.poster_selected.is_some()
        || query.poster_selected_excluded.is_some()
    {
        let posters = load_book_poster_summaries(database_file).await?;

        if query.poster_types.is_some() || query.poster_selected.is_some() {
            books.retain(|row| {
                posters.get(&row.id).into_iter().flatten().any(|poster| {
                    poster_matches(poster, query.poster_types.as_ref(), query.poster_selected)
                })
            });
        }

        if query.poster_types_excluded.is_some() || query.poster_selected_excluded.is_some() {
            books.retain(|row| {
                !posters.get(&row.id).into_iter().flatten().any(|poster| {
                    poster_matches(
                        poster,
                        query.poster_types_excluded.as_ref(),
                        query.poster_selected_excluded,
                    )
                })
            });
        }
    }

    if let Some(media_profiles) = query.media_profiles.as_ref() {
        books.retain(|row| {
            let profile = media_profile_for_media_type(&row.media_type);
            media_profiles
                .iter()
                .any(|value| value.eq_ignore_ascii_case(profile))
        });
    }

    if let Some(media_profiles_excluded) = query.media_profiles_excluded.as_ref() {
        books.retain(|row| {
            let profile = media_profile_for_media_type(&row.media_type);
            !media_profiles_excluded
                .iter()
                .any(|value| value.eq_ignore_ascii_case(profile))
        });
    }

    if let Some(deleted) = query.deleted {
        books.retain(|row| row.deleted == deleted);
    }

    if let Some(oneshot) = query.oneshot {
        books.retain(|row| row.oneshot == oneshot);
    }

    if let Some(number_sorts) = query.number_sorts.as_ref() {
        books.retain(|row| {
            row.metadata_number_sort
                .map(|number_sort| {
                    number_sorts
                        .iter()
                        .any(|value| (number_sort - *value).abs() <= f64::EPSILON)
                })
                .unwrap_or(false)
        });
    }

    if let Some(number_sorts_excluded) = query.number_sorts_excluded.as_ref() {
        books.retain(|row| {
            row.metadata_number_sort
                .map(|number_sort| {
                    !number_sorts_excluded
                        .iter()
                        .any(|value| (number_sort - *value).abs() <= f64::EPSILON)
                })
                .unwrap_or(false)
        });
    }

    if let Some(number_sort_gt) = query.number_sort_gt {
        books.retain(|row| {
            row.metadata_number_sort
                .map(|number_sort| number_sort > number_sort_gt)
                .unwrap_or(false)
        });
    }

    if let Some(number_sort_lt) = query.number_sort_lt {
        books.retain(|row| {
            row.metadata_number_sort
                .map(|number_sort| number_sort < number_sort_lt)
                .unwrap_or(false)
        });
    }

    if let Some(media_statuses) = query.media_statuses.as_ref() {
        books.retain(|row| {
            media_statuses
                .iter()
                .any(|value| value.eq_ignore_ascii_case(&row.media_status))
        });
    }

    if let Some(media_statuses_excluded) = query.media_statuses_excluded.as_ref() {
        books.retain(|row| {
            !media_statuses_excluded
                .iter()
                .any(|value| value.eq_ignore_ascii_case(&row.media_status))
        });
    }

    if let Some(read_statuses) = query.read_statuses.as_ref() {
        if context.user_id.is_none() {
            books.clear();
        } else {
            books.retain(|row| {
                read_statuses
                    .iter()
                    .any(|value| value.eq_ignore_ascii_case(&row.read_status))
            });
        }
    }

    if let Some(read_statuses_excluded) = query.read_statuses_excluded.as_ref() {
        if context.user_id.is_none() {
            books.clear();
        } else {
            books.retain(|row| {
                !read_statuses_excluded
                    .iter()
                    .any(|value| value.eq_ignore_ascii_case(&row.read_status))
            });
        }
    }

    if let Some(release_dates) = query.release_dates.as_ref() {
        books.retain(|row| {
            let Some(release_date) = row.metadata_release_date.as_ref() else {
                return false;
            };
            release_dates.iter().any(|value| value == release_date)
        });
    }

    if let Some(release_dates_excluded) = query.release_dates_excluded.as_ref() {
        books.retain(|row| {
            let Some(release_date) = row.metadata_release_date.as_ref() else {
                return true;
            };
            !release_dates_excluded
                .iter()
                .any(|value| value == release_date)
        });
    }

    if let Some(release_dates_null) = query.release_dates_null {
        books.retain(|row| row.metadata_release_date.is_none() == release_dates_null);
    }

    if let Some(release_date_gt) = query.release_date_gt.as_ref() {
        books.retain(|row| {
            let Some(release_date) = row.metadata_release_date.as_ref() else {
                return false;
            };
            release_date > release_date_gt
        });
    }

    if let Some(release_date_lt) = query.release_date_lt.as_ref() {
        books.retain(|row| {
            let Some(release_date) = row.metadata_release_date.as_ref() else {
                return false;
            };
            release_date < release_date_lt
        });
    }

    if let Some(release_date_in_last_days) = query.release_date_in_last_days
        && let Some(cutoff) =
            persisted_utc_date_minus_days(database_file, release_date_in_last_days).await?
    {
        books.retain(|row| {
            let Some(release_date) = row.metadata_release_date.as_ref() else {
                return false;
            };
            release_date > &cutoff
        });
    }

    if let Some(release_date_not_in_last_days) = query.release_date_not_in_last_days
        && let Some(cutoff) =
            persisted_utc_date_minus_days(database_file, release_date_not_in_last_days).await?
    {
        books.retain(|row| {
            let Some(release_date) = row.metadata_release_date.as_ref() else {
                return false;
            };
            release_date < &cutoff
        });
    }

    if let Some(release_date_begins_with) = query.release_date_begins_with.as_ref() {
        books.retain(|row| {
            let Some(release_date) = row.metadata_release_date.as_ref() else {
                return false;
            };
            let normalized = release_date.to_ascii_lowercase();
            release_date_begins_with
                .iter()
                .any(|value| normalized.starts_with(value))
        });
    }

    if let Some(release_date_ends_with) = query.release_date_ends_with.as_ref() {
        books.retain(|row| {
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
        query.release_date_begins_with_excluded.as_ref()
    {
        books.retain(|row| {
            let Some(release_date) = row.metadata_release_date.as_ref() else {
                return true;
            };
            let normalized = release_date.to_ascii_lowercase();
            !release_date_begins_with_excluded
                .iter()
                .any(|value| normalized.starts_with(value))
        });
    }

    if let Some(release_date_ends_with_excluded) = query.release_date_ends_with_excluded.as_ref() {
        books.retain(|row| {
            let Some(release_date) = row.metadata_release_date.as_ref() else {
                return true;
            };
            let normalized = release_date.to_ascii_lowercase();
            !release_date_ends_with_excluded
                .iter()
                .any(|value| normalized.ends_with(value))
        });
    }

    if let Some(release_date_contains_excluded) = query.release_date_contains_excluded.as_ref() {
        books.retain(|row| {
            let Some(release_date) = row.metadata_release_date.as_ref() else {
                return true;
            };
            let normalized = release_date.to_ascii_lowercase();
            !release_date_contains_excluded
                .iter()
                .any(|value| normalized.contains(value))
        });
    }

    if let Some(search) = query.search.as_ref() {
        let normalized = search.trim().to_ascii_lowercase();
        if !normalized.is_empty() {
            books.retain(|row| row.title.to_ascii_lowercase().contains(&normalized));
        }
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
    if !database_file.exists() || !runtime_books_filters_match_runtime_shape(filters) {
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
            PersistedBooksBrowseQuery {
                library_ids: filters.library_ids.clone(),
                series_ids: filters.series_ids.clone(),
                series_ids_excluded: filters.series_ids_excluded.clone(),
                read_list_ids: filters.read_list_ids.clone(),
                read_list_ids_excluded: filters.read_list_ids_excluded.clone(),
                titles: filters.titles.clone(),
                titles_excluded: filters.titles_excluded.clone(),
                titles_contains: filters.titles_contains.clone(),
                titles_contains_excluded: filters.titles_contains_excluded.clone(),
                titles_begins_with: filters.titles_begins_with.clone(),
                titles_begins_with_excluded: filters.titles_begins_with_excluded.clone(),
                titles_ends_with: filters.titles_ends_with.clone(),
                titles_ends_with_excluded: filters.titles_ends_with_excluded.clone(),
                deleted: filters.deleted,
                oneshot: filters.oneshot,
                tags: filters.tags.clone(),
                tags_excluded: filters.tags_excluded.clone(),
                tags_null: filters.tags_null,
                media_profiles: filters.media_profiles.clone(),
                media_profiles_excluded: filters.media_profiles_excluded.clone(),
                authors: filters.authors.clone(),
                authors_excluded: filters.authors_excluded.clone(),
                poster_types: filters.poster_types.clone(),
                poster_types_excluded: filters.poster_types_excluded.clone(),
                poster_selected: filters.poster_selected,
                poster_selected_excluded: filters.poster_selected_excluded,
                media_statuses: filters.media_statuses.clone(),
                media_statuses_excluded: filters.media_statuses_excluded.clone(),
                read_statuses: filters.read_statuses.clone(),
                read_statuses_excluded: filters.read_statuses_excluded.clone(),
                release_dates: filters.release_dates.clone(),
                release_dates_excluded: filters.release_dates_excluded.clone(),
                release_dates_null: filters.release_dates_null,
                release_date_gt: filters.release_date_gt.clone(),
                release_date_lt: filters.release_date_lt.clone(),
                release_date_begins_with: filters.release_date_begins_with.clone(),
                release_date_ends_with: filters.release_date_ends_with.clone(),
                release_date_contains_excluded: filters.release_date_contains_excluded.clone(),
                release_date_begins_with_excluded: filters
                    .release_date_begins_with_excluded
                    .clone(),
                release_date_ends_with_excluded: filters.release_date_ends_with_excluded.clone(),
                release_date_in_last_days: filters.release_date_in_last_days,
                release_date_not_in_last_days: filters.release_date_not_in_last_days,
                number_sorts: filters.number_sorts.clone(),
                number_sorts_excluded: filters.number_sorts_excluded.clone(),
                number_sort_gt: filters.number_sort_gt,
                number_sort_lt: filters.number_sort_lt,
                search: full_text_search,
                page,
                size,
                unpaged,
                sort_modes,
            },
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
            }
            webui_bridge_books_filters_from_payload(payload)
        }
    };

    if !strict_runtime_shape {
        restrict_books_filters_to_persisted_shape(&mut filters);
        filters.library_ids =
            remap_requested_library_ids_for_persisted(database_file, filters.library_ids.as_ref())
                .await;
    }

    if strict_runtime_shape && !runtime_books_filters_match_runtime_shape(&filters) {
        return Some(invalid_runtime_books_list_response(
            DiscoveryError::InvalidSemantics(
                "unsupported runtime books filter combination".to_string(),
            ),
        ));
    }

    let requested_library_ids =
        requested_library_ids_for_runtime_shape(strict_runtime_shape, filters.library_ids.clone());
    let context = match auth_state.resolve_query_context(headers, requested_library_ids.as_deref())
    {
        Some(context) => context,
        None => return Some(StatusCode::UNAUTHORIZED.into_response()),
    };

    if let Some(series_id) = oneshot_bootstrap_series_id.clone() {
        filters.direct_browse_family = Some(DirectBrowseBooksListFamily::BrowseOneshotBootstrap);
        filters.series_ids = Some(vec![series_id]);
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

    let _ = uri;
    let _ = payload;
    let _ = full_text_search;
    let _ = is_admin;

    None
}

pub async fn runtime_owned_books_latest_response(
    headers: &HeaderMap,
    uri: &Uri,
    auth_state: &DiscoveryAuthState,
    database_file: &FsPath,
) -> Option<Response> {
    let query = uri.query().unwrap_or_default();
    let sorts = query_values(query, "sort");
    if !sorts.is_empty() {
        return None;
    }

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
        PersistedBooksBrowseQuery {
            library_ids,
            series_ids: None,
            series_ids_excluded: None,
            read_list_ids: None,
            read_list_ids_excluded: None,
            titles: None,
            titles_excluded: None,
            titles_contains: None,
            titles_contains_excluded: None,
            titles_begins_with: None,
            titles_begins_with_excluded: None,
            titles_ends_with: None,
            titles_ends_with_excluded: None,
            deleted: None,
            oneshot: None,
            tags: None,
            tags_excluded: None,
            tags_null: None,
            media_profiles: None,
            media_profiles_excluded: None,
            authors: None,
            authors_excluded: None,
            poster_types: None,
            poster_types_excluded: None,
            poster_selected: None,
            poster_selected_excluded: None,
            media_statuses: None,
            media_statuses_excluded: None,
            read_statuses: None,
            read_statuses_excluded: None,
            release_dates: None,
            release_dates_excluded: None,
            release_dates_null: None,
            release_date_gt: None,
            release_date_lt: None,
            release_date_begins_with: None,
            release_date_ends_with: None,
            release_date_contains_excluded: None,
            release_date_begins_with_excluded: None,
            release_date_ends_with_excluded: None,
            release_date_in_last_days: None,
            release_date_not_in_last_days: None,
            number_sorts: None,
            number_sorts_excluded: None,
            number_sort_gt: None,
            number_sort_lt: None,
            search: None,
            page,
            size,
            unpaged,
            sort_modes: vec![PersistedBooksSortMode::LastModifiedDateDesc],
        },
    )
    .await
    {
        Ok(page) => {
            let mut response =
                Json(books_page_payload(page, context.is_admin, !unpaged)).into_response();
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
