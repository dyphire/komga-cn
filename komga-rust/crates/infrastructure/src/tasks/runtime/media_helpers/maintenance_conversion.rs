use super::archive_utils::{
    build_stored_zip_archive, load_rar_entries_for_conversion, metadata_updated_unix_seconds,
    normalize_library_relative_url,
};
use super::library_flags::load_library_maintenance_flags;
use super::media_analysis::{expected_extension_for_media_type, is_rar_media_type};
use super::*;

#[path = "maintenance_conversion/conversion_pipeline.rs"]
mod conversion_pipeline;
#[path = "maintenance_conversion/extension_repair.rs"]
mod extension_repair;
#[path = "maintenance_conversion/file_hash_queries.rs"]
mod file_hash_queries;

pub(in crate::task_queue) use conversion_pipeline::{convert_book, find_books_to_convert};
pub(in crate::task_queue) use extension_repair::{
    find_books_for_extension_repair, repair_extension,
};
pub(in crate::task_queue) use file_hash_queries::find_books_with_missing_file_hash;
