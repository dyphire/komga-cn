use std::collections::BTreeSet;

use crate::search::index_lifecycle::SearchEntityType;
use komga_application::task_processing::{
    ScanOneLibrary, TaskExecutionOutcome, TaskProcessingError,
};

use super::JobRuntime;

pub(super) enum TaskJobCommand<'a> {
    ScanLibrary(ScanOneLibrary),
    HashBookPages {
        book_id: &'a str,
    },
    HashBook {
        book_id: &'a str,
        koreader: bool,
    },
    FindBooksWithMissingPageHash {
        library_id: &'a str,
        priority: i32,
    },
    FindDuplicatePagesToDelete {
        library_id: &'a str,
        priority: i32,
    },
    RemoveHashedPages {
        book_id: &'a str,
        pages: Vec<super::HashedPageToDelete>,
        priority: i32,
    },
    AnalyzeBook {
        book_id: &'a str,
        priority: i32,
    },
    RebuildIndex {
        entity_types: Option<Vec<SearchEntityType>>,
    },
    UpgradeIndex,
    FindBookThumbnailsToRegenerate {
        for_bigger_result_only: bool,
        priority: i32,
    },
    RefreshBookMetadata {
        book_id: &'a str,
        capabilities: BTreeSet<String>,
        priority: i32,
    },
    RefreshSeriesMetadata {
        series_id: &'a str,
        priority: i32,
    },
    AggregateSeriesMetadata {
        series_id: &'a str,
    },
    RefreshBookLocalArtwork {
        book_id: &'a str,
    },
    GenerateBookThumbnail {
        book_id: &'a str,
    },
    RefreshSeriesLocalArtwork {
        series_id: &'a str,
    },
    EmptyTrash {
        library_id: &'a str,
    },
    DeleteBook {
        book_id: &'a str,
    },
    DeleteSeries {
        series_id: &'a str,
    },
    RepairExtension {
        book_id: &'a str,
    },
    FindBooksToConvert {
        library_id: &'a str,
        priority: i32,
    },
    ConvertBook {
        book_id: &'a str,
    },
    ImportBook {
        payload: String,
        priority: i32,
    },
}

pub(super) struct TaskJobDispatcher<'a> {
    runtime: JobRuntime<'a>,
}

impl<'a> TaskJobDispatcher<'a> {
    pub(super) fn new(runtime: JobRuntime<'a>) -> Self {
        Self { runtime }
    }

    pub(super) async fn execute(
        &self,
        command: TaskJobCommand<'_>,
    ) -> Result<TaskExecutionOutcome, TaskProcessingError> {
        match command {
            TaskJobCommand::ScanLibrary(request) => {
                super::scanner_jobs::execute_scan_library(&self.runtime, request).await
            }
            TaskJobCommand::HashBookPages { book_id } => {
                super::scanner_jobs::execute_hash_book_pages(&self.runtime, book_id).await
            }
            TaskJobCommand::HashBook { book_id, koreader } => {
                super::scanner_jobs::execute_hash_book(&self.runtime, book_id, koreader).await
            }
            TaskJobCommand::FindBooksWithMissingPageHash {
                library_id,
                priority,
            } => {
                super::scanner_jobs::execute_find_books_with_missing_page_hash(
                    &self.runtime,
                    library_id,
                    priority,
                )
                .await
            }
            TaskJobCommand::FindDuplicatePagesToDelete {
                library_id,
                priority,
            } => {
                super::scanner_jobs::execute_find_duplicate_pages_to_delete(
                    &self.runtime,
                    library_id,
                    priority,
                )
                .await
            }
            TaskJobCommand::RemoveHashedPages {
                book_id,
                pages,
                priority,
            } => {
                super::scanner_jobs::execute_remove_hashed_pages(
                    &self.runtime,
                    book_id,
                    &pages,
                    priority,
                )
                .await
            }
            TaskJobCommand::AnalyzeBook { book_id, priority } => {
                super::index_jobs::execute_analyze_book(&self.runtime, book_id, priority).await
            }
            TaskJobCommand::RebuildIndex { entity_types } => {
                super::index_jobs::execute_rebuild_index(&self.runtime, entity_types.as_deref())
                    .await
            }
            TaskJobCommand::UpgradeIndex => Ok(TaskExecutionOutcome::completed()),
            TaskJobCommand::FindBookThumbnailsToRegenerate {
                for_bigger_result_only,
                priority,
            } => {
                super::index_jobs::execute_find_book_thumbnails_to_regenerate(
                    &self.runtime,
                    for_bigger_result_only,
                    priority,
                )
                .await
            }
            TaskJobCommand::RefreshBookMetadata {
                book_id,
                capabilities,
                priority,
            } => {
                super::maintenance_jobs::execute_refresh_book_metadata(
                    &self.runtime,
                    book_id,
                    &capabilities,
                    priority,
                )
                .await
            }
            TaskJobCommand::RefreshSeriesMetadata {
                series_id,
                priority,
            } => {
                super::maintenance_jobs::execute_refresh_series_metadata(
                    &self.runtime,
                    series_id,
                    priority,
                )
                .await
            }
            TaskJobCommand::AggregateSeriesMetadata { series_id } => {
                super::maintenance_jobs::execute_aggregate_series_metadata(&self.runtime, series_id)
                    .await
            }
            TaskJobCommand::RefreshBookLocalArtwork { book_id } => {
                super::maintenance_jobs::execute_refresh_book_local_artwork(&self.runtime, book_id)
                    .await
            }
            TaskJobCommand::GenerateBookThumbnail { book_id } => {
                super::maintenance_jobs::execute_generate_book_thumbnail(&self.runtime, book_id)
                    .await
            }
            TaskJobCommand::RefreshSeriesLocalArtwork { series_id } => {
                super::maintenance_jobs::execute_refresh_series_local_artwork(
                    &self.runtime,
                    series_id,
                )
                .await
            }
            TaskJobCommand::EmptyTrash { library_id } => {
                super::maintenance_jobs::execute_empty_trash(&self.runtime, library_id).await
            }
            TaskJobCommand::DeleteBook { book_id } => {
                super::maintenance_jobs::execute_delete_book(&self.runtime, book_id).await
            }
            TaskJobCommand::DeleteSeries { series_id } => {
                super::maintenance_jobs::execute_delete_series(&self.runtime, series_id).await
            }
            TaskJobCommand::RepairExtension { book_id } => {
                super::maintenance_jobs::execute_repair_extension(&self.runtime, book_id).await
            }
            TaskJobCommand::FindBooksToConvert {
                library_id,
                priority,
            } => {
                super::maintenance_jobs::execute_find_books_to_convert(
                    &self.runtime,
                    library_id,
                    priority,
                )
                .await
            }
            TaskJobCommand::ConvertBook { book_id } => {
                super::maintenance_jobs::execute_convert_book(&self.runtime, book_id).await
            }
            TaskJobCommand::ImportBook { payload, priority } => {
                super::import_jobs::execute_import_book(&self.runtime, payload, priority).await
            }
        }
    }
}
