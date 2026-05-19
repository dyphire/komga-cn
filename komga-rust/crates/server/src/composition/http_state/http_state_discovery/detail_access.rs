use super::*;

#[derive(Clone)]
pub(crate) struct RuntimeDiscoveryDetailService {
    db: DatabaseHandle,
    index_dir: PathBuf,
}

#[async_trait::async_trait]
impl BookAccessService for RuntimeDiscoveryDetailService {
    async fn load_book_id_by_sorted_position(
        &self,
        index: usize,
    ) -> Result<Option<String>, String> {
        infrastructure_detail_books::load_book_id_by_sorted_position(self.db.read_pool(), index)
            .await
    }

    async fn load_persisted_book_resource(
        &self,
        book_id: &str,
    ) -> Result<Option<PersistedBookResourceRecord>, String> {
        infrastructure_detail_books::load_persisted_book_resource(self.db.read_pool(), book_id)
            .await
            .map(|value| {
                value.map(|row| PersistedBookResourceRecord {
                    library_id: row.library_id,
                    age_rating: row.age_rating,
                    sharing_labels: row.sharing_labels,
                })
            })
    }

    async fn load_persisted_book_detail(
        &self,
        book_id: &str,
        user_id: Option<&str>,
    ) -> Result<Option<PersistedBookDetailRecord>, String> {
        infrastructure_detail_books::load_persisted_book_detail(
            self.db.read_pool(),
            book_id,
            user_id,
        )
        .await
        .map(|value| {
            value.map(|row| PersistedBookDetailRecord {
                id: row.id,
                series_id: row.series_id,
                series_title: row.series_title,
                series_title_sort: row.series_title_sort,
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
                metadata_title_lock: row.metadata_title_lock,
                metadata_summary_lock: row.metadata_summary_lock,
                metadata_number_lock: row.metadata_number_lock,
                metadata_number_sort_lock: row.metadata_number_sort_lock,
                metadata_release_date_lock: row.metadata_release_date_lock,
                metadata_authors: row.metadata_authors,
                metadata_authors_lock: row.metadata_authors_lock,
                metadata_tags: row.metadata_tags,
                metadata_tags_lock: row.metadata_tags_lock,
                metadata_isbn: row.metadata_isbn,
                metadata_isbn_lock: row.metadata_isbn_lock,
                metadata_links: row.metadata_links,
                metadata_links_lock: row.metadata_links_lock,
                metadata_created: row.metadata_created,
                metadata_last_modified: row.metadata_last_modified,
                media_epub_divina_compatible: row.media_epub_divina_compatible,
                media_epub_is_kepub: row.media_epub_is_kepub,
                read_progress: row.read_progress.map(|progress| {
                    DiscoveryPersistedReadProgressRecord {
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
    }

    async fn load_persisted_book_sibling_id(
        &self,
        book_id: &str,
        direction: PersistedBookSiblingDirectionRecord,
    ) -> Result<Option<String>, String> {
        let direction = match direction {
            PersistedBookSiblingDirectionRecord::Previous => {
                infrastructure_detail_books::PersistedBookSiblingDirectionRecord::Previous
            }
            PersistedBookSiblingDirectionRecord::Next => {
                infrastructure_detail_books::PersistedBookSiblingDirectionRecord::Next
            }
        };

        infrastructure_detail_books::load_persisted_book_sibling_id(
            self.db.read_pool(),
            book_id,
            direction,
        )
        .await
    }

    async fn load_persisted_book_authors(
        &self,
        book_id: &str,
    ) -> Result<Vec<PersistedBookAuthorRecord>, String> {
        readlists::load_persisted_book_authors(self.db.read_pool(), book_id)
            .await
            .map(|rows| {
                rows.into_iter()
                    .map(|row| PersistedBookAuthorRecord {
                        name: row.name,
                        role: row.role,
                    })
                    .collect()
            })
    }
}

#[async_trait::async_trait]
impl SeriesAccessService for RuntimeDiscoveryDetailService {
    async fn load_series_library_id(&self, series_id: &str) -> Result<Option<String>, String> {
        collections::load_series_library_id(self.db.read_pool(), series_id).await
    }

    async fn load_series_restrictions(
        &self,
        series_id: &str,
    ) -> Result<PersistedSeriesRestrictionRecord, String> {
        collections::load_series_restrictions(self.db.read_pool(), series_id)
            .await
            .map(|row| PersistedSeriesRestrictionRecord {
                age_rating: row.age_rating,
                labels: row.labels,
            })
    }

    async fn load_series_id_by_sorted_position(
        &self,
        index: usize,
    ) -> Result<Option<String>, String> {
        infrastructure_detail_series::load_series_id_by_sorted_position(self.db.read_pool(), index)
            .await
    }

    async fn load_persisted_series_resource(
        &self,
        series_id: &str,
    ) -> Result<Option<PersistedSeriesResourceRecord>, String> {
        infrastructure_detail_series::load_persisted_series_resource(self.db.read_pool(), series_id)
            .await
            .map(|value| {
                value.map(|row| PersistedSeriesResourceRecord {
                    library_id: row.library_id,
                    age_rating: row.age_rating,
                    sharing_labels: row.sharing_labels,
                })
            })
    }

    async fn load_persisted_series_detail(
        &self,
        series_id: &str,
    ) -> Result<Option<PersistedSeriesDetailRecord>, String> {
        infrastructure_detail_series::load_persisted_series_detail(self.db.read_pool(), series_id)
            .await
            .map(|value| {
                value.map(|row| PersistedSeriesDetailRecord {
                    id: row.id,
                    library_id: row.library_id,
                    name: row.name,
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
    }

    async fn load_persisted_series_summaries(&self) -> Result<Vec<SeriesSummaryRecord>, String> {
        infrastructure_discovery_series::load_persisted_series_summaries(self.db.read_pool())
            .await
            .map(|rows| {
                rows.into_iter()
                    .map(|row| SeriesSummaryRecord {
                        id: row.id,
                        genres: row.genres,
                        tags: row.tags,
                        alternate_titles: row.alternate_titles,
                        books_metadata_authors: row.books_metadata_authors,
                        books_metadata_tags: row.books_metadata_tags,
                        books_metadata_release_date: row.books_metadata_release_date,
                        books_metadata_summary: row.books_metadata_summary,
                        books_metadata_summary_number: row.books_metadata_summary_number,
                        books_metadata_created: row.books_metadata_created,
                        books_metadata_last_modified: row.books_metadata_last_modified,
                    })
                    .collect()
            })
    }

    async fn load_series_total_book_counts(&self) -> Result<HashMap<String, i64>, String> {
        runtime_queries::load_series_total_book_counts(self.db.read_pool()).await
    }

    async fn load_series_read_progress_counts(
        &self,
        user_id: &str,
    ) -> Result<HashMap<String, (i64, i64)>, String> {
        runtime_queries::load_series_read_progress_counts(self.db.read_pool(), user_id).await
    }

    async fn load_persisted_series_collections(
        &self,
        series_id: &str,
    ) -> Result<Vec<PersistedSeriesCollectionRecord>, String> {
        infrastructure_detail_series::load_persisted_series_collections(
            self.db.read_pool(),
            series_id,
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
    }

    async fn load_existing_series_metadata(
        &self,
        series_id: &str,
    ) -> Result<Option<ExistingSeriesMetadataRecord>, String> {
        infrastructure_detail_series::load_existing_series_metadata(self.db.read_pool(), series_id)
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
    }

    async fn persist_series_metadata_update(
        &self,
        series_id: &str,
        update: SeriesMetadataUpdateRecord,
    ) -> Result<bool, String> {
        infrastructure_detail_series::persist_series_metadata_update(
            self.db.write_pool(),
            series_id,
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
                    .map(
                        |link| infrastructure_detail_series::SeriesMetadataLinkRecord {
                            label: link.label,
                            url: link.url,
                        },
                    )
                    .collect(),
                links_lock: update.links_lock,
                alternate_titles: update
                    .alternate_titles
                    .into_iter()
                    .map(
                        |title| infrastructure_detail_series::SeriesAlternateTitleRecord {
                            label: title.label,
                            title: title.title,
                        },
                    )
                    .collect(),
                alternate_titles_lock: update.alternate_titles_lock,
            },
        )
        .await
    }

    async fn refresh_series_search_documents_after_metadata_update(
        &self,
        series_id: &str,
    ) -> Result<(), String> {
        infrastructure_detail_series::refresh_series_after_metadata_update(
            self.db.write_pool(),
            series_id,
        )
        .await?;

        sync_series_and_oneshot_books_after_metadata_update(
            self.db.write_pool(),
            self.db.database_file(),
            self.index_dir.as_path(),
            series_id,
        )
        .await
    }
}

#[async_trait::async_trait]
impl CollectionAccessService for RuntimeDiscoveryDetailService {
    async fn persisted_collections_exist(&self) -> Result<bool, String> {
        collections::persisted_collections_exist(self.db.read_pool()).await
    }

    async fn load_persisted_collections(
        &self,
    ) -> Result<Vec<PersistedCollectionAccessRecord>, String> {
        collections::load_persisted_collections(self.db.read_pool())
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
    }

    async fn load_persisted_collection_series_ids(
        &self,
        collection_id: &str,
    ) -> Result<Vec<String>, String> {
        collections::load_persisted_collection_series_ids(self.db.read_pool(), collection_id).await
    }

    async fn load_persisted_collection_detail(
        &self,
        collection_id: &str,
    ) -> Result<Option<PersistedCollectionAccessRecord>, String> {
        collections::load_persisted_collection_detail(self.db.read_pool(), collection_id)
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
    }

    async fn persist_collection_create(
        &self,
        collection_id: &str,
        name: &str,
        ordered: bool,
        series_ids: &[String],
    ) -> Result<(), String> {
        collections::persist_collection_create(
            self.db.write_pool(),
            collection_id,
            name,
            ordered,
            series_ids,
        )
        .await
    }

    async fn persist_collection_update(
        &self,
        collection_id: &str,
        name: &str,
        ordered: bool,
        series_ids: &[String],
    ) -> Result<bool, String> {
        collections::persist_collection_update(
            self.db.write_pool(),
            collection_id,
            name,
            ordered,
            series_ids,
        )
        .await
    }

    async fn delete_persisted_collection(&self, collection_id: &str) -> Result<bool, String> {
        collections::delete_persisted_collection(self.db.write_pool(), collection_id).await
    }

    async fn upsert_collection_search_document(&self, collection_id: &str) -> Result<bool, String> {
        sync_entity_upsert_from_database(
            self.db.write_pool(),
            self.db.database_file(),
            self.index_dir.as_path(),
            SearchEntityType::Collection,
            collection_id,
        )
        .await
    }

    async fn delete_collection_search_document(&self, collection_id: &str) -> Result<(), String> {
        sync_entity_delete_from_index(
            self.db.write_pool(),
            self.index_dir.as_path(),
            SearchEntityType::Collection,
            collection_id,
        )
        .await
    }
}

#[async_trait::async_trait]
impl ReadlistAccessService for RuntimeDiscoveryDetailService {
    async fn load_persisted_readlists(
        &self,
    ) -> Result<Vec<DiscoveryPersistedReadlistRecord>, String> {
        readlists::load_persisted_readlists(self.db.read_pool())
            .await
            .map(|rows| {
                rows.into_iter()
                    .map(|row| DiscoveryPersistedReadlistRecord {
                        id: row.id,
                        name: row.name,
                        summary: row.summary,
                        ordered: row.ordered,
                        created_date: row.created_date,
                        last_modified_date: row.last_modified_date,
                    })
                    .collect()
            })
    }

    async fn load_persisted_readlist_detail(
        &self,
        readlist_id: &str,
    ) -> Result<Option<DiscoveryPersistedReadlistRecord>, String> {
        readlists::load_persisted_readlist_detail(self.db.read_pool(), readlist_id)
            .await
            .map(|value| {
                value.map(|row| DiscoveryPersistedReadlistRecord {
                    id: row.id,
                    name: row.name,
                    summary: row.summary,
                    ordered: row.ordered,
                    created_date: row.created_date,
                    last_modified_date: row.last_modified_date,
                })
            })
    }

    async fn load_persisted_readlist_book_rows(
        &self,
        readlist_id: &str,
    ) -> Result<Vec<DiscoveryPersistedReadlistBookRecord>, String> {
        readlists::load_persisted_readlist_book_rows(self.db.read_pool(), readlist_id)
            .await
            .map(|rows| {
                rows.into_iter()
                    .map(|row| DiscoveryPersistedReadlistBookRecord {
                        book_id: row.book_id,
                        library_id: row.library_id,
                    })
                    .collect()
            })
    }

    async fn load_comicrack_match_candidates(
        &self,
    ) -> Result<Vec<PersistedComicrackMatchCandidateRecord>, String> {
        readlists::load_comicrack_match_candidates(self.db.read_pool())
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
    }

    async fn persist_readlist_create(
        &self,
        readlist_id: &str,
        name: &str,
        summary: &str,
        ordered: bool,
        book_ids: &[String],
    ) -> Result<(), String> {
        readlists::persist_readlist_create(
            self.db.write_pool(),
            readlist_id,
            name,
            summary,
            ordered,
            book_ids,
        )
        .await
    }

    async fn persist_readlist_update(
        &self,
        readlist_id: &str,
        name: &str,
        summary: &str,
        ordered: bool,
        book_ids: &[String],
    ) -> Result<bool, String> {
        readlists::persist_readlist_update(
            self.db.write_pool(),
            readlist_id,
            name,
            summary,
            ordered,
            book_ids,
        )
        .await
    }

    async fn delete_persisted_readlist(&self, readlist_id: &str) -> Result<bool, String> {
        readlists::delete_persisted_readlist(self.db.write_pool(), readlist_id).await
    }

    async fn upsert_readlist_search_document(&self, readlist_id: &str) -> Result<bool, String> {
        sync_entity_upsert_from_database(
            self.db.write_pool(),
            self.db.database_file(),
            self.index_dir.as_path(),
            SearchEntityType::ReadList,
            readlist_id,
        )
        .await
    }

    async fn delete_readlist_search_document(&self, readlist_id: &str) -> Result<(), String> {
        sync_entity_delete_from_index(
            self.db.write_pool(),
            self.index_dir.as_path(),
            SearchEntityType::ReadList,
            readlist_id,
        )
        .await
    }
}

pub(super) fn compose_discovery_detail_service(
    db: DatabaseHandle,
    index_dir: PathBuf,
) -> RuntimeDiscoveryDetailService {
    RuntimeDiscoveryDetailService { db, index_dir }
}
