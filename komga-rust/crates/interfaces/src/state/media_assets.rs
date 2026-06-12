use std::sync::Arc;

use axum::extract::FromRef;
use komga_application::discovery::{
    CollectionSeriesPort, PersistedBookIdResolverPort, PersistedSeriesIdResolverPort,
    PersistedSetVisibilityService,
};

use super::app_state::{HttpAppState, ReadProgressState};
use super::identity::IdentityState;
use super::task_queue::TaskQueueState;

#[derive(Clone)]
pub struct MediaAssetsState {
    pub(crate) read_progress: ReadProgressState,
    pub(crate) identity: IdentityState,
    pub(crate) task_queue: TaskQueueState,
    pub(crate) book_detail: Arc<dyn komga_application::discovery::BookDetailPort>,
    pub(crate) series_access: Arc<dyn CollectionSeriesPort>,
    pub(crate) persisted_set_visibility: Arc<dyn PersistedSetVisibilityService>,
    pub(crate) book_id_resolver: Arc<dyn PersistedBookIdResolverPort>,
    pub(crate) series_id_resolver: Arc<dyn PersistedSeriesIdResolverPort>,
    pub(crate) book_media_reader: Arc<dyn komga_application::media_assets::BookMediaReaderPort>,
    pub(crate) manifest_reader: Arc<dyn komga_application::media_assets::ManifestReaderPort>,
    pub(crate) manifest_content: Arc<dyn komga_application::media_assets::ManifestContentPort>,
    pub(crate) manifest_metadata: Arc<dyn komga_application::media_assets::ManifestMetadataPort>,
    pub(crate) archive_reader: Arc<dyn komga_application::media_assets::ArchiveReaderPort>,
    pub(crate) archive_builder: Arc<dyn komga_application::media_assets::ArchiveBuilderPort>,
    pub(crate) thumbnail_reader: Arc<dyn komga_application::media_assets::ThumbnailReaderPort>,
    pub(crate) epub_navigation_reader:
        Arc<dyn komga_application::media_assets::EpubNavigationReaderPort>,
    pub(crate) book_progression_reader:
        Arc<dyn komga_application::media_assets::BookProgressionSurfacePort>,
    pub(crate) read_progress_reader:
        Arc<dyn komga_application::media_assets::ReadProgressReaderPort>,
    pub(crate) series_relation: Arc<dyn komga_application::media_assets::SeriesRelationPort>,
    pub(crate) epub_navigation_content:
        Arc<dyn komga_application::media_assets::EpubNavigationContentPort>,
    pub(crate) book_media_content: Arc<dyn komga_application::media_assets::BookMediaContentPort>,
    pub(crate) content: Arc<dyn komga_application::media_assets::ContentResolverPort>,
    pub(crate) thumbnails: Arc<dyn komga_application::media_assets::ThumbnailWriterPort>,
    pub(crate) progress: Arc<dyn komga_application::media_assets::ProgressWriterPort>,
    pub(crate) read_progress_service: Arc<komga_application::media_assets::ReadProgressService>,
    pub(crate) metadata: Arc<komga_application::media_assets::MetadataWriter>,
    pub(crate) import: Arc<komga_application::media_assets::BookImportService>,
}

impl FromRef<Arc<HttpAppState>> for MediaAssetsState {
    fn from_ref(app: &Arc<HttpAppState>) -> Self {
        Self {
            read_progress: app.read_progress.clone(),
            identity: IdentityState::from_ref(app),
            task_queue: TaskQueueState::from_ref(app),
            book_detail: app.services.book_detail.clone(),
            series_access: app.services.series_access.clone(),
            persisted_set_visibility: app.services.persisted_set_visibility.clone(),
            book_id_resolver: app.services.book_id_resolver.clone(),
            series_id_resolver: app.services.series_id_resolver.clone(),
            book_media_reader: app.services.book_media_reader.clone(),
            manifest_reader: app.services.manifest_reader.clone(),
            manifest_content: app.services.manifest_content.clone(),
            manifest_metadata: app.services.manifest_metadata.clone(),
            archive_reader: app.services.archive_reader.clone(),
            archive_builder: app.services.archive_builder.clone(),
            thumbnail_reader: app.services.thumbnail_reader.clone(),
            epub_navigation_reader: app.services.epub_navigation_reader.clone(),
            book_progression_reader: app.services.book_progression_reader.clone(),
            read_progress_reader: app.services.read_progress_reader.clone(),
            series_relation: app.services.series_relation.clone(),
            epub_navigation_content: app.services.epub_navigation_content.clone(),
            book_media_content: app.services.book_media_content.clone(),
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
