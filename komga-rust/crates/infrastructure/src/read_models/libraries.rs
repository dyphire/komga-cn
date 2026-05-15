use sqlx::SqlitePool;

use komga_domain::discovery::{DiscoveryError, DiscoveryQueryContext};

use super::queries;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedLibraryReadModel {
    pub id: String,
    pub name: String,
    pub root: String,
    pub import_comicinfo_book: bool,
    pub import_comicinfo_series: bool,
    pub import_comicinfo_collection: bool,
    pub import_comicinfo_readlist: bool,
    pub import_comicinfo_series_append_volume: bool,
    pub import_epub_book: bool,
    pub import_epub_series: bool,
    pub import_mylar_series: bool,
    pub import_local_artwork: bool,
    pub import_barcode_isbn: bool,
    pub scan_force_modified_time: bool,
    pub scan_interval: String,
    pub scan_on_startup: bool,
    pub scan_cbx: bool,
    pub scan_pdf: bool,
    pub scan_epub: bool,
    pub scan_directory_exclusions: Vec<String>,
    pub repair_extensions: bool,
    pub convert_to_cbz: bool,
    pub empty_trash_after_scan: bool,
    pub series_cover: String,
    pub hash_files: bool,
    pub hash_pages: bool,
    pub hash_koreader: bool,
    pub analyze_dimensions: bool,
    pub oneshots_directory: Option<String>,
    pub unavailable: bool,
}

pub async fn list_persisted_libraries(
    pool: &SqlitePool,
    context: &DiscoveryQueryContext,
) -> Result<Vec<PersistedLibraryReadModel>, DiscoveryError> {
    queries::libraries::list_persisted_libraries_sqlx(pool.clone(), context).await
}

pub async fn get_persisted_library(
    pool: &SqlitePool,
    context: &DiscoveryQueryContext,
    library_id: &str,
) -> Result<Option<PersistedLibraryReadModel>, DiscoveryError> {
    queries::libraries::get_persisted_library_sqlx(pool.clone(), context, library_id).await
}
