use super::*;
use axum::extract::FromRef;

#[derive(Clone)]
pub struct MediaAssetsState {
    pub(crate) read_progress: ReadProgressState,
    pub(crate) identity: IdentityState,
    pub(crate) task_queue: TaskQueueState,
    pub(crate) discovery_detail: Arc<dyn DiscoveryDetailService>,
    pub(crate) reader: komga_infrastructure::media_reader::MediaReader,
    pub(crate) content: komga_infrastructure::content_resolver::ContentResolver,
    pub(crate) thumbnails: komga_infrastructure::thumbnail_writer::ThumbnailWriter,
    pub(crate) progress: komga_infrastructure::progress_writer::ProgressWriter,
    pub(crate) metadata: Arc<komga_application::media_assets::MetadataWriter>,
    pub(crate) import: Arc<
        komga_application::media_assets::MediaImportService<
            komga_infrastructure::filesystem::import::FilesystemImportPort,
        >,
    >,
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
