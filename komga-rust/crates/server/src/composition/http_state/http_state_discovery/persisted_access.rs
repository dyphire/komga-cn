use super::index_dirs::{register_discovery_index_dir, resolve_discovery_index_dir};
use super::*;

use std::collections::{BTreeMap, BTreeSet, HashMap};

use komga_infrastructure::database_handle::DatabaseHandle;
use komga_infrastructure::search::index_lifecycle::SearchQueryLifecycle;
use komga_interfaces::state::{
    DiscoveryAuthorService, DiscoveryBookFeedService, DiscoveryCollectionSearchService,
    DiscoveryLibraryMappingService, DiscoveryReadlistSearchService,
    PersistedDiscoveryListDataSource,
};

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

fn persisted_book_summary(row: models::BookSummary) -> PersistedBookSummary {
    PersistedBookSummary {
        id: row.id,
        series_id: row.series_id,
        library_id: row.library_id,
        series_title: row.series_title,
        series_title_sort: row.series_title_sort,
        title: row.title,
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

fn persisted_book_browse_entry(row: models::BookBrowseEntry) -> PersistedBookBrowseEntry {
    PersistedBookBrowseEntry {
        id: row.id,
        library_id: row.library_id,
        name: row.name,
        title: row.title,
    }
}

fn persisted_series_summary(row: models::SeriesSummary) -> PersistedSeriesSummary {
    PersistedSeriesSummary {
        id: row.id,
        library_id: row.library_id,
        name: row.name,
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

#[derive(Clone)]
pub(super) struct RuntimePersistedDiscoveryAccess {
    db: DatabaseHandle,
    index_dir: PathBuf,
}

#[async_trait::async_trait]
impl PersistedDiscoveryListDataSource for RuntimePersistedDiscoveryAccess {
    async fn load_book_poster_summaries(
        &self,
    ) -> Result<HashMap<String, Vec<PersistedBookPosterSummary>>, String> {
        let rows =
            infrastructure_discovery_books::load_book_poster_summaries(self.db.database_file())
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
    }

    async fn load_persisted_book_summaries(
        &self,
        user_id: Option<&str>,
    ) -> Result<Vec<PersistedBookSummary>, String> {
        infrastructure_discovery_books::load_persisted_book_summaries(
            self.db.database_file(),
            user_id,
        )
        .await
        .map(|rows| rows.into_iter().map(persisted_book_summary).collect())
    }

    async fn load_persisted_book_summaries_by_ids(
        &self,
        user_id: Option<&str>,
        ids: &[String],
    ) -> Result<Vec<PersistedBookSummary>, String> {
        infrastructure_discovery_books::load_persisted_book_summaries_by_ids(
            self.db.database_file(),
            user_id,
            ids,
        )
        .await
        .map(|rows| rows.into_iter().map(persisted_book_summary).collect())
    }

    async fn load_persisted_book_count(&self) -> Result<usize, String> {
        infrastructure_discovery_books::load_persisted_book_count(self.db.database_file()).await
    }

    async fn load_persisted_genres(
        &self,
        library_ids: Option<&[String]>,
        collection_id: Option<&str>,
    ) -> Result<Vec<String>, String> {
        facets::load_persisted_genres(self.db.database_file(), library_ids, collection_id).await
    }

    async fn load_persisted_tags(
        &self,
        library_ids: Option<&[String]>,
        collection_id: Option<&str>,
    ) -> Result<Vec<String>, String> {
        facets::load_persisted_tags(self.db.database_file(), library_ids, collection_id).await
    }

    async fn load_persisted_languages(
        &self,
        library_ids: Option<&[String]>,
        collection_id: Option<&str>,
    ) -> Result<Vec<String>, String> {
        facets::load_persisted_languages(self.db.database_file(), library_ids, collection_id).await
    }

    async fn load_persisted_publishers(
        &self,
        library_ids: Option<&[String]>,
        collection_id: Option<&str>,
    ) -> Result<Vec<String>, String> {
        facets::load_persisted_publishers(self.db.database_file(), library_ids, collection_id).await
    }

    async fn load_persisted_age_ratings(
        &self,
        library_ids: Option<&[String]>,
        collection_id: Option<&str>,
    ) -> Result<Vec<String>, String> {
        facets::load_persisted_age_ratings(self.db.database_file(), library_ids, collection_id)
            .await
    }

    async fn load_persisted_sharing_labels(
        &self,
        library_ids: Option<&[String]>,
        collection_id: Option<&str>,
    ) -> Result<Vec<String>, String> {
        facets::load_persisted_sharing_labels(self.db.database_file(), library_ids, collection_id)
            .await
    }

    async fn load_persisted_series_release_dates(
        &self,
        library_ids: Option<&[String]>,
        collection_id: Option<&str>,
    ) -> Result<Vec<String>, String> {
        facets::load_persisted_series_release_dates(
            self.db.database_file(),
            library_ids,
            collection_id,
        )
        .await
    }

    async fn load_persisted_series_tags(
        &self,
        library_ids: Option<&[String]>,
        collection_id: Option<&str>,
    ) -> Result<Vec<String>, String> {
        facets::load_persisted_series_tags(self.db.database_file(), library_ids, collection_id)
            .await
    }

    async fn load_collection_memberships(
        &self,
    ) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
        library_mappings::load_collection_memberships(self.db.database_file()).await
    }

    async fn load_collection_ordering(
        &self,
        collection_id: &str,
    ) -> Result<HashMap<String, i64>, String> {
        library_mappings::load_collection_ordering(self.db.database_file(), collection_id).await
    }

    async fn load_readlist_memberships(
        &self,
    ) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
        library_mappings::load_readlist_memberships(self.db.database_file()).await
    }

    async fn load_readlist_ordering(
        &self,
        readlist_id: &str,
    ) -> Result<HashMap<String, i64>, String> {
        library_mappings::load_readlist_ordering(self.db.database_file(), readlist_id).await
    }

    async fn load_persisted_book_tags(
        &self,
        scope: Option<PersistedBookTagsScope>,
        authorized_library_ids: Option<&[String]>,
    ) -> Result<Vec<String>, String> {
        let scope = scope.map(|scope| match scope {
            PersistedBookTagsScope::All => models::BookTagsScope::All,
            PersistedBookTagsScope::Series(id) => models::BookTagsScope::Series(id),
            PersistedBookTagsScope::Libraries(ids) => models::BookTagsScope::Libraries(ids),
            PersistedBookTagsScope::ReadList(id) => models::BookTagsScope::ReadList(id),
        });
        runtime_queries::load_persisted_book_tags(
            self.db.database_file(),
            scope.as_ref(),
            authorized_library_ids,
        )
        .await
    }

    async fn persisted_utc_date_minus_days(&self, days: i64) -> Result<Option<String>, String> {
        runtime_queries::persisted_utc_date_minus_days(self.db.database_file(), days).await
    }

    async fn load_series_read_progress_counts(
        &self,
        user_id: &str,
    ) -> Result<HashMap<String, (i64, i64)>, String> {
        runtime_queries::load_series_read_progress_counts(self.db.database_file(), user_id).await
    }

    async fn load_series_read_dates(
        &self,
        user_id: &str,
    ) -> Result<HashMap<String, String>, String> {
        runtime_queries::load_series_read_dates(self.db.database_file(), user_id).await
    }

    async fn load_series_total_book_counts(&self) -> Result<HashMap<String, i64>, String> {
        runtime_queries::load_series_total_book_counts(self.db.database_file()).await
    }

    async fn load_persisted_series_summaries(&self) -> Result<Vec<PersistedSeriesSummary>, String> {
        infrastructure_discovery_series::load_persisted_series_summaries(self.db.database_file())
            .await
            .map(|rows| rows.into_iter().map(persisted_series_summary).collect())
    }

    async fn load_persisted_series_summaries_by_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<PersistedSeriesSummary>, String> {
        infrastructure_discovery_series::load_persisted_series_summaries_by_ids(
            self.db.database_file(),
            ids,
        )
        .await
        .map(|rows| rows.into_iter().map(persisted_series_summary).collect())
    }

    async fn load_persisted_series_count(&self) -> Result<usize, String> {
        infrastructure_discovery_series::load_persisted_series_count(self.db.database_file()).await
    }

    async fn search_book_ids(&self, query: &str, limit: usize) -> Result<Vec<String>, String> {
        Ok(search_ids_or_empty(
            resolve_discovery_index_dir(self.db.database_file(), self.index_dir.as_path())
                .as_path(),
            query,
            SearchEntityType::Book,
            limit,
        ))
    }

    async fn search_series_scored_ids(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<(f32, String)>, String> {
        Ok(search_scored_ids_or_empty(
            resolve_discovery_index_dir(self.db.database_file(), self.index_dir.as_path())
                .as_path(),
            query,
            SearchEntityType::Series,
            limit,
        ))
    }
}

#[async_trait::async_trait]
impl DiscoveryAuthorService for RuntimePersistedDiscoveryAccess {
    async fn load_author_names(
        &self,
        search: &str,
        authorized_library_ids: Option<&[String]>,
    ) -> Result<Vec<String>, String> {
        authors::load_persisted_author_names(
            self.db.database_file(),
            search,
            authorized_library_ids,
        )
        .await
    }

    async fn load_author_roles(
        &self,
        authorized_library_ids: Option<&[String]>,
    ) -> Result<Vec<String>, String> {
        authors::load_persisted_author_roles(self.db.database_file(), authorized_library_ids).await
    }

    async fn load_authors_by_scope(
        &self,
        scope: PersistedAuthorsScope,
        authorized_library_ids: Option<&[String]>,
    ) -> Result<Vec<PersistedAuthorEntry>, String> {
        let mapped_scope = match scope {
            PersistedAuthorsScope::All => models::AuthorsScope::All,
            PersistedAuthorsScope::Libraries(ids) => models::AuthorsScope::Libraries(ids),
            PersistedAuthorsScope::Collection(id) => models::AuthorsScope::Collection(id),
            PersistedAuthorsScope::Series(id) => models::AuthorsScope::Series(id),
            PersistedAuthorsScope::ReadList(id) => models::AuthorsScope::ReadList(id),
        };
        let rows = authors::load_persisted_authors_by_scope(
            self.db.database_file(),
            &mapped_scope,
            authorized_library_ids,
        )
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| PersistedAuthorEntry {
                name: row.name,
                role: row.role,
            })
            .collect())
    }
}

#[async_trait::async_trait]
impl DiscoveryLibraryMappingService for RuntimePersistedDiscoveryAccess {
    async fn load_persisted_library_ids(&self) -> Result<Vec<String>, String> {
        library_mappings::load_persisted_library_ids(self.db.database_file()).await
    }
}

#[async_trait::async_trait]
impl DiscoveryCollectionSearchService for RuntimePersistedDiscoveryAccess {
    async fn search_collection_ids(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<String>, String> {
        Ok(search_ids_or_empty(
            resolve_discovery_index_dir(self.db.database_file(), self.index_dir.as_path())
                .as_path(),
            query,
            SearchEntityType::Collection,
            limit,
        ))
    }
}

#[async_trait::async_trait]
impl DiscoveryReadlistSearchService for RuntimePersistedDiscoveryAccess {
    async fn search_readlist_scored_ids(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<(f32, String)>, String> {
        Ok(search_scored_ids_or_empty(
            resolve_discovery_index_dir(self.db.database_file(), self.index_dir.as_path())
                .as_path(),
            query,
            SearchEntityType::ReadList,
            limit,
        ))
    }
}

#[async_trait::async_trait]
impl DiscoveryBookFeedService for RuntimePersistedDiscoveryAccess {
    async fn load_ondeck_books(
        &self,
        user_id: &str,
    ) -> Result<Vec<PersistedBookBrowseEntry>, String> {
        runtime_queries::load_persisted_ondeck_books(self.db.database_file(), user_id)
            .await
            .map(|rows| rows.into_iter().map(persisted_book_browse_entry).collect())
    }

    async fn load_duplicate_books(&self) -> Result<Vec<PersistedBookBrowseEntry>, String> {
        runtime_queries::load_persisted_duplicate_books(self.db.database_file())
            .await
            .map(|rows| rows.into_iter().map(persisted_book_browse_entry).collect())
    }
}

pub(super) fn compose_persisted_discovery_list_data_source(
    db: DatabaseHandle,
    lucene_data_directory: PathBuf,
) -> Box<dyn PersistedDiscoveryListDataSource> {
    register_discovery_index_dir(db.database_file(), lucene_data_directory.as_path());
    Box::new(RuntimePersistedDiscoveryAccess {
        db,
        index_dir: lucene_data_directory,
    })
}

pub(super) fn compose_discovery_author_service(
    db: DatabaseHandle,
    lucene_data_directory: PathBuf,
) -> Box<dyn DiscoveryAuthorService> {
    Box::new(RuntimePersistedDiscoveryAccess {
        db,
        index_dir: lucene_data_directory,
    })
}

pub(super) fn compose_discovery_library_mapping_service(
    db: DatabaseHandle,
    lucene_data_directory: PathBuf,
) -> Box<dyn DiscoveryLibraryMappingService> {
    Box::new(RuntimePersistedDiscoveryAccess {
        db,
        index_dir: lucene_data_directory,
    })
}

pub(super) fn compose_discovery_collection_search_service(
    db: DatabaseHandle,
    lucene_data_directory: PathBuf,
) -> Box<dyn DiscoveryCollectionSearchService> {
    Box::new(RuntimePersistedDiscoveryAccess {
        db,
        index_dir: lucene_data_directory,
    })
}

pub(super) fn compose_discovery_readlist_search_service(
    db: DatabaseHandle,
    lucene_data_directory: PathBuf,
) -> Box<dyn DiscoveryReadlistSearchService> {
    Box::new(RuntimePersistedDiscoveryAccess {
        db,
        index_dir: lucene_data_directory,
    })
}

pub(super) fn compose_discovery_book_feed_service(
    db: DatabaseHandle,
    lucene_data_directory: PathBuf,
) -> Box<dyn DiscoveryBookFeedService> {
    Box::new(RuntimePersistedDiscoveryAccess {
        db,
        index_dir: lucene_data_directory,
    })
}
