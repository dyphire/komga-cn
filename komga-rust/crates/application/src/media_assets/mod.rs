mod archive_delivery;
#[cfg(test)]
mod archive_delivery_tests;
mod book_access;
mod book_import;
mod book_media_delivery;
#[cfg(test)]
mod book_media_delivery_tests;
mod book_progression;
#[cfg(test)]
mod book_progression_tests;
mod book_progression_write;
#[cfg(test)]
mod book_progression_write_tests;
mod epub_navigation;
#[cfg(test)]
mod epub_navigation_tests;
mod manifest_builder;
mod metadata_update;
mod metadata_writer;
mod page_retrieval;
mod ports;
mod read_progress_service;
mod thumbnail_operations;

pub use komga_domain::media_assets::ThumbnailType;

pub use archive_delivery::{
    ArchiveBuilderPort, ArchiveContentPort, ArchiveDelivery, ArchiveDeliveryAsset,
    ArchiveDeliveryService, ArchiveFileEntry, ArchiveReaderPort,
};
pub use book_import::{
    BookImportPort, BookImportService, BookImportSubmissionFailure,
    BookImportSubmissionFailureKind, BooksImportEntry, BooksImportPayload, ImportBookOutcome,
    ImportCopyMode, RuntimeBookImportEvent, RuntimeBookImportEventBatch,
    current_runtime_book_import_event_cursor, pending_runtime_book_import_events,
    register_runtime_book_import_event,
};
pub use book_media_delivery::{
    BookMediaContentPort, BookMediaDelivery, BookMediaDeliveryAsset, BookMediaDeliveryDisposition,
    BookMediaDeliveryService, BookMediaPageRequest, BookMediaReaderPort, BookThumbnailAsset,
    BookThumbnailDelivery,
};
pub use book_progression::{
    BookProgressionGetOutcome, BookProgressionLocator, BookProgressionOutcome,
    BookProgressionReaderPort, BookProgressionService, BookProgressionSurfacePort,
    BookProgressionUpdate, BookProgressionUpdateInput,
};
pub(crate) use book_progression_write::{
    BookProgressionConflictPolicy, BookProgressionWrite, BookProgressionWriteError,
    BookProgressionWriteService, BookProgressionWriteSource,
};
pub use book_progression_write::{BookProgressionWriteReaderPort, BookProgressionWriterPort};
pub use epub_navigation::{
    EpubNavigation, EpubNavigationContentPort, EpubNavigationError,
    EpubNavigationExtensionReaderPort, EpubNavigationLoadError, EpubNavigationReaderPort,
    NormalizedEpubLocator, load_book_epub_navigation, load_book_epub_navigation_extension,
    load_book_epub_positions, normalized_href_base,
};
pub use manifest_builder::{
    ManifestBuildOutcome, ManifestContentPort, ManifestContributor, ManifestHref, ManifestLinkItem,
    ManifestMetadata, ManifestMetadataPort, ManifestNavigationItem, ManifestProfile,
    ManifestReaderPort, ManifestReadingProgression, ManifestVariant, PersistedManifest,
    build_persisted_book_manifest,
};
pub use metadata_update::{
    BookMetadata, BookMetadataAuthor, BookMetadataBatchUpdateOutcome, BookMetadataLink,
    BookMetadataPatch, BookMetadataPort, BookMetadataService, BookMetadataUpdate,
    BookMetadataUpdateError,
};
pub use metadata_writer::{
    BookEventEmitter, MetadataUpdateResult, MetadataWriter, SearchSyncPort, TaskEnqueuePort,
};
pub use page_retrieval::{
    BookMediaRecord, BookPageRecord, PersistedMediaFileRecord, book_media_is_epub,
    book_media_is_pdf, book_media_is_rar_archive, book_media_is_single_image,
    book_media_is_zip_archive, book_media_supports_page_api, book_media_supports_page_image,
    content_type_from_filename, is_supported_page_image_file_name,
};
pub use ports::{
    ArchiveEntry, BookAccessRestrictions, BookMediaPort, BookProgressionInput,
    BookProgressionRecord, ContentAccessPort, ContentResolverPort, EntityExistencePort,
    EpubCoverImage, EpubExtensionBlob, EpubNavigationExtension, EpubNavigationLink,
    EpubNavigationPosition, ManifestBookRecord, MediaImageDimensions, ProgressWriterPort,
    ReadProgressReadPort, ReadProgressReaderPort, ReadProgressSurfacePort,
    ReadlistTachiyomiCounters, SeriesArchiveEntries, SeriesBookNumberSort, SeriesRelationPort,
    SeriesTachiyomiProgress, SeriesTachiyomiProgressBook, ThumbnailReadPort, ThumbnailWriterPort,
};
pub use read_progress_service::{ReadProgressService, SeriesReadProgressWriterPort};
pub use thumbnail_operations::{
    CollectionThumbnailRecord, EntityThumbnailBinary, EntityThumbnailRecord,
    ReadlistThumbnailRecord, SeriesThumbnailRecord, ThumbnailReaderPort,
};
