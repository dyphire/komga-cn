use super::task_records::{
    analyze_library_task_records, empty_trash_task_records, manual_scan_library_task_record,
    metadata_refresh_task_records,
};
use super::{LibraryCatalogMutationError, LibraryCatalogMutationPort, LibraryTaskResult};

fn task_result(task_records: Vec<crate::task_processing::TaskQueueRecord>) -> LibraryTaskResult {
    LibraryTaskResult { task_records }
}

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
        ensure_library_exists(&self.port, library_id).await?;
        Ok(task_result(empty_trash_task_records(library_id)))
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
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    use async_trait::async_trait;

    use super::*;
    use crate::library_catalog::LibraryRecord;

    type SeriesAndBookIds = (Vec<String>, Vec<(String, String)>);

    #[derive(Clone, Default)]
    struct TestPort {
        library: Option<LibraryRecord>,
        library_book_ids: Option<Vec<String>>,
        library_series_and_book_ids: Option<SeriesAndBookIds>,
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
            Ok(false)
        }

        async fn delete_library(&self, _library_id: &str) -> Result<bool, String> {
            Ok(false)
        }

        async fn library_book_ids_with_empty_hash(
            &self,
            _library_id: &str,
            _koreader: bool,
        ) -> Result<Vec<String>, String> {
            Ok(Vec::new())
        }

        async fn library_books_with_mismatched_extensions(
            &self,
            _library_id: &str,
        ) -> Result<Vec<(String, String)>, String> {
            Ok(Vec::new())
        }

        async fn library_book_ids(&self, _library_id: &str) -> Result<Option<Vec<String>>, String> {
            Ok(self.library_book_ids.clone())
        }

        async fn library_series_and_book_ids(
            &self,
            _library_id: &str,
        ) -> Result<Option<(Vec<String>, Vec<(String, String)>)>, String> {
            Ok(self.library_series_and_book_ids.clone())
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
    fn refresh_metadata_returns_empty_task_list_when_library_is_missing() {
        let service = LibraryTaskService::new(TestPort::default());

        let result = block_on(service.refresh_metadata("missing-library"))
            .expect("missing libraries should still return accepted empty metadata refresh tasks");

        assert!(result.task_records.is_empty());
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
