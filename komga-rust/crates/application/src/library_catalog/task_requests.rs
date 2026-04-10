use super::task_records::{
    analyze_library_task_records, empty_trash_task_records, manual_scan_library_task_record,
    metadata_refresh_task_records,
};
use super::{LibraryCatalogMutationError, LibraryCatalogMutationPort, LibraryTaskResult};

pub struct LibraryTaskService<P> {
    port: P,
}

impl<P> LibraryTaskService<P>
where
    P: LibraryCatalogMutationPort,
{
    pub fn new(port: P) -> Self {
        Self { port }
    }

    pub async fn scan_library(
        &self,
        library_id: &str,
        deep_scan: bool,
    ) -> Result<LibraryTaskResult, LibraryCatalogMutationError> {
        ensure_library_exists(&self.port, library_id).await?;
        Ok(LibraryTaskResult {
            task_records: vec![manual_scan_library_task_record(library_id, deep_scan)],
        })
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
        Ok(LibraryTaskResult {
            task_records: analyze_library_task_records(books),
        })
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
        Ok(LibraryTaskResult {
            task_records: metadata_refresh_task_records(series_ids, books),
        })
    }

    pub async fn empty_trash(
        &self,
        library_id: &str,
    ) -> Result<LibraryTaskResult, LibraryCatalogMutationError> {
        ensure_library_exists(&self.port, library_id).await?;
        Ok(LibraryTaskResult {
            task_records: empty_trash_task_records(library_id),
        })
    }
}

async fn ensure_library_exists<P>(
    port: &P,
    library_id: &str,
) -> Result<(), LibraryCatalogMutationError>
where
    P: LibraryCatalogMutationPort,
{
    let library = port
        .load_library(library_id)
        .await
        .map_err(LibraryCatalogMutationError::persistence)?;
    if library.is_none() {
        return Err(LibraryCatalogMutationError::NotFound);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::future::{Future, ready};
    use std::pin::Pin;
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    use super::*;
    use crate::library_catalog::LibraryRecord;

    type SeriesAndBookIds = (Vec<String>, Vec<(String, String)>);

    #[derive(Clone, Default)]
    struct TestPort {
        library: Option<LibraryRecord>,
        library_book_ids: Option<Vec<String>>,
        library_series_and_book_ids: Option<SeriesAndBookIds>,
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
            ready(Ok(false))
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
            _koreader: bool,
        ) -> impl std::future::Future<Output = Result<Vec<String>, String>> {
            ready(Ok(Vec::new()))
        }

        fn library_books_with_mismatched_extensions(
            &self,
            _library_id: &str,
        ) -> impl std::future::Future<Output = Result<Vec<(String, String)>, String>> {
            ready(Ok(Vec::new()))
        }

        fn library_book_ids(
            &self,
            _library_id: &str,
        ) -> impl std::future::Future<Output = Result<Option<Vec<String>>, String>> {
            ready(Ok(self.library_book_ids.clone()))
        }

        fn library_series_and_book_ids(
            &self,
            _library_id: &str,
        ) -> impl std::future::Future<
            Output = Result<Option<(Vec<String>, Vec<(String, String)>)>, String>,
        > {
            ready(Ok(self.library_series_and_book_ids.clone()))
        }
    }

    #[test]
    fn analyze_library_returns_empty_task_list_when_library_is_missing() {
        let service = LibraryTaskService::new(TestPort::default());

        let result = block_on(service.analyze_library("missing-library"))
            .expect("missing libraries should still yield an accepted empty task batch");

        assert!(result.task_records.is_empty());
    }

    #[test]
    fn scan_library_returns_not_found_when_library_is_missing() {
        let service = LibraryTaskService::new(TestPort::default());

        let error = block_on(service.scan_library("missing-library", true))
            .expect_err("missing libraries should reject scan requests");

        assert!(matches!(error, LibraryCatalogMutationError::NotFound));
    }

    #[test]
    fn scan_library_enqueues_kotlin_style_deep_scan_task_for_existing_library() {
        let service = LibraryTaskService::new(TestPort {
            library: Some(LibraryRecord::default_record("library-1".to_string())),
            ..TestPort::default()
        });

        let result = block_on(service.scan_library("library-1", true))
            .expect("existing libraries should enqueue scan tasks");

        assert_eq!(result.task_records.len(), 1);
        assert_eq!(
            result.task_records[0].id,
            "SCAN_LIBRARY:library-1:DEEP:true"
        );
        assert_eq!(result.task_records[0].simple_type, "SCAN_LIBRARY");
        assert_eq!(result.task_records[0].priority, 100);
        assert_eq!(result.task_records[0].group, None);
        assert_eq!(
            result.task_records[0]
                .payload
                .as_deref()
                .and_then(|payload| serde_json::from_str::<serde_json::Value>(payload).ok()),
            Some(serde_json::json!({
                "libraryId": "library-1",
                "scanDeep": true,
                "priority": 100,
                "groupId": serde_json::Value::Null,
                "uniqueId": "SCAN_LIBRARY:library-1:DEEP:true"
            }))
        );
    }

    #[test]
    fn analyze_library_groups_tasks_by_series_id() {
        let service = LibraryTaskService::new(TestPort {
            library_series_and_book_ids: Some((
                vec!["series-1".to_string()],
                vec![("book-1".to_string(), "series-1".to_string())],
            )),
            ..TestPort::default()
        });

        let result =
            block_on(service.analyze_library("library-1")).expect("analyze library should work");

        assert_eq!(result.task_records.len(), 1);
        assert_eq!(result.task_records[0].id, "ANALYZE_BOOK:book-1");
        assert_eq!(result.task_records[0].group.as_deref(), Some("series-1"));
    }

    #[test]
    fn refresh_metadata_returns_empty_task_list_when_library_is_missing() {
        let service = LibraryTaskService::new(TestPort::default());

        let result = block_on(service.refresh_metadata("missing-library"))
            .expect("missing libraries should still return accepted empty metadata refresh tasks");

        assert!(result.task_records.is_empty());
    }

    #[test]
    fn refresh_metadata_emits_book_metadata_tasks_grouped_by_series_id() {
        let service = LibraryTaskService::new(TestPort {
            library_series_and_book_ids: Some((
                vec!["series-1".to_string()],
                vec![("book-1".to_string(), "series-1".to_string())],
            )),
            ..TestPort::default()
        });

        let result = block_on(service.refresh_metadata("library-1"))
            .expect("existing metadata refresh inputs should enqueue tasks");

        let metadata = result
            .task_records
            .iter()
            .find(|task| task.simple_type == "REFRESH_BOOK_METADATA")
            .expect("book metadata refresh task should be present");

        assert_eq!(metadata.id, "REFRESH_BOOK_METADATA_book-1");
        assert_eq!(metadata.group.as_deref(), Some("series-1"));
    }

    #[test]
    fn empty_trash_enqueues_only_empty_trash_task_for_existing_library() {
        let service = LibraryTaskService::new(TestPort {
            library: Some(LibraryRecord::default_record("library-1".to_string())),
            ..TestPort::default()
        });

        let result = block_on(service.empty_trash("library-1"))
            .expect("existing libraries should enqueue empty-trash tasks");

        assert_eq!(result.task_records.len(), 1);
        assert_eq!(result.task_records[0].id, "EMPTY_TRASH:library-1");
        assert_eq!(result.task_records[0].simple_type, "EMPTY_TRASH");
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
        RawWaker::new(std::ptr::null(), &NOOP_WAKER_VTABLE)
    }

    unsafe fn noop_clone(_data: *const ()) -> RawWaker {
        unsafe { noop_raw_waker() }
    }

    unsafe fn noop_wake(_data: *const ()) {}

    static NOOP_WAKER_VTABLE: RawWakerVTable =
        RawWakerVTable::new(noop_clone, noop_wake, noop_wake, noop_wake);
}
