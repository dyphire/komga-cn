use std::path::{Path, PathBuf};

use komga_application::media_assets::{
    BookMediaRecord, BookPageRecord, CollectionThumbnailRecord, EntityThumbnailBinary,
    EntityThumbnailRecord, ReadlistThumbnailRecord, SeriesThumbnailRecord,
};
use serde_json::Value;

use super::backend::{RuntimeBookMetadataService, RuntimeMediaImportService, backend};

pub(crate) fn media_import_service(database_file: &Path) -> Box<dyn RuntimeMediaImportService> {
    (backend().media_import_service)(database_file.to_path_buf())
}

pub(crate) fn book_metadata_service(database_file: &Path) -> Box<dyn RuntimeBookMetadataService> {
    (backend().book_metadata_service)(database_file.to_path_buf())
}

pub(crate) async fn persist_book_page_hashes_with_media_content(
    database_file: &Path,
    book_id: &str,
) -> Result<(), String> {
    (backend().persist_book_page_hashes_with_media_content)(
        database_file.to_path_buf(),
        book_id.to_string(),
    )
    .await
}

pub(crate) fn decode_epub_positions(blob: &[u8]) -> Result<Vec<serde_json::Value>, String> {
    (backend().decode_epub_positions)(blob.to_vec())
}

pub(crate) fn load_epub_archive_positions(
    media: &komga_application::media_assets::BookMediaRecord,
) -> Option<Vec<serde_json::Value>> {
    (backend().load_epub_archive_positions)(media.clone())
}

pub(crate) fn read_media_file_bytes(path: &Path) -> Option<Vec<u8>> {
    (backend().read_media_file_bytes)(path.to_path_buf())
}

pub(crate) fn read_media_file_size(path: &Path) -> Option<i64> {
    (backend().read_media_file_size)(path.to_path_buf())
}

pub(crate) async fn load_persisted_book_media(
    database_file: &Path,
    book_id: &str,
) -> Result<Option<BookMediaRecord>, String> {
    (backend().load_persisted_book_media)(database_file.to_path_buf(), book_id.to_string()).await
}

pub(crate) async fn book_media_is_ready_status(
    database_file: &Path,
    book_id: &str,
) -> Result<bool, String> {
    (backend().book_media_is_ready_status)(database_file.to_path_buf(), book_id.to_string()).await
}

pub(crate) async fn load_persisted_series_thumbnail_media(
    database_file: &Path,
    series_id: &str,
) -> Result<Option<BookMediaRecord>, String> {
    (backend().load_persisted_series_thumbnail_media)(
        database_file.to_path_buf(),
        series_id.to_string(),
    )
    .await
}

pub(crate) async fn load_persisted_book_pages(
    database_file: &Path,
    book_id: &str,
) -> Result<Vec<BookPageRecord>, String> {
    (backend().load_persisted_book_pages)(database_file.to_path_buf(), book_id.to_string()).await
}

pub(crate) async fn load_persisted_book_page_row(
    database_file: &Path,
    book_id: &str,
    page_number: u64,
) -> Result<Option<BookPageRecord>, String> {
    (backend().load_persisted_book_page_row)(
        database_file.to_path_buf(),
        book_id.to_string(),
        page_number,
    )
    .await
}

pub(crate) fn resolve_book_page_bytes(
    media: &BookMediaRecord,
    page: &BookPageRecord,
    page_number: u64,
) -> Option<Vec<u8>> {
    (backend().resolve_book_page_bytes)(media.clone(), page.clone(), page_number)
}

pub(crate) fn load_archive_page_row(
    media: &BookMediaRecord,
    page_number: u64,
) -> Option<BookPageRecord> {
    (backend().load_archive_page_row)(media.clone(), page_number)
}

pub(crate) fn load_archive_page_rows(media: &BookMediaRecord) -> Option<Vec<BookPageRecord>> {
    (backend().load_archive_page_rows)(media.clone())
}

pub(crate) fn load_pdf_page_row(
    media: &BookMediaRecord,
    page_number: u64,
) -> Option<BookPageRecord> {
    (backend().load_pdf_page_row)(media.clone(), page_number)
}

pub(crate) fn load_generated_pdf_page_rows(media: &BookMediaRecord) -> Vec<BookPageRecord> {
    (backend().load_generated_pdf_page_rows)(media.clone())
}

pub(crate) fn read_pdf_page_as_single_page_pdf(
    media: &BookMediaRecord,
    page_number: u64,
) -> Option<Vec<u8>> {
    (backend().read_pdf_page_as_single_page_pdf)(media.clone(), page_number)
}

pub(crate) fn detect_pdf_page_count(media: &BookMediaRecord) -> Option<u64> {
    (backend().detect_pdf_page_count)(media.clone())
}

pub(crate) async fn load_persisted_epub_extension_blob(
    database_file: &Path,
    book_id: &str,
) -> Result<Option<(String, Vec<u8>)>, String> {
    (backend().load_persisted_epub_extension_blob)(database_file.to_path_buf(), book_id.to_string())
        .await
}

pub(crate) async fn load_series_book_ids(
    database_file: &Path,
    series_id: &str,
) -> Result<Vec<String>, String> {
    (backend().load_series_book_ids)(database_file.to_path_buf(), series_id.to_string()).await
}

pub(crate) async fn refresh_series_read_progress_row(
    database_file: &Path,
    series_id: &str,
    user_id: &str,
) -> Result<(), String> {
    (backend().refresh_series_read_progress_row)(
        database_file.to_path_buf(),
        series_id.to_string(),
        user_id.to_string(),
    )
    .await
}

pub(crate) async fn delete_series_read_progress_row(
    database_file: &Path,
    series_id: &str,
    user_id: &str,
) -> Result<(), String> {
    (backend().delete_series_read_progress_row)(
        database_file.to_path_buf(),
        series_id.to_string(),
        user_id.to_string(),
    )
    .await
}

pub(crate) async fn load_series_tachiyomi_progress(
    database_file: &Path,
    series_id: &str,
    user_id: &str,
) -> Result<Option<Value>, String> {
    (backend().load_series_tachiyomi_progress)(
        database_file.to_path_buf(),
        series_id.to_string(),
        user_id.to_string(),
    )
    .await
}

pub(crate) async fn load_book_progression(
    database_file: &Path,
    book_id: &str,
    user_id: &str,
) -> Result<Option<f64>, String> {
    (backend().load_book_progression)(
        database_file.to_path_buf(),
        book_id.to_string(),
        user_id.to_string(),
    )
    .await
}

pub(crate) async fn persist_read_progress(
    database_file: &Path,
    book_id: &str,
    user_id: &str,
    page: u64,
    completed: bool,
) -> Result<(), String> {
    (backend().persist_read_progress)(
        database_file.to_path_buf(),
        book_id.to_string(),
        user_id.to_string(),
        page,
        completed,
    )
    .await
}

pub(crate) async fn delete_persisted_read_progress(
    database_file: &Path,
    book_id: &str,
    user_id: &str,
) -> Result<(), String> {
    (backend().delete_persisted_read_progress)(
        database_file.to_path_buf(),
        book_id.to_string(),
        user_id.to_string(),
    )
    .await
}

pub(crate) async fn readlist_tachiyomi_counters(
    database_file: &Path,
    readlist_id: &str,
    user_id: &str,
) -> Result<Option<(u64, u64, u64, u64, u64)>, String> {
    (backend().readlist_tachiyomi_counters)(
        database_file.to_path_buf(),
        readlist_id.to_string(),
        user_id.to_string(),
    )
    .await
}

pub(crate) async fn persist_readlist_tachiyomi_progress(
    database_file: &Path,
    readlist_id: &str,
    user_id: &str,
    last_book_read: usize,
) -> Result<Option<()>, String> {
    (backend().persist_readlist_tachiyomi_progress)(
        database_file.to_path_buf(),
        readlist_id.to_string(),
        user_id.to_string(),
        last_book_read,
    )
    .await
}

pub(crate) async fn load_selected_book_thumbnail(
    database_file: &Path,
    book_id: &str,
) -> Result<Option<EntityThumbnailBinary>, String> {
    (backend().load_selected_book_thumbnail)(database_file.to_path_buf(), book_id.to_string()).await
}

pub(crate) async fn load_book_thumbnail_by_id(
    database_file: &Path,
    book_id: &str,
    thumbnail_id: &str,
) -> Result<Option<EntityThumbnailBinary>, String> {
    (backend().load_book_thumbnail_by_id)(
        database_file.to_path_buf(),
        book_id.to_string(),
        thumbnail_id.to_string(),
    )
    .await
}

pub(crate) async fn load_persisted_book_thumbnails(
    database_file: &Path,
    book_id: &str,
) -> Result<Vec<EntityThumbnailRecord>, String> {
    (backend().load_persisted_book_thumbnails)(database_file.to_path_buf(), book_id.to_string())
        .await
}

pub(crate) async fn insert_book_thumbnail(
    database_file: &Path,
    book_id: &str,
    thumbnail: &[u8],
    media_type: &str,
    width: i64,
    height: i64,
    selected: bool,
) -> Result<EntityThumbnailRecord, String> {
    (backend().insert_book_thumbnail)(
        database_file.to_path_buf(),
        book_id.to_string(),
        thumbnail.to_vec(),
        media_type.to_string(),
        width,
        height,
        selected,
    )
    .await
}

pub(crate) async fn select_book_thumbnail(
    database_file: &Path,
    book_id: &str,
    thumbnail_id: &str,
) -> Result<bool, String> {
    (backend().select_book_thumbnail)(
        database_file.to_path_buf(),
        book_id.to_string(),
        thumbnail_id.to_string(),
    )
    .await
}

pub(crate) async fn delete_book_thumbnail(
    database_file: &Path,
    book_id: &str,
    thumbnail_id: &str,
) -> Result<bool, String> {
    (backend().delete_book_thumbnail)(
        database_file.to_path_buf(),
        book_id.to_string(),
        thumbnail_id.to_string(),
    )
    .await
}

pub(crate) async fn load_persisted_readlist_thumbnails(
    database_file: &Path,
    readlist_id: &str,
) -> Result<Vec<ReadlistThumbnailRecord>, String> {
    (backend().load_persisted_readlist_thumbnails)(
        database_file.to_path_buf(),
        readlist_id.to_string(),
    )
    .await
}

pub(crate) async fn insert_readlist_thumbnail(
    database_file: &Path,
    readlist_id: &str,
    thumbnail: &[u8],
    media_type: &str,
    width: i64,
    height: i64,
    selected: bool,
) -> Result<ReadlistThumbnailRecord, String> {
    (backend().insert_readlist_thumbnail)(
        database_file.to_path_buf(),
        readlist_id.to_string(),
        thumbnail.to_vec(),
        media_type.to_string(),
        width,
        height,
        selected,
    )
    .await
}

pub(crate) async fn select_readlist_thumbnail(
    database_file: &Path,
    readlist_id: &str,
    thumbnail_id: &str,
) -> Result<bool, String> {
    (backend().select_readlist_thumbnail)(
        database_file.to_path_buf(),
        readlist_id.to_string(),
        thumbnail_id.to_string(),
    )
    .await
}

pub(crate) async fn delete_readlist_thumbnail(
    database_file: &Path,
    readlist_id: &str,
    thumbnail_id: &str,
) -> Result<bool, String> {
    (backend().delete_readlist_thumbnail)(
        database_file.to_path_buf(),
        readlist_id.to_string(),
        thumbnail_id.to_string(),
    )
    .await
}

pub(crate) async fn load_persisted_collection_thumbnails(
    database_file: &Path,
    collection_id: &str,
) -> Result<Vec<CollectionThumbnailRecord>, String> {
    (backend().load_persisted_collection_thumbnails)(
        database_file.to_path_buf(),
        collection_id.to_string(),
    )
    .await
}

pub(crate) async fn insert_collection_thumbnail(
    database_file: &Path,
    collection_id: &str,
    thumbnail: &[u8],
    media_type: &str,
    width: i64,
    height: i64,
    selected: bool,
) -> Result<CollectionThumbnailRecord, String> {
    (backend().insert_collection_thumbnail)(
        database_file.to_path_buf(),
        collection_id.to_string(),
        thumbnail.to_vec(),
        media_type.to_string(),
        width,
        height,
        selected,
    )
    .await
}

pub(crate) async fn select_collection_thumbnail(
    database_file: &Path,
    collection_id: &str,
    thumbnail_id: &str,
) -> Result<bool, String> {
    (backend().select_collection_thumbnail)(
        database_file.to_path_buf(),
        collection_id.to_string(),
        thumbnail_id.to_string(),
    )
    .await
}

pub(crate) async fn delete_collection_thumbnail(
    database_file: &Path,
    collection_id: &str,
    thumbnail_id: &str,
) -> Result<bool, String> {
    (backend().delete_collection_thumbnail)(
        database_file.to_path_buf(),
        collection_id.to_string(),
        thumbnail_id.to_string(),
    )
    .await
}

pub(crate) async fn load_selected_series_thumbnail(
    database_file: &Path,
    series_id: &str,
) -> Result<Option<EntityThumbnailBinary>, String> {
    (backend().load_selected_series_thumbnail)(database_file.to_path_buf(), series_id.to_string())
        .await
}

pub(crate) async fn load_persisted_series_thumbnails(
    database_file: &Path,
    series_id: &str,
) -> Result<Vec<SeriesThumbnailRecord>, String> {
    (backend().load_persisted_series_thumbnails)(database_file.to_path_buf(), series_id.to_string())
        .await
}

pub(crate) async fn load_series_thumbnail_by_id(
    database_file: &Path,
    series_id: &str,
    thumbnail_id: &str,
) -> Result<Option<EntityThumbnailBinary>, String> {
    (backend().load_series_thumbnail_by_id)(
        database_file.to_path_buf(),
        series_id.to_string(),
        thumbnail_id.to_string(),
    )
    .await
}

pub(crate) async fn insert_series_thumbnail(
    database_file: &Path,
    series_id: &str,
    thumbnail: &[u8],
    media_type: &str,
    width: i64,
    height: i64,
    selected: bool,
) -> Result<SeriesThumbnailRecord, String> {
    (backend().insert_series_thumbnail)(
        database_file.to_path_buf(),
        series_id.to_string(),
        thumbnail.to_vec(),
        media_type.to_string(),
        width,
        height,
        selected,
    )
    .await
}

pub(crate) async fn select_series_thumbnail(
    database_file: &Path,
    series_id: &str,
    thumbnail_id: &str,
) -> Result<bool, String> {
    (backend().select_series_thumbnail)(
        database_file.to_path_buf(),
        series_id.to_string(),
        thumbnail_id.to_string(),
    )
    .await
}

pub(crate) async fn delete_series_thumbnail(
    database_file: &Path,
    series_id: &str,
    thumbnail_id: &str,
) -> Result<bool, String> {
    (backend().delete_series_thumbnail)(
        database_file.to_path_buf(),
        series_id.to_string(),
        thumbnail_id.to_string(),
    )
    .await
}

pub(crate) async fn load_persisted_readlist_name(
    database_file: &Path,
    readlist_id: &str,
) -> Result<Option<String>, String> {
    (backend().load_persisted_readlist_name)(database_file.to_path_buf(), readlist_id.to_string())
        .await
}

pub(crate) async fn load_book_restrictions(
    database_file: &Path,
    book_id: &str,
) -> Result<Option<(Option<u16>, Vec<String>)>, String> {
    (backend().load_book_restrictions)(database_file.to_path_buf(), book_id.to_string()).await
}

pub(crate) async fn load_series_archive_entries(
    database_file: &Path,
    series_id: &str,
) -> Result<Option<(String, String, Vec<(String, PathBuf)>)>, String> {
    (backend().load_series_archive_entries)(database_file.to_path_buf(), series_id.to_string())
        .await
}

pub(crate) fn is_font_resource(resource_name: &str) -> bool {
    (backend().is_font_resource)(resource_name.to_string())
}

pub(crate) fn read_epub_resource_bytes(epub_path: &Path, resource_name: &str) -> Option<Vec<u8>> {
    (backend().read_epub_resource_bytes)(epub_path.to_path_buf(), resource_name.to_string())
}

pub(crate) async fn load_persisted_manifest_book(
    database_file: &Path,
    book_id: &str,
) -> Result<Option<(String, String, String)>, String> {
    (backend().load_persisted_manifest_book)(database_file.to_path_buf(), book_id.to_string()).await
}

pub(crate) async fn persisted_book_exists(
    database_file: &Path,
    book_id: &str,
) -> Result<bool, String> {
    (backend().persisted_book_exists)(database_file.to_path_buf(), book_id.to_string()).await
}

pub(crate) async fn persisted_book_ids(database_file: &Path) -> Result<Vec<String>, String> {
    (backend().persisted_book_ids)(database_file.to_path_buf()).await
}

pub(crate) async fn persisted_series_exists(
    database_file: &Path,
    series_id: &str,
) -> Result<bool, String> {
    (backend().persisted_series_exists)(database_file.to_path_buf(), series_id.to_string()).await
}

pub(crate) async fn persisted_readlist_exists(
    database_file: &Path,
    readlist_id: &str,
) -> Result<bool, String> {
    (backend().persisted_readlist_exists)(database_file.to_path_buf(), readlist_id.to_string())
        .await
}

pub(crate) async fn persisted_collection_exists(
    database_file: &Path,
    collection_id: &str,
) -> Result<bool, String> {
    (backend().persisted_collection_exists)(database_file.to_path_buf(), collection_id.to_string())
        .await
}

pub(crate) async fn load_series_book_number_sorts(
    database_file: &Path,
    series_id: &str,
) -> Result<Vec<(String, f64)>, String> {
    (backend().load_series_book_number_sorts)(database_file.to_path_buf(), series_id.to_string())
        .await
}

pub(crate) async fn load_book_page_count(
    database_file: &Path,
    book_id: &str,
) -> Result<Option<u64>, String> {
    (backend().load_book_page_count)(database_file.to_path_buf(), book_id.to_string()).await
}

pub(crate) async fn persist_book_progression(
    database_file: &Path,
    book_id: &str,
    user_id: &str,
    page: f64,
) -> Result<(), String> {
    (backend().persist_book_progression)(
        database_file.to_path_buf(),
        book_id.to_string(),
        user_id.to_string(),
        page,
    )
    .await
}
