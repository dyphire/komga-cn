mod book_import;
mod manifest_builder;
mod metadata_update;
pub mod metadata_writer;
pub mod operations;
mod page_hash_models;
mod page_retrieval;
mod ports;
mod read_progress_service;
mod thumbnail_operations;

pub use book_import::{
    BookImportPort, BookImportService, BookImportSubmissionFailure,
    BookImportSubmissionFailureKind, BooksImportEntry, BooksImportPayload, ImportBookOutcome,
    ImportCopyMode, QueuedBookImportPayload, RuntimeBookImportEvent,
    current_runtime_book_import_event_cursor, generate_prefixed_id, parse_books_import_payload,
    pending_runtime_book_import_events, register_runtime_book_import_event,
};
pub use manifest_builder::{
    ManifestBuildOutcome, ManifestVariant, PersistedManifest, build_persisted_book_manifest,
};
pub use metadata_update::{
    BookMetadata, BookMetadataAuthor, BookMetadataLink, BookMetadataPatch, BookMetadataPort,
    BookMetadataService,
};
pub use metadata_writer::{MetadataUpdateResult, MetadataWriter};
pub use page_hash_models::{PageHashDeleteTarget, PageHashDeleteTargetPage, PageHashThumbnail};
pub use page_retrieval::{
    BookMediaRecord, BookPageRecord, PersistedMediaFileRecord, book_media_is_epub,
    book_media_is_pdf, book_media_is_rar_archive, book_media_is_single_image,
    book_media_is_zip_archive, book_media_supports_page_api, book_media_supports_page_image,
    content_type_from_filename, is_supported_page_image_file_name,
};
pub use ports::{
    BookMediaPort, BookProgressionInput, ContentAccessPort, ContentResolverPort,
    EntityExistencePort, EpubPositionsExtension, MediaReaderPort, ProgressWriterPort,
    ReadProgressReadPort, ReadProgressSurfacePort, SeriesRelationPort, ThumbnailReadPort,
    ThumbnailWriterPort,
};
pub use read_progress_service::ReadProgressService;
pub use thumbnail_operations::{
    CollectionThumbnailRecord, EntityThumbnailBinary, EntityThumbnailRecord,
    ReadlistThumbnailRecord, SeriesThumbnailRecord,
};
