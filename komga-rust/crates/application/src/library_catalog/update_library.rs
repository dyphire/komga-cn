use super::task_records::{library_should_rescan, scan_library_task_record};
use super::{
    LibraryCatalogMutationError, LibraryCatalogMutationPort, LibraryChangeSet, LibraryTaskResult,
};
use crate::task_processing::TaskQueueRecord;
use serde_json::json;

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
            task_records.extend(
                book_ids
                    .into_iter()
                    .map(|book_id| hash_book_task_record(&book_id, 0)),
            );
        }
        if library.hash_koreader && !previous_library.hash_koreader {
            let book_ids = self
                .port
                .library_book_ids_with_empty_hash(&library.id, true)
                .await
                .map_err(LibraryCatalogMutationError::persistence)?;
            task_records.extend(
                book_ids
                    .into_iter()
                    .map(|book_id| hash_book_koreader_task_record(&book_id, 0)),
            );
        }
        if library.hash_pages && !previous_library.hash_pages {
            task_records.push(find_books_with_missing_page_hash_task_record(
                &library.id,
                10,
            ));
        }
        if library.repair_extensions && !previous_library.repair_extensions {
            let book_ids = self
                .port
                .library_books_with_mismatched_extensions(&library.id)
                .await
                .map_err(LibraryCatalogMutationError::persistence)?;
            task_records.extend(book_ids.into_iter().map(|(book_id, series_id)| {
                repair_extension_task_record(&book_id, &series_id, 10)
            }));
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

fn hash_book_task_record(book_id: &str, priority: i32) -> TaskQueueRecord {
    let task_id = format!("HASH_BOOK_{book_id}");
    TaskQueueRecord::new(task_id.clone(), priority, None)
        .with_simple_type("HASH_BOOK")
        .with_payload(
            json!({
                "bookId": book_id,
                "priority": priority,
                "groupId": serde_json::Value::Null,
                "uniqueId": task_id,
            })
            .to_string(),
        )
}

fn hash_book_koreader_task_record(book_id: &str, priority: i32) -> TaskQueueRecord {
    let task_id = format!("HASH_BOOK_KOREADER_{book_id}");
    TaskQueueRecord::new(task_id.clone(), priority, None)
        .with_simple_type("HASH_BOOK_KOREADER")
        .with_payload(
            json!({
                "bookId": book_id,
                "priority": priority,
                "groupId": serde_json::Value::Null,
                "uniqueId": task_id,
            })
            .to_string(),
        )
}

fn find_books_with_missing_page_hash_task_record(
    library_id: &str,
    _priority: i32,
) -> TaskQueueRecord {
    let task_id = format!("FIND_BOOKS_WITH_MISSING_PAGE_HASH_{library_id}");
    TaskQueueRecord::new(task_id.clone(), 0, None)
        .with_simple_type("FIND_BOOKS_WITH_MISSING_PAGE_HASH")
        .with_payload(
            json!({
                "libraryId": library_id,
                "priority": 0,
                "groupId": serde_json::Value::Null,
                "uniqueId": task_id,
            })
            .to_string(),
        )
}

fn repair_extension_task_record(book_id: &str, series_id: &str, priority: i32) -> TaskQueueRecord {
    let task_id = format!("REPAIR_EXTENSION_{book_id}");
    TaskQueueRecord::new(task_id.clone(), priority, Some(series_id.to_string()))
        .with_simple_type("REPAIR_EXTENSION")
        .with_payload(
            json!({
                "bookId": book_id,
                "priority": priority,
                "groupId": series_id,
                "uniqueId": task_id,
            })
            .to_string(),
        )
}

#[cfg(test)]
mod tests {
    use std::future::{Future, ready};
    use std::pin::Pin;
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    use super::*;
    use crate::library_catalog::LibraryRecord;
    use serde_json::json;

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
    fn enabling_hash_files_emits_kotlin_style_hash_book_task() {
        let service = UpdateLibraryService::new(TestPort {
            library: Some(LibraryRecord {
                hash_files: false,
                ..LibraryRecord::default_record("library-1".to_string())
            }),
            empty_hash_book_ids: vec!["book-1".to_string()],
            ..TestPort::default()
        });

        let result = block_on(service.update_library(
            "library-1",
            LibraryChangeSet {
                hash_files: Some(true),
                ..LibraryChangeSet::default()
            },
        ))
        .expect("enabling hash-files should succeed");

        assert_eq!(result.task_records.len(), 1);
        assert_eq!(result.task_records[0].id, "HASH_BOOK_book-1");
        assert_eq!(result.task_records[0].simple_type, "HASH_BOOK");
        assert_eq!(result.task_records[0].priority, 0);
        assert_eq!(result.task_records[0].group, None);
        assert_eq!(
            result.task_records[0]
                .payload
                .as_deref()
                .and_then(|payload| serde_json::from_str::<serde_json::Value>(payload).ok()),
            Some(json!({
                "bookId": "book-1",
                "priority": 0,
                "groupId": serde_json::Value::Null,
                "uniqueId": "HASH_BOOK_book-1"
            }))
        );
    }

    #[test]
    fn enabling_hash_koreader_emits_kotlin_style_hash_book_koreader_task() {
        let service = UpdateLibraryService::new(TestPort {
            library: Some(LibraryRecord::default_record("library-1".to_string())),
            empty_hash_koreader_book_ids: vec!["book-1".to_string()],
            ..TestPort::default()
        });

        let result = block_on(service.update_library(
            "library-1",
            LibraryChangeSet {
                hash_koreader: Some(true),
                ..LibraryChangeSet::default()
            },
        ))
        .expect("enabling hash-koreader should succeed");

        assert_eq!(result.task_records.len(), 1);
        assert_eq!(result.task_records[0].id, "HASH_BOOK_KOREADER_book-1");
        assert_eq!(result.task_records[0].simple_type, "HASH_BOOK_KOREADER");
        assert_eq!(result.task_records[0].priority, 0);
        assert_eq!(result.task_records[0].group, None);
        assert_eq!(
            result.task_records[0]
                .payload
                .as_deref()
                .and_then(|payload| serde_json::from_str::<serde_json::Value>(payload).ok()),
            Some(json!({
                "bookId": "book-1",
                "priority": 0,
                "groupId": serde_json::Value::Null,
                "uniqueId": "HASH_BOOK_KOREADER_book-1"
            }))
        );
    }

    #[test]
    fn enabling_hash_pages_emits_kotlin_style_find_books_with_missing_page_hash_task() {
        let service = UpdateLibraryService::new(TestPort {
            library: Some(LibraryRecord::default_record("library-1".to_string())),
            ..TestPort::default()
        });

        let result = block_on(service.update_library(
            "library-1",
            LibraryChangeSet {
                hash_pages: Some(true),
                ..LibraryChangeSet::default()
            },
        ))
        .expect("enabling hash-pages should succeed");

        assert_eq!(result.task_records.len(), 1);
        assert_eq!(
            result.task_records[0].id,
            "FIND_BOOKS_WITH_MISSING_PAGE_HASH_library-1"
        );
        assert_eq!(
            result.task_records[0].simple_type,
            "FIND_BOOKS_WITH_MISSING_PAGE_HASH"
        );
        assert_eq!(result.task_records[0].group, None);
        assert_eq!(
            result.task_records[0]
                .payload
                .as_deref()
                .and_then(|payload| serde_json::from_str::<serde_json::Value>(payload).ok()),
            Some(json!({
                "libraryId": "library-1",
                "priority": 0,
                "groupId": serde_json::Value::Null,
                "uniqueId": "FIND_BOOKS_WITH_MISSING_PAGE_HASH_library-1"
            }))
        );
    }

    #[test]
    fn enabling_repair_extensions_emits_kotlin_style_repair_extension_tasks_grouped_by_series() {
        let service = UpdateLibraryService::new(TestPort {
            library: Some(LibraryRecord::default_record("library-1".to_string())),
            mismatched_extension_books: vec![("book-1".to_string(), "series-1".to_string())],
            ..TestPort::default()
        });

        let result = block_on(service.update_library(
            "library-1",
            LibraryChangeSet {
                repair_extensions: Some(true),
                ..LibraryChangeSet::default()
            },
        ))
        .expect("enabling repair-extensions should succeed");

        assert_eq!(result.task_records.len(), 1);
        assert_eq!(result.task_records[0].id, "REPAIR_EXTENSION_book-1");
        assert_eq!(result.task_records[0].simple_type, "REPAIR_EXTENSION");
        assert_eq!(result.task_records[0].priority, 10);
        assert_eq!(result.task_records[0].group.as_deref(), Some("series-1"));
        assert_eq!(
            result.task_records[0]
                .payload
                .as_deref()
                .and_then(|payload| serde_json::from_str::<serde_json::Value>(payload).ok()),
            Some(json!({
                "bookId": "book-1",
                "priority": 10,
                "groupId": "series-1",
                "uniqueId": "REPAIR_EXTENSION_book-1"
            }))
        );
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
