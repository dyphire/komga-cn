use super::index_dirs::{register_discovery_index_dir, resolve_discovery_index_dir};
use super::*;

pub(super) fn compose_persisted_discovery_access_backend(
    database_file: &std::path::Path,
    lucene_data_directory: &std::path::Path,
) -> PersistedDiscoveryAccessBackend {
    register_discovery_index_dir(database_file, lucene_data_directory);

    match decide_startup_lifecycle(lucene_data_directory) {
        Ok(SearchStartupLifecycle::Ready) => {}
        Ok(SearchStartupLifecycle::RebuildRequired) => {
            prepare_for_rebuild(lucene_data_directory).unwrap_or_else(|error| {
                panic!("prepare discovery search index startup rebuild failed: {error}")
            });
            rebuild_index_from_database(database_file, lucene_data_directory).unwrap_or_else(
                |error| panic!("rebuild discovery search index startup path failed: {error}"),
            );
        }
        Err(error) => panic!("discovery search startup lifecycle failed: {error}"),
    }

    let lucene_data_directory = lucene_data_directory.to_path_buf();
    PersistedDiscoveryAccessBackend {
        load_persisted_author_names: Arc::new(|database_file, search, authorized_library_ids| {
            Box::pin(async move {
                infrastructure_discovery::load_persisted_author_names(
                    database_file.as_path(),
                    &search,
                    authorized_library_ids.as_deref(),
                )
                .await
            })
        }),
        load_persisted_author_roles: Arc::new(|database_file, authorized_library_ids| {
            Box::pin(async move {
                infrastructure_discovery::load_persisted_author_roles(
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
                        PersistedAuthorsScope::All => infrastructure_discovery::AuthorsScope::All,
                        PersistedAuthorsScope::Libraries(ids) => {
                            infrastructure_discovery::AuthorsScope::Libraries(ids)
                        }
                        PersistedAuthorsScope::Collection(id) => {
                            infrastructure_discovery::AuthorsScope::Collection(id)
                        }
                        PersistedAuthorsScope::Series(id) => {
                            infrastructure_discovery::AuthorsScope::Series(id)
                        }
                        PersistedAuthorsScope::ReadList(id) => {
                            infrastructure_discovery::AuthorsScope::ReadList(id)
                        }
                    };
                    let rows = infrastructure_discovery::load_persisted_authors_by_scope(
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
                    infrastructure_discovery::load_book_poster_summaries(database_file.as_path())
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
                let rows = infrastructure_discovery::load_persisted_book_summaries(
                    database_file.as_path(),
                    user_id.as_deref(),
                )
                .await?;
                Ok(rows
                    .into_iter()
                    .map(|row| PersistedBookSummary {
                        id: row.id,
                        series_id: row.series_id,
                        library_id: row.library_id,
                        title: row.title,
                        created: row.created,
                        last_modified: row.last_modified,
                        media_status: row.media_status,
                        media_type: row.media_type,
                        read_status: row.read_status,
                        metadata_number_sort: row.metadata_number_sort,
                        metadata_release_date: row.metadata_release_date,
                        deleted: row.deleted,
                        oneshot: row.oneshot,
                        genres: row.genres,
                        language: row.language,
                        publisher: row.publisher,
                        age_rating: row.age_rating,
                        metadata_tags: row.metadata_tags,
                        metadata_authors: row.metadata_authors,
                    })
                    .collect())
            })
        }),
        load_persisted_book_summaries_by_ids: Arc::new(|database_file, user_id, ids| {
            Box::pin(async move {
                let rows = infrastructure_discovery::load_persisted_book_summaries_by_ids(
                    database_file.as_path(),
                    user_id.as_deref(),
                    &ids,
                )
                .await?;
                Ok(rows
                    .into_iter()
                    .map(|row| PersistedBookSummary {
                        id: row.id,
                        series_id: row.series_id,
                        library_id: row.library_id,
                        title: row.title,
                        created: row.created,
                        last_modified: row.last_modified,
                        media_status: row.media_status,
                        media_type: row.media_type,
                        read_status: row.read_status,
                        metadata_number_sort: row.metadata_number_sort,
                        metadata_release_date: row.metadata_release_date,
                        deleted: row.deleted,
                        oneshot: row.oneshot,
                        genres: row.genres,
                        language: row.language,
                        publisher: row.publisher,
                        age_rating: row.age_rating,
                        metadata_tags: row.metadata_tags,
                        metadata_authors: row.metadata_authors,
                    })
                    .collect())
            })
        }),
        load_persisted_book_count: Arc::new(|database_file| {
            Box::pin(async move {
                infrastructure_discovery::load_persisted_book_count(database_file.as_path()).await
            })
        }),
        persisted_books_exist: Arc::new(|database_file| {
            Box::pin(async move {
                infrastructure_discovery::persisted_books_exist(database_file.as_path()).await
            })
        }),
        load_persisted_genres: Arc::new(|database_file, library_ids, collection_id| {
            Box::pin(async move {
                infrastructure_discovery::load_persisted_genres(
                    database_file.as_path(),
                    library_ids.as_deref(),
                    collection_id.as_deref(),
                )
                .await
            })
        }),
        load_persisted_tags: Arc::new(|database_file, library_ids, collection_id| {
            Box::pin(async move {
                infrastructure_discovery::load_persisted_tags(
                    database_file.as_path(),
                    library_ids.as_deref(),
                    collection_id.as_deref(),
                )
                .await
            })
        }),
        load_persisted_languages: Arc::new(|database_file, library_ids, collection_id| {
            Box::pin(async move {
                infrastructure_discovery::load_persisted_languages(
                    database_file.as_path(),
                    library_ids.as_deref(),
                    collection_id.as_deref(),
                )
                .await
            })
        }),
        load_persisted_publishers: Arc::new(|database_file, library_ids, collection_id| {
            Box::pin(async move {
                infrastructure_discovery::load_persisted_publishers(
                    database_file.as_path(),
                    library_ids.as_deref(),
                    collection_id.as_deref(),
                )
                .await
            })
        }),
        load_persisted_age_ratings: Arc::new(|database_file, library_ids, collection_id| {
            Box::pin(async move {
                infrastructure_discovery::load_persisted_age_ratings(
                    database_file.as_path(),
                    library_ids.as_deref(),
                    collection_id.as_deref(),
                )
                .await
            })
        }),
        load_persisted_sharing_labels: Arc::new(|database_file, library_ids, collection_id| {
            Box::pin(async move {
                infrastructure_discovery::load_persisted_sharing_labels(
                    database_file.as_path(),
                    library_ids.as_deref(),
                    collection_id.as_deref(),
                )
                .await
            })
        }),
        load_persisted_series_release_dates: Arc::new(
            |database_file, library_ids, collection_id| {
                Box::pin(async move {
                    infrastructure_discovery::load_persisted_series_release_dates(
                        database_file.as_path(),
                        library_ids.as_deref(),
                        collection_id.as_deref(),
                    )
                    .await
                })
            },
        ),
        load_persisted_series_tags: Arc::new(|database_file, library_ids, collection_id| {
            Box::pin(async move {
                infrastructure_discovery::load_persisted_series_tags(
                    database_file.as_path(),
                    library_ids.as_deref(),
                    collection_id.as_deref(),
                )
                .await
            })
        }),
        load_persisted_library_ids: Arc::new(|database_file| {
            Box::pin(async move {
                infrastructure_discovery::load_persisted_library_ids(database_file.as_path()).await
            })
        }),
        load_collection_memberships: Arc::new(|database_file| {
            Box::pin(async move {
                infrastructure_discovery::load_collection_memberships(database_file.as_path()).await
            })
        }),
        load_readlist_memberships: Arc::new(|database_file| {
            Box::pin(async move {
                infrastructure_discovery::load_readlist_memberships(database_file.as_path()).await
            })
        }),
        load_persisted_ondeck_books: Arc::new(|database_file, user_id| {
            Box::pin(async move {
                let rows = infrastructure_discovery::load_persisted_ondeck_books(
                    database_file.as_path(),
                    &user_id,
                )
                .await?;
                Ok(rows
                    .into_iter()
                    .map(|row| PersistedBookBrowseEntry {
                        id: row.id,
                        library_id: row.library_id,
                        name: row.name,
                        title: row.title,
                    })
                    .collect())
            })
        }),
        load_persisted_duplicate_books: Arc::new(|database_file| {
            Box::pin(async move {
                let rows = infrastructure_discovery::load_persisted_duplicate_books(
                    database_file.as_path(),
                )
                .await?;
                Ok(rows
                    .into_iter()
                    .map(|row| PersistedBookBrowseEntry {
                        id: row.id,
                        library_id: row.library_id,
                        name: row.name,
                        title: row.title,
                    })
                    .collect())
            })
        }),
        load_persisted_book_tags: Arc::new(|database_file, scope, authorized_library_ids| {
            Box::pin(async move {
                let mapped_scope = scope.map(|scope| match scope {
                    PersistedBookTagsScope::All => infrastructure_discovery::BookTagsScope::All,
                    PersistedBookTagsScope::Series(series_id) => {
                        infrastructure_discovery::BookTagsScope::Series(series_id)
                    }
                    PersistedBookTagsScope::Libraries(library_ids) => {
                        infrastructure_discovery::BookTagsScope::Libraries(library_ids)
                    }
                    PersistedBookTagsScope::ReadList(readlist_id) => {
                        infrastructure_discovery::BookTagsScope::ReadList(readlist_id)
                    }
                });
                infrastructure_discovery::load_persisted_book_tags(
                    database_file.as_path(),
                    mapped_scope.as_ref(),
                    authorized_library_ids.as_deref(),
                )
                .await
            })
        }),
        persisted_utc_date_minus_days: Arc::new(|database_file, days| {
            Box::pin(async move {
                infrastructure_discovery::persisted_utc_date_minus_days(
                    database_file.as_path(),
                    days,
                )
                .await
            })
        }),
        load_series_read_progress_counts: Arc::new(|database_file, user_id| {
            Box::pin(async move {
                infrastructure_discovery::load_series_read_progress_counts(
                    database_file.as_path(),
                    &user_id,
                )
                .await
            })
        }),
        load_series_total_book_counts: Arc::new(|database_file| {
            Box::pin(async move {
                infrastructure_discovery::load_series_total_book_counts(database_file.as_path())
                    .await
            })
        }),
        load_persisted_series_summaries: Arc::new(|database_file| {
            Box::pin(async move {
                let rows = infrastructure_discovery::load_persisted_series_summaries(
                    database_file.as_path(),
                )
                .await?;
                Ok(rows
                    .into_iter()
                    .map(|row| PersistedSeriesSummary {
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
                    })
                    .collect())
            })
        }),
        load_persisted_series_summaries_by_ids: Arc::new(|database_file, ids| {
            Box::pin(async move {
                let rows = infrastructure_discovery::load_persisted_series_summaries_by_ids(
                    database_file.as_path(),
                    &ids,
                )
                .await?;
                Ok(rows
                    .into_iter()
                    .map(|row| PersistedSeriesSummary {
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
                    })
                    .collect())
            })
        }),
        load_persisted_series_count: Arc::new(|database_file| {
            Box::pin(async move {
                infrastructure_discovery::load_persisted_series_count(database_file.as_path()).await
            })
        }),
        persisted_series_exist: Arc::new(|database_file| {
            Box::pin(async move {
                infrastructure_discovery::persisted_series_exist(database_file.as_path()).await
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
                    let index = SearchIndexLifecycle::bootstrap(index_dir.as_path())
                        .map_err(|error| format!("bootstrap search index for books: {error}"))?;
                    index
                        .search_ids(&query, SearchEntityType::Book, limit)
                        .map_err(|error| format!("search index books query: {error}"))
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
                    let index =
                        SearchIndexLifecycle::bootstrap(index_dir.as_path()).map_err(|error| {
                            format!("bootstrap search index for collections: {error}")
                        })?;
                    index
                        .search_ids(&query, SearchEntityType::Collection, limit)
                        .map_err(|error| format!("search index collections query: {error}"))
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
                    let index =
                        SearchIndexLifecycle::bootstrap(index_dir.as_path()).map_err(|error| {
                            format!("bootstrap search index for readlists: {error}")
                        })?;
                    index
                        .search_scored_ids(&query, SearchEntityType::ReadList, limit)
                        .map_err(|error| format!("search index readlists query: {error}"))
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
                    let index = SearchIndexLifecycle::bootstrap(index_dir.as_path())
                        .map_err(|error| format!("bootstrap search index for series: {error}"))?;
                    index
                        .search_scored_ids(&query, SearchEntityType::Series, limit)
                        .map_err(|error| format!("search index series query: {error}"))
                })
            }
        }),
    }
}
