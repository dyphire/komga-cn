use std::sync::Arc;

use axum::extract::FromRef;
use komga_application::discovery::PersistedBookIdResolverPort;
use komga_application::opds::{
    OpdsBrowseCatalogPort, OpdsCollectionDetailPersistedPort, OpdsFeedCatalogPort,
    OpdsFeedPersistedPort, OpdsLibraryPersistedPort, OpdsPublisherPersistedPort,
    OpdsReadlistDetailPersistedPort, OpdsSearchPersistedPort, OpdsSeriesPersistedPort,
};

use super::app_state::HttpAppState;

#[derive(Clone)]
pub struct OpdsState {
    pub(crate) server_settings: Arc<dyn komga_application::operational::ServerSettingsPort>,
    pub(crate) opds_feed_catalog: Arc<dyn OpdsFeedCatalogPort>,
    pub(crate) opds_browse_catalog: Arc<dyn OpdsBrowseCatalogPort>,
    pub(crate) opds_feed_persisted: Arc<dyn OpdsFeedPersistedPort>,
    pub(crate) opds_library_persisted: Arc<dyn OpdsLibraryPersistedPort>,
    pub(crate) opds_publisher_persisted: Arc<dyn OpdsPublisherPersistedPort>,
    pub(crate) opds_collection_detail_persisted: Arc<dyn OpdsCollectionDetailPersistedPort>,
    pub(crate) opds_readlist_detail_persisted: Arc<dyn OpdsReadlistDetailPersistedPort>,
    pub(crate) opds_series_persisted: Arc<dyn OpdsSeriesPersistedPort>,
    pub(crate) opds_search_persisted: Arc<dyn OpdsSearchPersistedPort>,
    pub(crate) book_id_resolver: Arc<dyn PersistedBookIdResolverPort>,
    pub(crate) book_media_reader: Arc<dyn komga_application::media_assets::BookMediaReaderPort>,
    pub(crate) book_media_content: Arc<dyn komga_application::media_assets::BookMediaContentPort>,
    pub(crate) manifest_reader: Arc<dyn komga_application::media_assets::ManifestReaderPort>,
    pub(crate) manifest_content: Arc<dyn komga_application::media_assets::ManifestContentPort>,
    pub(crate) manifest_metadata: Arc<dyn komga_application::media_assets::ManifestMetadataPort>,
}

impl FromRef<Arc<HttpAppState>> for OpdsState {
    fn from_ref(app: &Arc<HttpAppState>) -> Self {
        Self {
            server_settings: app.services.server_settings.clone(),
            opds_feed_catalog: app.services.opds_feed_catalog.clone(),
            opds_browse_catalog: app.services.opds_browse_catalog.clone(),
            opds_feed_persisted: app.services.opds_feed_persisted.clone(),
            opds_library_persisted: app.services.opds_library_persisted.clone(),
            opds_publisher_persisted: app.services.opds_publisher_persisted.clone(),
            opds_collection_detail_persisted: app.services.opds_collection_detail_persisted.clone(),
            opds_readlist_detail_persisted: app.services.opds_readlist_detail_persisted.clone(),
            opds_series_persisted: app.services.opds_series_persisted.clone(),
            opds_search_persisted: app.services.opds_search_persisted.clone(),
            book_id_resolver: app.services.book_id_resolver.clone(),
            book_media_reader: app.services.book_media_reader.clone(),
            book_media_content: app.services.book_media_content.clone(),
            manifest_reader: app.services.manifest_reader.clone(),
            manifest_content: app.services.manifest_content.clone(),
            manifest_metadata: app.services.manifest_metadata.clone(),
        }
    }
}
