use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use komga_application::discovery::{
    BookDetailPort, BookReadModel, CollectionCreateResult, CollectionListQuery,
    CollectionMutationError, CollectionMutationInput, CollectionMutationService, CollectionPort,
    CollectionProjectionService, CollectionReadModel, CollectionSearchPort,
    ComicRackReadListMatchResult, ComicRackReadListMatchService, ComicRackReadListRequest,
    DiscoveryPersistedReadlistBookRecord, DiscoveryPersistedReadlistRecord,
    ExistingSeriesMetadataRecord, PersistedBookIdResolverPort, PersistedBookResourceRecord,
    PersistedBookSiblingDirectionRecord, PersistedCollectionAccessRecord,
    PersistedComicrackMatchCandidateRecord, PersistedSeriesCollectionRecord,
    PersistedSeriesDetailRecord, PersistedSeriesIdResolverPort, PersistedSeriesResourceRecord,
    PersistedSeriesRestrictionRecord, PersistedSetService, PersistedSetVisibilityService,
    ReadListBooksQuery, ReadListReadModel, ReadListsQuery, ReadlistComicRackMatchPort,
    ReadlistCreateResult, ReadlistMutationError, ReadlistMutationInput, ReadlistMutationPort,
    ReadlistMutationService, ReadlistProjectionPort, ReadlistProjectionService, ReadlistSearchPort,
    ScoredSearchHit, SeriesDetailPort, SeriesMetadataUpdateRecord, SeriesMetadataWritePort,
    SeriesReadModel, SeriesReadProgressCounts,
};
use komga_application::runtime_sse::RuntimeSseEventSink;
use komga_domain::discovery::{DiscoveryQueryContext, PageEnvelope};

use crate::database_handle::DatabaseHandle;
use crate::discovery_persisted_access::{load_persisted_series_read_models, runtime_queries};
use crate::search::engine::SearchIndexEngine;
use crate::search::index_lifecycle::SearchEntityType;

use super::books;
use super::collections;
use super::readlists;
use super::series;

#[derive(Clone)]
pub struct DiscoveryDetailAccess {
    db: DatabaseHandle,
    search: SearchIndexEngine,
    runtime_events: Arc<dyn RuntimeSseEventSink>,
}

impl DiscoveryDetailAccess {
    pub fn new(
        db: DatabaseHandle,
        index_dir: PathBuf,
        owns_search_index: bool,
        runtime_events: Arc<dyn RuntimeSseEventSink>,
    ) -> Self {
        let search = SearchIndexEngine::new(db.write_pool().clone(), index_dir, owns_search_index);
        Self {
            db,
            search,
            runtime_events,
        }
    }
}

#[async_trait::async_trait]
impl PersistedSetVisibilityService for DiscoveryDetailAccess {
    async fn visible_collection_series_ids(
        &self,
        context: &DiscoveryQueryContext,
        collection_id: &str,
    ) -> anyhow::Result<Vec<String>> {
        CollectionProjectionService::new(self, self, self)
            .visible_collection_series_ids(context, collection_id)
            .await
    }

    async fn visible_readlist_book_ids(
        &self,
        context: &DiscoveryQueryContext,
        readlist_id: &str,
    ) -> anyhow::Result<Option<Vec<String>>> {
        ReadlistProjectionService::new(self, self, self)
            .visible_readlist_book_ids(context, readlist_id)
            .await
    }
}

#[async_trait::async_trait]
impl PersistedSetService for DiscoveryDetailAccess {
    async fn list_collections(
        &self,
        visibility_context: &DiscoveryQueryContext,
        request_scope_context: Option<&DiscoveryQueryContext>,
        query: CollectionListQuery,
    ) -> anyhow::Result<PageEnvelope<CollectionReadModel>> {
        CollectionProjectionService::new(self, self, self)
            .list_collections(visibility_context, request_scope_context, query)
            .await
    }

    async fn collection_detail(
        &self,
        context: &DiscoveryQueryContext,
        collection_id: &str,
    ) -> anyhow::Result<Option<CollectionReadModel>> {
        CollectionProjectionService::new(self, self, self)
            .collection_detail(context, collection_id)
            .await
    }

    async fn collection_for_mutation(
        &self,
        collection_id: &str,
    ) -> anyhow::Result<Option<CollectionReadModel>> {
        let Some(row) =
            collections::load_persisted_collection_detail(self.db.read_pool(), collection_id)
                .await?
        else {
            return Ok(None);
        };
        let series_ids =
            collections::load_persisted_collection_series_ids(self.db.read_pool(), collection_id)
                .await?;
        Ok(Some(CollectionReadModel {
            id: row.id,
            name: row.name,
            ordered: row.ordered,
            series_ids,
            created_date: row.created_date,
            last_modified_date: row.last_modified_date,
            filtered: false,
        }))
    }

    async fn visible_collections(
        &self,
        context: &DiscoveryQueryContext,
        collections: Vec<CollectionReadModel>,
    ) -> anyhow::Result<Vec<CollectionReadModel>> {
        CollectionProjectionService::new(self, self, self)
            .visible_collections(context, collections)
            .await
    }

    async fn create_collection(
        &self,
        input: CollectionMutationInput,
    ) -> Result<CollectionCreateResult, CollectionMutationError> {
        CollectionMutationService::new(self, self.runtime_events.as_ref())
            .create_collection(input)
            .await
    }

    async fn update_collection(
        &self,
        collection_id: &str,
        input: CollectionMutationInput,
    ) -> Result<bool, CollectionMutationError> {
        CollectionMutationService::new(self, self.runtime_events.as_ref())
            .update_collection(collection_id, input)
            .await
    }

    async fn delete_collection(
        &self,
        collection_id: &str,
    ) -> Result<bool, CollectionMutationError> {
        CollectionMutationService::new(self, self.runtime_events.as_ref())
            .delete_collection(collection_id)
            .await
    }

    async fn list_readlists(
        &self,
        requested_context: &DiscoveryQueryContext,
        visibility_context: &DiscoveryQueryContext,
        query: ReadListsQuery,
    ) -> anyhow::Result<PageEnvelope<ReadListReadModel>> {
        ReadlistProjectionService::new(self, self, self)
            .list_readlists(requested_context, visibility_context, query)
            .await
    }

    async fn readlist_detail(
        &self,
        context: &DiscoveryQueryContext,
        readlist_id: &str,
    ) -> anyhow::Result<Option<ReadListReadModel>> {
        ReadlistProjectionService::new(self, self, self)
            .readlist_detail(context, readlist_id)
            .await
    }

    async fn readlist_for_mutation(
        &self,
        readlist_id: &str,
    ) -> anyhow::Result<Option<ReadListReadModel>> {
        let Some(row) =
            readlists::load_persisted_readlist_detail(self.db.read_pool(), readlist_id).await?
        else {
            return Ok(None);
        };
        let book_ids =
            readlists::load_persisted_readlist_book_rows(self.db.read_pool(), readlist_id)
                .await?
                .into_iter()
                .map(|row| row.book_id)
                .collect();
        Ok(Some(ReadListReadModel {
            id: row.id,
            name: row.name,
            summary: row.summary,
            ordered: row.ordered,
            book_ids,
            created_date: row.created_date,
            last_modified_date: row.last_modified_date,
            filtered: false,
        }))
    }

    async fn match_comicrack_readlist(
        &self,
        request: &ComicRackReadListRequest,
    ) -> anyhow::Result<ComicRackReadListMatchResult> {
        ComicRackReadListMatchService::new(self)
            .match_readlist(request)
            .await
    }

    async fn list_readlist_books(
        &self,
        context: &DiscoveryQueryContext,
        query: ReadListBooksQuery,
    ) -> anyhow::Result<Option<PageEnvelope<BookReadModel>>> {
        ReadlistProjectionService::new(self, self, self)
            .list_readlist_books(context, query)
            .await
    }

    async fn readlist_book_sibling(
        &self,
        context: &DiscoveryQueryContext,
        readlist_id: &str,
        book_id: &str,
        next: bool,
    ) -> anyhow::Result<Option<BookReadModel>> {
        ReadlistProjectionService::new(self, self, self)
            .readlist_book_sibling(context, readlist_id, book_id, next)
            .await
    }

    async fn readlists_for_book(
        &self,
        candidate_library_ids: Option<&[String]>,
        visibility_context: &DiscoveryQueryContext,
        book_id: &str,
    ) -> anyhow::Result<Vec<ReadListReadModel>> {
        ReadlistProjectionService::new(self, self, self)
            .readlists_for_book(candidate_library_ids, visibility_context, book_id)
            .await
    }

    async fn create_readlist(
        &self,
        input: ReadlistMutationInput,
    ) -> Result<ReadlistCreateResult, ReadlistMutationError> {
        ReadlistMutationService::new(self, self.runtime_events.as_ref())
            .create_readlist(input)
            .await
    }

    async fn update_readlist(
        &self,
        readlist_id: &str,
        input: ReadlistMutationInput,
    ) -> Result<bool, ReadlistMutationError> {
        ReadlistMutationService::new(self, self.runtime_events.as_ref())
            .update_readlist(readlist_id, input)
            .await
    }

    async fn delete_readlist(&self, readlist_id: &str) -> Result<bool, ReadlistMutationError> {
        ReadlistMutationService::new(self, self.runtime_events.as_ref())
            .delete_readlist(readlist_id)
            .await
    }
}

#[async_trait::async_trait]
impl CollectionSearchPort for DiscoveryDetailAccess {
    async fn search_collection_ids(
        &self,
        query: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<String>> {
        self.search
            .search_ids(query, SearchEntityType::Collection, limit)
    }
}

#[async_trait::async_trait]
impl ReadlistSearchPort for DiscoveryDetailAccess {
    async fn search_readlist_scored_ids(
        &self,
        query: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<ScoredSearchHit>> {
        Ok(self
            .search
            .search_scored_ids(query, SearchEntityType::ReadList, limit)?
            .into_iter()
            .map(|hit| ScoredSearchHit {
                score: hit.score,
                id: hit.id,
            })
            .collect())
    }
}

#[async_trait::async_trait]
impl BookDetailPort for DiscoveryDetailAccess {
    async fn load_persisted_book_resource(
        &self,
        book_id: &str,
    ) -> anyhow::Result<Option<PersistedBookResourceRecord>> {
        books::load_persisted_book_resource(self.db.read_pool(), book_id).await
    }

    async fn load_persisted_book_detail(
        &self,
        book_id: &str,
        user_id: Option<&str>,
    ) -> anyhow::Result<Option<BookReadModel>> {
        books::load_persisted_book_detail(self.db.read_pool(), book_id, user_id).await
    }

    async fn load_persisted_book_sibling_id(
        &self,
        book_id: &str,
        direction: PersistedBookSiblingDirectionRecord,
    ) -> anyhow::Result<Option<String>> {
        books::load_persisted_book_sibling_id(self.db.read_pool(), book_id, direction).await
    }
}

#[async_trait::async_trait]
impl PersistedBookIdResolverPort for DiscoveryDetailAccess {
    async fn persisted_book_resource_exists(&self, book_id: &str) -> anyhow::Result<bool> {
        books::load_persisted_book_resource(self.db.read_pool(), book_id)
            .await
            .map(|record| record.is_some())
    }

    async fn load_book_id_by_sorted_position(
        &self,
        index: usize,
    ) -> anyhow::Result<Option<String>> {
        books::load_book_id_by_sorted_position(self.db.read_pool(), index).await
    }
}

#[async_trait::async_trait]
impl SeriesDetailPort for DiscoveryDetailAccess {
    async fn load_series_library_id(&self, series_id: &str) -> anyhow::Result<Option<String>> {
        collections::load_series_library_id(self.db.read_pool(), series_id).await
    }

    async fn load_series_restrictions(
        &self,
        series_id: &str,
    ) -> anyhow::Result<PersistedSeriesRestrictionRecord> {
        collections::load_series_restrictions(self.db.read_pool(), series_id).await
    }

    async fn load_persisted_series_resource(
        &self,
        series_id: &str,
    ) -> anyhow::Result<Option<PersistedSeriesResourceRecord>> {
        series::load_persisted_series_resource(self.db.read_pool(), series_id).await
    }

    async fn load_persisted_series_detail(
        &self,
        series_id: &str,
    ) -> anyhow::Result<Option<PersistedSeriesDetailRecord>> {
        series::load_persisted_series_detail(self.db.read_pool(), series_id).await
    }

    async fn load_persisted_series_summaries(&self) -> anyhow::Result<Vec<SeriesReadModel>> {
        load_persisted_series_read_models(self.db.read_pool()).await
    }

    async fn load_series_total_book_counts(&self) -> anyhow::Result<HashMap<String, i64>> {
        runtime_queries::load_series_total_book_counts(self.db.read_pool()).await
    }

    async fn load_series_read_progress_counts(
        &self,
        user_id: &str,
    ) -> anyhow::Result<HashMap<String, SeriesReadProgressCounts>> {
        runtime_queries::load_series_read_progress_counts(self.db.read_pool(), user_id).await
    }

    async fn load_persisted_series_collections(
        &self,
        series_id: &str,
    ) -> anyhow::Result<Vec<PersistedSeriesCollectionRecord>> {
        series::load_persisted_series_collections(self.db.read_pool(), series_id).await
    }

    async fn load_existing_series_metadata(
        &self,
        series_id: &str,
    ) -> anyhow::Result<Option<ExistingSeriesMetadataRecord>> {
        series::load_existing_series_metadata(self.db.read_pool(), series_id).await
    }
}

#[async_trait::async_trait]
impl PersistedSeriesIdResolverPort for DiscoveryDetailAccess {
    async fn persisted_series_resource_exists(&self, series_id: &str) -> anyhow::Result<bool> {
        series::load_persisted_series_resource(self.db.read_pool(), series_id)
            .await
            .map(|record| record.is_some())
    }

    async fn load_series_id_by_sorted_position(
        &self,
        index: usize,
    ) -> anyhow::Result<Option<String>> {
        series::load_series_id_by_sorted_position(self.db.read_pool(), index).await
    }
}

#[async_trait::async_trait]
impl SeriesMetadataWritePort for DiscoveryDetailAccess {
    async fn load_series_library_id(&self, series_id: &str) -> anyhow::Result<Option<String>> {
        collections::load_series_library_id(self.db.read_pool(), series_id).await
    }

    async fn load_existing_series_metadata(
        &self,
        series_id: &str,
    ) -> anyhow::Result<Option<ExistingSeriesMetadataRecord>> {
        series::load_existing_series_metadata(self.db.read_pool(), series_id).await
    }

    async fn persist_series_metadata_update(
        &self,
        series_id: &str,
        update: SeriesMetadataUpdateRecord,
    ) -> anyhow::Result<bool> {
        series::persist_series_metadata_update(self.db.write_pool(), series_id, update).await
    }

    async fn refresh_series_search_documents_after_metadata_update(
        &self,
        series_id: &str,
    ) -> anyhow::Result<()> {
        series::refresh_series_after_metadata_update(self.db.write_pool(), series_id).await?;

        self.search
            .refresh_series_after_metadata_update(series_id)
            .await
    }
}

#[async_trait::async_trait]
impl CollectionPort for DiscoveryDetailAccess {
    async fn persisted_collections_exist(&self) -> anyhow::Result<bool> {
        collections::persisted_collections_exist(self.db.read_pool()).await
    }

    async fn load_persisted_collections(
        &self,
    ) -> anyhow::Result<Vec<PersistedCollectionAccessRecord>> {
        collections::load_persisted_collections(self.db.read_pool()).await
    }

    async fn load_persisted_collection_series_ids(
        &self,
        collection_id: &str,
    ) -> anyhow::Result<Vec<String>> {
        collections::load_persisted_collection_series_ids(self.db.read_pool(), collection_id).await
    }

    async fn load_persisted_collection_detail(
        &self,
        collection_id: &str,
    ) -> anyhow::Result<Option<PersistedCollectionAccessRecord>> {
        collections::load_persisted_collection_detail(self.db.read_pool(), collection_id).await
    }

    async fn persist_collection_create(
        &self,
        collection_id: &str,
        name: &str,
        ordered: bool,
        series_ids: &[String],
    ) -> anyhow::Result<()> {
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
    ) -> anyhow::Result<bool> {
        collections::persist_collection_update(
            self.db.write_pool(),
            collection_id,
            name,
            ordered,
            series_ids,
        )
        .await
    }

    async fn delete_persisted_collection(&self, collection_id: &str) -> anyhow::Result<bool> {
        collections::delete_persisted_collection(self.db.write_pool(), collection_id).await
    }

    async fn upsert_collection_search_document(&self, collection_id: &str) -> anyhow::Result<bool> {
        self.search.upsert_collection(collection_id).await
    }

    async fn delete_collection_search_document(&self, collection_id: &str) -> anyhow::Result<()> {
        self.search.delete_collection(collection_id).await
    }
}

#[async_trait::async_trait]
impl ReadlistProjectionPort for DiscoveryDetailAccess {
    async fn load_persisted_readlists(
        &self,
    ) -> anyhow::Result<Vec<DiscoveryPersistedReadlistRecord>> {
        readlists::load_persisted_readlists(self.db.read_pool()).await
    }

    async fn load_persisted_readlist_detail(
        &self,
        readlist_id: &str,
    ) -> anyhow::Result<Option<DiscoveryPersistedReadlistRecord>> {
        readlists::load_persisted_readlist_detail(self.db.read_pool(), readlist_id).await
    }

    async fn load_persisted_readlist_book_rows(
        &self,
        readlist_id: &str,
    ) -> anyhow::Result<Vec<DiscoveryPersistedReadlistBookRecord>> {
        readlists::load_persisted_readlist_book_rows(self.db.read_pool(), readlist_id).await
    }
}

#[async_trait::async_trait]
impl ReadlistComicRackMatchPort for DiscoveryDetailAccess {
    async fn load_persisted_readlists(
        &self,
    ) -> anyhow::Result<Vec<DiscoveryPersistedReadlistRecord>> {
        readlists::load_persisted_readlists(self.db.read_pool()).await
    }

    async fn load_comicrack_match_candidates(
        &self,
    ) -> anyhow::Result<Vec<PersistedComicrackMatchCandidateRecord>> {
        readlists::load_comicrack_match_candidates(self.db.read_pool()).await
    }
}

#[async_trait::async_trait]
impl ReadlistMutationPort for DiscoveryDetailAccess {
    async fn persist_readlist_create(
        &self,
        readlist_id: &str,
        name: &str,
        summary: &str,
        ordered: bool,
        book_ids: &[String],
    ) -> anyhow::Result<()> {
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
    ) -> anyhow::Result<bool> {
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

    async fn delete_persisted_readlist(&self, readlist_id: &str) -> anyhow::Result<bool> {
        readlists::delete_persisted_readlist(self.db.write_pool(), readlist_id).await
    }

    async fn upsert_readlist_search_document(&self, readlist_id: &str) -> anyhow::Result<bool> {
        self.search.upsert_readlist(readlist_id).await
    }

    async fn delete_readlist_search_document(&self, readlist_id: &str) -> anyhow::Result<()> {
        self.search.delete_readlist(readlist_id).await
    }
}
