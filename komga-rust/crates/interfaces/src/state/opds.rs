use super::*;
use axum::extract::FromRef;

pub use komga_application::opds::{
    BrowsePublisherEntry, BrowseSeriesNavigationEntry, OpdsBookAuthorEntry, OpdsBookFeedEntry,
    OpdsCatalogPort, OpdsPersistedBookAuthorRecord, OpdsPersistedPort, OpdsReadlistEntry,
    OpdsSeriesEntry, PersistedBookFeedRecord, PersistedBookSearchRecord, PersistedLibraryRecord,
    PersistedNamedRecord, PersistedReadlistBookRecord, PersistedReadlistRecord,
    PersistedSeriesBookRecord, PersistedSeriesRecord, PersistedSeriesSearchRecord,
};

#[derive(Clone)]
pub struct OpdsState {
    pub(crate) server_settings: Arc<dyn komga_application::operational::ServerSettingsPort>,
    pub(crate) opds_catalog: Arc<dyn OpdsCatalogPort>,
    pub(crate) opds_persisted: Arc<dyn OpdsPersistedPort>,
    pub(crate) discovery_detail: Arc<dyn komga_application::discovery::DiscoveryDetailPort>,
    pub(crate) reader: Arc<dyn komga_application::media_assets::MediaReaderPort>,
    pub(crate) content: Arc<dyn komga_application::media_assets::ContentResolverPort>,
}

impl FromRef<Arc<HttpAppState>> for OpdsState {
    fn from_ref(app: &Arc<HttpAppState>) -> Self {
        Self {
            server_settings: app.services.server_settings.clone(),
            opds_catalog: app.services.opds_catalog.clone(),
            opds_persisted: app.services.opds_persisted.clone(),
            discovery_detail: app.services.discovery_detail.clone(),
            reader: app.services.media_reader.clone(),
            content: app.services.content_resolver.clone(),
        }
    }
}
