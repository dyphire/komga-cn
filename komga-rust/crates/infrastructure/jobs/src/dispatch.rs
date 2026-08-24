use komga_application::task_processing::{
    RebuildIndexEntity, RuntimeTaskRequest, TaskExecutionOutcome, TaskProcessingError,
    TaskQueueRecord,
};
use komga_infrastructure_search::SearchEntityType;

use super::JobRuntime;

pub(super) struct TaskJobDispatcher<'a> {
    runtime: JobRuntime<'a>,
}

impl<'a> TaskJobDispatcher<'a> {
    pub(super) fn new(runtime: JobRuntime<'a>) -> Self {
        Self { runtime }
    }

    pub(super) async fn execute_record(
        &self,
        task: &TaskQueueRecord,
    ) -> Result<TaskExecutionOutcome, TaskProcessingError> {
        let request = RuntimeTaskRequest::from_queue_record(task)?;

        self.execute(request).await
    }

    pub(super) async fn execute(
        &self,
        request: RuntimeTaskRequest,
    ) -> Result<TaskExecutionOutcome, TaskProcessingError> {
        match request {
            RuntimeTaskRequest::ScanLibrary(request) => {
                super::jobs::execute_scan_library(&self.runtime, request).await
            }
            RuntimeTaskRequest::HashBookPages { book_id } => {
                super::jobs::execute_hash_book_pages(&self.runtime, &book_id).await
            }
            RuntimeTaskRequest::HashBook { book_id, koreader } => {
                super::jobs::execute_hash_book(&self.runtime, &book_id, koreader).await
            }
            RuntimeTaskRequest::FindBooksWithMissingPageHash {
                library_id,
                priority,
            } => {
                super::jobs::execute_find_books_with_missing_page_hash(
                    &self.runtime,
                    &library_id,
                    priority,
                )
                .await
            }
            RuntimeTaskRequest::FindDuplicatePagesToDelete {
                library_id,
                priority,
            } => {
                super::jobs::execute_find_duplicate_pages_to_delete(
                    &self.runtime,
                    &library_id,
                    priority,
                )
                .await
            }
            RuntimeTaskRequest::RemoveHashedPages {
                book_id,
                pages,
                priority,
            } => {
                super::jobs::execute_remove_hashed_pages(&self.runtime, &book_id, &pages, priority)
                    .await
            }
            RuntimeTaskRequest::AnalyzeBook { book_id, priority } => {
                super::jobs::execute_analyze_book(&self.runtime, &book_id, priority).await
            }
            RuntimeTaskRequest::RebuildIndex { entities } => {
                let entity_types = entities.map(|entities| {
                    entities
                        .into_iter()
                        .map(search_entity_type)
                        .collect::<Vec<_>>()
                });
                super::jobs::execute_rebuild_index(&self.runtime, entity_types.as_deref()).await
            }
            RuntimeTaskRequest::UpgradeIndex => Ok(TaskExecutionOutcome::completed()),
            RuntimeTaskRequest::FindBookThumbnailsToRegenerate {
                for_bigger_result_only,
                priority,
            } => {
                super::jobs::execute_find_book_thumbnails_to_regenerate(
                    &self.runtime,
                    for_bigger_result_only,
                    priority,
                )
                .await
            }
            RuntimeTaskRequest::RefreshBookMetadata {
                book_id,
                capabilities,
                priority,
            } => {
                super::jobs::execute_refresh_book_metadata(
                    &self.runtime,
                    &book_id,
                    &capabilities,
                    priority,
                )
                .await
            }
            RuntimeTaskRequest::RefreshSeriesMetadata {
                series_id,
                priority,
            } => {
                super::jobs::execute_refresh_series_metadata(&self.runtime, &series_id, priority)
                    .await
            }
            RuntimeTaskRequest::AggregateSeriesMetadata { series_id } => {
                super::jobs::execute_aggregate_series_metadata(&self.runtime, &series_id).await
            }
            RuntimeTaskRequest::RefreshBookLocalArtwork { book_id } => {
                super::jobs::execute_refresh_book_local_artwork(&self.runtime, &book_id).await
            }
            RuntimeTaskRequest::GenerateBookThumbnail { book_id } => {
                super::jobs::execute_generate_book_thumbnail(&self.runtime, &book_id).await
            }
            RuntimeTaskRequest::RefreshSeriesLocalArtwork { series_id } => {
                super::jobs::execute_refresh_series_local_artwork(&self.runtime, &series_id).await
            }
            RuntimeTaskRequest::EmptyTrash { library_id } => {
                super::jobs::execute_empty_trash(&self.runtime, &library_id).await
            }
            RuntimeTaskRequest::DeleteBook { book_id } => {
                super::jobs::execute_delete_book(&self.runtime, &book_id).await
            }
            RuntimeTaskRequest::DeleteSeries { series_id } => {
                super::jobs::execute_delete_series(&self.runtime, &series_id).await
            }
            RuntimeTaskRequest::RepairExtension { book_id } => {
                super::jobs::execute_repair_extension(&self.runtime, &book_id).await
            }
            RuntimeTaskRequest::FindBooksToConvert {
                library_id,
                priority,
            } => {
                super::jobs::execute_find_books_to_convert(&self.runtime, &library_id, priority)
                    .await
            }
            RuntimeTaskRequest::ConvertBook { book_id } => {
                super::jobs::execute_convert_book(&self.runtime, &book_id).await
            }
            RuntimeTaskRequest::ImportBook { payload, priority } => {
                super::jobs::execute_import_book(&self.runtime, payload, priority).await
            }
        }
    }
}

fn search_entity_type(entity: RebuildIndexEntity) -> SearchEntityType {
    match entity {
        RebuildIndexEntity::Book => SearchEntityType::Book,
        RebuildIndexEntity::Series => SearchEntityType::Series,
        RebuildIndexEntity::Collection => SearchEntityType::Collection,
        RebuildIndexEntity::ReadList => SearchEntityType::ReadList,
    }
}
