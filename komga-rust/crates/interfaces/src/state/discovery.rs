use std::sync::Arc;

use axum::extract::FromRef;
use komga_application::discovery::{
    AuthorFacetPort, BookDetailPort, BookSpecialListPort, DiscoveryBrowseService,
    DiscoveryFacetService, LibraryIdMappingPort, PersistedBookIdResolverPort,
    PersistedSeriesIdResolverPort, PersistedSetService, SeriesDetailPort, SeriesMetadataWritePort,
};
use komga_application::runtime_sse::RuntimeSseEventSource;

use super::app_state::HttpAppState;
use super::identity::IdentityState;
use crate::discovery_auth::state::DiscoveryAuthState;

#[derive(Clone)]
pub struct DiscoveryState {
    pub(crate) discovery_auth: DiscoveryAuthState,
    pub(crate) identity: IdentityState,
    pub(crate) runtime_events: Arc<dyn RuntimeSseEventSource>,
    pub(crate) author_facets: Arc<dyn AuthorFacetPort>,
    pub(crate) library_id_mapping: Arc<dyn LibraryIdMappingPort>,
    pub(crate) book_special_lists: Arc<dyn BookSpecialListPort>,
    pub(crate) persisted_sets: Arc<dyn PersistedSetService>,
    pub(crate) book_detail: Arc<dyn BookDetailPort>,
    pub(crate) book_id_resolver: Arc<dyn PersistedBookIdResolverPort>,
    pub(crate) series_detail: Arc<dyn SeriesDetailPort>,
    pub(crate) series_metadata: Arc<dyn SeriesMetadataWritePort>,
    pub(crate) series_id_resolver: Arc<dyn PersistedSeriesIdResolverPort>,
    pub(crate) discovery_browse: Arc<dyn DiscoveryBrowseService>,
    pub(crate) discovery_facets: Arc<dyn DiscoveryFacetService>,
}

impl FromRef<Arc<HttpAppState>> for DiscoveryState {
    fn from_ref(app: &Arc<HttpAppState>) -> Self {
        Self {
            discovery_auth: app.discovery_auth.clone(),
            identity: IdentityState::from_ref(app),
            runtime_events: app.services.runtime_events.clone(),
            author_facets: app.services.author_facets.clone(),
            library_id_mapping: app.services.library_id_mapping.clone(),
            book_special_lists: app.services.book_special_lists.clone(),
            persisted_sets: app.services.persisted_sets.clone(),
            book_detail: app.services.book_detail.clone(),
            book_id_resolver: app.services.book_id_resolver.clone(),
            series_detail: app.services.series_detail.clone(),
            series_metadata: app.services.series_metadata.clone(),
            series_id_resolver: app.services.series_id_resolver.clone(),
            discovery_browse: app.services.discovery_browse.clone(),
            discovery_facets: app.services.discovery_facets.clone(),
        }
    }
}
