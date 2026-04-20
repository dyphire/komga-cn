use super::index_dirs::{register_discovery_index_dir, resolve_discovery_index_dir};
use super::*;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;

use komga_infrastructure::search::index_lifecycle::SearchQueryLifecycle;
use komga_interfaces::state::PersistedDiscoveryService;

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
pub(super) struct RuntimePersistedDiscoveryService {
    lucene_data_directory: PathBuf,
}

#[async_trait::async_trait]
impl PersistedDiscoveryService for RuntimePersistedDiscoveryService {
    async fn load_persisted_author_names(
        &self,
        database_file: PathBuf,
        search: String,
        authorized_library_ids: Option<Vec<String>>,
    ) -> Result<Vec<String>, String> {
        infrastructure_discovery_authors::load_persisted_author_names(
            database_file.as_path(),
            &search,
            authorized_library_ids.as_deref(),
        )
        .await
    }

    async fn load_persisted_author_roles(
        &self,
        database_file: PathBuf,
        authorized_library_ids: Option<Vec<String>>,
    ) -> Result<Vec<String>, String> {
        infrastructure_discovery_authors::load_persisted_author_roles(
            database_file.as_path(),
            authorized_library_ids.as_deref(),
        )
        .await
    }

    async fn load_persisted_authors_by_scope(
        &self,
        database_file: PathBuf,
        scope: PersistedAuthorsScope,
        authorized_library_ids: Option<Vec<String>>,
    ) -> Result<Vec<PersistedAuthorEntry>, String> {
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
    }

    async fn load_book_poster_summaries(
        &self,
        database_file: PathBuf,
    ) -> Result<HashMap<String, Vec<PersistedBookPosterSummary>>, String> {
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
    }

    async fn load_persisted_book_summaries(
        &self,
        database_file: PathBuf,
        user_id: Option<String>,
    ) -> Result<Vec<PersistedBookSummary>, String> {
        infrastructure_discovery_books::load_persisted_book_summaries(
            database_file.as_path(),
            user_id.as_deref(),
        )
        .await
        .map(|rows| rows.into_iter().map(persisted_book_summary).collect())
    }

    async fn load_persisted_book_summaries_by_ids(
        &self,
        database_file: PathBuf,
        user_id: Option<String>,
        ids: Vec<String>,
    ) -> Result<Vec<PersistedBookSummary>, String> {
        infrastructure_discovery_books::load_persisted_book_summaries_by_ids(
            database_file.as_path(),
            user_id.as_deref(),
            ids.as_slice(),
        )
        .await
        .map(|rows| rows.into_iter().map(persisted_book_summary).collect())
    }

    async fn load_persisted_book_count(&self, database_file: PathBuf) -> Result<usize, String> {
        infrastructure_discovery_books::load_persisted_book_count(database_file.as_path()).await
    }

    async fn load_persisted_genres(
        &self,
        database_file: PathBuf,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String> {
        infrastructure_discovery_facets::load_persisted_genres(
            database_file.as_path(),
            library_ids.as_deref(),
            collection_id.as_deref(),
        )
        .await
    }

    async fn load_persisted_tags(
        &self,
        database_file: PathBuf,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String> {
        infrastructure_discovery_facets::load_persisted_tags(
            database_file.as_path(),
            library_ids.as_deref(),
            collection_id.as_deref(),
        )
        .await
    }

    async fn load_persisted_languages(
        &self,
        database_file: PathBuf,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String> {
        infrastructure_discovery_facets::load_persisted_languages(
            database_file.as_path(),
            library_ids.as_deref(),
            collection_id.as_deref(),
        )
        .await
    }

    async fn load_persisted_publishers(
        &self,
        database_file: PathBuf,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String> {
        infrastructure_discovery_facets::load_persisted_publishers(
            database_file.as_path(),
            library_ids.as_deref(),
            collection_id.as_deref(),
        )
        .await
    }

    async fn load_persisted_age_ratings(
        &self,
        database_file: PathBuf,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String> {
        infrastructure_discovery_facets::load_persisted_age_ratings(
            database_file.as_path(),
            library_ids.as_deref(),
            collection_id.as_deref(),
        )
        .await
    }

    async fn load_persisted_sharing_labels(
        &self,
        database_file: PathBuf,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String> {
        infrastructure_discovery_facets::load_persisted_sharing_labels(
            database_file.as_path(),
            library_ids.as_deref(),
            collection_id.as_deref(),
        )
        .await
    }

    async fn load_persisted_series_release_dates(
        &self,
        database_file: PathBuf,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String> {
        infrastructure_discovery_facets::load_persisted_series_release_dates(
            database_file.as_path(),
            library_ids.as_deref(),
            collection_id.as_deref(),
        )
        .await
    }

    async fn load_persisted_series_tags(
        &self,
        database_file: PathBuf,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String> {
        infrastructure_discovery_facets::load_persisted_series_tags(
            database_file.as_path(),
            library_ids.as_deref(),
            collection_id.as_deref(),
        )
        .await
    }

    async fn load_persisted_library_ids(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<String>, String> {
        infrastructure_discovery_library_mappings::load_persisted_library_ids(
            database_file.as_path(),
        )
        .await
    }

    async fn load_collection_memberships(
        &self,
        database_file: PathBuf,
    ) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
        infrastructure_discovery_library_mappings::load_collection_memberships(
            database_file.as_path(),
        )
        .await
    }

    async fn load_collection_ordering(
        &self,
        database_file: PathBuf,
        collection_id: String,
    ) -> Result<HashMap<String, i64>, String> {
        infrastructure_discovery_library_mappings::load_collection_ordering(
            database_file.as_path(),
            &collection_id,
        )
        .await
    }

    async fn load_readlist_memberships(
        &self,
        database_file: PathBuf,
    ) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
        infrastructure_discovery_library_mappings::load_readlist_memberships(
            database_file.as_path(),
        )
        .await
    }

    async fn load_persisted_ondeck_books(
        &self,
        database_file: PathBuf,
        user_id: String,
    ) -> Result<Vec<PersistedBookBrowseEntry>, String> {
        infrastructure_discovery_runtime_queries::load_persisted_ondeck_books(
            database_file.as_path(),
            &user_id,
        )
        .await
        .map(|rows| rows.into_iter().map(persisted_book_browse_entry).collect())
    }

    async fn load_persisted_duplicate_books(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<PersistedBookBrowseEntry>, String> {
        infrastructure_discovery_runtime_queries::load_persisted_duplicate_books(
            database_file.as_path(),
        )
        .await
        .map(|rows| rows.into_iter().map(persisted_book_browse_entry).collect())
    }

    async fn load_persisted_book_tags(
        &self,
        database_file: PathBuf,
        scope: Option<PersistedBookTagsScope>,
        authorized_library_ids: Option<Vec<String>>,
    ) -> Result<Vec<String>, String> {
        let scope = scope.map(|scope| match scope {
            PersistedBookTagsScope::All => infrastructure_discovery_models::BookTagsScope::All,
            PersistedBookTagsScope::Series(id) => {
                infrastructure_discovery_models::BookTagsScope::Series(id)
            }
            PersistedBookTagsScope::Libraries(ids) => {
                infrastructure_discovery_models::BookTagsScope::Libraries(ids)
            }
            PersistedBookTagsScope::ReadList(id) => {
                infrastructure_discovery_models::BookTagsScope::ReadList(id)
            }
        });
        infrastructure_discovery_runtime_queries::load_persisted_book_tags(
            database_file.as_path(),
            scope.as_ref(),
            authorized_library_ids.as_deref(),
        )
        .await
    }

    async fn persisted_utc_date_minus_days(
        &self,
        database_file: PathBuf,
        days: i64,
    ) -> Result<Option<String>, String> {
        infrastructure_discovery_runtime_queries::persisted_utc_date_minus_days(
            database_file.as_path(),
            days,
        )
        .await
    }

    async fn load_series_read_progress_counts(
        &self,
        database_file: PathBuf,
        user_id: String,
    ) -> Result<HashMap<String, (i64, i64)>, String> {
        infrastructure_discovery_runtime_queries::load_series_read_progress_counts(
            database_file.as_path(),
            &user_id,
        )
        .await
    }

    async fn load_series_read_dates(
        &self,
        database_file: PathBuf,
        user_id: String,
    ) -> Result<HashMap<String, String>, String> {
        infrastructure_discovery_runtime_queries::load_series_read_dates(
            database_file.as_path(),
            &user_id,
        )
        .await
    }

    async fn load_series_total_book_counts(
        &self,
        database_file: PathBuf,
    ) -> Result<HashMap<String, i64>, String> {
        infrastructure_discovery_runtime_queries::load_series_total_book_counts(
            database_file.as_path(),
        )
        .await
    }

    async fn load_persisted_series_summaries(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<PersistedSeriesSummary>, String> {
        infrastructure_discovery_series::load_persisted_series_summaries(database_file.as_path())
            .await
            .map(|rows| rows.into_iter().map(persisted_series_summary).collect())
    }

    async fn load_persisted_series_summaries_by_ids(
        &self,
        database_file: PathBuf,
        ids: Vec<String>,
    ) -> Result<Vec<PersistedSeriesSummary>, String> {
        infrastructure_discovery_series::load_persisted_series_summaries_by_ids(
            database_file.as_path(),
            ids.as_slice(),
        )
        .await
        .map(|rows| rows.into_iter().map(persisted_series_summary).collect())
    }

    async fn load_persisted_series_count(&self, database_file: PathBuf) -> Result<usize, String> {
        infrastructure_discovery_series::load_persisted_series_count(database_file.as_path()).await
    }

    async fn persisted_series_exist(&self, database_file: PathBuf) -> Result<bool, String> {
        infrastructure_discovery_series::persisted_series_exist(database_file.as_path()).await
    }

    async fn search_book_ids(
        &self,
        database_file: PathBuf,
        query: String,
        limit: usize,
    ) -> Result<Vec<String>, String> {
        Ok(search_ids_or_empty(
            resolve_discovery_index_dir(
                database_file.as_path(),
                self.lucene_data_directory.as_path(),
            )
            .as_path(),
            &query,
            SearchEntityType::Book,
            limit,
        ))
    }

    async fn search_collection_ids(
        &self,
        database_file: PathBuf,
        query: String,
        limit: usize,
    ) -> Result<Vec<String>, String> {
        Ok(search_ids_or_empty(
            resolve_discovery_index_dir(
                database_file.as_path(),
                self.lucene_data_directory.as_path(),
            )
            .as_path(),
            &query,
            SearchEntityType::Collection,
            limit,
        ))
    }

    async fn search_readlist_scored_ids(
        &self,
        database_file: PathBuf,
        query: String,
        limit: usize,
    ) -> Result<Vec<(f32, String)>, String> {
        Ok(search_scored_ids_or_empty(
            resolve_discovery_index_dir(
                database_file.as_path(),
                self.lucene_data_directory.as_path(),
            )
            .as_path(),
            &query,
            SearchEntityType::ReadList,
            limit,
        ))
    }

    async fn search_series_scored_ids(
        &self,
        database_file: PathBuf,
        query: String,
        limit: usize,
    ) -> Result<Vec<(f32, String)>, String> {
        Ok(search_scored_ids_or_empty(
            resolve_discovery_index_dir(
                database_file.as_path(),
                self.lucene_data_directory.as_path(),
            )
            .as_path(),
            &query,
            SearchEntityType::Series,
            limit,
        ))
    }
}

pub(super) fn compose_persisted_discovery_service(
    database_file: &std::path::Path,
    lucene_data_directory: &std::path::Path,
) -> Arc<dyn PersistedDiscoveryService> {
    register_discovery_index_dir(database_file, lucene_data_directory);
    Arc::new(RuntimePersistedDiscoveryService {
        lucene_data_directory: lucene_data_directory.to_path_buf(),
    })
}
