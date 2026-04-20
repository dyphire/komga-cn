use super::*;
use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use komga_infrastructure::search::index_lifecycle::{SearchEntityType, SearchQueryLifecycle};
use komga_interfaces::state::{
    BrowsePublisherEntry, BrowseSeriesNavigationEntry, OpdsBookAuthorEntry, OpdsBookFeedEntry,
    OpdsCatalogService, OpdsPersistedService, OpdsReadlistEntry, OpdsSeriesEntry,
    PersistedBookFeedRecord, PersistedBookSearchRecord, PersistedLibraryRecord,
    PersistedNamedRecord, PersistedReadlistBookRecord, PersistedReadlistRecord,
    PersistedSeriesBookRecord, PersistedSeriesRecord, PersistedSeriesSearchRecord,
};

use super::http_state_discovery::resolve_discovery_index_dir;

const OPDS_SEARCH_GROUP_LIMIT: i64 = 20;

#[derive(Clone)]
pub(super) struct RuntimeOpdsCatalogService;

#[async_trait]
impl OpdsCatalogService for RuntimeOpdsCatalogService {
    async fn load_browse_series_navigation_entries(
        &self,
        database_file: PathBuf,
        allowed_library_ids: Option<HashSet<String>>,
        library_id: Option<String>,
        publishers: Vec<String>,
        page: usize,
        size: usize,
    ) -> Result<(Vec<BrowseSeriesNavigationEntry>, usize), String> {
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
    }

    async fn load_browse_publisher_entries(
        &self,
        database_file: PathBuf,
        allowed_library_ids: Option<HashSet<String>>,
        library_id: Option<String>,
    ) -> Result<Vec<BrowsePublisherEntry>, String> {
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
    }

    async fn load_keep_reading_books(
        &self,
        database_file: PathBuf,
        user_id: String,
        library_id: Option<String>,
    ) -> Result<Vec<OpdsBookFeedEntry>, String> {
        infrastructure_opds_catalog::load_keep_reading_books(
            database_file.as_path(),
            &user_id,
            library_id.as_deref(),
        )
        .await
        .map(|rows| rows.into_iter().map(map_opds_book_feed_entry).collect())
        .map_err(|error| error.to_string())
    }

    async fn load_on_deck_books(
        &self,
        database_file: PathBuf,
        user_id: String,
        library_id: Option<String>,
    ) -> Result<Vec<OpdsBookFeedEntry>, String> {
        infrastructure_opds_catalog::load_on_deck_books(
            database_file.as_path(),
            &user_id,
            library_id.as_deref(),
        )
        .await
        .map(|rows| rows.into_iter().map(map_opds_book_feed_entry).collect())
        .map_err(|error| error.to_string())
    }

    async fn load_latest_books(
        &self,
        database_file: PathBuf,
        library_id: Option<String>,
        limit: i64,
    ) -> Result<Vec<OpdsBookFeedEntry>, String> {
        infrastructure_opds_catalog::load_latest_books(
            database_file.as_path(),
            library_id.as_deref(),
            limit,
        )
        .await
        .map(|rows| rows.into_iter().map(map_opds_book_feed_entry).collect())
        .map_err(|error| error.to_string())
    }

    async fn load_latest_books_paged(
        &self,
        database_file: PathBuf,
        allowed_library_ids: Option<HashSet<String>>,
        user_id: Option<String>,
        library_id: Option<String>,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<OpdsBookFeedEntry>, String> {
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
    }

    async fn load_latest_series(
        &self,
        database_file: PathBuf,
        library_id: Option<String>,
        limit: i64,
    ) -> Result<Vec<OpdsSeriesEntry>, String> {
        infrastructure_opds_catalog::load_latest_series(
            database_file.as_path(),
            library_id.as_deref(),
            limit,
        )
        .await
        .map(|rows| rows.into_iter().map(map_opds_series_entry).collect())
        .map_err(|error| error.to_string())
    }

    async fn load_latest_series_paged(
        &self,
        database_file: PathBuf,
        allowed_library_ids: Option<HashSet<String>>,
        library_id: Option<String>,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<OpdsSeriesEntry>, String> {
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
    }

    async fn load_library_series(
        &self,
        database_file: PathBuf,
        library_id: String,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<OpdsSeriesEntry>, String> {
        infrastructure_opds_catalog::load_library_series(
            database_file.as_path(),
            &library_id,
            offset,
            limit,
        )
        .await
        .map(|rows| rows.into_iter().map(map_opds_series_entry).collect())
        .map_err(|error| error.to_string())
    }

    async fn load_series_page(
        &self,
        database_file: PathBuf,
        allowed_library_ids: Option<HashSet<String>>,
        search: Option<String>,
        publishers: Vec<String>,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<OpdsSeriesEntry>, String> {
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
    }

    async fn load_all_readlists(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<OpdsReadlistEntry>, String> {
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
    }
}

#[derive(Clone)]
pub(super) struct RuntimeOpdsPersistedService {
    pub(super) lucene_data_directory: PathBuf,
}

#[async_trait]
impl OpdsPersistedService for RuntimeOpdsPersistedService {
    async fn load_libraries(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<PersistedLibraryRecord>, String> {
        infrastructure_opds_persisted::load_libraries(database_file.as_path())
            .await
            .map(|rows| rows.into_iter().map(map_persisted_library_record).collect())
            .map_err(|error| error.to_string())
    }

    async fn load_library(
        &self,
        database_file: PathBuf,
        library_id: String,
    ) -> Result<Option<PersistedLibraryRecord>, String> {
        infrastructure_opds_persisted::load_library(database_file.as_path(), &library_id)
            .await
            .map(|value| value.map(map_persisted_library_record))
            .map_err(|error| error.to_string())
    }

    async fn load_readlists_for_library(
        &self,
        database_file: PathBuf,
        library_id: String,
    ) -> Result<Vec<PersistedReadlistRecord>, String> {
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
    }

    async fn load_series(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Option<PersistedSeriesRecord>, String> {
        infrastructure_opds_persisted::load_series(database_file.as_path(), &series_id)
            .await
            .map(|value| value.map(map_persisted_series_record))
            .map_err(|error| error.to_string())
    }

    async fn load_series_books_paged(
        &self,
        database_file: PathBuf,
        series_id: String,
        user_id: String,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<PersistedSeriesBookRecord>, String> {
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
    }

    async fn load_series_tags(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Vec<String>, String> {
        infrastructure_opds_persisted::load_series_tags(database_file.as_path(), &series_id)
            .await
            .map_err(|error| error.to_string())
    }

    async fn load_readlist(
        &self,
        database_file: PathBuf,
        readlist_id: String,
    ) -> Result<Option<PersistedReadlistRecord>, String> {
        infrastructure_opds_persisted::load_readlist(database_file.as_path(), &readlist_id)
            .await
            .map(|value| value.map(map_persisted_readlist_record))
            .map_err(|error| error.to_string())
    }

    async fn load_readlist_books(
        &self,
        database_file: PathBuf,
        readlist_id: String,
    ) -> Result<Vec<PersistedReadlistBookRecord>, String> {
        infrastructure_opds_persisted::load_readlist_books(database_file.as_path(), &readlist_id)
            .await
            .map(|rows| {
                rows.into_iter()
                    .map(map_persisted_readlist_book_record)
                    .collect()
            })
            .map_err(|error| error.to_string())
    }

    async fn load_unified_search_results(
        &self,
        database_file: PathBuf,
        query: String,
    ) -> Result<
        (
            Vec<PersistedSeriesSearchRecord>,
            Vec<PersistedBookSearchRecord>,
            Vec<PersistedNamedRecord>,
            Vec<PersistedNamedRecord>,
        ),
        String,
    > {
        let trimmed_query = query.trim().to_string();
        let (series, books, collections, readlists) = if trimmed_query.is_empty() {
            load_blank_opds_search_results(database_file.as_path()).await?
        } else {
            let index_dir = resolve_discovery_index_dir(
                database_file.as_path(),
                self.lucene_data_directory.as_path(),
            );
            match SearchQueryLifecycle::bootstrap(index_dir.as_path()) {
                Ok(index) => (
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
                ),
                Err(_) => (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
            }
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
    }

    async fn load_publishers(
        &self,
        database_file: PathBuf,
        allowed_library_ids: Option<HashSet<String>>,
    ) -> Result<Vec<String>, String> {
        infrastructure_opds_persisted::load_publishers(
            database_file.as_path(),
            &allowed_library_ids,
        )
        .await
        .map_err(|error| error.to_string())
    }

    async fn load_collections(
        &self,
        database_file: PathBuf,
        library_id: Option<String>,
    ) -> Result<Vec<PersistedNamedRecord>, String> {
        infrastructure_opds_persisted::load_collections(
            database_file.as_path(),
            library_id.as_deref(),
        )
        .await
        .map(|rows| rows.into_iter().map(map_persisted_named_record).collect())
        .map_err(|error| error.to_string())
    }

    async fn load_collection(
        &self,
        database_file: PathBuf,
        collection_id: String,
    ) -> Result<Option<PersistedNamedRecord>, String> {
        infrastructure_opds_persisted::load_collection(database_file.as_path(), &collection_id)
            .await
            .map(|value| value.map(map_persisted_named_record))
            .map_err(|error| error.to_string())
    }

    async fn load_collection_books(
        &self,
        database_file: PathBuf,
        collection_id: String,
    ) -> Result<Vec<PersistedBookFeedRecord>, String> {
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
    }

    async fn load_collection_series(
        &self,
        database_file: PathBuf,
        collection_id: String,
        ordered: bool,
    ) -> Result<Vec<PersistedSeriesRecord>, String> {
        infrastructure_opds_persisted::load_collection_series(
            database_file.as_path(),
            &collection_id,
            ordered,
        )
        .await
        .map(|rows| rows.into_iter().map(map_persisted_series_record).collect())
        .map_err(|error| error.to_string())
    }
}

pub(super) fn compose_opds_services(
    lucene_data_directory: &std::path::Path,
) -> (RuntimeOpdsCatalogService, RuntimeOpdsPersistedService) {
    (
        RuntimeOpdsCatalogService,
        RuntimeOpdsPersistedService {
            lucene_data_directory: lucene_data_directory.to_path_buf(),
        },
    )
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
            .map(|author| OpdsBookAuthorEntry {
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
    index: &SearchQueryLifecycle,
    query: &str,
) -> Result<Vec<infrastructure_opds_persisted::PersistedSeriesSearchRecord>, String> {
    let limit = infrastructure_opds_persisted::load_series_search_count(database_file)
        .await
        .map_err(|error| format!("load OPDS series search count: {error}"))?
        .max(1);
    let ids = index
        .search_ids(query, SearchEntityType::Series, limit)
        .unwrap_or_default();
    ordered_series_search_rows(database_file, &ids).await
}

async fn load_ranked_book_search_results(
    database_file: &std::path::Path,
    index: &SearchQueryLifecycle,
    query: &str,
) -> Result<Vec<infrastructure_opds_persisted::PersistedBookSearchRecord>, String> {
    let limit = infrastructure_opds_persisted::load_book_search_count(database_file)
        .await
        .map_err(|error| format!("load OPDS book search count: {error}"))?
        .max(1);
    let ids = index
        .search_ids(query, SearchEntityType::Book, limit)
        .unwrap_or_default();
    ordered_book_search_rows(database_file, &ids).await
}

async fn load_ranked_collection_search_results(
    database_file: &std::path::Path,
    index: &SearchQueryLifecycle,
    query: &str,
) -> Result<Vec<infrastructure_opds_persisted::PersistedNamedRecord>, String> {
    let limit = infrastructure_opds_persisted::load_collection_search_count(database_file)
        .await
        .map_err(|error| format!("load OPDS collection search count: {error}"))?
        .max(1);
    let ids = index
        .search_ids(query, SearchEntityType::Collection, limit)
        .unwrap_or_default();
    ordered_collection_search_rows(database_file, &ids).await
}

async fn load_ranked_readlist_search_results(
    database_file: &std::path::Path,
    index: &SearchQueryLifecycle,
    query: &str,
) -> Result<Vec<infrastructure_opds_persisted::PersistedNamedRecord>, String> {
    let limit = infrastructure_opds_persisted::load_readlist_search_count(database_file)
        .await
        .map_err(|error| format!("load OPDS readlist search count: {error}"))?
        .max(1);
    let ids = index
        .search_ids(query, SearchEntityType::ReadList, limit)
        .unwrap_or_default();
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
