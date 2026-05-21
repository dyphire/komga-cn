use super::*;
use axum::extract::FromRef;

#[derive(Clone)]
pub struct MediaAssetsState {
    pub(crate) read_progress: ReadProgressState,
    pub(crate) identity: IdentityState,
    pub(crate) task_queue: TaskQueueState,
    pub(crate) discovery_detail: Arc<dyn komga_application::discovery::DiscoveryDetailPort>,
    pub(crate) reader: Arc<dyn komga_application::media_assets::MediaReaderPort>,
    pub(crate) content: Arc<dyn komga_application::media_assets::ContentResolverPort>,
    pub(crate) thumbnails: Arc<dyn komga_application::media_assets::ThumbnailWriterPort>,
    pub(crate) progress: Arc<dyn komga_application::media_assets::ProgressWriterPort>,
    pub(crate) metadata: Arc<komga_application::media_assets::MetadataWriter>,
    pub(crate) import: Arc<komga_application::media_assets::MediaImportService>,
}

impl FromRef<Arc<HttpAppState>> for MediaAssetsState {
    fn from_ref(app: &Arc<HttpAppState>) -> Self {
        Self {
            read_progress: app.read_progress.clone(),
            identity: IdentityState::from_ref(app),
            task_queue: TaskQueueState::from_ref(app),
            discovery_detail: app.services.discovery_detail.clone(),
            reader: app.services.media_reader.clone(),
            content: app.services.content_resolver.clone(),
            thumbnails: app.services.thumbnail_writer.clone(),
            progress: app.services.progress_writer.clone(),
            metadata: app.services.metadata_writer.clone(),
            import: app.services.import_service.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedMediaFileRecord {
    pub file_name: String,
    pub media_type: String,
    pub sub_type: Option<String>,
}
