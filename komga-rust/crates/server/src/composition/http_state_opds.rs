use super::*;
use std::collections::HashMap;

use komga_infrastructure::{SearchEntityType, SearchIndexLifecycle};

use super::http_state_discovery::resolve_discovery_index_dir;

const OPDS_SEARCH_GROUP_LIMIT: i64 = 20;

pub(super) fn install_opds_access_backends(lucene_data_directory: &std::path::Path) {
    let lucene_data_directory = lucene_data_directory.to_path_buf();
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
            |database_file, allowed_library_ids, user_id, library_id, offset, limit| {
                Box::pin(async move {
                    infrastructure_opds_catalog::load_latest_books_paged(
                        database_file.as_path(),
                        &allowed_library_ids,
                        user_id.as_deref(),
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
        load_series_books_paged: Arc::new(|database_file, series_id, user_id, offset, limit| {
            Box::pin(async move {
                infrastructure_opds_persisted::load_series_books_paged(
                    database_file.as_path(),
                    &series_id,
                    &user_id,
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
        load_series_tags: Arc::new(|database_file, series_id| {
            Box::pin(async move {
                infrastructure_opds_persisted::load_series_tags(database_file.as_path(), &series_id)
                    .await
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
        load_unified_search_results: Arc::new({
            let default_index_dir = lucene_data_directory.clone();
            move |database_file, query| {
                let default_index_dir = default_index_dir.clone();
                Box::pin(async move {
                    let trimmed_query = query.trim().to_string();
                    let (series, books, collections, readlists) = if trimmed_query.is_empty() {
                        load_blank_opds_search_results(database_file.as_path()).await?
                    } else {
                        let index_dir = resolve_discovery_index_dir(
                            database_file.as_path(),
                            default_index_dir.as_path(),
                        );
                        let index = SearchIndexLifecycle::bootstrap(index_dir.as_path())
                            .map_err(|error| format!("bootstrap OPDS search index: {error}"))?;

                        (
                            load_ranked_series_search_results(
                                database_file.as_path(),
                                &index,
                                &trimmed_query,
                            )
                            .await?,
                            load_ranked_book_search_results(
                                database_file.as_path(),
                                &index,
                                &trimmed_query,
                            )
                            .await?,
                            load_ranked_collection_search_results(
                                database_file.as_path(),
                                &index,
                                &trimmed_query,
                            )
                            .await?,
                            load_ranked_readlist_search_results(
                                database_file.as_path(),
                                &index,
                                &trimmed_query,
                            )
                            .await?,
                        )
                    };

                    Ok((
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
                    ))
                })
            }
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
        load_collection_series: Arc::new(|database_file, collection_id, ordered| {
            Box::pin(async move {
                infrastructure_opds_persisted::load_collection_series(
                    database_file.as_path(),
                    &collection_id,
                    ordered,
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
        series_id: row.series_id,
        title: row.title,
        series_title: row.series_title,
        number: row.number,
        number_sort: row.number_sort,
        summary: row.summary,
        isbn: row.isbn,
        authors: row
            .authors
            .into_iter()
            .map(|author| komga_interfaces::OpdsBookAuthorEntry {
                name: author.name,
                role: author.role,
            })
            .collect(),
        tags: row.tags,
        file_name: row.file_name,
        file_size: row.file_size,
        media_type: row.media_type,
        page_count: row.page_count,
        epub_divina_compatible: row.epub_divina_compatible,
        last_read: row.last_read,
        last_read_date: row.last_read_date,
        library_id: row.library_id,
        age_rating: row.age_rating,
        sharing_labels: row.sharing_labels,
        last_modified: row.last_modified,
        release_date: row.release_date,
    }
}

fn map_opds_series_entry(
    row: infrastructure_opds_catalog::OpdsSeriesEntry,
) -> InterfacesOpdsSeriesEntry {
    InterfacesOpdsSeriesEntry {
        id: row.id,
        library_id: row.library_id,
        title: row.title,
        one_shot: row.one_shot,
        age_rating: row.age_rating,
        sharing_labels: row.sharing_labels,
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
        summary: row.summary,
        age_rating: row.age_rating,
        sharing_labels: row.sharing_labels,
        last_modified: row.last_modified,
    }
}

fn map_persisted_series_book_record(
    row: infrastructure_opds_persisted::PersistedSeriesBookRecord,
) -> InterfacesPersistedSeriesBookRecord {
    InterfacesPersistedSeriesBookRecord {
        id: row.id,
        series_id: row.series_id,
        title: row.title,
        series_title: row.series_title,
        number: row.number,
        number_sort: row.number_sort,
        summary: row.summary,
        isbn: row.isbn,
        authors: row
            .authors
            .into_iter()
            .map(map_persisted_book_author_record)
            .collect(),
        tags: row.tags,
        file_name: row.file_name,
        file_size: row.file_size,
        media_type: row.media_type,
        page_count: row.page_count,
        epub_divina_compatible: row.epub_divina_compatible,
        last_read: row.last_read,
        last_read_date: row.last_read_date,
        library_id: row.library_id,
        age_rating: row.age_rating,
        sharing_labels: row.sharing_labels,
        last_modified: row.last_modified,
        release_date: row.release_date,
    }
}

fn map_persisted_readlist_record(
    row: infrastructure_opds_persisted::PersistedReadlistRecord,
) -> InterfacesPersistedReadlistRecord {
    InterfacesPersistedReadlistRecord {
        id: row.id,
        name: row.name,
        last_modified: row.last_modified,
        ordered: row.ordered,
    }
}

fn map_persisted_book_author_record(
    row: infrastructure_opds_persisted::PersistedBookAuthorRecord,
) -> InterfacesPersistedBookAuthorRecord {
    InterfacesPersistedBookAuthorRecord {
        name: row.name,
        role: row.role,
    }
}

fn map_persisted_readlist_book_record(
    row: infrastructure_opds_persisted::PersistedReadlistBookRecord,
) -> InterfacesPersistedReadlistBookRecord {
    InterfacesPersistedReadlistBookRecord {
        id: row.id,
        series_id: row.series_id,
        title: row.title,
        series_title: row.series_title,
        number: row.number,
        number_sort: row.number_sort,
        summary: row.summary,
        isbn: row.isbn,
        authors: row
            .authors
            .into_iter()
            .map(map_persisted_book_author_record)
            .collect(),
        tags: row.tags,
        file_name: row.file_name,
        file_size: row.file_size,
        media_type: row.media_type,
        media_status: row.media_status,
        page_count: row.page_count,
        epub_divina_compatible: row.epub_divina_compatible,
        library_id: row.library_id,
        age_rating: row.age_rating,
        sharing_labels: row.sharing_labels,
        last_modified: row.last_modified,
        release_date: row.release_date,
    }
}

fn map_persisted_series_search_record(
    row: infrastructure_opds_persisted::PersistedSeriesSearchRecord,
) -> InterfacesPersistedSeriesSearchRecord {
    InterfacesPersistedSeriesSearchRecord {
        id: row.id,
        title: row.title,
        library_id: row.library_id,
        age_rating: row.age_rating,
        sharing_labels: row.sharing_labels,
        last_modified: row.last_modified,
    }
}

fn map_persisted_book_search_record(
    row: infrastructure_opds_persisted::PersistedBookSearchRecord,
) -> InterfacesPersistedBookSearchRecord {
    InterfacesPersistedBookSearchRecord {
        id: row.id,
        series_id: row.series_id,
        title: row.title,
        series_title: row.series_title,
        number: row.number,
        number_sort: row.number_sort,
        summary: row.summary,
        isbn: row.isbn,
        authors: row
            .authors
            .into_iter()
            .map(map_persisted_book_author_record)
            .collect(),
        tags: row.tags,
        file_name: row.file_name,
        file_size: row.file_size,
        media_type: row.media_type,
        page_count: row.page_count,
        epub_divina_compatible: row.epub_divina_compatible,
        library_id: row.library_id,
        age_rating: row.age_rating,
        sharing_labels: row.sharing_labels,
        last_modified: row.last_modified,
        release_date: row.release_date,
    }
}

fn map_persisted_named_record(
    row: infrastructure_opds_persisted::PersistedNamedRecord,
) -> InterfacesPersistedNamedRecord {
    InterfacesPersistedNamedRecord {
        id: row.id,
        name: row.name,
        last_modified: row.last_modified,
        ordered: row.ordered,
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

async fn load_blank_opds_search_results(
    database_file: &std::path::Path,
) -> Result<
    (
        Vec<infrastructure_opds_persisted::PersistedSeriesSearchRecord>,
        Vec<infrastructure_opds_persisted::PersistedBookSearchRecord>,
        Vec<infrastructure_opds_persisted::PersistedNamedRecord>,
        Vec<infrastructure_opds_persisted::PersistedNamedRecord>,
    ),
    String,
> {
    Ok((
        infrastructure_opds_persisted::load_series_search_records_limited(
            database_file,
            OPDS_SEARCH_GROUP_LIMIT,
        )
        .await
        .map_err(|error| format!("load blank OPDS series search rows: {error}"))?,
        infrastructure_opds_persisted::load_book_search_records_limited(
            database_file,
            OPDS_SEARCH_GROUP_LIMIT,
        )
        .await
        .map_err(|error| format!("load blank OPDS book search rows: {error}"))?,
        infrastructure_opds_persisted::load_collection_search_records_limited(
            database_file,
            OPDS_SEARCH_GROUP_LIMIT,
        )
        .await
        .map_err(|error| format!("load blank OPDS collection search rows: {error}"))?,
        infrastructure_opds_persisted::load_readlist_search_records_limited(
            database_file,
            OPDS_SEARCH_GROUP_LIMIT,
        )
        .await
        .map_err(|error| format!("load blank OPDS readlist search rows: {error}"))?,
    ))
}

async fn load_ranked_series_search_results(
    database_file: &std::path::Path,
    index: &SearchIndexLifecycle,
    query: &str,
) -> Result<Vec<infrastructure_opds_persisted::PersistedSeriesSearchRecord>, String> {
    let limit = infrastructure_opds_persisted::load_series_search_count(database_file)
        .await
        .map_err(|error| format!("load OPDS series search count: {error}"))?
        .max(1);
    let ids = index
        .search_ids(query, SearchEntityType::Series, limit)
        .map_err(|error| format!("search OPDS series query: {error}"))?;
    ordered_series_search_rows(database_file, &ids).await
}

async fn load_ranked_book_search_results(
    database_file: &std::path::Path,
    index: &SearchIndexLifecycle,
    query: &str,
) -> Result<Vec<infrastructure_opds_persisted::PersistedBookSearchRecord>, String> {
    let limit = infrastructure_opds_persisted::load_book_search_count(database_file)
        .await
        .map_err(|error| format!("load OPDS book search count: {error}"))?
        .max(1);
    let ids = index
        .search_ids(query, SearchEntityType::Book, limit)
        .map_err(|error| format!("search OPDS book query: {error}"))?;
    ordered_book_search_rows(database_file, &ids).await
}

async fn load_ranked_collection_search_results(
    database_file: &std::path::Path,
    index: &SearchIndexLifecycle,
    query: &str,
) -> Result<Vec<infrastructure_opds_persisted::PersistedNamedRecord>, String> {
    let limit = infrastructure_opds_persisted::load_collection_search_count(database_file)
        .await
        .map_err(|error| format!("load OPDS collection search count: {error}"))?
        .max(1);
    let ids = index
        .search_ids(query, SearchEntityType::Collection, limit)
        .map_err(|error| format!("search OPDS collection query: {error}"))?;
    ordered_collection_search_rows(database_file, &ids).await
}

async fn load_ranked_readlist_search_results(
    database_file: &std::path::Path,
    index: &SearchIndexLifecycle,
    query: &str,
) -> Result<Vec<infrastructure_opds_persisted::PersistedNamedRecord>, String> {
    let limit = infrastructure_opds_persisted::load_readlist_search_count(database_file)
        .await
        .map_err(|error| format!("load OPDS readlist search count: {error}"))?
        .max(1);
    let ids = index
        .search_ids(query, SearchEntityType::ReadList, limit)
        .map_err(|error| format!("search OPDS readlist query: {error}"))?;
    ordered_readlist_search_rows(database_file, &ids).await
}

async fn ordered_series_search_rows(
    database_file: &std::path::Path,
    ids: &[String],
) -> Result<Vec<infrastructure_opds_persisted::PersistedSeriesSearchRecord>, String> {
    let rows = infrastructure_opds_persisted::load_series_search_records_by_ids(database_file, ids)
        .await
        .map_err(|error| format!("load OPDS series search rows by ids: {error}"))?;
    let mut by_id = rows
        .into_iter()
        .map(|row| (row.id.clone(), row))
        .collect::<HashMap<_, _>>();
    Ok(ids.iter().filter_map(|id| by_id.remove(id)).collect())
}

async fn ordered_book_search_rows(
    database_file: &std::path::Path,
    ids: &[String],
) -> Result<Vec<infrastructure_opds_persisted::PersistedBookSearchRecord>, String> {
    let rows = infrastructure_opds_persisted::load_book_search_records_by_ids(database_file, ids)
        .await
        .map_err(|error| format!("load OPDS book search rows by ids: {error}"))?;
    let mut by_id = rows
        .into_iter()
        .map(|row| (row.id.clone(), row))
        .collect::<HashMap<_, _>>();
    Ok(ids.iter().filter_map(|id| by_id.remove(id)).collect())
}

async fn ordered_collection_search_rows(
    database_file: &std::path::Path,
    ids: &[String],
) -> Result<Vec<infrastructure_opds_persisted::PersistedNamedRecord>, String> {
    let rows =
        infrastructure_opds_persisted::load_collection_search_records_by_ids(database_file, ids)
            .await
            .map_err(|error| format!("load OPDS collection search rows by ids: {error}"))?;
    let mut by_id = rows
        .into_iter()
        .map(|row| (row.id.clone(), row))
        .collect::<HashMap<_, _>>();
    Ok(ids.iter().filter_map(|id| by_id.remove(id)).collect())
}

async fn ordered_readlist_search_rows(
    database_file: &std::path::Path,
    ids: &[String],
) -> Result<Vec<infrastructure_opds_persisted::PersistedNamedRecord>, String> {
    let rows =
        infrastructure_opds_persisted::load_readlist_search_records_by_ids(database_file, ids)
            .await
            .map_err(|error| format!("load OPDS readlist search rows by ids: {error}"))?;
    let mut by_id = rows
        .into_iter()
        .map(|row| (row.id.clone(), row))
        .collect::<HashMap<_, _>>();
    Ok(ids.iter().filter_map(|id| by_id.remove(id)).collect())
}
