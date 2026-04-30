use std::path::PathBuf;

use komga_application::library_catalog::{
    LibraryCatalogMutationPort, LibraryCatalogReadPort, LibraryRecord,
};
use komga_application::runtime_sse::register_runtime_sse_event;
use komga_domain::discovery::{DiscoveryError, DiscoveryQueryContext};
use serde_json::json;
use sqlx::SqlitePool;

use crate::read_models::{get_persisted_library, list_persisted_libraries};
use crate::sqlite::write_models::libraries::{
    PersistedLibraryWriteModel, delete_persisted_library, library_book_ids,
    library_book_ids_with_empty_hash, library_series_and_book_ids,
    load_persisted_library_write_model, persist_library_create, persist_library_update,
    validate_library_before_persist,
};

#[derive(Clone, Debug)]
pub struct SqliteLibraryCatalogAdapter {
    database_file: PathBuf,
    task_write_pool: SqlitePool,
}

impl SqliteLibraryCatalogAdapter {
    pub fn new(database_file: impl Into<PathBuf>, task_write_pool: SqlitePool) -> Self {
        Self {
            database_file: database_file.into(),
            task_write_pool,
        }
    }
}

impl LibraryCatalogReadPort for SqliteLibraryCatalogAdapter {
    async fn list_libraries(
        &self,
        context: &DiscoveryQueryContext,
    ) -> Result<Vec<LibraryRecord>, DiscoveryError> {
        let libraries = list_persisted_libraries(self.database_file.as_path(), context).await?;
        Ok(libraries.into_iter().map(LibraryRecord::from).collect())
    }

    async fn get_library(
        &self,
        context: &DiscoveryQueryContext,
        library_id: &str,
    ) -> Result<Option<LibraryRecord>, DiscoveryError> {
        let library =
            get_persisted_library(self.database_file.as_path(), context, library_id).await?;
        Ok(library.map(LibraryRecord::from))
    }
}

impl LibraryCatalogMutationPort for SqliteLibraryCatalogAdapter {
    async fn load_library(&self, library_id: &str) -> Result<Option<LibraryRecord>, String> {
        load_persisted_library_write_model(self.database_file.as_path(), library_id)
            .await
            .map(|library| library.map(LibraryRecord::from))
            .map_err(|error| format!("load persisted library: {error}"))
    }

    async fn validate_library(&self, library: &LibraryRecord) -> Result<(), String> {
        validate_library_before_persist(self.database_file.as_path(), &library.clone().into()).await
    }

    async fn create_library(&self, library: &LibraryRecord) -> Result<(), String> {
        persist_library_create(self.database_file.as_path(), &library.clone().into())
            .await
            .map_err(|error| format!("persist library create: {error}"))?;
        register_runtime_sse_event(
            "LibraryAdded",
            json!({ "libraryId": library.id }),
            false,
            None,
        );
        Ok(())
    }

    async fn update_library(&self, library: &LibraryRecord) -> Result<bool, String> {
        let updated = persist_library_update(self.database_file.as_path(), &library.clone().into())
            .await
            .map_err(|error| format!("persist library update: {error}"))?;
        if updated {
            register_runtime_sse_event(
                "LibraryChanged",
                json!({ "libraryId": library.id }),
                false,
                None,
            );
        }
        Ok(updated)
    }

    async fn delete_library(&self, library_id: &str) -> Result<bool, String> {
        let deleted = delete_persisted_library(self.database_file.as_path(), library_id)
            .await
            .map_err(|error| format!("delete persisted library: {error}"))?;
        if deleted {
            register_runtime_sse_event(
                "LibraryDeleted",
                json!({ "libraryId": library_id }),
                false,
                None,
            );
        }
        Ok(deleted)
    }

    async fn library_book_ids_with_empty_hash(
        &self,
        library_id: &str,
        koreader: bool,
    ) -> Result<Vec<String>, String> {
        library_book_ids_with_empty_hash(self.database_file.as_path(), library_id, koreader).await
    }

    async fn library_books_with_mismatched_extensions(
        &self,
        library_id: &str,
    ) -> Result<Vec<(String, String)>, String> {
        crate::task_queue::media_helpers::media_queries::load_books_for_extension_repair(
            &self.task_write_pool,
            library_id,
        )
        .await
        .map(|rows| {
            rows.into_iter()
                .map(|row| (row.book_id, row.series_id))
                .collect()
        })
        .map_err(|error| format!("load library mismatched extension books: {error}"))
    }

    async fn library_book_ids(&self, library_id: &str) -> Result<Option<Vec<String>>, String> {
        library_book_ids(self.database_file.as_path(), library_id)
            .await
            .map_err(|error| format!("load library book ids: {error}"))
    }

    async fn library_series_and_book_ids(
        &self,
        library_id: &str,
    ) -> Result<Option<(Vec<String>, Vec<(String, String)>)>, String> {
        library_series_and_book_ids(self.database_file.as_path(), library_id)
            .await
            .map_err(|error| format!("load library series and book ids: {error}"))
    }
}

impl From<crate::read_models::PersistedLibraryReadModel> for LibraryRecord {
    fn from(value: crate::read_models::PersistedLibraryReadModel) -> Self {
        Self {
            id: value.id,
            name: value.name,
            root: value.root,
            import_comicinfo_book: value.import_comicinfo_book,
            import_comicinfo_series: value.import_comicinfo_series,
            import_comicinfo_collection: value.import_comicinfo_collection,
            import_comicinfo_readlist: value.import_comicinfo_readlist,
            import_comicinfo_series_append_volume: value.import_comicinfo_series_append_volume,
            import_epub_book: value.import_epub_book,
            import_epub_series: value.import_epub_series,
            import_mylar_series: value.import_mylar_series,
            import_local_artwork: value.import_local_artwork,
            import_barcode_isbn: value.import_barcode_isbn,
            scan_force_modified_time: value.scan_force_modified_time,
            scan_interval: value.scan_interval,
            scan_on_startup: value.scan_on_startup,
            scan_cbx: value.scan_cbx,
            scan_pdf: value.scan_pdf,
            scan_epub: value.scan_epub,
            scan_directory_exclusions: value.scan_directory_exclusions,
            repair_extensions: value.repair_extensions,
            convert_to_cbz: value.convert_to_cbz,
            empty_trash_after_scan: value.empty_trash_after_scan,
            series_cover: value.series_cover,
            hash_files: value.hash_files,
            hash_pages: value.hash_pages,
            hash_koreader: value.hash_koreader,
            analyze_dimensions: value.analyze_dimensions,
            oneshots_directory: value.oneshots_directory,
            unavailable: value.unavailable,
        }
    }
}

impl From<crate::sqlite::write_models::libraries::PersistedLibraryWriteModel> for LibraryRecord {
    fn from(value: crate::sqlite::write_models::libraries::PersistedLibraryWriteModel) -> Self {
        Self {
            id: value.id,
            name: value.name,
            root: value.root,
            import_comicinfo_book: value.import_comicinfo_book,
            import_comicinfo_series: value.import_comicinfo_series,
            import_comicinfo_collection: value.import_comicinfo_collection,
            import_comicinfo_readlist: value.import_comicinfo_readlist,
            import_comicinfo_series_append_volume: value.import_comicinfo_series_append_volume,
            import_epub_book: value.import_epub_book,
            import_epub_series: value.import_epub_series,
            import_mylar_series: value.import_mylar_series,
            import_local_artwork: value.import_local_artwork,
            import_barcode_isbn: value.import_barcode_isbn,
            scan_force_modified_time: value.scan_force_modified_time,
            scan_interval: value.scan_interval,
            scan_on_startup: value.scan_on_startup,
            scan_cbx: value.scan_cbx,
            scan_pdf: value.scan_pdf,
            scan_epub: value.scan_epub,
            scan_directory_exclusions: value.scan_directory_exclusions,
            repair_extensions: value.repair_extensions,
            convert_to_cbz: value.convert_to_cbz,
            empty_trash_after_scan: value.empty_trash_after_scan,
            series_cover: value.series_cover,
            hash_files: value.hash_files,
            hash_pages: value.hash_pages,
            hash_koreader: value.hash_koreader,
            analyze_dimensions: value.analyze_dimensions,
            oneshots_directory: value.oneshots_directory,
            unavailable: value.unavailable,
        }
    }
}

impl From<LibraryRecord> for PersistedLibraryWriteModel {
    fn from(value: LibraryRecord) -> Self {
        Self {
            id: value.id,
            name: value.name,
            root: value.root,
            import_comicinfo_book: value.import_comicinfo_book,
            import_comicinfo_series: value.import_comicinfo_series,
            import_comicinfo_collection: value.import_comicinfo_collection,
            import_comicinfo_readlist: value.import_comicinfo_readlist,
            import_comicinfo_series_append_volume: value.import_comicinfo_series_append_volume,
            import_epub_book: value.import_epub_book,
            import_epub_series: value.import_epub_series,
            import_mylar_series: value.import_mylar_series,
            import_local_artwork: value.import_local_artwork,
            import_barcode_isbn: value.import_barcode_isbn,
            scan_force_modified_time: value.scan_force_modified_time,
            scan_interval: value.scan_interval,
            scan_on_startup: value.scan_on_startup,
            scan_cbx: value.scan_cbx,
            scan_pdf: value.scan_pdf,
            scan_epub: value.scan_epub,
            scan_directory_exclusions: value.scan_directory_exclusions,
            repair_extensions: value.repair_extensions,
            convert_to_cbz: value.convert_to_cbz,
            empty_trash_after_scan: value.empty_trash_after_scan,
            series_cover: value.series_cover,
            hash_files: value.hash_files,
            hash_pages: value.hash_pages,
            hash_koreader: value.hash_koreader,
            analyze_dimensions: value.analyze_dimensions,
            oneshots_directory: value.oneshots_directory,
            unavailable: value.unavailable,
        }
    }
}
