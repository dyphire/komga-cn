#![allow(unused_variables)]

use super::*;

#[derive(Default)]
pub(crate) struct NoopDiscoveryDetailService;
#[async_trait]
impl DiscoveryDetailService for NoopDiscoveryDetailService {
    async fn load_book_id_by_sorted_position(
        &self,
        index: usize,
    ) -> Result<Option<String>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_book_resource(
        &self,
        book_id: &str,
    ) -> Result<Option<PersistedBookResourceRecord>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_book_detail(
        &self,
        book_id: &str,
        user_id: Option<&str>,
    ) -> Result<Option<PersistedBookDetailRecord>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_book_sibling_id(
        &self,
        book_id: &str,
        direction: PersistedBookSiblingDirectionRecord,
    ) -> Result<Option<String>, String> {
        panic!("unused test service")
    }
    async fn persisted_collections_exist(&self) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn load_persisted_collections(
        &self,
    ) -> Result<Vec<PersistedCollectionAccessRecord>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_collection_series_ids(
        &self,
        collection_id: &str,
    ) -> Result<Vec<String>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_collection_detail(
        &self,
        collection_id: &str,
    ) -> Result<Option<PersistedCollectionAccessRecord>, String> {
        panic!("unused test service")
    }
    async fn load_series_library_id(&self, series_id: &str) -> Result<Option<String>, String> {
        panic!("unused test service")
    }
    async fn load_series_restrictions(
        &self,
        series_id: &str,
    ) -> Result<PersistedSeriesRestrictionRecord, String> {
        panic!("unused test service")
    }
    async fn persist_collection_create(
        &self,
        collection_id: &str,
        name: &str,
        ordered: bool,
        series_ids: &[String],
    ) -> Result<(), String> {
        panic!("unused test service")
    }
    async fn persist_collection_update(
        &self,
        collection_id: &str,
        name: &str,
        ordered: bool,
        series_ids: &[String],
    ) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn delete_persisted_collection(&self, collection_id: &str) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn upsert_collection_search_document(&self, collection_id: &str) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn delete_collection_search_document(&self, collection_id: &str) -> Result<(), String> {
        panic!("unused test service")
    }
    async fn load_persisted_readlists(
        &self,
    ) -> Result<Vec<DiscoveryPersistedReadlistRecord>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_readlist_detail(
        &self,
        readlist_id: &str,
    ) -> Result<Option<DiscoveryPersistedReadlistRecord>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_readlist_book_rows(
        &self,
        readlist_id: &str,
    ) -> Result<Vec<DiscoveryPersistedReadlistBookRecord>, String> {
        panic!("unused test service")
    }
    async fn load_comicrack_match_candidates(
        &self,
    ) -> Result<Vec<PersistedComicrackMatchCandidateRecord>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_book_authors(
        &self,
        book_id: &str,
    ) -> Result<Vec<PersistedBookAuthorRecord>, String> {
        panic!("unused test service")
    }
    async fn persist_readlist_create(
        &self,
        readlist_id: &str,
        name: &str,
        summary: &str,
        ordered: bool,
        book_ids: &[String],
    ) -> Result<(), String> {
        panic!("unused test service")
    }
    async fn persist_readlist_update(
        &self,
        readlist_id: &str,
        name: &str,
        summary: &str,
        ordered: bool,
        book_ids: &[String],
    ) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn delete_persisted_readlist(&self, readlist_id: &str) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn upsert_readlist_search_document(&self, readlist_id: &str) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn delete_readlist_search_document(&self, readlist_id: &str) -> Result<(), String> {
        panic!("unused test service")
    }
    async fn load_persisted_series_resource(
        &self,
        series_id: &str,
    ) -> Result<Option<PersistedSeriesResourceRecord>, String> {
        panic!("unused test service")
    }
    async fn load_series_id_by_sorted_position(
        &self,
        index: usize,
    ) -> Result<Option<String>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_series_detail(
        &self,
        series_id: &str,
    ) -> Result<Option<PersistedSeriesDetailRecord>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_series_summaries(&self) -> Result<Vec<SeriesSummaryRecord>, String> {
        panic!("unused test service")
    }
    async fn load_series_total_book_counts(&self) -> Result<HashMap<String, i64>, String> {
        panic!("unused test service")
    }
    async fn load_series_read_progress_counts(
        &self,
        user_id: &str,
    ) -> Result<HashMap<String, (i64, i64)>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_series_collections(
        &self,
        series_id: &str,
    ) -> Result<Vec<PersistedSeriesCollectionRecord>, String> {
        panic!("unused test service")
    }
    async fn load_existing_series_metadata(
        &self,
        series_id: &str,
    ) -> Result<Option<ExistingSeriesMetadataRecord>, String> {
        panic!("unused test service")
    }
    async fn persist_series_metadata_update(
        &self,
        series_id: &str,
        update: SeriesMetadataUpdateRecord,
    ) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn refresh_series_search_documents_after_metadata_update(
        &self,
        series_id: &str,
    ) -> Result<(), String> {
        panic!("unused test service")
    }
}

pub(crate) struct NoopBookMetadataPort;
#[async_trait]
impl komga_application::media_assets::BookMetadataPort for NoopBookMetadataPort {
    async fn load_book_metadata(
        &self,
        _book_id: &str,
    ) -> Result<Option<komga_application::media_assets::BookMetadata>, String> {
        Ok(None)
    }
    async fn load_book_series_id(&self, _book_id: &str) -> Result<Option<String>, String> {
        Ok(None)
    }
    async fn load_book_library_id(&self, _book_id: &str) -> Result<Option<String>, String> {
        Ok(None)
    }
    async fn persist_book_metadata(
        &self,
        _book_id: &str,
        _metadata: &komga_application::media_assets::BookMetadata,
    ) -> Result<bool, String> {
        Ok(false)
    }
}

pub(crate) struct NoopSearchSyncPort;
#[async_trait]
impl komga_application::media_assets::metadata_writer::SearchSyncPort for NoopSearchSyncPort {
    async fn sync_book(&self, _book_id: &str) -> Result<(), String> {
        Ok(())
    }
}
