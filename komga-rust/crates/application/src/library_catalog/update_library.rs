use super::task_records::{library_should_rescan, scan_library_task_record};
use super::{
    LibraryCatalogMutationError, LibraryCatalogMutationPort, LibraryChangeSet, LibraryTaskResult,
};
use crate::task_processing::TaskQueueRecord;

pub struct UpdateLibraryService<P> {
    port: P,
}

impl<P> UpdateLibraryService<P>
where
    P: LibraryCatalogMutationPort,
{
    pub fn new(port: P) -> Self {
        Self { port }
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

        let mut task_records = Vec::new();
        if library_should_rescan(&previous_library, &library) {
            task_records.push(scan_library_task_record(&library.id, false));
        }
        if library.hash_files && !previous_library.hash_files {
            let book_ids = self
                .port
                .library_book_ids_with_empty_hash(&library.id, false)
                .await
                .map_err(LibraryCatalogMutationError::persistence)?;
            task_records.extend(book_ids.into_iter().map(|book_id| {
                TaskQueueRecord::new(format!("HASH_BOOK:{book_id}"), 10, Some(book_id))
            }));
        }
        if library.hash_koreader && !previous_library.hash_koreader {
            let book_ids = self
                .port
                .library_book_ids_with_empty_hash(&library.id, true)
                .await
                .map_err(LibraryCatalogMutationError::persistence)?;
            task_records.extend(book_ids.into_iter().map(|book_id| {
                TaskQueueRecord::new(format!("HASH_BOOK_KOREADER:{book_id}"), 10, Some(book_id))
            }));
        }
        if library.hash_pages && !previous_library.hash_pages {
            task_records.push(TaskQueueRecord::new(
                format!("FIND_BOOKS_WITH_MISSING_PAGE_HASH:{}", library.id),
                10,
                Some(library.id.clone()),
            ));
        }
        if library.repair_extensions && !previous_library.repair_extensions {
            task_records.push(TaskQueueRecord::new(
                format!("REPAIR_EXTENSIONS:{}", library.id),
                10,
                Some(library.id.clone()),
            ));
        }
        if library.convert_to_cbz && !previous_library.convert_to_cbz {
            task_records.push(TaskQueueRecord::new(
                format!("FIND_BOOKS_TO_CONVERT:{}", library.id),
                10,
                Some(library.id.clone()),
            ));
        }

        Ok(LibraryTaskResult { task_records })
    }
}
