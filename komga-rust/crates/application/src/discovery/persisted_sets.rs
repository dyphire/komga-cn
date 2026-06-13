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
    ) -> Result<Vec<String>, String>;

    async fn visible_readlist_book_ids(
        &self,
        context: &DiscoveryQueryContext,
        readlist_id: &str,
    ) -> Result<Option<Vec<String>>, String>;
}

#[async_trait::async_trait]
pub trait PersistedSetService: PersistedSetVisibilityService {
    async fn list_collections(
        &self,
        visibility_context: &DiscoveryQueryContext,
        request_scope_context: Option<&DiscoveryQueryContext>,
        query: CollectionListQuery,
    ) -> Result<PageEnvelope<CollectionReadModel>, String>;

    async fn collection_detail(
        &self,
        context: &DiscoveryQueryContext,
        collection_id: &str,
    ) -> Result<Option<CollectionReadModel>, String>;

    async fn collection_for_mutation(
        &self,
        collection_id: &str,
    ) -> Result<Option<CollectionReadModel>, String>;

    async fn visible_collections(
        &self,
        context: &DiscoveryQueryContext,
        collections: Vec<CollectionReadModel>,
    ) -> Result<Vec<CollectionReadModel>, String>;

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
    ) -> Result<PageEnvelope<ReadListReadModel>, String>;

    async fn readlist_detail(
        &self,
        context: &DiscoveryQueryContext,
        readlist_id: &str,
    ) -> Result<Option<ReadListReadModel>, String>;

    async fn readlist_for_mutation(
        &self,
        readlist_id: &str,
    ) -> Result<Option<ReadListReadModel>, String>;

    async fn match_comicrack_readlist(
        &self,
        request: &ComicRackReadListRequest,
    ) -> Result<ComicRackReadListMatchResult, String>;

    async fn list_readlist_books(
        &self,
        context: &DiscoveryQueryContext,
        query: ReadListBooksQuery,
    ) -> Result<Option<PageEnvelope<BookReadModel>>, String>;

    async fn readlist_book_sibling(
        &self,
        context: &DiscoveryQueryContext,
        readlist_id: &str,
        book_id: &str,
        next: bool,
    ) -> Result<Option<BookReadModel>, String>;

    async fn readlists_for_book(
        &self,
        candidate_library_ids: Option<&[String]>,
        visibility_context: &DiscoveryQueryContext,
        book_id: &str,
    ) -> Result<Vec<ReadListReadModel>, String>;

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
