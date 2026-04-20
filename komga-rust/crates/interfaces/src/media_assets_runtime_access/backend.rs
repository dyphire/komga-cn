#![allow(clippy::type_complexity)]

use komga_application::media_assets::{BookMetadataPatch, BooksImportPayload};
use komga_application::task_processing::TaskQueueRecord;

pub trait RuntimeMediaImportService: Send + Sync {
    fn enqueue_books(
        &self,
        payload: BooksImportPayload,
        next_task_id: &mut dyn FnMut() -> String,
    ) -> Result<Vec<TaskQueueRecord>, String>;

    fn process_queued_book_payload<'a>(
        &'a self,
        task_payload: &'a str,
        import_priority: i32,
    ) -> futures_util::future::BoxFuture<'a, Result<Vec<TaskQueueRecord>, String>>;
}

pub trait RuntimeBookMetadataService: Send + Sync {
    fn update_book_metadata<'a>(
        &'a self,
        book_id: &'a str,
        patch: &'a BookMetadataPatch,
    ) -> futures_util::future::BoxFuture<'a, Result<Option<Option<String>>, String>>;

    fn batch_update_book_metadata<'a>(
        &'a self,
        updates: Vec<(String, BookMetadataPatch)>,
    ) -> futures_util::future::BoxFuture<'a, Result<Vec<String>, String>>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedMediaFileRecord {
    pub file_name: String,
    pub media_type: String,
    pub sub_type: Option<String>,
}
