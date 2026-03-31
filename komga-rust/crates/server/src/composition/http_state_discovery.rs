use super::*;
use std::sync::OnceLock;

use komga_infrastructure::{
    SearchEntityType, SearchIndexLifecycle, SearchStartupLifecycle, decide_startup_lifecycle,
    prepare_for_rebuild, rebuild_index_from_database, sync_entity_delete_from_index,
    sync_entity_upsert_from_database, sync_series_and_oneshot_books_after_metadata_update,
};

fn discovery_index_dir_mappings() -> &'static std::sync::RwLock<HashMap<PathBuf, PathBuf>> {
    static DISCOVERY_INDEX_DIR_MAPPINGS: OnceLock<std::sync::RwLock<HashMap<PathBuf, PathBuf>>> =
        OnceLock::new();
    DISCOVERY_INDEX_DIR_MAPPINGS.get_or_init(|| std::sync::RwLock::new(HashMap::new()))
}

pub(super) fn register_discovery_index_dir(
    database_file: &std::path::Path,
    lucene_data_directory: &std::path::Path,
) {
    let key = database_file.to_path_buf();
    let value = lucene_data_directory.to_path_buf();
    let mappings = discovery_index_dir_mappings();
    let mut guard = mappings
        .write()
        .expect("discovery index-dir mapping write lock should not be poisoned");
    guard.insert(key, value);
}

pub(super) fn resolve_discovery_index_dir(
    database_file: &std::path::Path,
    default_lucene_data_directory: &std::path::Path,
) -> PathBuf {
    let mappings = discovery_index_dir_mappings();
    let guard = mappings
        .read()
        .expect("discovery index-dir mapping read lock should not be poisoned");
    guard
        .get(database_file)
        .cloned()
        .unwrap_or_else(|| default_lucene_data_directory.to_path_buf())
}

pub(super) fn compose_discovery_detail_access_backends() -> DiscoveryDetailAccessBackends {
    DiscoveryDetailAccessBackends {
        books: DiscoveryDetailBooksAccessBackend {
            load_book_id_by_sorted_position: Arc::new(|database_file, index| {
                Box::pin(async move {
                    infrastructure_detail_books::load_book_id_by_sorted_position(
                        database_file.as_path(),
                        index,
                    )
                    .await
                })
            }),
            load_persisted_book_resource: Arc::new(|database_file, book_id| {
                Box::pin(async move {
                    infrastructure_detail_books::load_persisted_book_resource(
                        database_file.as_path(),
                        &book_id,
                    )
                    .await
                    .map(|value| {
                        value.map(|row| PersistedBookResourceRecord {
                            library_id: row.library_id,
                            age_rating: row.age_rating,
                            sharing_labels: row.sharing_labels,
                        })
                    })
                })
            }),
            load_persisted_book_detail: Arc::new(|database_file, book_id, user_id| {
                Box::pin(async move {
                    infrastructure_detail_books::load_persisted_book_detail(
                        database_file.as_path(),
                        &book_id,
                        user_id.as_deref(),
                    )
                    .await
                    .map(|value| {
                        value.map(|row| PersistedBookDetailRecord {
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
                            metadata_title: row.metadata_title,
                            metadata_summary: row.metadata_summary,
                            metadata_number: row.metadata_number,
                            metadata_number_sort: row.metadata_number_sort,
                            metadata_release_date: row.metadata_release_date,
                            metadata_authors: row.metadata_authors,
                            metadata_tags: row.metadata_tags,
                            metadata_isbn: row.metadata_isbn,
                            metadata_created: row.metadata_created,
                            metadata_last_modified: row.metadata_last_modified,
                            read_progress: row.read_progress.map(|progress| {
                                PersistedBookReadProgressRecord {
                                    page: progress.page,
                                    completed: progress.completed,
                                    read_date: progress.read_date,
                                    created: progress.created,
                                    last_modified: progress.last_modified,
                                    device_id: progress.device_id,
                                    device_name: progress.device_name,
                                }
                            }),
                            deleted: row.deleted,
                            file_hash: row.file_hash,
                            oneshot: row.oneshot,
                        })
                    })
                })
            }),
            load_persisted_book_sibling_id: Arc::new(|database_file, book_id, direction| {
                Box::pin(async move {
                    let direction = match direction {
                        PersistedBookSiblingDirectionRecord::Previous => {
                            infrastructure_detail_books::PersistedBookSiblingDirectionRecord::Previous
                        }
                        PersistedBookSiblingDirectionRecord::Next => {
                            infrastructure_detail_books::PersistedBookSiblingDirectionRecord::Next
                        }
                    };

                    infrastructure_detail_books::load_persisted_book_sibling_id(
                        database_file.as_path(),
                        &book_id,
                        direction,
                    )
                    .await
                })
            }),
        },
        collections: DiscoveryDetailCollectionsAccessBackend {
            persisted_collections_exist: Arc::new(|database_file| {
                Box::pin(async move {
                    infrastructure_detail_collections::persisted_collections_exist(
                        database_file.as_path(),
                    )
                    .await
                })
            }),
            load_persisted_collections: Arc::new(|database_file| {
                Box::pin(async move {
                    infrastructure_detail_collections::load_persisted_collections(
                        database_file.as_path(),
                    )
                    .await
                    .map(|rows| {
                        rows.into_iter()
                            .map(|row| PersistedCollectionAccessRecord {
                                id: row.id,
                                name: row.name,
                                ordered: row.ordered,
                                created_date: row.created_date,
                                last_modified_date: row.last_modified_date,
                            })
                            .collect()
                    })
                })
            }),
            load_persisted_collection_series_ids: Arc::new(|database_file, collection_id| {
                Box::pin(async move {
                    infrastructure_detail_collections::load_persisted_collection_series_ids(
                        database_file.as_path(),
                        &collection_id,
                    )
                    .await
                })
            }),
            load_persisted_collection_detail: Arc::new(|database_file, collection_id| {
                Box::pin(async move {
                    infrastructure_detail_collections::load_persisted_collection_detail(
                        database_file.as_path(),
                        &collection_id,
                    )
                    .await
                    .map(|value| {
                        value.map(|row| PersistedCollectionAccessRecord {
                            id: row.id,
                            name: row.name,
                            ordered: row.ordered,
                            created_date: row.created_date,
                            last_modified_date: row.last_modified_date,
                        })
                    })
                })
            }),
            load_series_library_id: Arc::new(|database_file, series_id| {
                Box::pin(async move {
                    infrastructure_detail_collections::load_series_library_id(
                        database_file.as_path(),
                        &series_id,
                    )
                    .await
                })
            }),
            load_series_restrictions: Arc::new(|database_file, series_id| {
                Box::pin(async move {
                    infrastructure_detail_collections::load_series_restrictions(
                        database_file.as_path(),
                        &series_id,
                    )
                    .await
                    .map(|row| PersistedSeriesRestrictionRecord {
                        age_rating: row.age_rating,
                        labels: row.labels,
                    })
                })
            }),
            persist_collection_create: Arc::new(
                |database_file, collection_id, name, ordered, series_ids| {
                    Box::pin(async move {
                        infrastructure_detail_collections::persist_collection_create(
                            database_file.as_path(),
                            &collection_id,
                            &name,
                            ordered,
                            &series_ids,
                        )
                        .await
                    })
                },
            ),
            persist_collection_update: Arc::new(
                |database_file, collection_id, name, ordered, series_ids| {
                    Box::pin(async move {
                        infrastructure_detail_collections::persist_collection_update(
                            database_file.as_path(),
                            &collection_id,
                            &name,
                            ordered,
                            &series_ids,
                        )
                        .await
                    })
                },
            ),
            delete_persisted_collection: Arc::new(|database_file, collection_id| {
                Box::pin(async move {
                    infrastructure_detail_collections::delete_persisted_collection(
                        database_file.as_path(),
                        &collection_id,
                    )
                    .await
                })
            }),
            upsert_collection_search_document: Arc::new(
                |database_file, index_dir, collection_id| {
                    Box::pin(async move {
                        sync_entity_upsert_from_database(
                            database_file.as_path(),
                            index_dir.as_path(),
                            SearchEntityType::Collection,
                            &collection_id,
                        )
                    })
                },
            ),
            delete_collection_search_document: Arc::new(|index_dir, collection_id| {
                Box::pin(async move {
                    sync_entity_delete_from_index(
                        index_dir.as_path(),
                        SearchEntityType::Collection,
                        &collection_id,
                    )
                })
            }),
        },
        readlists: DiscoveryDetailReadlistsAccessBackend {
            persisted_readlists_exist: Arc::new(|database_file| {
                Box::pin(async move {
                    infrastructure_detail_readlists::persisted_readlists_exist(
                        database_file.as_path(),
                    )
                    .await
                })
            }),
            load_persisted_readlists: Arc::new(|database_file| {
                Box::pin(async move {
                    infrastructure_detail_readlists::load_persisted_readlists(
                        database_file.as_path(),
                    )
                    .await
                    .map(|rows| {
                        rows.into_iter()
                            .map(|row| PersistedReadlistRecord {
                                id: row.id,
                                name: row.name,
                                summary: row.summary,
                                ordered: row.ordered,
                                created_date: row.created_date,
                                last_modified_date: row.last_modified_date,
                            })
                            .collect()
                    })
                })
            }),
            load_persisted_readlist_detail: Arc::new(|database_file, readlist_id| {
                Box::pin(async move {
                    infrastructure_detail_readlists::load_persisted_readlist_detail(
                        database_file.as_path(),
                        &readlist_id,
                    )
                    .await
                    .map(|value| {
                        value.map(|row| PersistedReadlistRecord {
                            id: row.id,
                            name: row.name,
                            summary: row.summary,
                            ordered: row.ordered,
                            created_date: row.created_date,
                            last_modified_date: row.last_modified_date,
                        })
                    })
                })
            }),
            load_persisted_readlist_book_rows: Arc::new(|database_file, readlist_id| {
                Box::pin(async move {
                    infrastructure_detail_readlists::load_persisted_readlist_book_rows(
                        database_file.as_path(),
                        &readlist_id,
                    )
                    .await
                    .map(|rows| {
                        rows.into_iter()
                            .map(|row| PersistedReadlistBookRecord {
                                book_id: row.book_id,
                                library_id: row.library_id,
                            })
                            .collect()
                    })
                })
            }),
            load_comicrack_match_candidates: Arc::new(|database_file| {
                Box::pin(async move {
                    infrastructure_detail_readlists::load_comicrack_match_candidates(
                        database_file.as_path(),
                    )
                    .await
                    .map(|rows| {
                        rows.into_iter()
                            .map(|row| PersistedComicrackMatchCandidateRecord {
                                series_id: row.series_id,
                                series_title: row.series_title,
                                series_release_date: row.series_release_date,
                                book_id: row.book_id,
                                book_title: row.book_title,
                                book_number: row.book_number,
                            })
                            .collect()
                    })
                })
            }),
            load_persisted_book_authors: Arc::new(|database_file, book_id| {
                Box::pin(async move {
                    infrastructure_detail_readlists::load_persisted_book_authors(
                        database_file.as_path(),
                        &book_id,
                    )
                    .await
                    .map(|rows| {
                        rows.into_iter()
                            .map(|row| PersistedBookAuthorRecord {
                                name: row.name,
                                role: row.role,
                            })
                            .collect()
                    })
                })
            }),
            persist_readlist_create: Arc::new(
                |database_file, readlist_id, name, summary, ordered, book_ids| {
                    Box::pin(async move {
                        infrastructure_detail_readlists::persist_readlist_create(
                            database_file.as_path(),
                            &readlist_id,
                            &name,
                            &summary,
                            ordered,
                            &book_ids,
                        )
                        .await
                    })
                },
            ),
            persist_readlist_update: Arc::new(
                |database_file, readlist_id, name, summary, ordered, book_ids| {
                    Box::pin(async move {
                        infrastructure_detail_readlists::persist_readlist_update(
                            database_file.as_path(),
                            &readlist_id,
                            &name,
                            &summary,
                            ordered,
                            &book_ids,
                        )
                        .await
                    })
                },
            ),
            delete_persisted_readlist: Arc::new(|database_file, readlist_id| {
                Box::pin(async move {
                    infrastructure_detail_readlists::delete_persisted_readlist(
                        database_file.as_path(),
                        &readlist_id,
                    )
                    .await
                })
            }),
            upsert_readlist_search_document: Arc::new(|database_file, index_dir, readlist_id| {
                Box::pin(async move {
                    sync_entity_upsert_from_database(
                        database_file.as_path(),
                        index_dir.as_path(),
                        SearchEntityType::ReadList,
                        &readlist_id,
                    )
                })
            }),
            delete_readlist_search_document: Arc::new(|index_dir, readlist_id| {
                Box::pin(async move {
                    sync_entity_delete_from_index(
                        index_dir.as_path(),
                        SearchEntityType::ReadList,
                        &readlist_id,
                    )
                })
            }),
        },
        series: DiscoveryDetailSeriesAccessBackend {
            load_persisted_series_resource: Arc::new(|database_file, series_id| {
                Box::pin(async move {
                    infrastructure_detail_series::load_persisted_series_resource(
                        database_file.as_path(),
                        &series_id,
                    )
                    .await
                    .map(|value| {
                        value.map(|row| PersistedSeriesResourceRecord {
                            library_id: row.library_id,
                            age_rating: row.age_rating,
                            sharing_labels: row.sharing_labels,
                        })
                    })
                })
            }),
            load_series_id_by_sorted_position: Arc::new(|database_file, index| {
                Box::pin(async move {
                    infrastructure_detail_series::load_series_id_by_sorted_position(
                        database_file.as_path(),
                        index,
                    )
                    .await
                })
            }),
            load_persisted_series_detail: Arc::new(|database_file, series_id| {
                Box::pin(async move {
                    infrastructure_detail_series::load_persisted_series_detail(
                        database_file.as_path(),
                        &series_id,
                    )
                    .await
                    .map(|value| {
                        value.map(|row| PersistedSeriesDetailRecord {
                            id: row.id,
                            library_id: row.library_id,
                            title: row.title,
                            title_sort: row.title_sort,
                            url: row.url,
                            created: row.created,
                            last_modified: row.last_modified,
                            file_last_modified: row.file_last_modified,
                            books_count: row.books_count,
                            status: row.status,
                            summary: row.summary,
                            reading_direction: row.reading_direction,
                            publisher: row.publisher,
                            age_rating: row.age_rating,
                            language: row.language,
                            sharing_labels: row.sharing_labels,
                            metadata_created: row.metadata_created,
                            metadata_last_modified: row.metadata_last_modified,
                            deleted: row.deleted,
                            oneshot: row.oneshot,
                        })
                    })
                })
            }),
            load_persisted_series_summaries: Arc::new(|database_file| {
                Box::pin(async move {
                    infrastructure_discovery::load_persisted_series_summaries(
                        database_file.as_path(),
                    )
                    .await
                    .map(|rows| {
                        rows.into_iter()
                            .map(|row| SeriesSummaryRecord {
                                id: row.id,
                                genres: row.genres,
                                tags: row.tags,
                                alternate_titles: row.alternate_titles,
                                books_metadata_tags: row.books_metadata_tags,
                                books_metadata_release_date: row.books_metadata_release_date,
                                books_metadata_summary: row.books_metadata_summary,
                                books_metadata_summary_number: row.books_metadata_summary_number,
                                books_metadata_created: row.books_metadata_created,
                                books_metadata_last_modified: row.books_metadata_last_modified,
                            })
                            .collect()
                    })
                })
            }),
            load_series_total_book_counts: Arc::new(|database_file| {
                Box::pin(async move {
                    infrastructure_discovery::load_series_total_book_counts(database_file.as_path())
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
            load_persisted_series_collections: Arc::new(|database_file, series_id| {
                Box::pin(async move {
                    infrastructure_detail_series::load_persisted_series_collections(
                        database_file.as_path(),
                        &series_id,
                    )
                    .await
                    .map(|rows| {
                        rows.into_iter()
                            .map(|row| PersistedSeriesCollectionRecord {
                                id: row.id,
                                name: row.name,
                                ordered: row.ordered,
                                series_ids: row.series_ids,
                                created_date: row.created_date,
                                last_modified_date: row.last_modified_date,
                            })
                            .collect()
                    })
                })
            }),
            load_existing_series_metadata: Arc::new(|database_file, series_id| {
                Box::pin(async move {
                    infrastructure_detail_series::load_existing_series_metadata(
                        database_file.as_path(),
                        &series_id,
                    )
                    .await
                    .map(|value| {
                        value.map(|row| ExistingSeriesMetadataRecord {
                            status: row.status,
                            status_lock: row.status_lock,
                            title: row.title,
                            title_lock: row.title_lock,
                            title_sort: row.title_sort,
                            title_sort_lock: row.title_sort_lock,
                            summary: row.summary,
                            summary_lock: row.summary_lock,
                            reading_direction: row.reading_direction,
                            reading_direction_lock: row.reading_direction_lock,
                            publisher: row.publisher,
                            publisher_lock: row.publisher_lock,
                            age_rating: row.age_rating,
                            age_rating_lock: row.age_rating_lock,
                            language: row.language,
                            language_lock: row.language_lock,
                            genres: row.genres,
                            genres_lock: row.genres_lock,
                            tags: row.tags,
                            tags_lock: row.tags_lock,
                            total_book_count: row.total_book_count,
                            total_book_count_lock: row.total_book_count_lock,
                            sharing_labels: row.sharing_labels,
                            sharing_labels_lock: row.sharing_labels_lock,
                            links: row
                                .links
                                .into_iter()
                                .map(|link| SeriesMetadataLinkRecord {
                                    label: link.label,
                                    url: link.url,
                                })
                                .collect(),
                            links_lock: row.links_lock,
                            alternate_titles: row
                                .alternate_titles
                                .into_iter()
                                .map(|title| SeriesAlternateTitleRecord {
                                    label: title.label,
                                    title: title.title,
                                })
                                .collect(),
                            alternate_titles_lock: row.alternate_titles_lock,
                        })
                    })
                })
            }),
            persist_series_metadata_update: Arc::new(|database_file, series_id, update| {
                Box::pin(async move {
                    infrastructure_detail_series::persist_series_metadata_update(
                        database_file.as_path(),
                        &series_id,
                        infrastructure_detail_series::SeriesMetadataUpdateRecord {
                            status: update.status,
                            status_lock: update.status_lock,
                            title: update.title,
                            title_lock: update.title_lock,
                            title_sort: update.title_sort,
                            title_sort_lock: update.title_sort_lock,
                            summary: update.summary,
                            summary_lock: update.summary_lock,
                            reading_direction: update.reading_direction,
                            reading_direction_lock: update.reading_direction_lock,
                            publisher: update.publisher,
                            publisher_lock: update.publisher_lock,
                            age_rating: update.age_rating,
                            age_rating_lock: update.age_rating_lock,
                            language: update.language,
                            language_lock: update.language_lock,
                            genres: update.genres,
                            genres_lock: update.genres_lock,
                            tags: update.tags,
                            tags_lock: update.tags_lock,
                            total_book_count: update.total_book_count,
                            total_book_count_lock: update.total_book_count_lock,
                            sharing_labels: update.sharing_labels,
                            sharing_labels_lock: update.sharing_labels_lock,
                            links: update
                                .links
                                .into_iter()
                                .map(|link| {
                                    infrastructure_detail_series::SeriesMetadataLinkRecord {
                                        label: link.label,
                                        url: link.url,
                                    }
                                })
                                .collect(),
                            links_lock: update.links_lock,
                            alternate_titles: update
                                .alternate_titles
                                .into_iter()
                                .map(|title| {
                                    infrastructure_detail_series::SeriesAlternateTitleRecord {
                                        label: title.label,
                                        title: title.title,
                                    }
                                })
                                .collect(),
                            alternate_titles_lock: update.alternate_titles_lock,
                        },
                    )
                    .await
                })
            }),
            refresh_series_search_documents_after_metadata_update: Arc::new(
                |database_file, index_dir, series_id| {
                    Box::pin(async move {
                        infrastructure_detail_series::refresh_series_after_metadata_update(
                            database_file.as_path(),
                            &series_id,
                        )
                        .await?;

                        sync_series_and_oneshot_books_after_metadata_update(
                            database_file.as_path(),
                            index_dir.as_path(),
                            &series_id,
                        )
                    })
                },
            ),
        },
    }
}

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
        load_persisted_author_names: Arc::new(|database_file, search| {
            Box::pin(async move {
                infrastructure_discovery::load_persisted_author_names(
                    database_file.as_path(),
                    &search,
                )
                .await
            })
        }),
        load_persisted_author_roles: Arc::new(|database_file| {
            Box::pin(async move {
                infrastructure_discovery::load_persisted_author_roles(database_file.as_path()).await
            })
        }),
        load_persisted_authors_by_scope: Arc::new(|database_file, scope| {
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
        }),
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
        load_persisted_book_tags: Arc::new(|database_file, scope| {
            Box::pin(async move {
                let mapped_scope = scope.map(|scope| match scope {
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
        search_series_ids: Arc::new({
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
                        .search_ids(&query, SearchEntityType::Series, limit)
                        .map_err(|error| format!("search index series query: {error}"))
                })
            }
        }),
    }
}
