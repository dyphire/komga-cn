use komga_domain::discovery::{DiscoveryQueryContext, PageEnvelope};

use super::{
    BookReadModel, CollectionCreateResult, CollectionListQuery, CollectionMutationError,
    CollectionMutationInput, CollectionReadModel, ComicRackReadListMatchResult,
    ComicRackReadListRequest, ReadListBooksQuery, ReadListReadModel, ReadListsQuery,
    ReadlistCreateResult, ReadlistMutationError, ReadlistMutationInput,
};

#[async_trait::async_trait]
pub trait PersistedSetVisibilityService: Send + Sync {
    async fn visible_collection_series_ids(
        &self,
        context: &DiscoveryQueryContext,
        collection_id: &str,
    ) -> anyhow::Result<Vec<String>>;

    async fn visible_readlist_book_ids(
        &self,
        context: &DiscoveryQueryContext,
        readlist_id: &str,
    ) -> anyhow::Result<Option<Vec<String>>>;
}

#[async_trait::async_trait]
pub trait PersistedSetService: PersistedSetVisibilityService {
    async fn list_collections(
        &self,
        visibility_context: &DiscoveryQueryContext,
        request_scope_context: Option<&DiscoveryQueryContext>,
        query: CollectionListQuery,
    ) -> anyhow::Result<PageEnvelope<CollectionReadModel>>;

    async fn collection_detail(
        &self,
        context: &DiscoveryQueryContext,
        collection_id: &str,
    ) -> anyhow::Result<Option<CollectionReadModel>>;

    async fn collection_for_mutation(
        &self,
        collection_id: &str,
    ) -> anyhow::Result<Option<CollectionReadModel>>;

    async fn visible_collections(
        &self,
        context: &DiscoveryQueryContext,
        collections: Vec<CollectionReadModel>,
    ) -> anyhow::Result<Vec<CollectionReadModel>>;

    async fn create_collection(
        &self,
        input: CollectionMutationInput,
    ) -> Result<CollectionCreateResult, CollectionMutationError>;

    async fn update_collection(
        &self,
        collection_id: &str,
        input: CollectionMutationInput,
    ) -> Result<bool, CollectionMutationError>;

    async fn delete_collection(&self, collection_id: &str)
    -> Result<bool, CollectionMutationError>;

    async fn list_readlists(
        &self,
        requested_context: &DiscoveryQueryContext,
        visibility_context: &DiscoveryQueryContext,
        query: ReadListsQuery,
    ) -> anyhow::Result<PageEnvelope<ReadListReadModel>>;

    async fn readlist_detail(
        &self,
        context: &DiscoveryQueryContext,
        readlist_id: &str,
    ) -> anyhow::Result<Option<ReadListReadModel>>;

    async fn readlist_for_mutation(
        &self,
        readlist_id: &str,
    ) -> anyhow::Result<Option<ReadListReadModel>>;

    async fn match_comicrack_readlist(
        &self,
        request: &ComicRackReadListRequest,
    ) -> anyhow::Result<ComicRackReadListMatchResult>;

    async fn list_readlist_books(
        &self,
        context: &DiscoveryQueryContext,
        query: ReadListBooksQuery,
    ) -> anyhow::Result<Option<PageEnvelope<BookReadModel>>>;

    async fn readlist_book_sibling(
        &self,
        context: &DiscoveryQueryContext,
        readlist_id: &str,
        book_id: &str,
        next: bool,
    ) -> anyhow::Result<Option<BookReadModel>>;

    async fn readlists_for_book(
        &self,
        candidate_library_ids: Option<&[String]>,
        visibility_context: &DiscoveryQueryContext,
        book_id: &str,
    ) -> anyhow::Result<Vec<ReadListReadModel>>;

    async fn create_readlist(
        &self,
        input: ReadlistMutationInput,
    ) -> Result<ReadlistCreateResult, ReadlistMutationError>;

    async fn update_readlist(
        &self,
        readlist_id: &str,
        input: ReadlistMutationInput,
    ) -> Result<bool, ReadlistMutationError>;

    async fn delete_readlist(&self, readlist_id: &str) -> Result<bool, ReadlistMutationError>;
}
