use super::*;

#[async_trait]
pub trait DiscoveryDetailService: Send + Sync {
    async fn load_book_id_by_sorted_position(
        &self,
        database_file: PathBuf,
        index: usize,
    ) -> Result<Option<String>, String>;

    async fn load_persisted_book_resource(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Option<DiscoveryPersistedBookResourceRecord>, String>;

    async fn load_persisted_book_detail(
        &self,
        database_file: PathBuf,
        book_id: String,
        user_id: Option<String>,
    ) -> Result<Option<DiscoveryPersistedBookDetailRecord>, String>;

    async fn load_persisted_book_sibling_id(
        &self,
        database_file: PathBuf,
        book_id: String,
        direction: PersistedBookSiblingDirectionRecord,
    ) -> Result<Option<String>, String>;

    async fn persisted_collections_exist(&self, database_file: PathBuf) -> Result<bool, String>;

    async fn load_persisted_collections(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<PersistedCollectionAccessRecord>, String>;

    async fn load_persisted_collection_series_ids(
        &self,
        database_file: PathBuf,
        collection_id: String,
    ) -> Result<Vec<String>, String>;

    async fn load_persisted_collection_detail(
        &self,
        database_file: PathBuf,
        collection_id: String,
    ) -> Result<Option<PersistedCollectionAccessRecord>, String>;

    async fn load_series_library_id(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Option<String>, String>;

    async fn load_series_restrictions(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<PersistedSeriesRestrictionRecord, String>;

    async fn persist_collection_create(
        &self,
        database_file: PathBuf,
        collection_id: String,
        name: String,
        ordered: bool,
        series_ids: Vec<String>,
    ) -> Result<(), String>;

    async fn persist_collection_update(
        &self,
        database_file: PathBuf,
        collection_id: String,
        name: String,
        ordered: bool,
        series_ids: Vec<String>,
    ) -> Result<bool, String>;

    async fn delete_persisted_collection(
        &self,
        database_file: PathBuf,
        collection_id: String,
    ) -> Result<bool, String>;

    async fn upsert_collection_search_document(
        &self,
        database_file: PathBuf,
        index_dir: PathBuf,
        collection_id: String,
    ) -> Result<bool, String>;

    async fn delete_collection_search_document(
        &self,
        index_dir: PathBuf,
        collection_id: String,
    ) -> Result<(), String>;

    async fn load_persisted_readlists(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<DiscoveryPersistedReadlistRecord>, String>;

    async fn load_persisted_readlist_detail(
        &self,
        database_file: PathBuf,
        readlist_id: String,
    ) -> Result<Option<DiscoveryPersistedReadlistRecord>, String>;

    async fn load_persisted_readlist_book_rows(
        &self,
        database_file: PathBuf,
        readlist_id: String,
    ) -> Result<Vec<DiscoveryPersistedReadlistBookRecord>, String>;

    async fn load_comicrack_match_candidates(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<PersistedComicrackMatchCandidateRecord>, String>;

    async fn load_persisted_book_authors(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Vec<PersistedBookAuthorRecord>, String>;

    async fn persist_readlist_create(
        &self,
        database_file: PathBuf,
        readlist_id: String,
        name: String,
        summary: String,
        ordered: bool,
        book_ids: Vec<String>,
    ) -> Result<(), String>;

    async fn persist_readlist_update(
        &self,
        database_file: PathBuf,
        readlist_id: String,
        name: String,
        summary: String,
        ordered: bool,
        book_ids: Vec<String>,
    ) -> Result<bool, String>;

    async fn delete_persisted_readlist(
        &self,
        database_file: PathBuf,
        readlist_id: String,
    ) -> Result<bool, String>;

    async fn upsert_readlist_search_document(
        &self,
        database_file: PathBuf,
        index_dir: PathBuf,
        readlist_id: String,
    ) -> Result<bool, String>;

    async fn delete_readlist_search_document(
        &self,
        index_dir: PathBuf,
        readlist_id: String,
    ) -> Result<(), String>;

    async fn load_persisted_series_resource(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Option<PersistedSeriesResourceRecord>, String>;

    async fn load_series_id_by_sorted_position(
        &self,
        database_file: PathBuf,
        index: usize,
    ) -> Result<Option<String>, String>;

    async fn load_persisted_series_detail(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Option<PersistedSeriesDetailRecord>, String>;

    async fn load_persisted_series_summaries(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<SeriesSummaryRecord>, String>;

    async fn load_series_total_book_counts(
        &self,
        database_file: PathBuf,
    ) -> Result<HashMap<String, i64>, String>;

    async fn load_series_read_progress_counts(
        &self,
        database_file: PathBuf,
        user_id: String,
    ) -> Result<HashMap<String, (i64, i64)>, String>;

    async fn load_persisted_series_collections(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Vec<PersistedSeriesCollectionRecord>, String>;

    async fn load_existing_series_metadata(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Option<ExistingSeriesMetadataRecord>, String>;

    async fn persist_series_metadata_update(
        &self,
        database_file: PathBuf,
        series_id: String,
        update: SeriesMetadataUpdateRecord,
    ) -> Result<bool, String>;

    async fn refresh_series_search_documents_after_metadata_update(
        &self,
        database_file: PathBuf,
        index_dir: PathBuf,
        series_id: String,
    ) -> Result<(), String>;
}

#[async_trait]
impl<T> DiscoveryDetailService for Arc<T>
where
    T: DiscoveryDetailService + ?Sized,
{
    async fn load_book_id_by_sorted_position(
        &self,
        database_file: PathBuf,
        index: usize,
    ) -> Result<Option<String>, String> {
        (**self)
            .load_book_id_by_sorted_position(database_file, index)
            .await
    }

    async fn load_persisted_book_resource(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Option<DiscoveryPersistedBookResourceRecord>, String> {
        (**self)
            .load_persisted_book_resource(database_file, book_id)
            .await
    }

    async fn load_persisted_book_detail(
        &self,
        database_file: PathBuf,
        book_id: String,
        user_id: Option<String>,
    ) -> Result<Option<DiscoveryPersistedBookDetailRecord>, String> {
        (**self)
            .load_persisted_book_detail(database_file, book_id, user_id)
            .await
    }

    async fn load_persisted_book_sibling_id(
        &self,
        database_file: PathBuf,
        book_id: String,
        direction: PersistedBookSiblingDirectionRecord,
    ) -> Result<Option<String>, String> {
        (**self)
            .load_persisted_book_sibling_id(database_file, book_id, direction)
            .await
    }

    async fn persisted_collections_exist(&self, database_file: PathBuf) -> Result<bool, String> {
        (**self).persisted_collections_exist(database_file).await
    }

    async fn load_persisted_collections(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<PersistedCollectionAccessRecord>, String> {
        (**self).load_persisted_collections(database_file).await
    }

    async fn load_persisted_collection_series_ids(
        &self,
        database_file: PathBuf,
        collection_id: String,
    ) -> Result<Vec<String>, String> {
        (**self)
            .load_persisted_collection_series_ids(database_file, collection_id)
            .await
    }

    async fn load_persisted_collection_detail(
        &self,
        database_file: PathBuf,
        collection_id: String,
    ) -> Result<Option<PersistedCollectionAccessRecord>, String> {
        (**self)
            .load_persisted_collection_detail(database_file, collection_id)
            .await
    }

    async fn load_series_library_id(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Option<String>, String> {
        (**self)
            .load_series_library_id(database_file, series_id)
            .await
    }

    async fn load_series_restrictions(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<PersistedSeriesRestrictionRecord, String> {
        (**self)
            .load_series_restrictions(database_file, series_id)
            .await
    }

    async fn persist_collection_create(
        &self,
        database_file: PathBuf,
        collection_id: String,
        name: String,
        ordered: bool,
        series_ids: Vec<String>,
    ) -> Result<(), String> {
        (**self)
            .persist_collection_create(database_file, collection_id, name, ordered, series_ids)
            .await
    }

    async fn persist_collection_update(
        &self,
        database_file: PathBuf,
        collection_id: String,
        name: String,
        ordered: bool,
        series_ids: Vec<String>,
    ) -> Result<bool, String> {
        (**self)
            .persist_collection_update(database_file, collection_id, name, ordered, series_ids)
            .await
    }

    async fn delete_persisted_collection(
        &self,
        database_file: PathBuf,
        collection_id: String,
    ) -> Result<bool, String> {
        (**self)
            .delete_persisted_collection(database_file, collection_id)
            .await
    }

    async fn upsert_collection_search_document(
        &self,
        database_file: PathBuf,
        index_dir: PathBuf,
        collection_id: String,
    ) -> Result<bool, String> {
        (**self)
            .upsert_collection_search_document(database_file, index_dir, collection_id)
            .await
    }

    async fn delete_collection_search_document(
        &self,
        index_dir: PathBuf,
        collection_id: String,
    ) -> Result<(), String> {
        (**self)
            .delete_collection_search_document(index_dir, collection_id)
            .await
    }

    async fn load_persisted_readlists(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<DiscoveryPersistedReadlistRecord>, String> {
        (**self).load_persisted_readlists(database_file).await
    }

    async fn load_persisted_readlist_detail(
        &self,
        database_file: PathBuf,
        readlist_id: String,
    ) -> Result<Option<DiscoveryPersistedReadlistRecord>, String> {
        (**self)
            .load_persisted_readlist_detail(database_file, readlist_id)
            .await
    }

    async fn load_persisted_readlist_book_rows(
        &self,
        database_file: PathBuf,
        readlist_id: String,
    ) -> Result<Vec<DiscoveryPersistedReadlistBookRecord>, String> {
        (**self)
            .load_persisted_readlist_book_rows(database_file, readlist_id)
            .await
    }

    async fn load_comicrack_match_candidates(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<PersistedComicrackMatchCandidateRecord>, String> {
        (**self)
            .load_comicrack_match_candidates(database_file)
            .await
    }

    async fn load_persisted_book_authors(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Vec<PersistedBookAuthorRecord>, String> {
        (**self)
            .load_persisted_book_authors(database_file, book_id)
            .await
    }

    async fn persist_readlist_create(
        &self,
        database_file: PathBuf,
        readlist_id: String,
        name: String,
        summary: String,
        ordered: bool,
        book_ids: Vec<String>,
    ) -> Result<(), String> {
        (**self)
            .persist_readlist_create(database_file, readlist_id, name, summary, ordered, book_ids)
            .await
    }

    async fn persist_readlist_update(
        &self,
        database_file: PathBuf,
        readlist_id: String,
        name: String,
        summary: String,
        ordered: bool,
        book_ids: Vec<String>,
    ) -> Result<bool, String> {
        (**self)
            .persist_readlist_update(database_file, readlist_id, name, summary, ordered, book_ids)
            .await
    }

    async fn delete_persisted_readlist(
        &self,
        database_file: PathBuf,
        readlist_id: String,
    ) -> Result<bool, String> {
        (**self)
            .delete_persisted_readlist(database_file, readlist_id)
            .await
    }

    async fn upsert_readlist_search_document(
        &self,
        database_file: PathBuf,
        index_dir: PathBuf,
        readlist_id: String,
    ) -> Result<bool, String> {
        (**self)
            .upsert_readlist_search_document(database_file, index_dir, readlist_id)
            .await
    }

    async fn delete_readlist_search_document(
        &self,
        index_dir: PathBuf,
        readlist_id: String,
    ) -> Result<(), String> {
        (**self)
            .delete_readlist_search_document(index_dir, readlist_id)
            .await
    }

    async fn load_persisted_series_resource(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Option<PersistedSeriesResourceRecord>, String> {
        (**self)
            .load_persisted_series_resource(database_file, series_id)
            .await
    }

    async fn load_series_id_by_sorted_position(
        &self,
        database_file: PathBuf,
        index: usize,
    ) -> Result<Option<String>, String> {
        (**self)
            .load_series_id_by_sorted_position(database_file, index)
            .await
    }

    async fn load_persisted_series_detail(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Option<PersistedSeriesDetailRecord>, String> {
        (**self)
            .load_persisted_series_detail(database_file, series_id)
            .await
    }

    async fn load_persisted_series_summaries(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<SeriesSummaryRecord>, String> {
        (**self)
            .load_persisted_series_summaries(database_file)
            .await
    }

    async fn load_series_total_book_counts(
        &self,
        database_file: PathBuf,
    ) -> Result<HashMap<String, i64>, String> {
        (**self).load_series_total_book_counts(database_file).await
    }

    async fn load_series_read_progress_counts(
        &self,
        database_file: PathBuf,
        user_id: String,
    ) -> Result<HashMap<String, (i64, i64)>, String> {
        (**self)
            .load_series_read_progress_counts(database_file, user_id)
            .await
    }

    async fn load_persisted_series_collections(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Vec<PersistedSeriesCollectionRecord>, String> {
        (**self)
            .load_persisted_series_collections(database_file, series_id)
            .await
    }

    async fn load_existing_series_metadata(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Option<ExistingSeriesMetadataRecord>, String> {
        (**self)
            .load_existing_series_metadata(database_file, series_id)
            .await
    }

    async fn persist_series_metadata_update(
        &self,
        database_file: PathBuf,
        series_id: String,
        update: SeriesMetadataUpdateRecord,
    ) -> Result<bool, String> {
        (**self)
            .persist_series_metadata_update(database_file, series_id, update)
            .await
    }

    async fn refresh_series_search_documents_after_metadata_update(
        &self,
        database_file: PathBuf,
        index_dir: PathBuf,
        series_id: String,
    ) -> Result<(), String> {
        (**self)
            .refresh_series_search_documents_after_metadata_update(
                database_file,
                index_dir,
                series_id,
            )
            .await
    }
}
