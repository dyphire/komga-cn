use super::index_dirs::{register_discovery_index_dir, resolve_discovery_index_dir};
use super::*;

use komga_infrastructure::search::index_lifecycle::SearchQueryLifecycle;

fn search_ids_or_empty(
    index_dir: &std::path::Path,
    query: &str,
    entity_type: SearchEntityType,
    limit: usize,
) -> Vec<String> {
    let Ok(index) = SearchQueryLifecycle::bootstrap(index_dir) else {
        return Vec::new();
    };

    index
        .search_ids(query, entity_type, limit)
        .unwrap_or_default()
}

fn search_scored_ids_or_empty(
    index_dir: &std::path::Path,
    query: &str,
    entity_type: SearchEntityType,
    limit: usize,
) -> Vec<(f32, String)> {
    let Ok(index) = SearchQueryLifecycle::bootstrap(index_dir) else {
        return Vec::new();
    };

    index
        .search_scored_ids(query, entity_type, limit)
        .unwrap_or_default()
}

macro_rules! forward_string_facet_loader {
    ($loader:path) => {
        Arc::new(|database_file, library_ids, collection_id| {
            Box::pin(async move {
                $loader(
                    database_file.as_path(),
                    library_ids.as_deref(),
                    collection_id.as_deref(),
                )
                .await
            })
        })
    };
}

fn persisted_book_summary(
    row: infrastructure_discovery_models::BookSummary,
) -> PersistedBookSummary {
    PersistedBookSummary {
        id: row.id,
        series_id: row.series_id,
        library_id: row.library_id,
        series_title: row.series_title,
        title: row.title,
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
        read_status: row.read_status,
        metadata_title_lock: row.metadata_title_lock,
        metadata_summary: row.metadata_summary,
        metadata_summary_lock: row.metadata_summary_lock,
        metadata_number: row.metadata_number,
        metadata_number_lock: row.metadata_number_lock,
        metadata_number_sort: row.metadata_number_sort,
        metadata_number_sort_lock: row.metadata_number_sort_lock,
        metadata_release_date: row.metadata_release_date,
        metadata_release_date_lock: row.metadata_release_date_lock,
        metadata_authors_lock: row.metadata_authors_lock,
        metadata_tags_lock: row.metadata_tags_lock,
        metadata_isbn: row.metadata_isbn,
        metadata_isbn_lock: row.metadata_isbn_lock,
        metadata_links_lock: row.metadata_links_lock,
        metadata_created: row.metadata_created,
        metadata_last_modified: row.metadata_last_modified,
        file_hash: row.file_hash,
        read_progress: row
            .read_progress
            .map(|progress| PersistedReadProgressSummary {
                page: progress.page,
                completed: progress.completed,
                read_date: progress.read_date,
                created: progress.created,
                last_modified: progress.last_modified,
                device_id: progress.device_id,
                device_name: progress.device_name,
            }),
        deleted: row.deleted,
        oneshot: row.oneshot,
        genres: row.genres,
        language: row.language,
        publisher: row.publisher,
        age_rating: row.age_rating,
        metadata_tags: row.metadata_tags,
        metadata_authors: row
            .metadata_authors
            .into_iter()
            .map(|author| PersistedAuthorEntry {
                name: author.name,
                role: author.role,
            })
            .collect(),
        metadata_links: row
            .metadata_links
            .into_iter()
            .map(|link| PersistedWebLinkEntry {
                label: link.label,
                url: link.url,
            })
            .collect(),
    }
}

fn persisted_book_browse_entry(
    row: infrastructure_discovery_models::BookBrowseEntry,
) -> PersistedBookBrowseEntry {
    PersistedBookBrowseEntry {
        id: row.id,
        library_id: row.library_id,
        name: row.name,
        title: row.title,
    }
}

fn persisted_series_summary(
    row: infrastructure_discovery_models::SeriesSummary,
) -> PersistedSeriesSummary {
    PersistedSeriesSummary {
        id: row.id,
        library_id: row.library_id,
        title: row.title,
        title_sort: row.title_sort,
        labels: row.labels,
        created: row.created,
        last_modified: row.last_modified,
        file_last_modified: row.file_last_modified,
        books_count: row.books_count,
        books_read_count: row.books_read_count,
        books_unread_count: row.books_unread_count,
        books_in_progress_count: row.books_in_progress_count,
        status: row.status,
        summary: row.summary,
        reading_direction: row.reading_direction,
        publisher: row.publisher,
        age_rating: row.age_rating,
        language: row.language,
        genres: row.genres,
        tags: row.tags,
        alternate_titles: row.alternate_titles,
        metadata_created: row.metadata_created,
        metadata_last_modified: row.metadata_last_modified,
        books_metadata_authors: row.books_metadata_authors,
        books_metadata_tags: row.books_metadata_tags,
        books_metadata_release_date: row.books_metadata_release_date,
        books_metadata_summary: row.books_metadata_summary,
        books_metadata_summary_number: row.books_metadata_summary_number,
        books_metadata_created: row.books_metadata_created,
        books_metadata_last_modified: row.books_metadata_last_modified,
        deleted: row.deleted,
        oneshot: row.oneshot,
    }
}

pub(super) fn compose_persisted_discovery_access_backend(
    database_file: &std::path::Path,
    lucene_data_directory: &std::path::Path,
) -> PersistedDiscoveryAccessBackend {
    register_discovery_index_dir(database_file, lucene_data_directory);

    let lucene_data_directory = lucene_data_directory.to_path_buf();
    PersistedDiscoveryAccessBackend {
        load_persisted_author_names: Arc::new(|database_file, search, authorized_library_ids| {
            Box::pin(async move {
                infrastructure_discovery_authors::load_persisted_author_names(
                    database_file.as_path(),
                    &search,
                    authorized_library_ids.as_deref(),
                )
                .await
            })
        }),
        load_persisted_author_roles: Arc::new(|database_file, authorized_library_ids| {
            Box::pin(async move {
                infrastructure_discovery_authors::load_persisted_author_roles(
                    database_file.as_path(),
                    authorized_library_ids.as_deref(),
                )
                .await
            })
        }),
        load_persisted_authors_by_scope: Arc::new(
            |database_file, scope, authorized_library_ids| {
                Box::pin(async move {
                    let mapped_scope = match scope {
                        PersistedAuthorsScope::All => infrastructure_discovery_models::AuthorsScope::All,
                        PersistedAuthorsScope::Libraries(ids) => {
                            infrastructure_discovery_models::AuthorsScope::Libraries(ids)
                        }
                        PersistedAuthorsScope::Collection(id) => {
                            infrastructure_discovery_models::AuthorsScope::Collection(id)
                        }
                        PersistedAuthorsScope::Series(id) => {
                            infrastructure_discovery_models::AuthorsScope::Series(id)
                        }
                        PersistedAuthorsScope::ReadList(id) => {
                            infrastructure_discovery_models::AuthorsScope::ReadList(id)
                        }
                    };
                    let rows = infrastructure_discovery_authors::load_persisted_authors_by_scope(
                        database_file.as_path(),
                        &mapped_scope,
                        authorized_library_ids.as_deref(),
                    )
                    .await?;
                    Ok(rows
                        .into_iter()
                        .map(|row| PersistedAuthorEntry {
                            name: row.name,
                            role: row.role,
                        })
                        .collect())
                })
            },
        ),
        load_book_poster_summaries: Arc::new(|database_file| {
            Box::pin(async move {
                let rows =
                    infrastructure_discovery_books::load_book_poster_summaries(database_file.as_path())
                        .await?;
                Ok(rows
                    .into_iter()
                    .map(|(book_id, values)| {
                        (
                            book_id,
                            values
                                .into_iter()
                                .map(|value| PersistedBookPosterSummary {
                                    thumbnail_type: value.thumbnail_type,
                                    selected: value.selected,
                                })
                                .collect(),
                        )
                    })
                    .collect())
            })
        }),
        load_persisted_book_summaries: Arc::new(|database_file, user_id| {
            Box::pin(async move {
                let rows = infrastructure_discovery_books::load_persisted_book_summaries(
                    database_file.as_path(),
                    user_id.as_deref(),
                )
                .await?;
                Ok(rows.into_iter().map(persisted_book_summary).collect())
            })
        }),
        load_persisted_book_summaries_by_ids: Arc::new(|database_file, user_id, ids| {
            Box::pin(async move {
                let rows = infrastructure_discovery_books::load_persisted_book_summaries_by_ids(
                    database_file.as_path(),
                    user_id.as_deref(),
                    &ids,
                )
                .await?;
                Ok(rows.into_iter().map(persisted_book_summary).collect())
            })
        }),
        load_persisted_book_count: Arc::new(|database_file| {
            Box::pin(async move {
                infrastructure_discovery_books::load_persisted_book_count(database_file.as_path()).await
            })
        }),
        load_persisted_genres: forward_string_facet_loader!(
            infrastructure_discovery_facets::load_persisted_genres
        ),
        load_persisted_tags: forward_string_facet_loader!(
            infrastructure_discovery_facets::load_persisted_tags
        ),
        load_persisted_languages: forward_string_facet_loader!(
            infrastructure_discovery_facets::load_persisted_languages
        ),
        load_persisted_publishers: forward_string_facet_loader!(
            infrastructure_discovery_facets::load_persisted_publishers
        ),
        load_persisted_age_ratings: forward_string_facet_loader!(
            infrastructure_discovery_facets::load_persisted_age_ratings
        ),
        load_persisted_sharing_labels: forward_string_facet_loader!(
            infrastructure_discovery_facets::load_persisted_sharing_labels
        ),
        load_persisted_series_release_dates: forward_string_facet_loader!(
            infrastructure_discovery_facets::load_persisted_series_release_dates
        ),
        load_persisted_series_tags: forward_string_facet_loader!(
            infrastructure_discovery_facets::load_persisted_series_tags
        ),
        load_persisted_library_ids: Arc::new(|database_file| {
            Box::pin(async move {
                infrastructure_discovery_library_mappings::load_persisted_library_ids(database_file.as_path()).await
            })
        }),
        load_collection_memberships: Arc::new(|database_file| {
            Box::pin(async move {
                infrastructure_discovery_library_mappings::load_collection_memberships(database_file.as_path()).await
            })
        }),
        load_readlist_memberships: Arc::new(|database_file| {
            Box::pin(async move {
                infrastructure_discovery_library_mappings::load_readlist_memberships(database_file.as_path()).await
            })
        }),
        load_persisted_ondeck_books: Arc::new(|database_file, user_id| {
            Box::pin(async move {
                let rows = infrastructure_discovery_runtime_queries::load_persisted_ondeck_books(
                    database_file.as_path(),
                    &user_id,
                )
                .await?;
                Ok(rows.into_iter().map(persisted_book_browse_entry).collect())
            })
        }),
        load_persisted_duplicate_books: Arc::new(|database_file| {
            Box::pin(async move {
                let rows = infrastructure_discovery_runtime_queries::load_persisted_duplicate_books(
                    database_file.as_path(),
                )
                .await?;
                Ok(rows.into_iter().map(persisted_book_browse_entry).collect())
            })
        }),
        load_persisted_book_tags: Arc::new(|database_file, scope, authorized_library_ids| {
            Box::pin(async move {
                let mapped_scope = scope.map(|scope| match scope {
                    PersistedBookTagsScope::All => infrastructure_discovery_models::BookTagsScope::All,
                    PersistedBookTagsScope::Series(series_id) => {
                        infrastructure_discovery_models::BookTagsScope::Series(series_id)
                    }
                    PersistedBookTagsScope::Libraries(library_ids) => {
                        infrastructure_discovery_models::BookTagsScope::Libraries(library_ids)
                    }
                    PersistedBookTagsScope::ReadList(readlist_id) => {
                        infrastructure_discovery_models::BookTagsScope::ReadList(readlist_id)
                    }
                });
                infrastructure_discovery_runtime_queries::load_persisted_book_tags(
                    database_file.as_path(),
                    mapped_scope.as_ref(),
                    authorized_library_ids.as_deref(),
                )
                .await
            })
        }),
        persisted_utc_date_minus_days: Arc::new(|database_file, days| {
            Box::pin(async move {
                infrastructure_discovery_runtime_queries::persisted_utc_date_minus_days(
                    database_file.as_path(),
                    days,
                )
                .await
            })
        }),
        load_series_read_progress_counts: Arc::new(|database_file, user_id| {
            Box::pin(async move {
                infrastructure_discovery_runtime_queries::load_series_read_progress_counts(
                    database_file.as_path(),
                    &user_id,
                )
                .await
            })
        }),
        load_series_total_book_counts: Arc::new(|database_file| {
            Box::pin(async move {
                infrastructure_discovery_runtime_queries::load_series_total_book_counts(database_file.as_path())
                    .await
            })
        }),
        load_persisted_series_summaries: Arc::new(|database_file| {
            Box::pin(async move {
                let rows = infrastructure_discovery_series::load_persisted_series_summaries(
                    database_file.as_path(),
                )
                .await?;
                Ok(rows.into_iter().map(persisted_series_summary).collect())
            })
        }),
        load_persisted_series_summaries_by_ids: Arc::new(|database_file, ids| {
            Box::pin(async move {
                let rows = infrastructure_discovery_series::load_persisted_series_summaries_by_ids(
                    database_file.as_path(),
                    &ids,
                )
                .await?;
                Ok(rows.into_iter().map(persisted_series_summary).collect())
            })
        }),
        load_persisted_series_count: Arc::new(|database_file| {
            Box::pin(async move {
                infrastructure_discovery_series::load_persisted_series_count(database_file.as_path()).await
            })
        }),
        persisted_series_exist: Arc::new(|database_file| {
            Box::pin(async move {
                infrastructure_discovery_series::persisted_series_exist(database_file.as_path()).await
            })
        }),
        search_book_ids: Arc::new({
            let default_index_dir = lucene_data_directory.clone();
            move |database_file, query, limit| {
                let default_index_dir = default_index_dir.clone();
                Box::pin(async move {
                    let index_dir = resolve_discovery_index_dir(
                        database_file.as_path(),
                        default_index_dir.as_path(),
                    );
                    Ok(search_ids_or_empty(
                        index_dir.as_path(),
                        &query,
                        SearchEntityType::Book,
                        limit,
                    ))
                })
            }
        }),
        search_collection_ids: Arc::new({
            let default_index_dir = lucene_data_directory.clone();
            move |database_file, query, limit| {
                let default_index_dir = default_index_dir.clone();
                Box::pin(async move {
                    let index_dir = resolve_discovery_index_dir(
                        database_file.as_path(),
                        default_index_dir.as_path(),
                    );
                    Ok(search_ids_or_empty(
                        index_dir.as_path(),
                        &query,
                        SearchEntityType::Collection,
                        limit,
                    ))
                })
            }
        }),
        search_readlist_scored_ids: Arc::new({
            let default_index_dir = lucene_data_directory.clone();
            move |database_file, query, limit| {
                let default_index_dir = default_index_dir.clone();
                Box::pin(async move {
                    let index_dir = resolve_discovery_index_dir(
                        database_file.as_path(),
                        default_index_dir.as_path(),
                    );
                    Ok(search_scored_ids_or_empty(
                        index_dir.as_path(),
                        &query,
                        SearchEntityType::ReadList,
                        limit,
                    ))
                })
            }
        }),
        search_series_scored_ids: Arc::new({
            let default_index_dir = lucene_data_directory.clone();
            move |database_file, query, limit| {
                let default_index_dir = default_index_dir.clone();
                Box::pin(async move {
                    let index_dir = resolve_discovery_index_dir(
                        database_file.as_path(),
                        default_index_dir.as_path(),
                    );
                    Ok(search_scored_ids_or_empty(
                        index_dir.as_path(),
                        &query,
                        SearchEntityType::Series,
                        limit,
                    ))
                })
            }
        }),
    }
}
