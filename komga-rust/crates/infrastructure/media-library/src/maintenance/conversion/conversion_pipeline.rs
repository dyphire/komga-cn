use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

use komga_application::task_processing::TaskProcessingError;
use komga_domain::discovery::MediaStatus;
use zip::ZipArchive;

use super::super::archive::{
    build_stored_zip_archive, load_rar_entries_for_conversion, metadata_updated_unix_seconds,
    normalize_library_relative_url,
};
use super::super::persistence::{
    PersistedBookToConvert, PersistedHashedPageToDelete, load_book_conversion_target,
    load_book_hashed_pages, load_books_to_convert, load_library_maintenance_flags,
};
use super::super::updates::{
    BookPageHashWrite, persist_book_conversion, persist_book_conversion_events,
    persist_book_page_hashes,
};
use crate::MediaLibraryJobContext;
use crate::analysis::is_rar_media_type;
use komga_infrastructure_base::file_io::remove_file_after_release;
use komga_infrastructure_base::{resolve_library_item_path, resolve_stored_path};

struct PreparedBookConversion {
    destination_path: PathBuf,
    destination_url: String,
    file_last_modified: i64,
    file_size: i64,
    source_path: PathBuf,
}

fn restored_page_hashes(
    current_pages: &[PersistedHashedPageToDelete],
    previous_pages: &[PersistedHashedPageToDelete],
) -> Vec<BookPageHashWrite> {
    current_pages
        .iter()
        .filter_map(|current_page| {
            previous_pages
                .iter()
                .find(|previous_page| {
                    previous_page.file_size == current_page.file_size
                        && previous_page.media_type == current_page.media_type
                        && previous_page.file_name == current_page.file_name
                        && !previous_page.file_hash.trim().is_empty()
                })
                .map(|previous_page| BookPageHashWrite {
                    page_number: current_page.page_number,
                    file_hash: previous_page.file_hash.clone(),
                })
        })
        .collect()
}

pub async fn find_books_to_convert(
    runtime: &MediaLibraryJobContext,
    library_id: &str,
) -> Result<Vec<PersistedBookToConvert>, TaskProcessingError> {
    let maintenance_flags =
        load_library_maintenance_flags(runtime.database().read_pool(), library_id)
            .await
            .map_err(TaskProcessingError::runtime)?;
    if !maintenance_flags.convert_to_cbz {
        return Ok(Vec::new());
    }

    load_books_to_convert(runtime.database().read_pool(), library_id)
        .await
        .map_err(TaskProcessingError::runtime)
}

pub async fn convert_book(
    runtime: &MediaLibraryJobContext,
    book_id: &str,
) -> Result<(), TaskProcessingError> {
    let book_id = book_id.to_string();

    let Some(source) = load_book_conversion_target(runtime.database().read_pool(), &book_id)
        .await
        .map_err(TaskProcessingError::runtime)?
    else {
        return Ok(());
    };

    if !source.convert_to_cbz {
        return Ok(());
    }
    if runtime.book_conversion_failed_before(&book_id) {
        return Ok(());
    }

    if source.media_status != Some(MediaStatus::Ready) {
        return Ok(());
    }
    if !is_rar_media_type(&source.media_type) {
        return Ok(());
    }

    let library_root = resolve_stored_path(&source.library_root);
    let source_path = resolve_library_item_path(&source.library_root, &source.book_url);
    let source_file_last_modified = source.file_last_modified;
    let convert_book_id = book_id.clone();
    let prepared_conversion = (|| {
        let source_metadata = match fs::metadata(&source_path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(TaskProcessingError::runtime(format!(
                    "failed to read source file metadata for conversion '{convert_book_id}' ('{}'): {error}",
                    source_path.display(),
                )));
            }
        };
        let current_source_file_last_modified =
            metadata_updated_unix_seconds(&source_metadata, &source_path)
                .map_err(TaskProcessingError::runtime)?;
        if current_source_file_last_modified != source_file_last_modified {
            return Ok(None);
        }

        let destination_path = source_path.with_extension("cbz");
        match fs::metadata(&destination_path) {
            Ok(_) => {
                return Err(TaskProcessingError::runtime(format!(
                    "failed to convert book '{convert_book_id}' to CBZ: destination already exists '{}'",
                    destination_path.display(),
                )));
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => {
                return Err(TaskProcessingError::runtime(format!(
                    "failed to inspect conversion destination for '{convert_book_id}' ('{}'): {error}",
                    destination_path.display(),
                )));
            }
        }

        let payload = {
            let archive_entries = load_rar_entries_for_conversion(&source_path)?;
            if archive_entries.is_empty() {
                return Err(TaskProcessingError::runtime(format!(
                    "failed to convert book '{convert_book_id}' to CBZ: no archive entries extracted",
                )));
            }

            build_stored_zip_archive(archive_entries)?
        };
        fs::write(&destination_path, payload).map_err(|error| {
            TaskProcessingError::runtime(format!(
                "failed to write converted CBZ file for '{convert_book_id}' to '{}': {error}",
                destination_path.display(),
            ))
        })?;

        {
            let destination_file = fs::File::open(&destination_path).map_err(|error| {
                TaskProcessingError::runtime(format!(
                    "failed to open converted file for '{convert_book_id}' ('{}'): {error}",
                    destination_path.display(),
                ))
            })?;
            let _validated_archive = ZipArchive::new(destination_file).map_err(|error| {
                TaskProcessingError::runtime(format!(
                    "failed to validate converted CBZ for '{convert_book_id}': {error}",
                ))
            })?;
        }

        let destination_url = normalize_library_relative_url(&library_root, &destination_path)?;
        let destination_metadata = fs::metadata(&destination_path).map_err(|error| {
            TaskProcessingError::runtime(format!(
                "failed to read converted CBZ metadata for '{convert_book_id}' ('{}'): {error}",
                destination_path.display(),
            ))
        })?;
        let destination_file_last_modified =
            metadata_updated_unix_seconds(&destination_metadata, &destination_path)
                .map_err(TaskProcessingError::runtime)?;

        Ok(Some(PreparedBookConversion {
            destination_path,
            destination_url,
            file_last_modified: destination_file_last_modified,
            file_size: destination_metadata.len() as i64,
            source_path,
        }))
    })();

    let Some(conversion) = (match prepared_conversion {
        Ok(prepared) => prepared,
        Err(error) => {
            runtime.mark_book_conversion_failed(&book_id);
            return Err(error);
        }
    }) else {
        return Ok(());
    };

    if let Err(error) = persist_book_conversion(
        runtime.database().write_pool(),
        runtime.runtime_events(),
        &book_id,
        &source.library_id,
        &source.book_url,
        &conversion.destination_url,
        conversion.file_last_modified,
        conversion.file_size,
    )
    .await
    {
        let revert_destination_path = conversion.destination_path.clone();
        let _ = tokio::fs::remove_file(&revert_destination_path).await;
        return Err(TaskProcessingError::runtime(error));
    }

    let source_path_for_delete = conversion.source_path.clone();
    let source_deleted = tokio::task::spawn_blocking(move || {
        remove_file_after_release(&source_path_for_delete).unwrap_or(false)
    })
    .await
    .unwrap_or(false);
    persist_book_conversion_events(
        runtime.database().write_pool(),
        &book_id,
        &source.series_id,
        &conversion.source_path,
        &conversion.destination_path,
        source_deleted,
    )
    .await
    .map_err(TaskProcessingError::runtime)?;

    let previous_hashed_pages = load_book_hashed_pages(runtime.database().read_pool(), &book_id)
        .await
        .map_err(TaskProcessingError::runtime)?;

    crate::analysis::analyze_book(runtime, &book_id).await?;

    let analyzed_pages = load_book_hashed_pages(runtime.database().read_pool(), &book_id)
        .await
        .map_err(TaskProcessingError::runtime)?;
    let page_hashes_to_restore = restored_page_hashes(&analyzed_pages, &previous_hashed_pages);
    persist_book_page_hashes(
        runtime.database().write_pool(),
        &book_id,
        &page_hashes_to_restore,
    )
    .await
    .map_err(TaskProcessingError::runtime)?;

    Ok(())
}
