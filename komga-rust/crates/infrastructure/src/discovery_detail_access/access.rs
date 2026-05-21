use std::collections::HashMap;
use std::path::PathBuf;

use crate::database_handle::DatabaseHandle;
use crate::discovery_persisted_access::{
    runtime_queries, series as infrastructure_discovery_series,
};
use crate::search::index_lifecycle::SearchEntityType;
use crate::search::runtime_tasks::{
    sync_entity_delete_from_index, sync_entity_upsert_from_database,
    sync_series_and_oneshot_books_after_metadata_update,
};

use super::books::{
    self, PersistedBookDetailRecord, PersistedBookResourceRecord,
    PersistedBookSiblingDirectionRecord,
};
use super::collections::{self, PersistedCollectionAccessRecord, PersistedSeriesRestrictionRecord};
use super::readlists::{
    self, DiscoveryPersistedReadlistBookRecord, DiscoveryPersistedReadlistRecord,
    PersistedBookAuthorRecord, PersistedComicrackMatchCandidateRecord,
};
use super::series::{
    self, ExistingSeriesMetadataRecord, PersistedSeriesCollectionRecord,
    PersistedSeriesDetailRecord, PersistedSeriesResourceRecord, SeriesMetadataUpdateRecord,
};
use crate::discovery_persisted_access::models::SeriesSummary;

#[derive(Clone)]
pub struct DiscoveryDetailAccess {
    db: DatabaseHandle,
    index_dir: PathBuf,
}

impl DiscoveryDetailAccess {
    pub fn new(db: DatabaseHandle, index_dir: PathBuf) -> Self {
        Self { db, index_dir }
    }

    pub async fn load_book_id_by_sorted_position(
        &self,
        index: usize,
    ) -> Result<Option<String>, String> {
        books::load_book_id_by_sorted_position(self.db.read_pool(), index).await
    }

    pub async fn load_persisted_book_resource(
        &self,
        book_id: &str,
    ) -> Result<Option<PersistedBookResourceRecord>, String> {
        books::load_persisted_book_resource(self.db.read_pool(), book_id).await
    }

    pub async fn load_persisted_book_detail(
        &self,
        book_id: &str,
        user_id: Option<&str>,
    ) -> Result<Option<PersistedBookDetailRecord>, String> {
        books::load_persisted_book_detail(self.db.read_pool(), book_id, user_id).await
    }

    pub async fn load_persisted_book_sibling_id(
        &self,
        book_id: &str,
        direction: PersistedBookSiblingDirectionRecord,
    ) -> Result<Option<String>, String> {
        books::load_persisted_book_sibling_id(self.db.read_pool(), book_id, direction).await
    }

    pub async fn load_persisted_book_authors(
        &self,
        book_id: &str,
    ) -> Result<Vec<PersistedBookAuthorRecord>, String> {
        readlists::load_persisted_book_authors(self.db.read_pool(), book_id).await
    }

    pub async fn load_series_library_id(&self, series_id: &str) -> Result<Option<String>, String> {
        collections::load_series_library_id(self.db.read_pool(), series_id).await
    }

    pub async fn load_series_restrictions(
        &self,
        series_id: &str,
    ) -> Result<PersistedSeriesRestrictionRecord, String> {
        collections::load_series_restrictions(self.db.read_pool(), series_id).await
    }

    pub async fn load_series_id_by_sorted_position(
        &self,
        index: usize,
    ) -> Result<Option<String>, String> {
        series::load_series_id_by_sorted_position(self.db.read_pool(), index).await
    }

    pub async fn load_persisted_series_resource(
        &self,
        series_id: &str,
    ) -> Result<Option<PersistedSeriesResourceRecord>, String> {
        series::load_persisted_series_resource(self.db.read_pool(), series_id).await
    }

    pub async fn load_persisted_series_detail(
        &self,
        series_id: &str,
    ) -> Result<Option<PersistedSeriesDetailRecord>, String> {
        series::load_persisted_series_detail(self.db.read_pool(), series_id).await
    }

    pub async fn load_persisted_series_summaries(&self) -> Result<Vec<SeriesSummary>, String> {
        infrastructure_discovery_series::load_persisted_series_summaries(self.db.read_pool()).await
    }

    pub async fn load_series_total_book_counts(&self) -> Result<HashMap<String, i64>, String> {
        runtime_queries::load_series_total_book_counts(self.db.read_pool()).await
    }

    pub async fn load_series_read_progress_counts(
        &self,
        user_id: &str,
    ) -> Result<HashMap<String, (i64, i64)>, String> {
        runtime_queries::load_series_read_progress_counts(self.db.read_pool(), user_id).await
    }

    pub async fn load_persisted_series_collections(
        &self,
        series_id: &str,
    ) -> Result<Vec<PersistedSeriesCollectionRecord>, String> {
        series::load_persisted_series_collections(self.db.read_pool(), series_id).await
    }

    pub async fn load_existing_series_metadata(
        &self,
        series_id: &str,
    ) -> Result<Option<ExistingSeriesMetadataRecord>, String> {
        series::load_existing_series_metadata(self.db.read_pool(), series_id).await
    }

    pub async fn persist_series_metadata_update(
        &self,
        series_id: &str,
        update: SeriesMetadataUpdateRecord,
    ) -> Result<bool, String> {
        series::persist_series_metadata_update(self.db.write_pool(), series_id, update).await
    }

    pub async fn refresh_series_search_documents_after_metadata_update(
        &self,
        series_id: &str,
    ) -> Result<(), String> {
        series::refresh_series_after_metadata_update(self.db.write_pool(), series_id).await?;

        sync_series_and_oneshot_books_after_metadata_update(
            self.db.write_pool(),
            self.db.database_file(),
            self.index_dir.as_path(),
            series_id,
        )
        .await
    }

    pub async fn persisted_collections_exist(&self) -> Result<bool, String> {
        collections::persisted_collections_exist(self.db.read_pool()).await
    }

    pub async fn load_persisted_collections(
        &self,
    ) -> Result<Vec<PersistedCollectionAccessRecord>, String> {
        collections::load_persisted_collections(self.db.read_pool()).await
    }

    pub async fn load_persisted_collection_series_ids(
        &self,
        collection_id: &str,
    ) -> Result<Vec<String>, String> {
        collections::load_persisted_collection_series_ids(self.db.read_pool(), collection_id).await
    }

    pub async fn load_persisted_collection_detail(
        &self,
        collection_id: &str,
    ) -> Result<Option<PersistedCollectionAccessRecord>, String> {
        collections::load_persisted_collection_detail(self.db.read_pool(), collection_id).await
    }

    pub async fn persist_collection_create(
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

    pub async fn persist_collection_update(
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

    pub async fn delete_persisted_collection(&self, collection_id: &str) -> Result<bool, String> {
        collections::delete_persisted_collection(self.db.write_pool(), collection_id).await
    }

    pub async fn upsert_collection_search_document(
        &self,
        collection_id: &str,
    ) -> Result<bool, String> {
        sync_entity_upsert_from_database(
            self.db.write_pool(),
            self.db.database_file(),
            self.index_dir.as_path(),
            SearchEntityType::Collection,
            collection_id,
        )
        .await
    }

    pub async fn delete_collection_search_document(
        &self,
        collection_id: &str,
    ) -> Result<(), String> {
        sync_entity_delete_from_index(
            self.db.write_pool(),
            self.index_dir.as_path(),
            SearchEntityType::Collection,
            collection_id,
        )
        .await
    }

    pub async fn load_persisted_readlists(
        &self,
    ) -> Result<Vec<DiscoveryPersistedReadlistRecord>, String> {
        readlists::load_persisted_readlists(self.db.read_pool()).await
    }

    pub async fn load_persisted_readlist_detail(
        &self,
        readlist_id: &str,
    ) -> Result<Option<DiscoveryPersistedReadlistRecord>, String> {
        readlists::load_persisted_readlist_detail(self.db.read_pool(), readlist_id).await
    }

    pub async fn load_persisted_readlist_book_rows(
        &self,
        readlist_id: &str,
    ) -> Result<Vec<DiscoveryPersistedReadlistBookRecord>, String> {
        readlists::load_persisted_readlist_book_rows(self.db.read_pool(), readlist_id).await
    }

    pub async fn load_comicrack_match_candidates(
        &self,
    ) -> Result<Vec<PersistedComicrackMatchCandidateRecord>, String> {
        readlists::load_comicrack_match_candidates(self.db.read_pool()).await
    }

    pub async fn persist_readlist_create(
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

    pub async fn persist_readlist_update(
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

    pub async fn delete_persisted_readlist(&self, readlist_id: &str) -> Result<bool, String> {
        readlists::delete_persisted_readlist(self.db.write_pool(), readlist_id).await
    }

    pub async fn upsert_readlist_search_document(&self, readlist_id: &str) -> Result<bool, String> {
        sync_entity_upsert_from_database(
            self.db.write_pool(),
            self.db.database_file(),
            self.index_dir.as_path(),
            SearchEntityType::ReadList,
            readlist_id,
        )
        .await
    }

    pub async fn delete_readlist_search_document(&self, readlist_id: &str) -> Result<(), String> {
        sync_entity_delete_from_index(
            self.db.write_pool(),
            self.index_dir.as_path(),
            SearchEntityType::ReadList,
            readlist_id,
        )
        .await
    }
}
