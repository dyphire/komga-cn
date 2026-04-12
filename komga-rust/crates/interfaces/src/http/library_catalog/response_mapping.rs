use komga_application::library_catalog::LibraryRecord;
use serde_json::{Value, json};

use crate::http::helpers::api_file_path;

pub(super) fn libraries_payload(libraries: Vec<LibraryRecord>, is_admin: bool) -> Value {
    Value::Array(
        libraries
            .into_iter()
            .map(|library| library_payload(&library, is_admin))
            .collect(),
    )
}

pub(super) fn library_payload(library: &LibraryRecord, is_admin: bool) -> Value {
    let root = if is_admin {
        api_file_path(&library.root)
    } else {
        String::new()
    };
    json!({
        "id": library.id,
        "name": library.name,
        "root": root,
        "importComicInfoBook": library.import_comicinfo_book,
        "importComicInfoSeries": library.import_comicinfo_series,
        "importComicInfoCollection": library.import_comicinfo_collection,
        "importComicInfoReadList": library.import_comicinfo_readlist,
        "importComicInfoSeriesAppendVolume": library.import_comicinfo_series_append_volume,
        "importEpubBook": library.import_epub_book,
        "importEpubSeries": library.import_epub_series,
        "importMylarSeries": library.import_mylar_series,
        "importLocalArtwork": library.import_local_artwork,
        "importBarcodeIsbn": library.import_barcode_isbn,
        "scanForceModifiedTime": library.scan_force_modified_time,
        "scanInterval": library.scan_interval,
        "scanOnStartup": library.scan_on_startup,
        "scanCbx": library.scan_cbx,
        "scanPdf": library.scan_pdf,
        "scanEpub": library.scan_epub,
        "scanDirectoryExclusions": library.scan_directory_exclusions,
        "repairExtensions": library.repair_extensions,
        "convertToCbz": library.convert_to_cbz,
        "emptyTrashAfterScan": library.empty_trash_after_scan,
        "seriesCover": library.series_cover,
        "hashFiles": library.hash_files,
        "hashPages": library.hash_pages,
        "hashKoreader": library.hash_koreader,
        "analyzeDimensions": library.analyze_dimensions,
        "oneshotsDirectory": library.oneshots_directory,
        "unavailable": library.unavailable,
    })
}
