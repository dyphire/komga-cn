use super::*;
use axum::extract::FromRef;

#[derive(Clone)]
pub struct MediaAssetsState {
    pub(crate) read_progress: ReadProgressState,
    pub(crate) identity: IdentityState,
    pub(crate) task_queue: TaskQueueState,
    pub(crate) book_detail: Arc<dyn komga_application::discovery::BookDetailPort>,
    pub(crate) series_detail: Arc<dyn komga_application::discovery::SeriesDetailPort>,
    pub(crate) collection: Arc<dyn komga_application::discovery::CollectionPort>,
    pub(crate) readlist: Arc<dyn komga_application::discovery::ReadlistPort>,
    pub(crate) reader: Arc<dyn komga_application::media_assets::MediaReaderPort>,
    pub(crate) content: Arc<dyn komga_application::media_assets::ContentResolverPort>,
    pub(crate) thumbnails: Arc<dyn komga_application::media_assets::ThumbnailWriterPort>,
    pub(crate) progress: Arc<dyn komga_application::media_assets::ProgressWriterPort>,
    pub(crate) read_progress_service: Arc<komga_application::media_assets::ReadProgressService>,
    pub(crate) metadata: Arc<komga_application::media_assets::MetadataWriter>,
    pub(crate) import: Arc<komga_application::media_assets::MediaImportService>,
}

impl FromRef<Arc<HttpAppState>> for MediaAssetsState {
    fn from_ref(app: &Arc<HttpAppState>) -> Self {
        Self {
            read_progress: app.read_progress.clone(),
            identity: IdentityState::from_ref(app),
            task_queue: TaskQueueState::from_ref(app),
            book_detail: app.services.book_detail.clone(),
            series_detail: app.services.series_detail.clone(),
            collection: app.services.collection.clone(),
            readlist: app.services.readlist.clone(),
            reader: app.services.media_reader.clone(),
            content: app.services.content_resolver.clone(),
            thumbnails: app.services.thumbnail_writer.clone(),
            progress: app.services.progress_writer.clone(),
            read_progress_service: app.services.read_progress_service.clone(),
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
