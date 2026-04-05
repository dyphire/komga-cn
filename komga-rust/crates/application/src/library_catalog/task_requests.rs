use super::task_records::{
    analyze_library_task_records, empty_trash_task_records, metadata_refresh_task_records,
    scan_library_task_record,
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
        Ok(LibraryTaskResult {
            task_records: vec![scan_library_task_record(library_id, deep_scan)],
        })
    }

    pub async fn analyze_library(
        &self,
        library_id: &str,
    ) -> Result<LibraryTaskResult, LibraryCatalogMutationError> {
        let book_ids = self
            .port
            .library_book_ids(library_id)
            .await
            .map_err(LibraryCatalogMutationError::persistence)?
            .unwrap_or_default();
        Ok(LibraryTaskResult {
            task_records: analyze_library_task_records(book_ids),
        })
    }

    pub async fn refresh_metadata(
        &self,
        library_id: &str,
    ) -> Result<LibraryTaskResult, LibraryCatalogMutationError> {
        let (series_ids, book_ids) = self
            .port
            .library_series_and_book_ids(library_id)
            .await
            .map_err(LibraryCatalogMutationError::persistence)?
            .unwrap_or_default();
        Ok(LibraryTaskResult {
            task_records: metadata_refresh_task_records(series_ids, book_ids),
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

    #[derive(Clone, Default)]
    struct TestPort {
        library: Option<LibraryRecord>,
        library_book_ids: Option<Vec<String>>,
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

        fn library_book_ids(
            &self,
            _library_id: &str,
        ) -> impl std::future::Future<Output = Result<Option<Vec<String>>, String>> {
            ready(Ok(self.library_book_ids.clone()))
        }

        fn library_series_and_book_ids(
            &self,
            _library_id: &str,
        ) -> impl std::future::Future<Output = Result<Option<(Vec<String>, Vec<String>)>, String>>
        {
            ready(Ok(None))
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
    fn scan_library_enqueues_task_even_when_library_is_missing() {
        let service = LibraryTaskService::new(TestPort::default());

        let result = block_on(service.scan_library("missing-library", true))
            .expect("missing libraries should still enqueue scan tasks");

        assert_eq!(result.task_records.len(), 1);
        assert_eq!(result.task_records[0].id, "SCAN_LIBRARY:missing-library");
        assert_eq!(result.task_records[0].simple_type, "SCAN_LIBRARY");
        assert_eq!(
            result.task_records[0].payload.as_deref(),
            Some(r#"{"deep":true}"#)
        );
    }

    #[test]
    fn refresh_metadata_returns_empty_task_list_when_library_is_missing() {
        let service = LibraryTaskService::new(TestPort::default());

        let result = block_on(service.refresh_metadata("missing-library"))
            .expect("missing libraries should still return accepted empty metadata refresh tasks");

        assert!(result.task_records.is_empty());
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
