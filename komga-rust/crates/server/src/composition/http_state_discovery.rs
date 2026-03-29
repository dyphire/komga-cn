use super::*;

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
            persisted_collection_exists: Arc::new(|database_file, collection_id| {
                Box::pin(async move {
                    infrastructure_detail_collections::persisted_collection_exists(
                        database_file.as_path(),
                        &collection_id,
                    )
                    .await
                })
            }),
            load_persisted_collection_series: Arc::new(|database_file, collection_id| {
                Box::pin(async move {
                    infrastructure_detail_collections::load_persisted_collection_series(
                        database_file.as_path(),
                        &collection_id,
                    )
                    .await
                    .map(|rows| {
                        rows.into_iter()
                            .map(|row| PersistedCollectionSeriesAccessRecord {
                                id: row.id,
                                library_id: row.library_id,
                                name: row.name,
                                title: row.title,
                                deleted: row.deleted,
                                oneshot: row.oneshot,
                            })
                            .collect()
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
                            title: row.title,
                            title_sort: row.title_sort,
                            summary: row.summary,
                        })
                    })
                })
            }),
            persist_series_metadata_update: Arc::new(
                |database_file, series_id, title, title_sort, summary| {
                    Box::pin(async move {
                        infrastructure_detail_series::persist_series_metadata_update(
                            database_file.as_path(),
                            &series_id,
                            &title,
                            &title_sort,
                            &summary,
                        )
                        .await
                    })
                },
            ),
            refresh_series_after_metadata_update: Arc::new(|database_file, series_id| {
                Box::pin(async move {
                    infrastructure_detail_series::refresh_series_after_metadata_update(
                        database_file.as_path(),
                        &series_id,
                    )
                    .await
                })
            }),
        },
    }
}

pub(super) fn compose_persisted_discovery_access_backend() -> PersistedDiscoveryAccessBackend {
    PersistedDiscoveryAccessBackend {
        load_persisted_authors: Arc::new(|database_file, library_id| {
            Box::pin(async move {
                let rows = infrastructure_discovery::load_persisted_authors(
                    database_file.as_path(),
                    library_id.as_deref(),
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
                        metadata_tags: row.metadata_tags,
                        metadata_authors: row.metadata_authors,
                    })
                    .collect())
            })
        }),
        persisted_books_exist: Arc::new(|database_file| {
            Box::pin(async move {
                infrastructure_discovery::persisted_books_exist(database_file.as_path()).await
            })
        }),
        load_persisted_genres: Arc::new(|database_file, library_id| {
            Box::pin(async move {
                infrastructure_discovery::load_persisted_genres(
                    database_file.as_path(),
                    library_id.as_deref(),
                )
                .await
            })
        }),
        load_persisted_tags: Arc::new(|database_file, library_id| {
            Box::pin(async move {
                infrastructure_discovery::load_persisted_tags(
                    database_file.as_path(),
                    library_id.as_deref(),
                )
                .await
            })
        }),
        load_persisted_languages: Arc::new(|database_file, library_id| {
            Box::pin(async move {
                infrastructure_discovery::load_persisted_languages(
                    database_file.as_path(),
                    library_id.as_deref(),
                )
                .await
            })
        }),
        load_persisted_publishers: Arc::new(|database_file, library_id| {
            Box::pin(async move {
                infrastructure_discovery::load_persisted_publishers(
                    database_file.as_path(),
                    library_id.as_deref(),
                )
                .await
            })
        }),
        load_persisted_age_ratings: Arc::new(|database_file, library_id| {
            Box::pin(async move {
                infrastructure_discovery::load_persisted_age_ratings(
                    database_file.as_path(),
                    library_id.as_deref(),
                )
                .await
            })
        }),
        load_persisted_sharing_labels: Arc::new(|database_file, library_id| {
            Box::pin(async move {
                infrastructure_discovery::load_persisted_sharing_labels(
                    database_file.as_path(),
                    library_id.as_deref(),
                )
                .await
            })
        }),
        load_persisted_series_release_dates: Arc::new(|database_file, library_id| {
            Box::pin(async move {
                infrastructure_discovery::load_persisted_series_release_dates(
                    database_file.as_path(),
                    library_id.as_deref(),
                )
                .await
            })
        }),
        load_persisted_series_tags: Arc::new(|database_file, library_id, collection_id| {
            Box::pin(async move {
                infrastructure_discovery::load_persisted_series_tags(
                    database_file.as_path(),
                    library_id.as_deref(),
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
                    PersistedBookTagsScope::Library(library_id) => {
                        infrastructure_discovery::BookTagsScope::Library(library_id)
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
        persisted_series_exist: Arc::new(|database_file| {
            Box::pin(async move {
                infrastructure_discovery::persisted_series_exist(database_file.as_path()).await
            })
        }),
    }
}
