use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

use async_trait::async_trait;
use serde_json::Value;

use super::{
    LibraryBookSeriesRecord, LibraryCatalogCommandService, LibraryCatalogMutationPort,
    LibraryChangeSet, LibraryRecord, LibrarySeriesAndBookIds,
};

#[test]
fn update_command_emits_follow_up_tasks_from_library_feature_toggles() {
    let service = LibraryCatalogCommandService::new(TestPort {
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
    assert_eq!(result.task_records[0].id, "FindBooksToConvert_library-1");
    assert_eq!(result.task_records[0].simple_type, "FindBooksToConvert");
    assert_eq!(result.task_records[0].priority, 0);
    assert_eq!(result.task_records[0].group, None);
}

#[test]
fn update_command_enqueues_repair_extension_tasks_for_mismatched_books() {
    let service = LibraryCatalogCommandService::new(TestPort {
        library: Some(LibraryRecord::default_record("library-1".to_string())),
        mismatched_extension_books: vec![LibraryBookSeriesRecord {
            book_id: "book-1".to_string(),
            series_id: "series-1".to_string(),
        }],
        ..TestPort::default()
    });

    let result = block_on(service.update_library(
        "library-1",
        LibraryChangeSet {
            repair_extensions: Some(true),
            ..LibraryChangeSet::default()
        },
    ))
    .expect("enabling repair extensions should enqueue mismatched book repair tasks");

    assert_eq!(result.task_records.len(), 1);
    let task = &result.task_records[0];
    assert_eq!(task.id, "RepairExtension_book-1");
    assert_eq!(task.simple_type, "RepairExtension");
    assert_eq!(task.priority, 0);
    assert_eq!(task.group.as_deref(), Some("series-1"));
    let payload: Value = serde_json::from_str(
        task.payload
            .as_deref()
            .expect("repair extension task should include book payload"),
    )
    .expect("repair extension task payload should be JSON");
    assert_eq!(
        payload.get("bookId").and_then(Value::as_str),
        Some("book-1")
    );
}

#[test]
fn analyze_command_returns_empty_task_list_when_library_is_missing() {
    let service = LibraryCatalogCommandService::new(TestPort::default());

    let result = block_on(service.analyze_library("missing-library"))
        .expect("missing libraries should still yield an accepted empty task batch");

    assert!(result.task_records.is_empty());
}

#[test]
fn scan_command_returns_not_found_when_library_is_missing() {
    let service = LibraryCatalogCommandService::new(TestPort::default());

    let error = block_on(service.scan_library("missing-library", true))
        .expect_err("missing libraries should reject scan requests");

    assert!(matches!(
        error,
        super::LibraryCatalogMutationError::NotFound
    ));
}

#[test]
fn refresh_metadata_command_returns_empty_task_list_when_library_is_missing() {
    let service = LibraryCatalogCommandService::new(TestPort::default());

    let result = block_on(service.refresh_metadata("missing-library"))
        .expect("missing libraries should still return accepted empty metadata refresh tasks");

    assert!(result.task_records.is_empty());
}

#[derive(Clone, Default)]
struct TestPort {
    library: Option<LibraryRecord>,
    empty_hash_book_ids: Vec<String>,
    empty_hash_koreader_book_ids: Vec<String>,
    mismatched_extension_books: Vec<LibraryBookSeriesRecord>,
    library_book_ids: Option<Vec<String>>,
    library_series_and_book_ids: Option<LibrarySeriesAndBookIds>,
}

#[async_trait]
impl LibraryCatalogMutationPort for TestPort {
    async fn load_library(&self, _library_id: &str) -> Result<Option<LibraryRecord>, String> {
        Ok(self.library.clone())
    }

    async fn validate_library(&self, _library: &LibraryRecord) -> Result<(), String> {
        Ok(())
    }

    async fn create_library(&self, _library: &LibraryRecord) -> Result<(), String> {
        Ok(())
    }

    async fn update_library(&self, _library: &LibraryRecord) -> Result<bool, String> {
        Ok(true)
    }

    async fn delete_library(&self, _library_id: &str) -> Result<bool, String> {
        Ok(false)
    }

    async fn library_book_ids_with_empty_hash(
        &self,
        _library_id: &str,
        koreader: bool,
    ) -> Result<Vec<String>, String> {
        Ok(if koreader {
            self.empty_hash_koreader_book_ids.clone()
        } else {
            self.empty_hash_book_ids.clone()
        })
    }

    async fn library_books_with_mismatched_extensions(
        &self,
        _library_id: &str,
    ) -> Result<Vec<LibraryBookSeriesRecord>, String> {
        Ok(self.mismatched_extension_books.clone())
    }

    async fn library_book_ids(&self, _library_id: &str) -> Result<Option<Vec<String>>, String> {
        Ok(self.library_book_ids.clone())
    }

    async fn library_series_and_book_ids(
        &self,
        _library_id: &str,
    ) -> Result<Option<LibrarySeriesAndBookIds>, String> {
        Ok(self.library_series_and_book_ids.clone())
    }
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
    RawWaker::new(std::ptr::null(), &NOOP_WAKER_VTABLE)
}

unsafe fn noop_clone(_data: *const ()) -> RawWaker {
    unsafe { noop_raw_waker() }
}

unsafe fn noop_wake(_data: *const ()) {}

static NOOP_WAKER_VTABLE: RawWakerVTable =
    RawWakerVTable::new(noop_clone, noop_wake, noop_wake, noop_wake);
