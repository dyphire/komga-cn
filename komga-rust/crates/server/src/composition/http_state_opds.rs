use super::*;

pub(super) fn install_opds_access_backends() {
    install_opds_manifest_access(OpdsManifestAccessBackend {
        load_manifest_book_record: Arc::new(|database_file, book_id| {
            Box::pin(async move {
                infrastructure_opds_manifest::load_manifest_book_record(
                    database_file.as_path(),
                    &book_id,
                )
                .await
                .map(|value| {
                    value.map(|row| InterfacesManifestBookRecord {
                        title: row.title,
                        file_name: row.file_name,
                        media_type: row.media_type,
                        page_count: row.page_count,
                    })
                })
                .map_err(|error| error.to_string())
            })
        }),
    });

    install_opds_catalog_access(OpdsCatalogAccessBackend {
        load_browse_series_navigation_entries: Arc::new(
            |database_file, allowed_library_ids, library_id, publishers, page, size| {
                Box::pin(async move {
                    infrastructure_opds_catalog::load_browse_series_navigation_entries(
                        database_file.as_path(),
                        &allowed_library_ids,
                        library_id.as_deref(),
                        &publishers,
                        page,
                        size,
                    )
                    .await
                    .map(|(rows, total)| {
                        (
                            rows.into_iter()
                                .map(|row| InterfacesBrowseSeriesNavigationEntry {
                                    id: row.id,
                                    title: row.title,
                                })
                                .collect(),
                            total,
                        )
                    })
                    .map_err(|error| error.to_string())
                })
            },
        ),
        load_browse_publisher_entries: Arc::new(
            |database_file, allowed_library_ids, library_id| {
                Box::pin(async move {
                    infrastructure_opds_catalog::load_browse_publisher_entries(
                        database_file.as_path(),
                        &allowed_library_ids,
                        library_id.as_deref(),
                    )
                    .await
                    .map(|rows| {
                        rows.into_iter()
                            .map(|row| InterfacesBrowsePublisherEntry {
                                publisher: row.publisher,
                            })
                            .collect()
                    })
                    .map_err(|error| error.to_string())
                })
            },
        ),
        load_keep_reading_books: Arc::new(|database_file, user_id, library_id| {
            Box::pin(async move {
                infrastructure_opds_catalog::load_keep_reading_books(
                    database_file.as_path(),
                    &user_id,
                    library_id.as_deref(),
                )
                .await
                .map(|rows| rows.into_iter().map(map_opds_book_feed_entry).collect())
                .map_err(|error| error.to_string())
            })
        }),
        load_on_deck_books: Arc::new(|database_file, user_id, library_id| {
            Box::pin(async move {
                infrastructure_opds_catalog::load_on_deck_books(
                    database_file.as_path(),
                    &user_id,
                    library_id.as_deref(),
                )
                .await
                .map(|rows| rows.into_iter().map(map_opds_book_feed_entry).collect())
                .map_err(|error| error.to_string())
            })
        }),
        load_latest_books: Arc::new(|database_file, library_id, limit| {
            Box::pin(async move {
                infrastructure_opds_catalog::load_latest_books(
                    database_file.as_path(),
                    library_id.as_deref(),
                    limit,
                )
                .await
                .map(|rows| rows.into_iter().map(map_opds_book_feed_entry).collect())
                .map_err(|error| error.to_string())
            })
        }),
        load_latest_books_paged: Arc::new(
            |database_file, allowed_library_ids, library_id, offset, limit| {
                Box::pin(async move {
                    infrastructure_opds_catalog::load_latest_books_paged(
                        database_file.as_path(),
                        &allowed_library_ids,
                        library_id.as_deref(),
                        offset,
                        limit,
                    )
                    .await
                    .map(|rows| rows.into_iter().map(map_opds_book_feed_entry).collect())
                    .map_err(|error| error.to_string())
                })
            },
        ),
        load_latest_series: Arc::new(|database_file, library_id, limit| {
            Box::pin(async move {
                infrastructure_opds_catalog::load_latest_series(
                    database_file.as_path(),
                    library_id.as_deref(),
                    limit,
                )
                .await
                .map(|rows| rows.into_iter().map(map_opds_series_entry).collect())
                .map_err(|error| error.to_string())
            })
        }),
        load_latest_series_paged: Arc::new(
            |database_file, allowed_library_ids, library_id, offset, limit| {
                Box::pin(async move {
                    infrastructure_opds_catalog::load_latest_series_paged(
                        database_file.as_path(),
                        &allowed_library_ids,
                        library_id.as_deref(),
                        offset,
                        limit,
                    )
                    .await
                    .map(|rows| rows.into_iter().map(map_opds_series_entry).collect())
                    .map_err(|error| error.to_string())
                })
            },
        ),
        load_library_series: Arc::new(|database_file, library_id, offset, limit| {
            Box::pin(async move {
                infrastructure_opds_catalog::load_library_series(
                    database_file.as_path(),
                    &library_id,
                    offset,
                    limit,
                )
                .await
                .map(|rows| rows.into_iter().map(map_opds_series_entry).collect())
                .map_err(|error| error.to_string())
            })
        }),
        load_series_page: Arc::new(
            |database_file, allowed_library_ids, search, publishers, offset, limit| {
                Box::pin(async move {
                    infrastructure_opds_catalog::load_series_page(
                        database_file.as_path(),
                        &allowed_library_ids,
                        search.as_deref(),
                        &publishers,
                        offset,
                        limit,
                    )
                    .await
                    .map(|rows| rows.into_iter().map(map_opds_series_entry).collect())
                    .map_err(|error| error.to_string())
                })
            },
        ),
        load_all_readlists: Arc::new(|database_file| {
            Box::pin(async move {
                infrastructure_opds_catalog::load_all_readlists(database_file.as_path())
                    .await
                    .map(|rows| {
                        rows.into_iter()
                            .map(|row| InterfacesOpdsReadlistEntry {
                                id: row.id,
                                name: row.name,
                                last_modified: row.last_modified,
                            })
                            .collect()
                    })
                    .map_err(|error| error.to_string())
            })
        }),
    });

    install_opds_persisted_access(OpdsPersistedAccessBackend {
        load_libraries: Arc::new(|database_file| {
            Box::pin(async move {
                infrastructure_opds_persisted::load_libraries(database_file.as_path())
                    .await
                    .map(|rows| rows.into_iter().map(map_persisted_library_record).collect())
                    .map_err(|error| error.to_string())
            })
        }),
        load_library: Arc::new(|database_file, library_id| {
            Box::pin(async move {
                infrastructure_opds_persisted::load_library(database_file.as_path(), &library_id)
                    .await
                    .map(|value| value.map(map_persisted_library_record))
                    .map_err(|error| error.to_string())
            })
        }),
        load_readlists_for_library: Arc::new(|database_file, library_id| {
            Box::pin(async move {
                infrastructure_opds_persisted::load_readlists_for_library(
                    database_file.as_path(),
                    &library_id,
                )
                .await
                .map(|rows| {
                    rows.into_iter()
                        .map(map_persisted_readlist_record)
                        .collect()
                })
                .map_err(|error| error.to_string())
            })
        }),
        load_series: Arc::new(|database_file, series_id| {
            Box::pin(async move {
                infrastructure_opds_persisted::load_series(database_file.as_path(), &series_id)
                    .await
                    .map(|value| value.map(map_persisted_series_record))
                    .map_err(|error| error.to_string())
            })
        }),
        load_series_books_paged: Arc::new(|database_file, series_id, offset, limit| {
            Box::pin(async move {
                infrastructure_opds_persisted::load_series_books_paged(
                    database_file.as_path(),
                    &series_id,
                    offset,
                    limit,
                )
                .await
                .map(|rows| {
                    rows.into_iter()
                        .map(map_persisted_series_book_record)
                        .collect()
                })
                .map_err(|error| error.to_string())
            })
        }),
        load_readlist: Arc::new(|database_file, readlist_id| {
            Box::pin(async move {
                infrastructure_opds_persisted::load_readlist(database_file.as_path(), &readlist_id)
                    .await
                    .map(|value| value.map(map_persisted_readlist_record))
                    .map_err(|error| error.to_string())
            })
        }),
        load_readlist_books: Arc::new(|database_file, readlist_id| {
            Box::pin(async move {
                infrastructure_opds_persisted::load_readlist_books(
                    database_file.as_path(),
                    &readlist_id,
                )
                .await
                .map(|rows| {
                    rows.into_iter()
                        .map(map_persisted_readlist_book_record)
                        .collect()
                })
                .map_err(|error| error.to_string())
            })
        }),
        load_search_results: Arc::new(|database_file, query| {
            Box::pin(async move {
                infrastructure_opds_persisted::load_search_results(database_file.as_path(), &query)
                    .await
                    .map(|(series, books, collections, readlists)| {
                        (
                            series
                                .into_iter()
                                .map(map_persisted_series_search_record)
                                .collect(),
                            books
                                .into_iter()
                                .map(map_persisted_book_search_record)
                                .collect(),
                            collections
                                .into_iter()
                                .map(map_persisted_named_record)
                                .collect(),
                            readlists
                                .into_iter()
                                .map(map_persisted_named_record)
                                .collect(),
                        )
                    })
                    .map_err(|error| error.to_string())
            })
        }),
        load_publishers: Arc::new(|database_file, allowed_library_ids| {
            Box::pin(async move {
                infrastructure_opds_persisted::load_publishers(
                    database_file.as_path(),
                    &allowed_library_ids,
                )
                .await
                .map_err(|error| error.to_string())
            })
        }),
        load_collections: Arc::new(|database_file, library_id| {
            Box::pin(async move {
                infrastructure_opds_persisted::load_collections(
                    database_file.as_path(),
                    library_id.as_deref(),
                )
                .await
                .map(|rows| rows.into_iter().map(map_persisted_named_record).collect())
                .map_err(|error| error.to_string())
            })
        }),
        load_collection: Arc::new(|database_file, collection_id| {
            Box::pin(async move {
                infrastructure_opds_persisted::load_collection(
                    database_file.as_path(),
                    &collection_id,
                )
                .await
                .map(|value| value.map(map_persisted_named_record))
                .map_err(|error| error.to_string())
            })
        }),
        load_collection_books: Arc::new(|database_file, collection_id| {
            Box::pin(async move {
                infrastructure_opds_persisted::load_collection_books(
                    database_file.as_path(),
                    &collection_id,
                )
                .await
                .map(|rows| {
                    rows.into_iter()
                        .map(map_persisted_book_feed_record)
                        .collect()
                })
                .map_err(|error| error.to_string())
            })
        }),
        load_collection_series: Arc::new(|database_file, collection_id| {
            Box::pin(async move {
                infrastructure_opds_persisted::load_collection_series(
                    database_file.as_path(),
                    &collection_id,
                )
                .await
                .map(|rows| rows.into_iter().map(map_persisted_series_record).collect())
                .map_err(|error| error.to_string())
            })
        }),
    });
}

fn map_opds_book_feed_entry(
    row: infrastructure_opds_catalog::OpdsBookFeedEntry,
) -> InterfacesOpdsBookFeedEntry {
    InterfacesOpdsBookFeedEntry {
        id: row.id,
        title: row.title,
        file_name: row.file_name,
        media_type: row.media_type,
        library_id: row.library_id,
        age_rating: row.age_rating,
        sharing_labels: row.sharing_labels,
        last_modified: row.last_modified,
    }
}

fn map_opds_series_entry(
    row: infrastructure_opds_catalog::OpdsSeriesEntry,
) -> InterfacesOpdsSeriesEntry {
    InterfacesOpdsSeriesEntry {
        id: row.id,
        library_id: row.library_id,
        title: row.title,
        last_modified: row.last_modified,
    }
}

fn map_persisted_library_record(
    row: infrastructure_opds_persisted::PersistedLibraryRecord,
) -> InterfacesPersistedLibraryRecord {
    InterfacesPersistedLibraryRecord {
        id: row.id,
        name: row.name,
        last_modified: row.last_modified,
    }
}

fn map_persisted_series_record(
    row: infrastructure_opds_persisted::PersistedSeriesRecord,
) -> InterfacesPersistedSeriesRecord {
    InterfacesPersistedSeriesRecord {
        id: row.id,
        library_id: row.library_id,
        title: row.title,
        last_modified: row.last_modified,
    }
}

fn map_persisted_series_book_record(
    row: infrastructure_opds_persisted::PersistedSeriesBookRecord,
) -> InterfacesPersistedSeriesBookRecord {
    InterfacesPersistedSeriesBookRecord {
        id: row.id,
        title: row.title,
        file_name: row.file_name,
        media_type: row.media_type,
        last_modified: row.last_modified,
    }
}

fn map_persisted_readlist_record(
    row: infrastructure_opds_persisted::PersistedReadlistRecord,
) -> InterfacesPersistedReadlistRecord {
    InterfacesPersistedReadlistRecord {
        id: row.id,
        name: row.name,
        last_modified: row.last_modified,
    }
}

fn map_persisted_readlist_book_record(
    row: infrastructure_opds_persisted::PersistedReadlistBookRecord,
) -> InterfacesPersistedReadlistBookRecord {
    InterfacesPersistedReadlistBookRecord {
        id: row.id,
        title: row.title,
        file_name: row.file_name,
        media_type: row.media_type,
        library_id: row.library_id,
        age_rating: row.age_rating,
        sharing_labels: row.sharing_labels,
        last_modified: row.last_modified,
    }
}

fn map_persisted_series_search_record(
    row: infrastructure_opds_persisted::PersistedSeriesSearchRecord,
) -> InterfacesPersistedSeriesSearchRecord {
    InterfacesPersistedSeriesSearchRecord {
        id: row.id,
        title: row.title,
        library_id: row.library_id,
    }
}

fn map_persisted_book_search_record(
    row: infrastructure_opds_persisted::PersistedBookSearchRecord,
) -> InterfacesPersistedBookSearchRecord {
    InterfacesPersistedBookSearchRecord {
        id: row.id,
        title: row.title,
        library_id: row.library_id,
    }
}

fn map_persisted_named_record(
    row: infrastructure_opds_persisted::PersistedNamedRecord,
) -> InterfacesPersistedNamedRecord {
    InterfacesPersistedNamedRecord {
        id: row.id,
        name: row.name,
    }
}

fn map_persisted_book_feed_record(
    row: infrastructure_opds_persisted::PersistedBookFeedRecord,
) -> InterfacesPersistedBookFeedRecord {
    InterfacesPersistedBookFeedRecord {
        id: row.id,
        title: row.title,
        file_name: row.file_name,
        media_type: row.media_type,
        library_id: row.library_id,
        age_rating: row.age_rating,
        sharing_labels: row.sharing_labels,
        last_modified: row.last_modified,
    }
}
