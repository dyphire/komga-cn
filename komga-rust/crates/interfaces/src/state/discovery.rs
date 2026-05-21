use super::*;
use axum::extract::FromRef;
use komga_application::discovery::{DiscoveryBrowseService, DiscoveryFacetService};

pub use komga_infrastructure::discovery_detail_access::DiscoveryDetailAccess;
pub use komga_infrastructure::discovery_detail_access::books::{
    DiscoveryPersistedReadProgressRecord, PersistedBookDetailRecord, PersistedBookResourceRecord,
    PersistedBookSiblingDirectionRecord,
};
pub use komga_infrastructure::discovery_detail_access::collections::{
    PersistedCollectionAccessRecord, PersistedSeriesRestrictionRecord,
};
pub use komga_infrastructure::discovery_detail_access::readlists::{
    DiscoveryPersistedReadlistBookRecord, DiscoveryPersistedReadlistRecord,
    PersistedBookAuthorRecord, PersistedComicrackMatchCandidateRecord,
};
pub use komga_infrastructure::discovery_detail_access::series::{
    ExistingSeriesMetadataRecord, PersistedSeriesCollectionRecord, PersistedSeriesDetailRecord,
    PersistedSeriesResourceRecord, SeriesAlternateTitleRecord, SeriesMetadataLinkRecord,
    SeriesMetadataUpdateRecord,
};
pub use komga_infrastructure::discovery_persisted_access::models::SeriesSummary as SeriesSummaryRecord;

#[derive(Clone)]
pub struct DiscoveryState {
    pub(crate) discovery_auth: DiscoveryAuthState,
    pub(crate) identity: IdentityState,
    pub(crate) discovery_search: Arc<dyn DiscoverySearchService>,
    pub(crate) discovery_detail: Arc<DiscoveryDetailAccess>,
    pub(crate) discovery_browse: Arc<dyn DiscoveryBrowseService>,
    pub(crate) discovery_facets: Arc<dyn DiscoveryFacetService>,
}

impl FromRef<Arc<HttpAppState>> for DiscoveryState {
    fn from_ref(app: &Arc<HttpAppState>) -> Self {
        Self {
            discovery_auth: app.discovery_auth.clone(),
            identity: IdentityState::from_ref(app),
            discovery_search: app.services.discovery_search.clone(),
            discovery_detail: app.services.discovery_detail.clone(),
            discovery_browse: app.services.discovery_browse.clone(),
            discovery_facets: app.services.discovery_facets.clone(),
        }
    }
}

#[async_trait]
pub trait DiscoverySearchService: Send + Sync {
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

    async fn load_persisted_library_ids(&self) -> Result<Vec<String>, String>;

    async fn search_collection_ids(&self, query: &str, limit: usize)
    -> Result<Vec<String>, String>;

    async fn search_readlist_scored_ids(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<(f32, String)>, String>;

    async fn load_ondeck_books(
        &self,
        user_id: &str,
    ) -> Result<Vec<PersistedBookBrowseEntry>, String>;

    async fn load_duplicate_books(&self) -> Result<Vec<PersistedBookBrowseEntry>, String>;
}
