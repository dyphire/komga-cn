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
        ensure_library_exists(&self.port, library_id).await?;
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
            .ok_or(LibraryCatalogMutationError::NotFound)?;
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
            .ok_or(LibraryCatalogMutationError::NotFound)?;
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
