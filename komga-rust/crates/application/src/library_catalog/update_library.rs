use super::task_records::{background_scan_library_task_record, library_should_rescan};
use super::{
    LibraryCatalogMutationError, LibraryCatalogMutationPort, LibraryChangeSet, LibraryTaskResult,
};
use crate::task_processing::{
    BookSeriesRef, DefaultLibraryTaskEmitter, DefaultTaskProtocolCatalog, LibraryTaskCommand,
    LibraryTaskEmitter, TaskQueueRecord,
};

fn library_task_emitter() -> DefaultLibraryTaskEmitter<DefaultTaskProtocolCatalog> {
    DefaultLibraryTaskEmitter::default()
}

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

        Ok(LibraryTaskResult { task_records })
    }
}

fn hash_book_task_records(book_ids: Vec<String>, priority: i32) -> Vec<TaskQueueRecord> {
    library_task_emitter()
        .emit(LibraryTaskCommand::HashBooks { book_ids, priority })
        .into_queue_records()
}

fn hash_book_koreader_task_records(book_ids: Vec<String>, priority: i32) -> Vec<TaskQueueRecord> {
    library_task_emitter()
        .emit(LibraryTaskCommand::HashKoreaderBooks { book_ids, priority })
        .into_queue_records()
}

fn find_books_with_missing_page_hash_task_records(library_id: &str) -> Vec<TaskQueueRecord> {
    library_task_emitter()
        .emit(LibraryTaskCommand::FindBooksWithMissingPageHash {
            library_id: library_id.to_string(),
        })
        .into_queue_records()
}

fn repair_extension_task_records(
    books: Vec<(String, String)>,
    priority: i32,
) -> Vec<TaskQueueRecord> {
    library_task_emitter()
        .emit(LibraryTaskCommand::RepairExtensions {
            books: books.into_iter().map(BookSeriesRef::from).collect(),
            priority,
        })
        .into_queue_records()
}

fn find_books_to_convert_task_records(library_id: &str) -> Vec<TaskQueueRecord> {
    library_task_emitter()
        .emit(LibraryTaskCommand::FindBooksToConvert {
            library_id: library_id.to_string(),
        })
        .into_queue_records()
}

#[cfg(test)]
mod tests {
    use std::future::{Future, ready};
    use std::pin::Pin;
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    use super::*;
    use crate::library_catalog::LibraryRecord;
    #[derive(Clone, Default)]
    struct TestPort {
        library: Option<LibraryRecord>,
        empty_hash_book_ids: Vec<String>,
        empty_hash_koreader_book_ids: Vec<String>,
        mismatched_extension_books: Vec<(String, String)>,
    }

    impl LibraryCatalogMutationPort for TestPort {
        fn load_library(
            &self,
            _library_id: &str,
        ) -> impl std::future::Future<Output = Result<Option<LibraryRecord>, String>> {
            ready(Ok(self.library.clone()))
        }

        fn validate_library(
            &self,
            _library: &LibraryRecord,
        ) -> impl std::future::Future<Output = Result<(), String>> {
            ready(Ok(()))
        }

        fn create_library(
            &self,
            _library: &LibraryRecord,
        ) -> impl std::future::Future<Output = Result<(), String>> {
            ready(Ok(()))
        }

        fn update_library(
            &self,
            _library: &LibraryRecord,
        ) -> impl std::future::Future<Output = Result<bool, String>> {
            ready(Ok(true))
        }

        fn delete_library(
            &self,
            _library_id: &str,
        ) -> impl std::future::Future<Output = Result<bool, String>> {
            ready(Ok(false))
        }

        fn library_book_ids_with_empty_hash(
            &self,
            _library_id: &str,
            koreader: bool,
        ) -> impl std::future::Future<Output = Result<Vec<String>, String>> {
            ready(Ok(if koreader {
                self.empty_hash_koreader_book_ids.clone()
            } else {
                self.empty_hash_book_ids.clone()
            }))
        }

        fn library_books_with_mismatched_extensions(
            &self,
            _library_id: &str,
        ) -> impl std::future::Future<Output = Result<Vec<(String, String)>, String>> {
            ready(Ok(self.mismatched_extension_books.clone()))
        }

        fn library_book_ids(
            &self,
            _library_id: &str,
        ) -> impl std::future::Future<Output = Result<Option<Vec<String>>, String>> {
            ready(Ok(None))
        }

        fn library_series_and_book_ids(
            &self,
            _library_id: &str,
        ) -> impl std::future::Future<
            Output = Result<Option<(Vec<String>, Vec<(String, String)>)>, String>,
        > {
            ready(Ok(None))
        }
    }

    #[test]
    fn enabling_convert_to_cbz_emits_ungrouped_lowest_priority_find_books_to_convert_task() {
        let service = UpdateLibraryService::new(TestPort {
            library: Some(LibraryRecord::default_record("library-1".to_string())),
            ..TestPort::default()
        });

        let result = block_on(service.update_library(
            "library-1",
            LibraryChangeSet {
                convert_to_cbz: Some(true),
                ..LibraryChangeSet::default()
            },
        ))
        .expect("enabling convert-to-cbz should succeed");

        assert_eq!(result.task_records.len(), 1);
        assert_eq!(result.task_records[0].id, "FIND_BOOKS_TO_CONVERT_library-1");
        assert_eq!(result.task_records[0].simple_type, "FIND_BOOKS_TO_CONVERT");
        assert_eq!(result.task_records[0].priority, 0);
        assert_eq!(result.task_records[0].group, None);
    }

    fn block_on<F: Future>(future: F) -> F::Output {
        let waker = unsafe { Waker::from_raw(noop_raw_waker()) };
        let mut future = Pin::from(Box::new(future));
        let mut context = Context::from_waker(&waker);

        loop {
            match future.as_mut().poll(&mut context) {
                Poll::Ready(output) => return output,
                Poll::Pending => std::thread::yield_now(),
            }
        }
    }

    unsafe fn noop_raw_waker() -> RawWaker {
        fn clone(_: *const ()) -> RawWaker {
            unsafe { noop_raw_waker() }
        }
        fn wake(_: *const ()) {}
        fn wake_by_ref(_: *const ()) {}
        fn drop(_: *const ()) {}

        RawWaker::new(
            std::ptr::null(),
            &RawWakerVTable::new(clone, wake, wake_by_ref, drop),
        )
    }
}
