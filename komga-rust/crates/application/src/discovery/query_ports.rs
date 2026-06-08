use async_trait::async_trait;

#[derive(Clone)]
pub struct PersistedBookBrowseEntry {
    pub id: String,
    pub library_id: String,
    pub name: String,
    pub title: String,
}

#[derive(Clone)]
pub enum PersistedAuthorsScope {
    All,
    Libraries(Vec<String>),
    Collection(String),
    Series(String),
    ReadList(String),
}

#[derive(Clone, serde::Serialize)]
pub struct PersistedAuthorEntry {
    pub name: String,
    pub role: String,
}

#[async_trait]
pub trait CollectionSearchPort: Send + Sync {
    async fn search_collection_ids(&self, query: &str, limit: usize)
    -> Result<Vec<String>, String>;
}

#[async_trait]
pub trait ReadlistSearchPort: Send + Sync {
    async fn search_readlist_scored_ids(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<(f32, String)>, String>;
}

#[async_trait]
pub trait AuthorFacetPort: Send + Sync {
    async fn load_author_names(
        &self,
        search: &str,
        authorized_library_ids: Option<&[String]>,
    ) -> Result<Vec<String>, String>;

    async fn load_author_roles(
        &self,
        authorized_library_ids: Option<&[String]>,
    ) -> Result<Vec<String>, String>;

    async fn load_authors_by_scope(
        &self,
        scope: PersistedAuthorsScope,
        authorized_library_ids: Option<&[String]>,
    ) -> Result<Vec<PersistedAuthorEntry>, String>;
}

#[async_trait]
pub trait LibraryIdMappingPort: Send + Sync {
    async fn load_persisted_library_ids(&self) -> Result<Vec<String>, String>;
}

#[async_trait]
pub trait BookSpecialListPort: Send + Sync {
    async fn load_ondeck_books(
        &self,
        user_id: &str,
    ) -> Result<Vec<PersistedBookBrowseEntry>, String>;

    async fn load_duplicate_books(&self) -> Result<Vec<PersistedBookBrowseEntry>, String>;
}
