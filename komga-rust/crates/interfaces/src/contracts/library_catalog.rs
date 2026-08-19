use komga_application::library_catalog::LibraryRecord;
use serde::Serialize;

use crate::helpers::api_file_path;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryDto {
    pub id: String,
    pub name: String,
    pub root: String,
    #[serde(rename = "importComicInfoBook")]
    pub import_comicinfo_book: bool,
    #[serde(rename = "importComicInfoSeries")]
    pub import_comicinfo_series: bool,
    #[serde(rename = "importComicInfoCollection")]
    pub import_comicinfo_collection: bool,
    #[serde(rename = "importComicInfoReadList")]
    pub import_comicinfo_readlist: bool,
    #[serde(rename = "importComicInfoSeriesAppendVolume")]
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

impl LibraryDto {
    pub fn from_record(library: &LibraryRecord, is_admin: bool) -> Self {
        Self {
            id: library.id.clone(),
            name: library.name.clone(),
            root: if is_admin {
                api_file_path(&library.root)
            } else {
                String::new()
            },
            import_comicinfo_book: library.import_comicinfo_book,
            import_comicinfo_series: library.import_comicinfo_series,
            import_comicinfo_collection: library.import_comicinfo_collection,
            import_comicinfo_readlist: library.import_comicinfo_readlist,
            import_comicinfo_series_append_volume: library.import_comicinfo_series_append_volume,
            import_epub_book: library.import_epub_book,
            import_epub_series: library.import_epub_series,
            import_mylar_series: library.import_mylar_series,
            import_local_artwork: library.import_local_artwork,
            import_barcode_isbn: library.import_barcode_isbn,
            scan_force_modified_time: library.scan_force_modified_time,
            scan_interval: library.scan_interval.persisted_name().to_string(),
            scan_on_startup: library.scan_on_startup,
            scan_cbx: library.scan_cbx,
            scan_pdf: library.scan_pdf,
            scan_epub: library.scan_epub,
            scan_directory_exclusions: library.scan_directory_exclusions.clone(),
            repair_extensions: library.repair_extensions,
            convert_to_cbz: library.convert_to_cbz,
            empty_trash_after_scan: library.empty_trash_after_scan,
            series_cover: library.series_cover.persisted_name().to_string(),
            hash_files: library.hash_files,
            hash_pages: library.hash_pages,
            hash_koreader: library.hash_koreader,
            analyze_dimensions: library.analyze_dimensions,
            oneshots_directory: library.oneshots_directory.clone(),
            unavailable: library.unavailable,
        }
    }
}
