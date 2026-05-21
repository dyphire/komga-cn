use super::*;
use axum::extract::FromRef;
use komga_application::discovery::{DiscoveryBrowseService, DiscoveryFacetService};

pub use komga_application::discovery::{
    DiscoveryDetailPort, DiscoveryPersistedReadProgressRecord,
    DiscoveryPersistedReadlistBookRecord, DiscoveryPersistedReadlistRecord, DiscoverySearchService,
    ExistingSeriesMetadataRecord, PersistedBookAuthorRecord, PersistedBookDetailRecord,
    PersistedBookResourceRecord, PersistedBookSiblingDirectionRecord,
    PersistedCollectionAccessRecord, PersistedComicrackMatchCandidateRecord,
    PersistedSeriesCollectionRecord, PersistedSeriesDetailRecord, PersistedSeriesResourceRecord,
    PersistedSeriesRestrictionRecord, SeriesAlternateTitleRecord, SeriesMetadataLinkRecord,
    SeriesMetadataUpdateRecord, SeriesSummaryRecord,
};

#[derive(Clone)]
pub struct DiscoveryState {
    pub(crate) discovery_auth: DiscoveryAuthState,
    pub(crate) identity: IdentityState,
    pub(crate) discovery_search: Arc<dyn DiscoverySearchService>,
    pub(crate) discovery_detail: Arc<dyn DiscoveryDetailPort>,
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
