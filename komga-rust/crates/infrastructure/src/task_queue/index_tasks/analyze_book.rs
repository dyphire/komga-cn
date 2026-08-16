use super::super::media_helpers::analyze_book_media_file;
use super::super::media_helpers::media_updates::adjust_analyzed_book_read_progress;
use super::super::runtime_context::JobRuntime;
use super::book_analysis_persistence::{
    AnalyzedBookMedia, AnalyzedBookMediaFile, AnalyzedBookPage, analyze_book_input,
    persist_book_analysis,
};
use crate::resolve_library_item_path;
use komga_application::task_processing::TaskProcessingError;
use komga_domain::discovery::MediaStatus;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::task_queue) struct AnalyzeBookOutcome {
    pub(in crate::task_queue) series_id: String,
    pub(in crate::task_queue) media_status: Option<MediaStatus>,
}

pub(in crate::task_queue) async fn analyze_book(
    runtime: &JobRuntime<'_>,
    book_id: &str,
) -> Result<AnalyzeBookOutcome, TaskProcessingError> {
    let book_id = book_id.to_string();
    if !runtime.database().owns_main_database() {
        return Ok(AnalyzeBookOutcome {
            series_id: String::new(),
            media_status: None,
        });
    }

    let Some(input) = analyze_book_input(runtime.database().read_pool(), &book_id)
        .await
        .map_err(TaskProcessingError::runtime)?
    else {
        return Ok(AnalyzeBookOutcome {
            series_id: String::new(),
            media_status: None,
        });
    };

    let file_path = resolve_library_item_path(&input.root, &input.url);
    let analysis =
        analyze_book_media_file(&file_path, input.analyze_dimensions).map_err(|error| {
            TaskProcessingError::runtime(format!(
                "failed to analyze media file for '{book_id}' ('{}'): {error}",
                file_path.display(),
            ))
        })?;

    let persisted = AnalyzedBookMedia {
        status: analysis.status,
        media_type: analysis.media_type,
        page_count: analysis.page_count,
        pages: analysis
            .pages
            .into_iter()
            .map(|page| AnalyzedBookPage {
                file_name: page.file_name,
                media_type: page.media_type,
                width: page.width,
                height: page.height,
                file_size: page.file_size,
            })
            .collect(),
        media_files: analysis
            .media_files
            .into_iter()
            .map(|file| AnalyzedBookMediaFile {
                file_name: file.file_name,
                media_type: file.media_type,
                sub_type: file.sub_type,
                file_size: file.file_size,
            })
            .collect(),
        epub_extension_blob: analysis.epub_extension_blob,
    };
    let current_page_count = persisted.page_count.min(i64::MAX as u64) as i64;

    persist_book_analysis(runtime.database().write_pool(), &book_id, &persisted)
        .await
        .map_err(TaskProcessingError::runtime)?;

    runtime
        .search_engine()
        .upsert_book(&book_id)
        .await
        .map_err(TaskProcessingError::runtime)?;

    adjust_analyzed_book_read_progress(
        runtime.database().write_pool(),
        &book_id,
        &input.series_id,
        input.previous_media_status,
        input.previous_page_count,
        current_page_count,
    )
    .await
    .map_err(TaskProcessingError::runtime)?;

    Ok(AnalyzeBookOutcome {
        series_id: input.series_id,
        media_status: Some(persisted.status),
    })
}
