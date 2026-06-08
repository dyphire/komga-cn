use std::io::Read;
use std::time::{SystemTime, UNIX_EPOCH};

use super::task_records::{
    analyze_library_task_records, background_scan_library_task_record, empty_trash_task_records,
    library_should_rescan, manual_scan_library_task_record, metadata_refresh_task_records,
};
use super::{
    LibraryCatalogMutationError, LibraryCatalogMutationPort, LibraryChangeSet, LibraryRecord,
};
use crate::task_processing::{
    BookSeriesRef, LibraryTaskCommand, TaskQueueRecord, emit_library_task_batch,
};

pub struct LibraryCatalogCommandService<P> {
    port: P,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateLibraryResult {
    pub library: LibraryRecord,
    pub task_records: Vec<TaskQueueRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LibraryTaskResult {
    pub task_records: Vec<TaskQueueRecord>,
}

impl<P> LibraryCatalogCommandService<P>
where
    P: LibraryCatalogMutationPort,
{
    pub fn new(port: P) -> Self {
        Self { port }
    }

    pub async fn create_library(
        &self,
        changes: LibraryChangeSet,
    ) -> Result<CreateLibraryResult, LibraryCatalogMutationError> {
        let mut library = LibraryRecord::default_record(generated_library_id());
        library.apply_changes(changes);
        ensure_name_and_root(&library)?;
        self.port
            .validate_library(&library)
            .await
            .map_err(LibraryCatalogMutationError::Validation)?;
        self.port
            .create_library(&library)
            .await
            .map_err(LibraryCatalogMutationError::persistence)?;

        Ok(CreateLibraryResult {
            task_records: vec![background_scan_library_task_record(&library.id, false)],
            library,
        })
    }

    pub async fn update_library(
        &self,
        library_id: &str,
        changes: LibraryChangeSet,
    ) -> Result<LibraryTaskResult, LibraryCatalogMutationError> {
        let mut library = self
            .port
            .load_library(library_id)
            .await
            .map_err(LibraryCatalogMutationError::persistence)?
            .ok_or(LibraryCatalogMutationError::NotFound)?;
        let previous_library = library.clone();
        library.apply_changes(changes);

        self.port
            .validate_library(&library)
            .await
            .map_err(LibraryCatalogMutationError::Validation)?;
        let updated = self
            .port
            .update_library(&library)
            .await
            .map_err(LibraryCatalogMutationError::persistence)?;
        if !updated {
            return Err(LibraryCatalogMutationError::NotFound);
        }

        Ok(task_result(
            self.follow_up_tasks_for_library_update(&previous_library, &library)
                .await?,
        ))
    }

    pub async fn delete_library(
        &self,
        library_id: &str,
    ) -> Result<bool, LibraryCatalogMutationError> {
        self.port
            .delete_library(library_id)
            .await
            .map_err(LibraryCatalogMutationError::persistence)
    }

    pub async fn scan_library(
        &self,
        library_id: &str,
        deep_scan: bool,
    ) -> Result<LibraryTaskResult, LibraryCatalogMutationError> {
        self.ensure_library_exists(library_id).await?;
        Ok(task_result(vec![manual_scan_library_task_record(
            library_id, deep_scan,
        )]))
    }

    pub async fn analyze_library(
        &self,
        library_id: &str,
    ) -> Result<LibraryTaskResult, LibraryCatalogMutationError> {
        let books = self
            .port
            .library_series_and_book_ids(library_id)
            .await
            .map_err(LibraryCatalogMutationError::persistence)?
            .unwrap_or_default()
            .1;
        Ok(task_result(analyze_library_task_records(books)))
    }

    pub async fn refresh_metadata(
        &self,
        library_id: &str,
    ) -> Result<LibraryTaskResult, LibraryCatalogMutationError> {
        let (series_ids, books) = self
            .port
            .library_series_and_book_ids(library_id)
            .await
            .map_err(LibraryCatalogMutationError::persistence)?
            .unwrap_or_default();
        Ok(task_result(metadata_refresh_task_records(
            series_ids, books,
        )))
    }

    pub async fn empty_trash(
        &self,
        library_id: &str,
    ) -> Result<LibraryTaskResult, LibraryCatalogMutationError> {
        self.ensure_library_exists(library_id).await?;
        Ok(task_result(empty_trash_task_records(library_id)))
    }

    async fn follow_up_tasks_for_library_update(
        &self,
        previous_library: &LibraryRecord,
        library: &LibraryRecord,
    ) -> Result<Vec<TaskQueueRecord>, LibraryCatalogMutationError> {
        let mut task_records = Vec::new();
        if library_should_rescan(previous_library, library) {
            task_records.push(background_scan_library_task_record(&library.id, false));
        }
        if library.hash_files && !previous_library.hash_files {
            let book_ids = self
                .port
                .library_book_ids_with_empty_hash(&library.id, false)
                .await
                .map_err(LibraryCatalogMutationError::persistence)?;
            task_records.extend(hash_book_task_records(book_ids, 0));
        }
        if library.hash_koreader && !previous_library.hash_koreader {
            let book_ids = self
                .port
                .library_book_ids_with_empty_hash(&library.id, true)
                .await
                .map_err(LibraryCatalogMutationError::persistence)?;
            task_records.extend(hash_book_koreader_task_records(book_ids, 0));
        }
        if library.hash_pages && !previous_library.hash_pages {
            task_records.extend(find_books_with_missing_page_hash_task_records(&library.id));
        }
        if library.repair_extensions && !previous_library.repair_extensions {
            let book_ids = self
                .port
                .library_books_with_mismatched_extensions(&library.id)
                .await
                .map_err(LibraryCatalogMutationError::persistence)?;
            task_records.extend(repair_extension_task_records(book_ids, 0));
        }
        if library.convert_to_cbz && !previous_library.convert_to_cbz {
            task_records.extend(find_books_to_convert_task_records(&library.id));
        }

        Ok(task_records)
    }

    async fn ensure_library_exists(
        &self,
        library_id: &str,
    ) -> Result<(), LibraryCatalogMutationError> {
        let library = self
            .port
            .load_library(library_id)
            .await
            .map_err(LibraryCatalogMutationError::persistence)?;
        if library.is_none() {
            return Err(LibraryCatalogMutationError::NotFound);
        }

        Ok(())
    }
}

fn task_result(task_records: Vec<TaskQueueRecord>) -> LibraryTaskResult {
    LibraryTaskResult { task_records }
}

fn ensure_name_and_root(library: &LibraryRecord) -> Result<(), LibraryCatalogMutationError> {
    if library.name.trim().is_empty() || library.root.trim().is_empty() {
        return Err(LibraryCatalogMutationError::Validation(
            "library create payload must provide non-empty name and root".to_string(),
        ));
    }

    Ok(())
}

fn generated_library_id() -> String {
    format!("library-{}", random_hex_token(12))
}

fn random_hex_token(byte_len: usize) -> String {
    let mut bytes = vec![0u8; byte_len];
    if let Ok(mut file) = std::fs::File::open("/dev/urandom") {
        let _ = file.read_exact(&mut bytes);
    } else {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = ((seed >> ((index % 8) * 8)) as u8) ^ (index as u8).wrapping_mul(17);
        }
    }

    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn hash_book_task_records(book_ids: Vec<String>, priority: i32) -> Vec<TaskQueueRecord> {
    emit_library_task_batch(LibraryTaskCommand::HashBooks { book_ids, priority })
        .into_queue_records()
}

fn hash_book_koreader_task_records(book_ids: Vec<String>, priority: i32) -> Vec<TaskQueueRecord> {
    emit_library_task_batch(LibraryTaskCommand::HashKoreaderBooks { book_ids, priority })
        .into_queue_records()
}

fn find_books_with_missing_page_hash_task_records(library_id: &str) -> Vec<TaskQueueRecord> {
    emit_library_task_batch(LibraryTaskCommand::FindBooksWithMissingPageHash {
        library_id: library_id.to_string(),
    })
    .into_queue_records()
}

fn repair_extension_task_records(
    books: Vec<(String, String)>,
    priority: i32,
) -> Vec<TaskQueueRecord> {
    emit_library_task_batch(LibraryTaskCommand::RepairExtensions {
        books: books.into_iter().map(BookSeriesRef::from).collect(),
        priority,
    })
    .into_queue_records()
}

fn find_books_to_convert_task_records(library_id: &str) -> Vec<TaskQueueRecord> {
    emit_library_task_batch(LibraryTaskCommand::FindBooksToConvert {
        library_id: library_id.to_string(),
    })
    .into_queue_records()
}
