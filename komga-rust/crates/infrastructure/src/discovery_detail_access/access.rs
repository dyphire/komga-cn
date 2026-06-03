use std::collections::HashMap;
use std::path::PathBuf;

use async_trait::async_trait;
use komga_application::discovery::{
    BookDetailPort, BookMetadataAuthorReadModel, BookReadModel, CollectionPort,
    DiscoveryPersistedReadlistBookRecord, DiscoveryPersistedReadlistRecord,
    ExistingSeriesMetadataRecord, PersistedBookResourceRecord, PersistedBookSiblingDirectionRecord,
    PersistedCollectionAccessRecord, PersistedComicrackMatchCandidateRecord,
    PersistedSeriesCollectionRecord, PersistedSeriesDetailRecord, PersistedSeriesResourceRecord,
    PersistedSeriesRestrictionRecord, ReadlistPort, SeriesDetailPort, SeriesMetadataUpdateRecord,
    SeriesReadModel,
};

use crate::database_handle::DatabaseHandle;
use crate::discovery_persisted_access::{
    runtime_queries, series as infrastructure_discovery_series,
};
use crate::search::sync::SearchIndexSync;

use super::books;
use super::collections;
use super::readlists;
use super::series;

#[derive(Clone)]
pub struct DiscoveryDetailAccess {
    db: DatabaseHandle,
    index_dir: PathBuf,
    owns_search_index: bool,
}

impl DiscoveryDetailAccess {
    pub fn new(db: DatabaseHandle, index_dir: PathBuf, owns_search_index: bool) -> Self {
        Self {
            db,
            index_dir,
            owns_search_index,
        }
    }

    fn search_sync(&self) -> SearchIndexSync {
        SearchIndexSync::new(
            self.db.write_pool().clone(),
            self.index_dir.clone(),
            self.owns_search_index,
        )
    }
}

#[async_trait]
impl BookDetailPort for DiscoveryDetailAccess {
    async fn load_book_id_by_sorted_position(
        &self,
        index: usize,
    ) -> Result<Option<String>, String> {
        books::load_book_id_by_sorted_position(self.db.read_pool(), index).await
    }

    async fn load_persisted_book_resource(
        &self,
        book_id: &str,
    ) -> Result<Option<PersistedBookResourceRecord>, String> {
        books::load_persisted_book_resource(self.db.read_pool(), book_id).await
    }

    async fn load_persisted_book_detail(
        &self,
        book_id: &str,
        user_id: Option<&str>,
    ) -> Result<Option<BookReadModel>, String> {
        books::load_persisted_book_detail(self.db.read_pool(), book_id, user_id).await
    }

    async fn load_persisted_book_sibling_id(
        &self,
        book_id: &str,
        direction: PersistedBookSiblingDirectionRecord,
    ) -> Result<Option<String>, String> {
        books::load_persisted_book_sibling_id(self.db.read_pool(), book_id, direction).await
    }

    async fn load_persisted_book_authors(
        &self,
        book_id: &str,
    ) -> Result<Vec<BookMetadataAuthorReadModel>, String> {
        readlists::load_persisted_book_authors(self.db.read_pool(), book_id).await
    }
}

#[async_trait]
impl SeriesDetailPort for DiscoveryDetailAccess {
    async fn load_series_library_id(&self, series_id: &str) -> Result<Option<String>, String> {
        collections::load_series_library_id(self.db.read_pool(), series_id).await
    }

    async fn load_series_restrictions(
        &self,
        series_id: &str,
    ) -> Result<PersistedSeriesRestrictionRecord, String> {
        collections::load_series_restrictions(self.db.read_pool(), series_id).await
    }

    async fn load_series_id_by_sorted_position(
        &self,
        index: usize,
    ) -> Result<Option<String>, String> {
        series::load_series_id_by_sorted_position(self.db.read_pool(), index).await
    }

    async fn load_persisted_series_resource(
        &self,
        series_id: &str,
    ) -> Result<Option<PersistedSeriesResourceRecord>, String> {
        series::load_persisted_series_resource(self.db.read_pool(), series_id).await
    }

    async fn load_persisted_series_detail(
        &self,
        series_id: &str,
    ) -> Result<Option<PersistedSeriesDetailRecord>, String> {
        series::load_persisted_series_detail(self.db.read_pool(), series_id).await
    }

    async fn load_persisted_series_summaries(&self) -> Result<Vec<SeriesReadModel>, String> {
        let summaries =
            infrastructure_discovery_series::load_persisted_series_summaries(self.db.read_pool())
                .await?;
        Ok(summaries
            .into_iter()
            .map(|s| SeriesReadModel {
                id: s.id,
                library_id: s.library_id,
                name: s.name,
                title: s.title,
                title_sort: s.title_sort,
                labels: s.labels,
                created: s.created,
                last_modified: s.last_modified,
                file_last_modified: s.file_last_modified,
                books_count: s.books_count,
                books_read_count: s.books_read_count,
                books_unread_count: s.books_unread_count,
                books_in_progress_count: s.books_in_progress_count,
                status: s.status,
                summary: s.summary,
                reading_direction: s.reading_direction,
                publisher: s.publisher,
                age_rating: s.age_rating,
                language: s.language,
                genres: s.genres,
                tags: s.tags,
                alternate_titles: s.alternate_titles,
                metadata_created: s.metadata_created,
                metadata_last_modified: s.metadata_last_modified,
                books_metadata_authors: s.books_metadata_authors,
                books_metadata_tags: s.books_metadata_tags,
                books_metadata_release_date: s.books_metadata_release_date,
                books_metadata_summary: s.books_metadata_summary,
                books_metadata_summary_number: s.books_metadata_summary_number,
                books_metadata_created: s.books_metadata_created,
                books_metadata_last_modified: s.books_metadata_last_modified,
                deleted: s.deleted,
                oneshot: s.oneshot,
            })
            .collect())
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
        series::load_persisted_series_collections(self.db.read_pool(), series_id).await
    }

    async fn load_existing_series_metadata(
        &self,
        series_id: &str,
    ) -> Result<Option<ExistingSeriesMetadataRecord>, String> {
        series::load_existing_series_metadata(self.db.read_pool(), series_id).await
    }

    async fn persist_series_metadata_update(
        &self,
        series_id: &str,
        update: SeriesMetadataUpdateRecord,
    ) -> Result<bool, String> {
        series::persist_series_metadata_update(self.db.write_pool(), series_id, update).await
    }

    async fn refresh_series_search_documents_after_metadata_update(
        &self,
        series_id: &str,
    ) -> Result<(), String> {
        series::refresh_series_after_metadata_update(self.db.write_pool(), series_id).await?;

        self.search_sync()
            .refresh_series_after_metadata_update(series_id)
            .await
    }
}

#[async_trait]
impl CollectionPort for DiscoveryDetailAccess {
    async fn persisted_collections_exist(&self) -> Result<bool, String> {
        collections::persisted_collections_exist(self.db.read_pool()).await
    }

    async fn load_persisted_collections(
        &self,
    ) -> Result<Vec<PersistedCollectionAccessRecord>, String> {
        collections::load_persisted_collections(self.db.read_pool()).await
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
        collections::load_persisted_collection_detail(self.db.read_pool(), collection_id).await
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
        self.search_sync().upsert_collection(collection_id).await
    }

    async fn delete_collection_search_document(&self, collection_id: &str) -> Result<(), String> {
        self.search_sync().delete_collection(collection_id).await
    }
}

#[async_trait]
impl ReadlistPort for DiscoveryDetailAccess {
    async fn load_persisted_readlists(
        &self,
    ) -> Result<Vec<DiscoveryPersistedReadlistRecord>, String> {
        readlists::load_persisted_readlists(self.db.read_pool()).await
    }

    async fn load_persisted_readlist_detail(
        &self,
        readlist_id: &str,
    ) -> Result<Option<DiscoveryPersistedReadlistRecord>, String> {
        readlists::load_persisted_readlist_detail(self.db.read_pool(), readlist_id).await
    }

    async fn load_persisted_readlist_book_rows(
        &self,
        readlist_id: &str,
    ) -> Result<Vec<DiscoveryPersistedReadlistBookRecord>, String> {
        readlists::load_persisted_readlist_book_rows(self.db.read_pool(), readlist_id).await
    }

    async fn load_comicrack_match_candidates(
        &self,
    ) -> Result<Vec<PersistedComicrackMatchCandidateRecord>, String> {
        readlists::load_comicrack_match_candidates(self.db.read_pool()).await
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
        self.search_sync().upsert_readlist(readlist_id).await
    }

    async fn delete_readlist_search_document(&self, readlist_id: &str) -> Result<(), String> {
        self.search_sync().delete_readlist(readlist_id).await
    }
}
