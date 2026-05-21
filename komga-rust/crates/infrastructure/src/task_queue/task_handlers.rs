use komga_application::task_processing::{
    TaskExecutionOutcome, TaskKind, TaskProcessingError, TaskQueueRecord,
};

use super::JobRuntime;

pub(crate) async fn execute(
    runtime: &JobRuntime<'_>,
    task: &TaskQueueRecord,
    task_target: Option<&str>,
    kind: TaskKind,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    match kind {
        TaskKind::ScanLibrary => {
            super::scanner_jobs::execute_scan_library(runtime, task, task_target).await
        }
        TaskKind::HashBookPages => {
            super::scanner_jobs::execute_hash_book_pages(runtime, task_target).await
        }
        TaskKind::HashBook => {
            super::scanner_jobs::execute_hash_book(runtime, task_target, false).await
        }
        TaskKind::HashBookKoreader => {
            super::scanner_jobs::execute_hash_book(runtime, task_target, true).await
        }
        TaskKind::FindBooksWithMissingPageHash => {
            super::scanner_jobs::execute_find_books_with_missing_page_hash(
                runtime,
                task,
                task_target,
            )
            .await
        }
        TaskKind::FindDuplicatePagesToDelete => {
            super::scanner_jobs::execute_find_duplicate_pages_to_delete(runtime, task, task_target)
                .await
        }
        TaskKind::RemoveHashedPages => {
            super::scanner_jobs::execute_remove_hashed_pages(runtime, task, task_target).await
        }
        TaskKind::AnalyzeBook => {
            super::index_jobs::execute_analyze_book(runtime, task, task_target).await
        }
        TaskKind::RebuildIndex => super::index_jobs::execute_rebuild_index(runtime, task).await,
        TaskKind::UpgradeIndex => Ok(TaskExecutionOutcome::completed()),
        TaskKind::FindBookThumbnailsToRegenerate => {
            super::index_jobs::execute_find_book_thumbnails_to_regenerate(runtime, task).await
        }
        TaskKind::RefreshBookMetadata => {
            super::maintenance_jobs::execute_refresh_book_metadata(runtime, task, task_target).await
        }
        TaskKind::RefreshSeriesMetadata => {
            super::maintenance_jobs::execute_refresh_series_metadata(runtime, task, task_target)
                .await
        }
        TaskKind::AggregateSeriesMetadata => {
            super::maintenance_jobs::execute_aggregate_series_metadata(runtime, task_target).await
        }
        TaskKind::RefreshBookLocalArtwork => {
            super::maintenance_jobs::execute_refresh_book_local_artwork(runtime, task_target).await
        }
        TaskKind::GenerateBookThumbnail => {
            super::maintenance_jobs::execute_generate_book_thumbnail(runtime, task_target).await
        }
        TaskKind::RefreshSeriesLocalArtwork => {
            super::maintenance_jobs::execute_refresh_series_local_artwork(runtime, task_target)
                .await
        }
        TaskKind::EmptyTrash => {
            super::maintenance_jobs::execute_empty_trash(runtime, task_target).await
        }
        TaskKind::DeleteBook => {
            super::maintenance_jobs::execute_delete_book(runtime, task_target).await
        }
        TaskKind::DeleteSeries => {
            super::maintenance_jobs::execute_delete_series(runtime, task_target).await
        }
        TaskKind::RepairExtension => {
            super::maintenance_jobs::execute_repair_extension(runtime, task_target).await
        }
        TaskKind::FindBooksToConvert => {
            super::maintenance_jobs::execute_find_books_to_convert(runtime, task, task_target).await
        }
        TaskKind::ConvertBook => {
            super::maintenance_jobs::execute_convert_book(runtime, task_target).await
        }
        TaskKind::ImportBook => super::import_jobs::execute_import_book(runtime, task).await,
    }
}
