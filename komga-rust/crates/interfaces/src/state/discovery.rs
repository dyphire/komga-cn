use super::*;
use axum::extract::FromRef;
use komga_application::discovery::{DiscoveryBrowseService, DiscoveryFacetService};

pub use komga_application::discovery::{
    AuthorFacetPort, BookDetailPort, BookSpecialListPort, CollectionPort, CollectionSearchPort,
    DiscoveryPersistedReadlistBookRecord, DiscoveryPersistedReadlistRecord,
    ExistingSeriesMetadataRecord, LibraryIdMappingPort, PersistedBookResourceRecord,
    PersistedBookSiblingDirectionRecord, PersistedCollectionAccessRecord,
    PersistedComicrackMatchCandidateRecord, PersistedSeriesCollectionRecord,
    PersistedSeriesDetailRecord, PersistedSeriesResourceRecord, PersistedSeriesRestrictionRecord,
    ReadlistPort, ReadlistSearchPort, SeriesAlternateTitleRecord, SeriesDetailPort,
    SeriesMetadataLinkRecord, SeriesMetadataUpdateRecord,
};

#[derive(Clone)]
pub struct DiscoveryState {
    pub(crate) discovery_auth: DiscoveryAuthState,
    pub(crate) identity: IdentityState,
    pub(crate) author_facets: Arc<dyn AuthorFacetPort>,
    pub(crate) library_id_mapping: Arc<dyn LibraryIdMappingPort>,
    pub(crate) book_special_lists: Arc<dyn BookSpecialListPort>,
    pub(crate) collection_search: Arc<dyn CollectionSearchPort>,
    pub(crate) readlist_search: Arc<dyn ReadlistSearchPort>,
    pub(crate) book_detail: Arc<dyn BookDetailPort>,
    pub(crate) series_detail: Arc<dyn SeriesDetailPort>,
    pub(crate) collection: Arc<dyn CollectionPort>,
    pub(crate) readlist: Arc<dyn ReadlistPort>,
    pub(crate) discovery_browse: Arc<dyn DiscoveryBrowseService>,
    pub(crate) discovery_facets: Arc<dyn DiscoveryFacetService>,
}

impl FromRef<Arc<HttpAppState>> for DiscoveryState {
    fn from_ref(app: &Arc<HttpAppState>) -> Self {
        Self {
            discovery_auth: app.discovery_auth.clone(),
            identity: IdentityState::from_ref(app),
            author_facets: app.services.author_facets.clone(),
            library_id_mapping: app.services.library_id_mapping.clone(),
            book_special_lists: app.services.book_special_lists.clone(),
            collection_search: app.services.collection_search.clone(),
            readlist_search: app.services.readlist_search.clone(),
            book_detail: app.services.book_detail.clone(),
            series_detail: app.services.series_detail.clone(),
            collection: app.services.collection.clone(),
            readlist: app.services.readlist.clone(),
            discovery_browse: app.services.discovery_browse.clone(),
            discovery_facets: app.services.discovery_facets.clone(),
        }
    }
}
