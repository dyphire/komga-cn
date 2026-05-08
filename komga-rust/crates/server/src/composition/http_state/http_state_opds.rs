use super::*;
use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use komga_infrastructure::database_handle::DatabaseHandle;
use komga_infrastructure::search::index_lifecycle::{SearchEntityType, SearchQueryLifecycle};
use komga_interfaces::state::{
    BrowsePublisherEntry, BrowseSeriesNavigationEntry, OpdsBookAuthorEntry, OpdsBookFeedEntry,
    OpdsCatalogService, OpdsPersistedBookAuthorRecord, OpdsPersistedService, OpdsReadlistEntry,
    OpdsSeriesEntry, PersistedBookFeedRecord, PersistedBookSearchRecord, PersistedLibraryRecord,
    PersistedNamedRecord, PersistedReadlistBookRecord, PersistedReadlistRecord,
    PersistedSeriesBookRecord, PersistedSeriesRecord, PersistedSeriesSearchRecord,
};

use super::http_state_discovery::resolve_discovery_index_dir;

const OPDS_SEARCH_GROUP_LIMIT: i64 = 20;

#[derive(Clone)]
pub(super) struct RuntimeOpdsCatalogService {
    pub(super) db: DatabaseHandle,
}

#[async_trait]
impl OpdsCatalogService for RuntimeOpdsCatalogService {
    async fn load_browse_series_navigation_entries(
        &self,
        allowed_library_ids: Option<&HashSet<String>>,
        library_id: Option<&str>,
        publishers: &[String],
        page: usize,
        size: usize,
    ) -> Result<(Vec<BrowseSeriesNavigationEntry>, usize), String> {
        opds_catalog_access::load_browse_series_navigation_entries(
            self.db.database_file(),
            allowed_library_ids,
            library_id,
            publishers,
            page,
            size,
        )
        .await
        .map(|(rows, total)| {
            (
                rows.into_iter()
                    .map(|row| BrowseSeriesNavigationEntry {
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
        allowed_library_ids: Option<&HashSet<String>>,
        library_id: Option<&str>,
    ) -> Result<Vec<BrowsePublisherEntry>, String> {
        opds_catalog_access::load_browse_publisher_entries(
            self.db.database_file(),
            allowed_library_ids,
            library_id,
        )
        .await
        .map(|rows| {
            rows.into_iter()
                .map(|row| BrowsePublisherEntry {
                    publisher: row.publisher,
                })
                .collect()
        })
        .map_err(|error| error.to_string())
    }

    async fn load_keep_reading_books(
        &self,
        user_id: &str,
        library_id: Option<&str>,
    ) -> Result<Vec<OpdsBookFeedEntry>, String> {
        opds_catalog_access::load_keep_reading_books(self.db.database_file(), user_id, library_id)
            .await
            .map(|rows| rows.into_iter().map(map_opds_book_feed_entry).collect())
            .map_err(|error| error.to_string())
    }

    async fn load_on_deck_books(
        &self,
        user_id: &str,
        library_id: Option<&str>,
    ) -> Result<Vec<OpdsBookFeedEntry>, String> {
        opds_catalog_access::load_on_deck_books(self.db.database_file(), user_id, library_id)
            .await
            .map(|rows| rows.into_iter().map(map_opds_book_feed_entry).collect())
            .map_err(|error| error.to_string())
    }

    async fn load_latest_books(
        &self,
        library_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<OpdsBookFeedEntry>, String> {
        opds_catalog_access::load_latest_books(self.db.database_file(), library_id, limit)
            .await
            .map(|rows| rows.into_iter().map(map_opds_book_feed_entry).collect())
            .map_err(|error| error.to_string())
    }

    async fn load_latest_books_paged(
        &self,
        allowed_library_ids: Option<&HashSet<String>>,
        user_id: Option<&str>,
        library_id: Option<&str>,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<OpdsBookFeedEntry>, String> {
        opds_catalog_access::load_latest_books_paged(
            self.db.database_file(),
            allowed_library_ids,
            user_id,
            library_id,
            offset,
            limit,
        )
        .await
        .map(|rows| rows.into_iter().map(map_opds_book_feed_entry).collect())
        .map_err(|error| error.to_string())
    }

    async fn load_latest_series(
        &self,
        library_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<OpdsSeriesEntry>, String> {
        opds_catalog_access::load_latest_series(self.db.database_file(), library_id, limit)
            .await
            .map(|rows| rows.into_iter().map(map_opds_series_entry).collect())
            .map_err(|error| error.to_string())
    }

    async fn load_latest_series_paged(
        &self,
        allowed_library_ids: Option<&HashSet<String>>,
        library_id: Option<&str>,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<OpdsSeriesEntry>, String> {
        opds_catalog_access::load_latest_series_paged(
            self.db.database_file(),
            allowed_library_ids,
            library_id,
            offset,
            limit,
        )
        .await
        .map(|rows| rows.into_iter().map(map_opds_series_entry).collect())
        .map_err(|error| error.to_string())
    }

    async fn load_library_series(
        &self,
        library_id: &str,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<OpdsSeriesEntry>, String> {
        opds_catalog_access::load_library_series(self.db.database_file(), library_id, offset, limit)
            .await
            .map(|rows| rows.into_iter().map(map_opds_series_entry).collect())
            .map_err(|error| error.to_string())
    }

    async fn load_series_page(
        &self,
        allowed_library_ids: Option<&HashSet<String>>,
        search: Option<&str>,
        publishers: &[String],
        offset: i64,
        limit: i64,
    ) -> Result<Vec<OpdsSeriesEntry>, String> {
        opds_catalog_access::load_series_page(
            self.db.database_file(),
            allowed_library_ids,
            search,
            publishers,
            offset,
            limit,
        )
        .await
        .map(|rows| rows.into_iter().map(map_opds_series_entry).collect())
        .map_err(|error| error.to_string())
    }

    async fn load_all_readlists(&self) -> Result<Vec<OpdsReadlistEntry>, String> {
        opds_catalog_access::load_all_readlists(self.db.database_file())
            .await
            .map(|rows| {
                rows.into_iter()
                    .map(|row| OpdsReadlistEntry {
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
    pub(super) db: DatabaseHandle,
    pub(super) lucene_data_directory: PathBuf,
}

#[async_trait]
impl OpdsPersistedService for RuntimeOpdsPersistedService {
    async fn load_libraries(&self) -> Result<Vec<PersistedLibraryRecord>, String> {
        opds_persisted_access::load_libraries(self.db.database_file())
            .await
            .map(|rows| rows.into_iter().map(map_persisted_library_record).collect())
            .map_err(|error| error.to_string())
    }

    async fn load_library(
        &self,
        library_id: &str,
    ) -> Result<Option<PersistedLibraryRecord>, String> {
        opds_persisted_access::load_library(self.db.database_file(), library_id)
            .await
            .map(|value| value.map(map_persisted_library_record))
            .map_err(|error| error.to_string())
    }

    async fn load_readlists_for_library(
        &self,
        library_id: &str,
    ) -> Result<Vec<PersistedReadlistRecord>, String> {
        opds_persisted_access::load_readlists_for_library(self.db.database_file(), library_id)
            .await
            .map(|rows| {
                rows.into_iter()
                    .map(map_persisted_readlist_record)
                    .collect()
            })
            .map_err(|error| error.to_string())
    }

    async fn load_series(&self, series_id: &str) -> Result<Option<PersistedSeriesRecord>, String> {
        opds_persisted_access::load_series(self.db.database_file(), series_id)
            .await
            .map(|value| value.map(map_persisted_series_record))
            .map_err(|error| error.to_string())
    }

    async fn load_series_books_paged(
        &self,
        series_id: &str,
        user_id: &str,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<PersistedSeriesBookRecord>, String> {
        opds_persisted_access::load_series_books_paged(
            self.db.database_file(),
            series_id,
            user_id,
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

    async fn load_series_tags(&self, series_id: &str) -> Result<Vec<String>, String> {
        opds_persisted_access::load_series_tags(self.db.database_file(), series_id)
            .await
            .map_err(|error| error.to_string())
    }

    async fn load_readlist(
        &self,
        readlist_id: &str,
    ) -> Result<Option<PersistedReadlistRecord>, String> {
        opds_persisted_access::load_readlist(self.db.database_file(), readlist_id)
            .await
            .map(|value| value.map(map_persisted_readlist_record))
            .map_err(|error| error.to_string())
    }

    async fn load_readlist_books(
        &self,
        readlist_id: &str,
    ) -> Result<Vec<PersistedReadlistBookRecord>, String> {
        opds_persisted_access::load_readlist_books(self.db.database_file(), readlist_id)
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
        query: &str,
    ) -> Result<
        (
            Vec<PersistedSeriesSearchRecord>,
            Vec<PersistedBookSearchRecord>,
            Vec<PersistedNamedRecord>,
            Vec<PersistedNamedRecord>,
        ),
        String,
    > {
        let trimmed_query = query.trim();
        let (series, books, collections, readlists) = if trimmed_query.is_empty() {
            load_blank_opds_search_results(self.db.database_file()).await?
        } else {
            let index_dir = resolve_discovery_index_dir(
                self.db.database_file(),
                self.lucene_data_directory.as_path(),
            );
            match SearchQueryLifecycle::bootstrap(index_dir.as_path()) {
                Ok(index) => (
                    load_ranked_series_search_results(
                        self.db.database_file(),
                        &index,
                        trimmed_query,
                    )
                    .await?,
                    load_ranked_book_search_results(self.db.database_file(), &index, trimmed_query)
                        .await?,
                    load_ranked_collection_search_results(
                        self.db.database_file(),
                        &index,
                        trimmed_query,
                    )
                    .await?,
                    load_ranked_readlist_search_results(
                        self.db.database_file(),
                        &index,
                        trimmed_query,
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
        allowed_library_ids: Option<&HashSet<String>>,
    ) -> Result<Vec<String>, String> {
        opds_persisted_access::load_publishers(self.db.database_file(), allowed_library_ids)
            .await
            .map_err(|error| error.to_string())
    }

    async fn load_collections(
        &self,
        library_id: Option<&str>,
    ) -> Result<Vec<PersistedNamedRecord>, String> {
        opds_persisted_access::load_collections(self.db.database_file(), library_id)
            .await
            .map(|rows| rows.into_iter().map(map_persisted_named_record).collect())
            .map_err(|error| error.to_string())
    }

    async fn load_collection(
        &self,
        collection_id: &str,
    ) -> Result<Option<PersistedNamedRecord>, String> {
        opds_persisted_access::load_collection(self.db.database_file(), collection_id)
            .await
            .map(|value| value.map(map_persisted_named_record))
            .map_err(|error| error.to_string())
    }

    async fn load_collection_books(
        &self,
        collection_id: &str,
    ) -> Result<Vec<PersistedBookFeedRecord>, String> {
        opds_persisted_access::load_collection_books(self.db.database_file(), collection_id)
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
        collection_id: &str,
        ordered: bool,
    ) -> Result<Vec<PersistedSeriesRecord>, String> {
        opds_persisted_access::load_collection_series(
            self.db.database_file(),
            collection_id,
            ordered,
        )
        .await
        .map(|rows| rows.into_iter().map(map_persisted_series_record).collect())
        .map_err(|error| error.to_string())
    }
}

pub(super) fn compose_opds_services(
    db: &DatabaseHandle,
    lucene_data_directory: &std::path::Path,
) -> (RuntimeOpdsCatalogService, RuntimeOpdsPersistedService) {
    (
        RuntimeOpdsCatalogService { db: db.clone() },
        RuntimeOpdsPersistedService {
            db: db.clone(),
            lucene_data_directory: lucene_data_directory.to_path_buf(),
        },
    )
}

fn map_opds_book_feed_entry(row: opds_catalog_access::OpdsBookFeedEntry) -> OpdsBookFeedEntry {
    OpdsBookFeedEntry {
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

fn map_opds_series_entry(row: opds_catalog_access::OpdsSeriesEntry) -> OpdsSeriesEntry {
    OpdsSeriesEntry {
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
    row: opds_persisted_access::PersistedLibraryRecord,
) -> PersistedLibraryRecord {
    PersistedLibraryRecord {
        id: row.id,
        name: row.name,
        last_modified: row.last_modified,
    }
}

fn map_persisted_series_record(
    row: opds_persisted_access::PersistedSeriesRecord,
) -> PersistedSeriesRecord {
    PersistedSeriesRecord {
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
    row: opds_persisted_access::PersistedSeriesBookRecord,
) -> PersistedSeriesBookRecord {
    PersistedSeriesBookRecord {
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
    row: opds_persisted_access::PersistedReadlistRecord,
) -> PersistedReadlistRecord {
    PersistedReadlistRecord {
        id: row.id,
        name: row.name,
        last_modified: row.last_modified,
        ordered: row.ordered,
    }
}

fn map_persisted_book_author_record(
    row: opds_persisted_access::PersistedBookAuthorRecord,
) -> OpdsPersistedBookAuthorRecord {
    OpdsPersistedBookAuthorRecord {
        name: row.name,
        role: row.role,
    }
}

fn map_persisted_readlist_book_record(
    row: opds_persisted_access::PersistedReadlistBookRecord,
) -> PersistedReadlistBookRecord {
    PersistedReadlistBookRecord {
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
    row: opds_persisted_access::PersistedSeriesSearchRecord,
) -> PersistedSeriesSearchRecord {
    PersistedSeriesSearchRecord {
        id: row.id,
        title: row.title,
        library_id: row.library_id,
        age_rating: row.age_rating,
        sharing_labels: row.sharing_labels,
        last_modified: row.last_modified,
    }
}

fn map_persisted_book_search_record(
    row: opds_persisted_access::PersistedBookSearchRecord,
) -> PersistedBookSearchRecord {
    PersistedBookSearchRecord {
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
    row: opds_persisted_access::PersistedNamedRecord,
) -> PersistedNamedRecord {
    PersistedNamedRecord {
        id: row.id,
        name: row.name,
        last_modified: row.last_modified,
        ordered: row.ordered,
    }
}

fn map_persisted_book_feed_record(
    row: opds_persisted_access::PersistedBookFeedRecord,
) -> PersistedBookFeedRecord {
    PersistedBookFeedRecord {
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
        Vec<opds_persisted_access::PersistedSeriesSearchRecord>,
        Vec<opds_persisted_access::PersistedBookSearchRecord>,
        Vec<opds_persisted_access::PersistedNamedRecord>,
        Vec<opds_persisted_access::PersistedNamedRecord>,
    ),
    String,
> {
    Ok((
        opds_persisted_access::load_series_search_records_limited(
            database_file,
            OPDS_SEARCH_GROUP_LIMIT,
        )
        .await
        .map_err(|error| format!("load blank OPDS series search rows: {error}"))?,
        opds_persisted_access::load_book_search_records_limited(
            database_file,
            OPDS_SEARCH_GROUP_LIMIT,
        )
        .await
        .map_err(|error| format!("load blank OPDS book search rows: {error}"))?,
        opds_persisted_access::load_collection_search_records_limited(
            database_file,
            OPDS_SEARCH_GROUP_LIMIT,
        )
        .await
        .map_err(|error| format!("load blank OPDS collection search rows: {error}"))?,
        opds_persisted_access::load_readlist_search_records_limited(
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
) -> Result<Vec<opds_persisted_access::PersistedSeriesSearchRecord>, String> {
    let limit = opds_persisted_access::load_series_search_count(database_file)
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
) -> Result<Vec<opds_persisted_access::PersistedBookSearchRecord>, String> {
    let limit = opds_persisted_access::load_book_search_count(database_file)
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
) -> Result<Vec<opds_persisted_access::PersistedNamedRecord>, String> {
    let limit = opds_persisted_access::load_collection_search_count(database_file)
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
) -> Result<Vec<opds_persisted_access::PersistedNamedRecord>, String> {
    let limit = opds_persisted_access::load_readlist_search_count(database_file)
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
) -> Result<Vec<opds_persisted_access::PersistedSeriesSearchRecord>, String> {
    let rows = opds_persisted_access::load_series_search_records_by_ids(database_file, ids)
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
) -> Result<Vec<opds_persisted_access::PersistedBookSearchRecord>, String> {
    let rows = opds_persisted_access::load_book_search_records_by_ids(database_file, ids)
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
) -> Result<Vec<opds_persisted_access::PersistedNamedRecord>, String> {
    let rows = opds_persisted_access::load_collection_search_records_by_ids(database_file, ids)
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
) -> Result<Vec<opds_persisted_access::PersistedNamedRecord>, String> {
    let rows = opds_persisted_access::load_readlist_search_records_by_ids(database_file, ids)
        .await
        .map_err(|error| format!("load OPDS readlist search rows by ids: {error}"))?;
    let mut by_id = rows
        .into_iter()
        .map(|row| (row.id.clone(), row))
        .collect::<HashMap<_, _>>();
    Ok(ids.iter().filter_map(|id| by_id.remove(id)).collect())
}
