use super::*;
use axum::extract::FromRef;

pub use komga_infrastructure::opds_catalog_access::{
    BrowsePublisherEntry, BrowseSeriesNavigationEntry, OpdsBookAuthorEntry, OpdsBookFeedEntry,
    OpdsCatalogAccess, OpdsReadlistEntry, OpdsSeriesEntry,
};
pub use komga_infrastructure::opds_persisted_access::{
    OpdsPersistedAccess, PersistedBookAuthorRecord as OpdsPersistedBookAuthorRecord,
    PersistedBookFeedRecord, PersistedBookSearchRecord, PersistedLibraryRecord,
    PersistedNamedRecord, PersistedReadlistBookRecord, PersistedReadlistRecord,
    PersistedSeriesBookRecord, PersistedSeriesRecord, PersistedSeriesSearchRecord,
};

#[derive(Clone)]
pub struct OpdsState {
    pub(crate) server_settings:
        Arc<komga_infrastructure::sqlite::write_models::server_settings::ServerSettingsStore>,
    pub(crate) opds_catalog: Arc<OpdsCatalogAccess>,
    pub(crate) opds_persisted: Arc<OpdsPersistedAccess>,
    pub(crate) discovery_detail:
        Arc<komga_infrastructure::discovery_detail_access::DiscoveryDetailAccess>,
    pub(crate) reader: komga_infrastructure::media_reader::MediaReader,
    pub(crate) content: komga_infrastructure::content_resolver::ContentResolver,
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
